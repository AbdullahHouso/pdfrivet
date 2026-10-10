# PDFRivet agent guide

A fast, low-memory, open-source (MPL-2.0) PDF reader and editor for Windows, macOS and Linux.
Rust + PDFium (pdfium-render) + Tauri 2 + Svelte 5. Aims at PDFgear-level features.
Website: https://pdfrivet.com (separate private repo). Releases: GitHub, with in-app updates.
These notes are for coding agents and new contributors.

Read `docs/ARCHITECTURE.md` before larger changes, and `CONTRIBUTING.md` for the rules below in full.

## Product boundaries

- A local desktop app: no telemetry, no analytics, no accounts, no hosted backend. Documents never
  leave the machine. The only network access is the update check (GitHub releases) and links the
  user clicks.
- Fast and light comes first. A feature that costs noticeable startup time, memory or install size
  needs a reason; measure before and after.
- Do not broaden a task into adjacent features or a general refactor. Preserve existing user
  behaviour unless the task changes it.
- Current limitations are not product exclusions. A missing feature or a roadmap milestone that
  hasn't been reached is not a reason to turn a request down.
- Fix upstream libraries upstream. A fork is a temporary bridge only, with a comment saying when to
  remove it (see pdfium-render below).

## Privacy

- PDFs the maintainer shares for testing (under `tests/private/` or elsewhere) are personal data.
  Use them to test rendering, memory and behaviour; never copy their text, names or pages into
  commits, fixtures, issues, logs or screenshots meant for others.
- Never log document text, form values, passwords or full file paths at a level that ships.

## Layout

- `crates/rivet-core`: all PDF logic. One PDFium worker thread (`engine.rs`, request enums),
  byte-budgeted LRU render cache, memory release by reopening documents (`document.rs`).
  Types shared with the UI derive `ts_rs::TS`; `cargo test` writes them to `app/src/lib/bindings/`.
- `crates/rivet-cli`: dev CLI (render, `make-test-pdf`). `xtask/`: `cargo xtask fetch-pdfium`.
- `app/src-tauri`: thin Tauri layer (`lib.rs` commands, the `rivet://` page protocol carrying raw
  RGBA, Windows-only `taskbar_tabs.rs` and printing).
- `app/src`: Svelte 5 (runes) UI. `App.svelte` wires things; components and state in `lib/`
  (`settings.svelte.ts` = tauri-plugin-store, `tabs.svelte.ts`, `recent.ts`, `docWindows.ts`).
- `locales/<code>/main.ftl`: Fluent strings; `locales/languages.json` lists languages.
- `tests/fixtures/`: small, freely licensed PDFs. `tests/private/` is git-ignored; never commit personal PDFs.

## Commands

```sh
cargo xtask fetch-pdfium                     # once: PDFium binaries into vendor/
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace                # or cargo test --workspace
cd app && bun install && bun tauri dev       # run the app
cd app && bun run format && bun run check && bun run test && bun run i18n:check
```

CI (`.github/workflows/ci.yml`) runs them on Linux and Windows on every push to `main`
(macOS on demand: `gh workflow run ci.yml --ref main -f macos=true`).

## Rules

- **PDF logic lives in `rivet-core`;** the Tauri layer and UI stay thin.
- **No user-facing text in Rust.** Return an `ErrorCode`; the UI translates it.
- **Every UI string goes in both `locales/en` and `locales/ar`** (`bun run i18n:check` enforces it).
  The Arabic copy is the maintainer's voice; flag new Arabic wording for review.
- **RTL is first-class:** CSS logical properties only (`inline`/`block`), icons that point
  somewhere get `flip-rtl`, mixed-direction text gets `dir="auto"`/`<bdi>`.
- **The name is always "PDFRivet"** in Latin letters, in every language.
- **Keyboard shortcuts** match through `shortcutKey()` (`lib/keys.ts`) so they work on non-Latin layouts.
- **Colours come from theme tokens** in `app/src/app.css` (Light, Dark, Black, System). No hard-coded colours.
- **Fonts are bundled** (`app/src/fonts.css`): Inter for Latin, Rubik for Arabic one weight step lighter.
- **Settings vs. per-file state:** Settings hold defaults for files opened the first time; each file
  remembers its own view (page, zoom, layout, scrolling, page colour) in the recent-files list.
