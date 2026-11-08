# Attribution

Every asset in this repository (icons, images, fonts, sample files) is listed here with its
author, source and licence. `cargo xtask assets` (part of `cargo xtask ci`) fails when a file
isn't. Icons in the app are drawn in code (`crates/ui-egui/src/icons/`) and need no entry. Most
of those drawings are vendored from DeckCraft by the ArtCraft team
([storytold/deckcraft](https://github.com/storytold/deckcraft), `crates/ui-egui/src/icons*`,
MIT OR Apache-2.0) into `crates/ui-egui/src/icons/deck/`; NoteCraft's additions in the same style
(`icons/deck/extra.rs`) redraw a few designs from WordCraft
([storytold/wordcraft](https://github.com/storytold/wordcraft), MIT OR Apache-2.0).

**Policy (mandatory):** NoteCraft contains **no Microsoft (OneNote, Office or other) iconography,
images, templates, fonts, sounds or other assets**, and no ArtCraft or Storyteller logos. Every
asset is original work by NoteCraft contributors or third-party material under a licence in the
allowlist in [`AGENTS.md`](AGENTS.md). Screenshots of other vendors' software are never committed.
Font files belong in [storytold/craft-fonts](https://github.com/storytold/craft-fonts), not here:
builds made with `CRAFT_FONTS_DIR` embed Carlito, Caladea and Liberation Sans/Serif/Mono (OFL-1.1)
from its `fonts/latin-manifest.txt` at build time (`crates/fonts/build.rs`, see `NOTICE`).

| File | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/app-icon/notecraft.svg` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | NoteCraft app icon (spiral notebook page, section tabs, pen); canonical artwork |
| `assets/app-icon/notecraft-1024.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, 1024 px render |
| `assets/app-icon/notecraft-macos-512.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, macOS grid render (runtime Dock icon) |
| `assets/app-icon/notecraft.icns` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, macOS bundle icon |
| `assets/app-icon/notecraft.ico` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Windows exe icon and web favicon |
| `assets/app-icon/hicolor/16x16/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/24x24/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/32x32/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/48x48/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/64x64/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/128x128/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/256x256/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/512x512/apps/com.ideademic.notecraft.png` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, Linux hicolor render |
| `assets/app-icon/hicolor/scalable/apps/com.ideademic.notecraft.svg` | Ideademic and NoteCraft contributors | original (hand-written SVG geometry; rendered by `packaging/icons.sh`) | MIT OR Apache-2.0 (`assets/app-icon/LICENSE.txt`) | App icon, copy of notecraft.svg |
| `assets/app-icon/README.md` | Ideademic and NoteCraft contributors | original | MIT OR Apache-2.0 | Icon documentation and palette |
| `assets/app-icon/LICENSE.txt` | (licence text) | original | — | Licence of the app icon files |
| `packaging/macos/dmg/background.svg` | Ideademic and NoteCraft contributors | original; links the app icon (`assets/app-icon/notecraft.svg`) rather than copying it; layout follows the Craft apps' DMG window; text outlined from Inter and JetBrains Mono (OFL-1.1, via storytold/craft-fonts) | MIT OR Apache-2.0 (outlined glyph shapes: OFL-1.1) | macOS DMG window background |
| `packaging/macos/dmg/background.tiff` | Ideademic and NoteCraft contributors | rendered from `packaging/macos/dmg/background.svg` by `packaging/macos/dmg/generate.py` | MIT OR Apache-2.0 | 1x + 2x HiDPI TIFF for the macOS DMG window; embeds LittleCMS's built-in sRGB profile ("No copyright, use freely") |
| `docs/images/hero.png` | Ideademic and NoteCraft contributors | original (headless render of NoteCraft by `crates/ui-egui/examples/ui_shot.rs`) | MIT OR Apache-2.0 | Screenshot of NoteCraft: a notebook with several sections |
| `docs/images/ink.png` | Ideademic and NoteCraft contributors | original (headless render of NoteCraft by `crates/ui-egui/examples/ui_shot.rs`) | MIT OR Apache-2.0 | Screenshot of NoteCraft: the Draw tab |
| `docs/images/printout.png` | Ideademic and NoteCraft contributors | original (headless render of NoteCraft by `crates/ui-egui/examples/ui_shot.rs`) | MIT OR Apache-2.0 | Screenshot of NoteCraft: a page with text, ink and a picture |
| `docs/images/dark.png` | Ideademic and NoteCraft contributors | original (headless render of NoteCraft by `crates/ui-egui/examples/ui_shot.rs`) | MIT OR Apache-2.0 | Screenshot of NoteCraft in dark mode |
