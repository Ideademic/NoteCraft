# NoteCraft app icon

A spiral-bound notebook page on a full-bleed NoteCraft-purple tile: five coil rings over the left
edge, three section tabs sticking out of the right edge (the middle one, the open section, in paper
colour), a title rule, a line of looped handwriting, a plain rule, and the Craft pen nib in the
bottom-right corner. The page is tilted −6° like the other Craft placeholder icons.

It is original geometry written by hand for this repository, in the house style of the Craft
apps' placeholder icons (512 tile, `rx=112`, cream paper, 14 px ink outlines with round joins). It
is deliberately **not** modelled on Microsoft OneNote's logo or any other vendor's artwork: no
letters, no "N", no overlapping panels, no Office colours. NoteCraft is a community app, so it
carries no ArtCraft or Storyteller logos either.

## Palette

Exactly three colours:

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | Line work, rings, handwriting, pen |
| Paper | `#efe9dc` | The page, the open section tab, the pen cap |
| NoteCraft purple (app colour) | `#c44df0` | The full-bleed field and the two other section tabs |

`#c44df0` is `APP_COLOR` in `crates/ui-egui` (`theme::Tokens`). The Linux metainfo derives its
branding colours from it: light `#f3dcfc`, dark `#5c1f78`.

## Geometry

A 512 × 512 viewBox, clipped to a rounded square with `rx=112`. Windows and Linux use the full
tile. The macOS icons (`.icns`, `notecraft-macos-512.png`) sit on Apple's grid: an 824/1024 body
with a transparent margin.

## Files

| File | Use |
|---|---|
| `notecraft.svg` | Canonical artwork; also `hicolor/scalable` |
| `notecraft-1024.png` | 1024 px render of the full tile |
| `notecraft-macos-512.png` | Runtime Dock icon on macOS (embedded by `apps/notecraft`) |
| `notecraft.icns` | macOS bundle icon (`CFBundleIconFile`) |
| `notecraft.ico` | Windows exe icon, 16–256 px (embedded by `apps/notecraft/build.rs`); web favicon |
| `hicolor/<size>/apps/com.ideademic.notecraft.png` | Linux icon theme, 16–512 px; the 256 px one is also the runtime window icon on Windows and Linux |
| `hicolor/scalable/apps/com.ideademic.notecraft.svg` | Linux scalable icon |
| `LICENSE.txt` | MIT OR Apache-2.0 |

The app ID is `com.ideademic.notecraft`. It's used for the hicolor icon names, the Wayland app ID
and `packaging/linux/com.ideademic.notecraft.desktop`.

## Regenerate

Edit `notecraft.svg`, then run `packaging/icons.sh`. It needs `resvg`, and `iconutil` for the
`.icns` (macOS only). It packs the `.ico` with `cargo xtask ico`. The outputs are committed, so
builds and packaging never need these tools.
