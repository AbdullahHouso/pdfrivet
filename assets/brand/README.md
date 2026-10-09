# PDFRivet brand

A page folded into a lowercase **r**, held together by a copper **rivet**.

| File | Use |
|---|---|
| `rivet-icon.svg` | The app icon (1024 × 1024 master). All platform icons in `app/src-tauri/icons/` are generated from it. |
| `rivet-glyph.svg` | The mark without the tile, for dark backgrounds (the cream page needs contrast). |
| `rivet-mono.svg` | Single colour (`currentColor`): tray icons, favicons, monochrome contexts. |
| `rivet-social.png` | GitHub social preview (1280 × 640). |
| `pdf-file-icon/pdf-<size>.svg` | The icon Windows shows on `.pdf` files once PDFRivet is the default PDF app. One drawing per size (16, 20, 24, 32, 40, 48, 64, 96, 256), each aligned to whole pixels; don't scale one size to make another. Built into `app/src-tauri/icons/pdf-file.ico`. |

Design source: [Figma – PDFRivet logo](https://www.figma.com/design/zE1HrzF2HSjyoD1zfm7qCN/Untitled?node-id=3-22)

## Palette

| Name | Hex | Use |
|---|---|---|
| Charcoal | `#17181C` | Icon tile, dark backgrounds |
| Graphite | `#2A2C33` | Icon tile highlight, dark surfaces |
| Paper | `#F4EFE7` | The page |
| Copper | `#DC7B49` | Rivet, accent on dark backgrounds |
| Copper deep | `#B4552A` | Rivet rim, accent on light backgrounds |
| Copper light | `#EA9460` | Fold highlight |

The UI accent uses Copper deep on light themes and Copper on dark themes
(with Charcoal text), which keeps text contrast at WCAG AA or better.

## Rules

- Don't recolour, stretch, rotate or add effects to the app icon.
- Keep clear space around the mark of at least the rivet's diameter.
- Below 24 px the rivet is not visible; that's expected — the "r" silhouette carries the icon.

## Regenerating platform icons

```sh
cd app
bun tauri icon ../assets/brand/rivet-icon.svg
rm -rf src-tauri/icons/android src-tauri/icons/ios   # mobile isn't built yet
```

`bun tauri icon` makes every size from one picture, which blurs the small sizes of the PDF
file icon, so `pdf-file.ico` is packed from the hand-drawn sizes instead (needs `rsvg-convert`
and Python with Pillow):

```sh
cd assets/brand/pdf-file-icon
for s in 16 20 24 32 40 48 64 96 256; do rsvg-convert pdf-$s.svg -o /tmp/pdf-$s.png; done
python3 -c "
from PIL import Image
S = [16, 20, 24, 32, 40, 48, 64, 96, 256]
im = {s: Image.open(f'/tmp/pdf-{s}.png') for s in S}
im[256].save('../../../app/src-tauri/icons/pdf-file.ico', sizes=[(s, s) for s in S],
             append_images=[im[s] for s in S[:-1]])"
```

The Windows installer gives `.pdf` files this icon through
`app/src-tauri/windows/installer-hooks.nsh`. Windows shows it only while PDFRivet is the
default app for PDFs; otherwise Explorer shows the default app's icon.

The logo is part of the PDFRivet project and licensed under the MPL-2.0 like the rest of the repository.
