# Contributing to PDFRivet

Thanks for helping! PDFRivet is early, so issues and ideas are as valuable as code.

## Setup

Follow "Building from source" in the [README](README.md). Before opening a pull request:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app && bun run format && bun run check && bun run test && bun run i18n:check
```

## Guidelines

- **Keep PDF logic in `rivet-core`.** The Tauri layer and UI should stay thin.
- **No user-facing text in Rust.** Return an `ErrorCode`; the UI translates it.
- **Use CSS logical properties** (`inline`/`block`) so layouts work in RTL languages.
- **Use the latest stable versions** of dependencies. Renovate opens update PRs weekly.
- **Test with real files.** Add small, freely licensed PDFs to `tests/fixtures/`
  (with their source in `tests/fixtures/src/` when possible). Never commit private documents;
  put them in `tests/private/`, which git ignores.

## Translating

1. Copy `locales/en/` to `locales/<code>/` (e.g. `fr`, `fa`, `ur`) and translate the values.
2. Add the language to `locales/languages.json` with its native name and `"dir": "ltr"` or `"rtl"`.
3. Run `cd app && bun run i18n:check`.

The app name **PDFRivet** is never translated or transliterated; keep it in Latin letters in every language.

That's all: the app picks the new language up automatically. Missing strings fall back
to English, so partial translations are welcome.

## Commit messages

Short imperative summary (`Add page thumbnails`), with details in the body if needed.