- **Latest stable versions** of everything (dependencies, toolchain, actions); pin exact versions
  for anything new. Renovate opens update PRs; hold back majors that break the toolchain.
- `unsafe_code` is denied workspace-wide; allow it only per item/module with a `SAFETY:` comment.
- Platform-specific code goes behind `cfg` blocks; a change for one platform must keep the other
  two compiling.
- Match the surrounding code's comment style: comments explain *why*, in plain English.
- Public and marketing text presents the app as multilingual with first-class RTL; don't single out
  English/Arabic.

## Pitfalls this code has already hit

- **WebView2 (Windows) differs from Chrome and WebKitGTK.** It kept painting a closed top-layer
  popover, and `:hover` went stale, so tooltips are a plain fixed element hidden on any pointer
  move off the target (`lib/tooltip.ts`). Test hover and popover behaviour in Edge, not only Chrome.
- **Native `<select>` is grey in dark mode** on some platforms; selects are drawn by us (`app.css`).
- **Every WebView2 window must start with the same browser arguments** or creating it fails.
  Windows opened from JS can't be given their own, so process-wide flags (`--disable-lcd-text`)
  go in `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, set at the top of `run()`.
- **PDFium keeps per-document caches** that grow as pages are visited; `rivet-core` reopens the
  document from its bytes every 100 pages, never while there are unsaved form changes.
- **Shortcuts must use `e.code` on non-Latin layouts** (Ctrl+S with the Arabic layout types "س").
- **PDFium text order:** right-to-left runs are reversed into reading order, which also swaps the
  letters of ligatures (lam-alef). Read characters through `text::raw_chars`, never
  `FPDFText_GetText` or `GetUnicode` directly. Mixed LTR/RTL lines can still come out in the
  wrong part order (a PDFium limitation).
- **PDFium annotations:** `FPDFAnnot_GetColor`/`SetColor` fail once an annotation has an
  appearance (clear it first, read colours from the drawing); `FPDFAnnot_SetRect` doesn't update
  the appearance's BBox; there is no API to write a Line's end points or delete a key.
  `annotations.rs` documents the workarounds.
- **PDFium writes pages lossily:** `FPDFPage_GenerateContent` re-serializes every object and drops
  colours set through colour spaces. Don't regenerate a page whose look must stay (redaction keeps
  pages as a rendered picture plus invisible text instead).
- **PDFium's save keeps orphans:** removed objects and unused resources stay in the file. Anything
  that must really leave the file (deleted annotations, redactions) goes through `prune.rs`.
- **Text boxes don't inherit fonts:** `input`/`textarea` need `font: inherit` (set in `app.css`), or
  they use the system font, which may lack Arabic (Linux).
- **Animations:** Svelte 5 transitions on a component's root need `|global`; dialogs must be closed
  by the app (prevent `cancel`, no `<form method="dialog">`) or they vanish before fading; animate
  only opacity/transform, and never the size of something that resizes the pages.
- **Right-click menus:** on Linux/macOS `contextmenu` fires on button down, so menus opened from it
  must be `popover="manual"` (an automatic popover closes on button up).
- **PDFium can't write the outline or `/IRT`:** bookmarks and replies are finished by lopdf on
  save (`outline.rs`, `prune.rs`). PDFium may write new annotations inline in `/Annots`; make them
  objects before anything points to them.
- **HTML drag and drop doesn't work in WebView2** while Tauri handles file drops: drag with
  pointer events (see `BookmarksPane.svelte`).
- **The `Icon` component's `<svg>` has class `icon`, and so do icon buttons:** style an icon
  inside a button with `button :global(svg)`, not `:global(.icon)` (that also hits the button).
- **Vite doesn't reload `locales/*.ftl`** (outside the app folder): restart the dev server after
  adding strings.
- **Fonts from the `rivet:` protocol:** the CSP's `font-src` blocks `@font-face` URLs on it; load
  them with `fetch` + `new FontFace(name, bytes)` (see `textBox.ts`).
- **Pointer capture swallows double-clicks** on the element under the pointer: a drag that
  captures on the layer means `dblclick` must be handled on the layer too.
- **Handlers read `{@const}` values when they run, not when rendered:** inside
  `{#if x}{@const a = x}`, an `onblur` that fires after a click cleared `x` sees `a` as null.
  Keep what a handler needs from when it was set up (see the comment box in `AnnotationLayer`).
- **Windows doesn't let apps make themselves the default PDF app** (since Windows 8; the choice is
  hash-protected). Send the user to Settings (`default_app.rs`); never write `UserChoice` keys.
  The installer registers PDFRivet under `RegisteredApplications` so it has a page there.
- **Windows-only Rust can't be checked from Linux** (ring needs `lib.exe`); rely on CI's Windows job.

## Dependencies to know

- pdfium-render comes from upstream's main branch via `[patch.crates-io]` in the root `Cargo.toml`
  (our merged PRs #276, #277: raw handles, `ModDate`). When **pdfium-render 0.9.5** is released,
  drop the patch. Raw PDFium calls go through `RawBindings` (`document.rs`): `pdfium.bindings()`
  and `bindings.get_handle_from_page/form/document(…)`.

## Development environment

- Development happens in WSL (Arch); the maintainer tests on Windows by pulling `main` and running
  `bun install && bun tauri dev`. Mention it when `package.json` changes.
- Port 5173 may be the maintainer's Windows dev server. Test a local copy on another port
  (`bunx vite --port 5199 --strictPort --host 127.0.0.1`), with headless Edge for the WebView2 engine.
- Never `pkill -f` with a pattern that also matches the running command; kill by PID or port (`fuser -k 5199/tcp`).
- Keep disk use low. Scratch files and screenshots go in the agent's scratchpad, never in the repo;
  delete one-off test output once its result is recorded. Don't `cargo clean` without a reason
  (stale build-script paths after moving the folder is one).

## Branches

Trunk-based: work on `main`, one topic per commit, each commit compiling and passing the checks.
No feature branches or pull requests for the maintainer's own work unless asked; they are for
outside contributors (squash them into one focused commit, keeping credit). Keep `main` linear:
no merge commits, fast-forward pulls only, and never rewrite published history without the
maintainer's explicit approval. Commit or push only when asked. Commit messages: short imperative
summary, details in the body.

## Definition of done

- Focused tests for changed behaviour (Rust tests in `rivet-core/tests`, Vitest next to the UI code).
- Docs updated when user-visible behaviour changes: `CHANGELOG.md` (under `## [Unreleased]` until
  a release), the manual checklist in `docs/testing/`, and `docs/ARCHITECTURE.md` for design changes.
- All the commands above pass. Don't weaken a lint, delete a test, or add an `allow` just to make
  them pass without explaining why the rule doesn't apply.
- Report coverage honestly: say what was run, what was only compiled (e.g. Windows code via CI),
  what was only seen in a browser, and what still needs the maintainer's test on Windows.

## Releasing

Don't cut a release for every fix. Work accumulates on `main` until there is something worth
announcing: a feature or a batch of fixes. The exception is a regression in something just
released, which goes out as soon as it is fixed.

1. Bump the version in `app/package.json` **and** the root `Cargo.toml` (CI checks they match).
2. Turn `## [Unreleased]` in `CHANGELOG.md` into `## [x.y.z] – YYYY-MM-DD`, written for users: it
   becomes the release notes and the in-app update notes. Describe known limitations honestly.
3. Commit, push `main`, and wait for CI. Tag `vX.Y.Z` on that pushed commit and push the tag
   **only when the maintainer says so**. `release.yml` builds, signs (updater key in GitHub
   secrets) and uploads to a draft release.
4. The release isn't finished when the tag is pushed: check every platform built, the assets and
   `latest.json` are there, and the notes read right. The maintainer tests the installer, then
   publishes. Releases must not be pre-releases, or in-app updates won't find them.
5. After publishing, check that `releases/latest/download/latest.json` answers and that the
   website's Download button gets the new installer.
