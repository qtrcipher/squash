# Squash — Brand Mark

**Concept:** a package gently pressed between two opposing chevrons — compression as
"smaller and neat," not destruction. Flat, monochrome-first (the glyph is a single flat
color knocked out of the tile), no text, legible from 16px up. Brief: `docs/04-design-direction.md` §4.

## Palette

| Use | Light | Dark |
|---|---|---|
| Tile (accent) | `#2563EB` | `#60A5FA` (swap fill for a dark-theme variant) |
| Glyph | `#FFFFFF` | `#FFFFFF` |

The master `icon.svg` ships in the light tile color. The glyph works as pure monochrome
(single-color silhouette) for menu-bar/tray use: render only the white `<g>` + tape
elements in any single color on transparent.

## Files

- `icon.svg` — hand-written master, 512 viewBox, flat geometry.
- `icon-1024.png` — 1024×1024 sRGB master rendered from the SVG.
- `../../app/src-tauri/icons/` — generated Tauri icon set (do not hand-edit).

## Regenerating

```sh
# 1024 master (rsvg-convert from librsvg; `brew install librsvg`)
rsvg-convert -w 1024 -h 1024 assets/brand/icon.svg -o assets/brand/icon-1024.png

# Full Tauri set (also refreshes icon.icns / icon.ico / Windows Square* tiles)
cd app && npx @tauri-apps/cli icon ../assets/brand/icon-1024.png -o src-tauri/icons
```

Then verify `cd app/src-tauri && cargo check` still passes (the icons are embedded via
`generate_context!`).
