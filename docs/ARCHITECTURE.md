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
- **Progressive pages:** a page coming into view first asks the cache (`cached=1`); if it was
  rendered before at that size it appears sharp at once. Otherwise a quick low-resolution
  preview shows while the sharp page renders. When zooming, the old pixels are stretched
  until the new render arrives.
- **PDFium's own caches:** PDFium keeps fonts, images and other data of every page it has
  shown until the document is closed (about 20 MB per 100 pages in image-heavy files). After
  100 pages the engine quietly reopens the document from the same bytes
  (`Document::release_memory`, ~30 ms, between requests), which keeps memory flat. It never
  does this while filled-in form fields are unsaved, because they exist only inside PDFium.

## Tabs and files from the OS

- Each tab (`tabs.svelte.ts`) keeps its page, zoom, rotation and scroll position. Only the
  active tab's viewer is mounted, so background tabs use no page memory.
- `tauri-plugin-single-instance`: launching PDFRivet again (e.g. double-clicking a PDF) sends the
  file to the running window. Files are queued in Rust and the UI is told with an
  `open-files` event (`take_pending_files`). macOS delivers files via `RunEvent::Opened`.
- Settings and recent files are stored with `tauri-plugin-store` (`settings.svelte.ts`) and
  edited in `SettingsDialog.svelte`. Every change is saved at once; other windows follow through
  the store's change events (the language, kept in `localStorage`, through `storage` events).
  Every window reads the same store and follows changes made in the others (`onChange`).
- **Separate windows** (setting "Open documents in"): each PDF opens in a window of its own
  (`docWindows.ts`, label `doc-*`, told its file through `?open=<path>` in the URL). Files
  from the OS are announced to one window only (the focused one), so they open once.
- **Windows taskbar previews** (`taskbar_tabs.rs`, Windows only): in tabs mode, each tab is
  represented by an invisible "proxy" window registered with `ITaskbarList3::RegisterTab`.
  The taskbar asks each proxy for its preview (`WM_DWMSENDICONICTHUMBNAIL`), answered with
  the tab's current page rendered by the engine; clicking or closing a preview is sent to the
  UI as a `taskbar-tab` event.
- **Shortcuts** (`keys.ts`): matched by the typed letter on Latin layouts and by key position
  on others, so Ctrl+S works while the Arabic layout is active.

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

## Forms and saving

- Field **reading** uses pdfium-render's safe API (`crates/rivet-core/src/forms.rs`).
- Field **changes** go through PDFium's own form-filling engine (`FORM_*` functions, as in
  Chrome's PDF viewer): a checkbox is "clicked", a text field is focused and its text
  replaced, an option is selected. PDFium applies each field type's rules and regenerates
  the field's appearance, so results look right in other readers too.
