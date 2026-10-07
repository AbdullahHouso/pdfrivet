<p align="center">
  <img src="assets/brand/rivet-icon.svg" alt="Rivet logo" width="128" height="128">
</p>

<h1 align="center">Rivet</h1>

<p align="center"><strong>A free, fast PDF reader and editor for Windows, Linux and macOS — in English and Arabic.</strong></p>

> 🚧 Early development: **0.1 "Reader"** is being tested. Editing features come in later versions.

## What it can do (0.1)

- Open several PDFs in **tabs**, from the app, by drag and drop, or with "Open with" in your file manager
- **Continuous scrolling** that stays fast on documents with thousands of pages
- **Zoom** (fit width, fit page, Ctrl + wheel, pinch) and **rotate** the view
- **Thumbnails** and the document **outline** (table of contents)
- **Password-protected** PDFs
- **Recent files** that reopen where you left off
- **English and Arabic** interface with full right-to-left layout, light and dark themes

Rivet aims to be one small app for everyday PDF work: reading, annotating, signing,
filling forms, organizing pages and converting — while staying light on memory and
easy to use. It is built with Rust, [Tauri](https://tauri.app) and
[PDFium](https://pdfium.googlesource.com/pdfium/) (the PDF engine inside Chrome).

Arabic and right-to-left languages are first-class: the whole interface mirrors, and
adding a new language needs no code changes (see [CONTRIBUTING.md](CONTRIBUTING.md)).

## Roadmap

| Milestone | Version | Scope |
|---|---|---|
| M0 ✅ | – | Foundations: engine, app shell, EN/AR UI, CI |
| M1 🧪 | 0.1 | Reader: tabs, continuous scroll, zoom, thumbnails, outline, installers |
| M2 | 0.2 | Text selection, copy and search |
| M3 | 0.3 | Page tools: merge, split, rotate, reorder, extract |
| M4 | 0.4 | Annotations and hand-drawn signatures |
| M5 | 0.5 | Form filling, export to images/text, images to PDF |

## Installing

Download the installer for your system from the
[Releases](https://github.com/AbdullahHouso/rivet-pdf/releases) page.
The installers are not code-signed yet:

- **Windows:** if SmartScreen warns you, choose *More info → Run anyway*.
- **macOS:** right-click Rivet and choose *Open* the first time.

## Building from source

You need [Rust](https://rustup.rs), [Bun](https://bun.sh) and [Node.js](https://nodejs.org),
plus the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
git clone https://github.com/AbdullahHouso/rivet-pdf.git
cd rivet-pdf
cargo xtask fetch-pdfium   # downloads the PDFium library for your OS
cd app
bun install
bun tauri dev              # starts Rivet in development mode
```

Useful commands:

```sh
cargo test --workspace                          # Rust tests
cargo run -p rivet-cli -- info file.pdf         # inspect a PDF from the terminal
cargo run -p rivet-cli -- render file.pdf 1 page.png
cargo run -p rivet-cli -- make-test-pdf big.pdf --pages 2000   # a big file for testing
cd app && bun run check && bun run test         # UI type check, lint, tests
```

## Project layout

```
crates/rivet-core   PDF logic (no UI), built on PDFium
crates/rivet-cli    command-line tool over rivet-core
app/                desktop app: Svelte UI (src/) + Tauri shell (src-tauri/)
locales/            translations (Fluent .ftl files)
xtask/              developer tasks (cargo xtask fetch-pdfium)
tests/fixtures/     small test PDFs
docs/               architecture notes and test checklists
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the pieces fit together.

## License

Rivet is licensed under the [Mozilla Public License 2.0](LICENSE).
PDFium is distributed under its own BSD-style license (bundled with the app).
