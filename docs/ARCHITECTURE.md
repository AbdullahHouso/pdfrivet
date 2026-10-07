# Architecture

```
┌──────────────── app (Tauri) ────────────────┐
│  Svelte UI (app/src)                        │
│    │ invoke("open_document")   fetch(rivet://page/…)
│    ▼                           ▼            │
│  Tauri commands         rivet:// protocol   │
│  (app/src-tauri/src/lib.rs)                 │
└──────────────┬──────────────────────────────┘
               │  Engine handle (cloneable, thread-safe)
               ▼
      rivet-core::Engine ── channel ──▶ "pdfium" worker thread
                                           owns Pdf + every open Document
                                           (PDFium is not thread-safe)
```

## Crates

- **rivet-core**: all PDF logic, no UI. `Pdf` loads PDFium; `Document` is one open
  file; `Engine` puts PDFium on one worker thread and talks to it over a channel.
- **rivet-cli**: a terminal front end to rivet-core for testing and debugging.
- **rivet-app** (`app/src-tauri`): thin Tauri layer. Commands return
  `Result<T, rivet_core::Error>`; errors carry a stable `code` that the UI translates.
- **xtask**: developer tasks. `cargo xtask fetch-pdfium` downloads the pinned PDFium
  build from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
  into `vendor/pdfium/<platform>/`. Release builds bundle it as a Tauri resource.

## Rendering pages

The UI asks for `rivet://localhost/page/<doc>/<page>?scale=<s>` (on Windows:
`http://rivet.localhost/...`). The response body is 8 bytes of header (width and height,
little-endian `u32`) followed by raw RGBA pixels, which the UI paints straight onto a
`<canvas>`. Skipping PNG encoding keeps rendering fast.

`scale = zoom × 96/72 × devicePixelRatio`, so pages stay sharp on high-DPI screens.
Query parameters: `scale`, `rot` (0/90/180/270, view only) and `thumb=1` for thumbnails.

### Keeping scrolling fast

- **Virtualized viewer** (`app/src/lib/Viewer.svelte`): page positions come from the page
  sizes (`layout.ts`, pure and unit-tested). Only visible pages ±1 are mounted, so a
  10,000-page file costs the same memory as a 10-page one.
- **Visible range:** the viewer calls `set_visible_pages`; the engine answers renders for
  pages far outside that range with `Cancelled` (HTTP 204) instead of rendering them.
  Thumbnails (`thumb=1`) are exempt.
- **Newest first:** the engine drains its queue and renders the most recent requests first.
- **Render cache:** a 48 MB LRU of rendered pages in the engine (`cache.rs`).
- **Progressive pages:** a quick low-resolution preview, then the sharp page; when zooming,
  the old pixels are stretched until the new render arrives.

## Tabs and files from the OS

- Each tab (`tabs.svelte.ts`) keeps its page, zoom, rotation and scroll position. Only the
  active tab's viewer is mounted, so background tabs use no page memory.
- `tauri-plugin-single-instance`: launching Rivet again (e.g. double-clicking a PDF) sends the
  file to the running window. Files are queued in Rust and the UI is told with an
  `open-files` event (`take_pending_files`). macOS delivers files via `RunEvent::Opened`.
- Settings and recent files are stored with `tauri-plugin-store` (`settings.svelte.ts`).

## Types shared with TypeScript

Rust types marked `#[derive(TS)]` are exported to `app/src/lib/bindings/` when
`cargo test -p rivet-core` runs. Commit the generated files; CI fails if they are stale.

## Internationalization

- Strings live in `locales/<code>/*.ftl` ([Fluent](https://projectfluent.org) syntax).
  `locales/languages.json` lists each language's code, native name and direction.
- The UI bundles every locale at build time (`import.meta.glob`), falls back to English
  for missing strings, and sets `<html lang dir>`. Layout uses CSS logical properties only
  (`margin-inline-start`, `padding-block`…), so RTL needs no special cases.
- The OS language comes from Rust (`sys-locale`), because not every webview reports it.
- Rust never produces user-facing text: errors are codes (`error-<code>` in Fluent).
