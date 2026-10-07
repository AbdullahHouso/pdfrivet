# Test fixtures

Small PDFs used by the automated tests. All were created for Rivet and are covered by the
project license.

| File | What it tests | Source |
|---|---|---|
| `basic.pdf` | 3 pages: A4, US Letter, A4 landscape; metadata | `src/basic.typ` |
| `arabic.pdf` | Arabic shaping, mixed RTL/LTR text, Arabic metadata | `src/arabic.typ` |
| `broken.pdf` | A truncated, invalid file | hand-written |
| `password.pdf` | AES-256 encryption; password `rivet` | `basic.pdf` encrypted with pypdf |

Rebuild with [Typst](https://typst.app): `typst compile src/basic.typ basic.pdf`.
