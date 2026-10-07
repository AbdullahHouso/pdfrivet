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