- pdfium-render doesn't expose the raw handles these functions need, so PDFRivet uses a fork
  with a tiny patch (`[patch.crates-io]` in `Cargo.toml`;
  https://github.com/AbdullahHouso/pdfium-render, branch `rivet/expose-raw-handles`).
  `forms.rs` is the only module allowed to use `unsafe`; each call explains why it's safe.
- In the UI, `FormLayer.svelte` puts invisible controls over the fields PDFium draws. Text
  fields open a real `<input>`, so typing Arabic and using input methods work normally.
  After each change the tab's `revision` goes up and the page re-renders.
- Files up to 512 MB are **loaded into memory** when opened, so the file isn't kept open:
  saving over it works on Windows, and it can be moved while PDFRivet shows it.
- **Saving** writes `.<name>.rivet-saving` next to the target, flushes it to disk and renames
  it over the target, so a crash never leaves a half-written PDF.
- Known PDFium limitation: in fields mixing Arabic and Latin text, PDFium may order numbers
  differently from the typed order (Chrome shares this).

## Text selection

- `rivet-core/src/text.rs` reads every character of a page with its box (PDFium's "loose"
  boxes, as tall as the font's line) and sends them to the UI over the protocol as
  `GET /text/<doc>/<page>`: a compact binary form (24 bytes per character, 4-byte aligned so
  the UI reads it with typed arrays). Boxes are page fractions, like links and fields.
- A selection is a range of **character positions in PDFium's text order**, from where the drag
  started (anchor) to where it is now (focus), possibly across pages (`textSelect.ts`). This is
  how Chrome's PDF viewer works. An invisible HTML text layer would rely on the browser's bidi
  rules, which don't match how PDFs place Arabic and other right-to-left characters.
- `pageText.ts` keeps the text of recent pages; `TextLayer.svelte` loads it when a page
  appears, so matching the pointer to a character is synchronous while dragging, and draws
  the selection. The Viewer handles the pointer (drag, double/triple click, Shift+click).
- Copying asks the engine for the text of the range (`FPDFText_GetText`, so PDFium's own
  spaces and line breaks are kept) and writes it with the webview's clipboard API (falling
  back to the copy command). The document's "copy" permission is honoured, as in Chrome.
- PDFium reverses right-to-left text into reading order, and that also reverses the letters of
  a ligature (lam-alef "لا" comes out as "ال"). Letters of one ligature share one glyph box, so
  `text.rs` turns such runs back (`fix_reversed_ligatures`); selection, copying and search all
  read characters through it. Copied text is built from those characters, not `FPDFText_GetText`.
- Known PDFium limitation: a line mixing left-to-right and right-to-left text may come out with
  its parts in the wrong order (PDFium decides one direction per line). Chrome shares this.
- On Linux and macOS the right-click menu event arrives when the button goes *down*, so the
  context menu is a manual popover (an automatic one closes again on button up).

## Search

- `rivet-core/src/search.rs` searches the page characters itself instead of using PDFium's
  `FPDFText_FindStart`, which compares exactly. Page text and query are both **folded**: Arabic
  diacritics and tatweel dropped, alef forms → ا, ى/ی → ي, ک → ك, Arabic-Indic and Persian digits
  → 0–9, compatibility decomposition (presentation forms, ligatures, accents), lower case unless
  "match case", and whitespace runs (including line breaks) → one space so phrases match across
  lines. Each folded character remembers its source character, so a match is a character range,
  exactly like a selection.
- The engine searches in **batches** (up to 40 pages or 60 ms) and returns where to continue.
  `search.svelte.ts` keeps asking for the next batch, adding results as they come; a new search
  simply stops asking for the old one. Renders run between batches, so scrolling stays smooth.
- Results are drawn by `TextLayer.svelte` (same rectangles as selections). The current result is
  scrolled into view by the Viewer; the sidebar's Search pane lists all results (virtualized).

## Annotations

- `rivet-core/src/annotations.rs` reads, adds, changes and deletes annotations through PDFium's
  `FPDFAnnot_*` functions (raw calls; pdfium-render wraps only a few). The UI exchanges
  `Annotation` values in page fractions, like everything else.
- Kinds and how they're stored: text markup = Highlight/Underline/StrikeOut/Squiggly with one
  quad per line; freehand = Ink; rectangle/ellipse = Square/Circle; **lines and arrows = Ink**
  (the line plus two strokes for the head) marked with a private `PDFRivetShape` key, because
  PDFium can't write a Line annotation's end points; sticky notes = Text. Annotations from other
  apps that PDFRivet can't edit come back as `Other` (shown, deletable).
- An annotation's id is its `/NM` name (ours get a fresh one; an unnamed annotation is `#index`
  until it is first changed).
- **Appearances:** PDFium draws missing appearance streams itself the first time a page renders
  and stores them in the file, so other readers show the same thing. Changing an annotation
  removes its appearance first (PDFium also refuses to recolour one that has an appearance).
  Exceptions: notes get our own icon in their colour (PDFium always draws notes yellow).
- **Reading colours:** once drawn, PDFium no longer reports `C`/`IC`, so colours are read from
  the appearance's path objects (what you actually see).
