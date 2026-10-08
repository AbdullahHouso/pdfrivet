# PDFRivet brand

A page folded into a lowercase **r**, held together by a copper **rivet**.

| File | Use |
|---|---|
| `rivet-icon.svg` | The app icon (1024 × 1024 master). All platform icons in `app/src-tauri/icons/` are generated from it. |
| `rivet-glyph.svg` | The mark without the tile, for dark backgrounds (the cream page needs contrast). |
| `rivet-mono.svg` | Single colour (`currentColor`): tray icons, favicons, monochrome contexts. |
| `rivet-social.png` | GitHub social preview (1280 × 640). |

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

The logo is part of the PDFRivet project and licensed under the MPL-2.0 like the rest of the repository.
