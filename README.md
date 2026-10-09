<p align="center">
  <img src="assets/brand/rivet-icon.svg" alt="PDFRivet logo" width="128" height="128">
</p>

<h1 align="center">PDFRivet</h1>

<p align="center"><strong>A free, fast PDF reader and editor for Windows, Linux and macOS.</strong></p>

> **0.1 "Reader"** is out: get it from [pdfrivet.com](https://pdfrivet.com). PDFRivet is young; editing features come in the next versions.

## What it can do (0.1)

- Open several PDFs in **tabs**, from the app, by drag and drop, or with "Open with" in your file manager
- **Continuous scrolling** that stays fast on documents with thousands of pages
- **Zoom** (fit page, fit width, actual size, Ctrl + wheel, pinch) and **rotate** the view
- **Single page or two pages** side by side (spreads follow the document's reading direction), with or without continuous scrolling
- **Print** with PDFRivet's own dialog and a live preview: printer, pages, copies, scaling, paper, two-sided
- **Thumbnails** and the document **outline** (table of contents)
- **Fill in forms** (text fields, checkboxes, radio buttons, drop-downs) and **save**, including over the open file
- Clickable **links** inside documents
- **Password-protected** PDFs
- **Recent files** that reopen where you left off
- **Document properties**: see a file's details and edit its title, author, subject and keywords
- **Updates from inside the app**, signed so only genuine releases install
- **Multilingual interface** with full right-to-left support, light and dark themes

PDFRivet aims to be one small app for everyday PDF work: reading, annotating, signing,
filling forms, organizing pages and converting — while staying light on memory and
easy to use. It is built with Rust, [Tauri](https://tauri.app) and
[PDFium](https://pdfium.googlesource.com/pdfium/) (the PDF engine inside Chrome).

Right-to-left languages are first-class: the whole interface mirrors, and two-page spreads
follow each document's reading direction. Adding a language needs no code changes, so more
translations (left-to-right and right-to-left alike) are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

| Milestone | Version | Scope |
|---|---|---|
| M0 ✅ | – | Foundations: engine, app shell, EN/AR UI, CI |
| M1 ✅ | 0.1 | Reader + forms: tabs, continuous scroll, zoom, thumbnails, outline, links, form filling, save, printing, document properties, in-app updates |
| M2 | 0.2 | Text selection, copy and search |
| M3 | 0.3 | Page tools: merge, split, rotate, reorder, extract |
| M4 | 0.4 | Annotations and hand-drawn signatures |
| M5 | 0.5 | Export to images/text, images to PDF, flattening forms |

## Installing

Download the installer for your system from [pdfrivet.com](https://pdfrivet.com) or the
[Releases](https://github.com/AbdullahHouso/pdfrivet/releases) page.
The installers are not code-signed yet:

- **Windows:** if SmartScreen warns you, choose *More info → Run anyway*.
- **macOS:** the first time, open *System Settings → Privacy & Security* and click *Open Anyway* next to the PDFRivet message.

After that, PDFRivet updates itself: it checks once a day and asks before installing
(menu → *Check for updates…* checks right away). See [CHANGELOG.md](CHANGELOG.md) for what changed.

## Building from source

You need [Rust](https://rustup.rs), [Bun](https://bun.sh) and [Node.js](https://nodejs.org),
plus the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
git clone https://github.com/AbdullahHouso/pdfrivet.git
cd pdfrivet
cargo xtask fetch-pdfium   # downloads the PDFium library for your OS
cd app
bun install
bun tauri dev              # starts PDFRivet in development mode
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

PDFRivet is licensed under the [Mozilla Public License 2.0](LICENSE).
PDFium is distributed under its own BSD-style license (bundled with the app).
The fonts bundled for text boxes, Rubik and Amiri, are under the SIL Open Font License 1.1
(see `crates/rivet-core/fonts/`).