- Removing a square's fill writes an empty string for `IC` (PDFium has no call to delete a key);
  readers treat it as "no fill".
- **UI:** `annotate.svelte.ts` holds the Annotate toolbar's state (tool, per-tool styles saved in
  settings) and the changes (`add`, `update`, `remove`, `undo`, `redo`), each recorded in the
  tab's `History` and re-rendering only its page (`tab.pageRevisions`). `AnnotationLayer.svelte`
  sits on each page: it draws previews while drawing or dragging (SVG), invisible shapes to click
  drawings and shapes, the selection frame with handles, and the colour/comment/delete menu.
  Text markup is created from a text selection (Viewer → `markSelection`) and is selected by a
  click through the text, using the page's annotation list kept in `tab.annotations`.
  Geometry (rotation, hit testing, moving, resizing, stroke simplification) is in
  `annotGeometry.ts`.
- Annotation edits mark the document as having unsaved changes (`unsaved_changes`), so the engine
  never reopens it to free memory before they're saved, and only the changed page's renders are
  dropped from the cache.

## Page display and printing

- `layout.ts` arranges pages in **rows** of one or two pages (`columns`), mirrored for
  right-to-left documents (`rtl`). The direction comes from the document, not the UI
  language: `rivet-core/src/direction.rs` samples the text of the first pages and reports
  `DocInfo.rtl`; users can override it per file ("Pages right to left"). With continuous scrolling off, the viewer shows only the
  current row and turns pages at the top/bottom edge (wheel, keys) or with ←/→.
- **Printing** has its own dialog (`PrintDialog.svelte`) with printer, copies, pages, sizing,
  paper, two-sided, grayscale and a live preview. Native printing lives in
  `rivet-core/src/print/`:
  - `mod.rs`: settings types and `place_page`, the page-on-paper maths used by both the
    preview (`print_placement` command) and real printing, so the preview matches the paper.
  - `windows.rs`: printers from the spooler (`EnumPrintersW`, `DeviceCapabilitiesW`), the
    driver's own settings window (`DocumentPropertiesW`), and printing by letting PDFium draw
    each page onto the printer DC (`FPDF_RenderPage`), like Chrome: vector output.
  - `cups.rs` (Linux/macOS): printers from `lpstat`/`lpoptions`, jobs as a PDF of the chosen
    pages sent to `lp` with CUPS options (copies, media, sides, colour mode, scaling).
  - "Use the system print dialog…" falls back to the webview flow (`print.ts`).

## Document properties

- `rivet-core/src/metadata.rs` reads the description and file details (with PDFium) and
  changes title/author/subject/keywords. PDFium can't write metadata, so on save lopdf adds an
  **incremental update** to the bytes PDFium wrote (only the Info dictionary and the XMP stream
  are appended; nothing else is rewritten). Password-protected files are read-only for now.
- The pdfium-render fork also fixes reading the modification date (`ModDate` key).

## Updates

- `tauri-plugin-updater` checks
  `https://github.com/AbdullahHouso/pdfrivet/releases/latest/download/latest.json`, a file
  `tauri-action` writes into every release. "Latest" means the newest *published* release that
  is not marked as a prerelease, so drafts are never offered.
- Every update file is signed with the project's private update key (a GitHub secret). The
  public half is `plugins.updater.pubkey` in `tauri.conf.json`; the app refuses any download
  whose signature doesn't match. **Losing the private key means existing installs can never
  update again**, so it is backed up outside the repository.
- `updater.svelte.ts` runs an automatic check at most once a day, 5 s after startup (release
  builds only, and only while "Check for updates automatically" is on). Automatic checks stay
  silent unless there is a new version the user hasn't skipped; "Check for updates…" always
  shows the result.
- After downloading, the app offers to save documents with changes, then installs and restarts.
  On Windows the NSIS installer runs in passive mode (progress bar only, per-user, no admin
  prompt) and starts the new version; on Linux the AppImage is replaced (a `.deb` install asks
  for the admin password through pkexec); on macOS the `.app` is replaced.
