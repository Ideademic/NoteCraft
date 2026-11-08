# Roadmap

NoteCraft is in **early alpha** (v0.2.0). It is usable for taking notes, but expect rough edges,
and keep backups of notebooks that matter: until the items under *Interoperability* are done,
treat `.one` files written by NoteCraft as NoteCraft's own.

Status words: **done**, **partial**, **planned**.

## v0.2.0 (now)

| Area | Status | Notes |
|---|---|---|
| Notebooks, section groups, sections, pages, subpages | done | Create, rename, reorder, colour, delete; page list with subpages |
| Recycle Bin | done | Deleted pages and sections kept for 60 days in the notebook's `OneNote_RecycleBin` folder; restore, restore to, delete forever, empty |
| Page versions | partial | Kept while editing, viewed read-only, restored; written as MS-ONE version history. Whether OneNote shows NoteCraft's versions is unverified |
| Unread pages, Recent Edits | done | Pages changed since you last saw them are bold; Next Unread; Recent Edits by period |
| Note containers | done | Click anywhere and type; rich text in real fonts, paragraph styles, alignment, bullet and number styles, indent levels |
| Tables | partial | Insert, edit, rows and columns, widths, shading, hidden borders, sort, convert to text; saved as MS-ONE tables. No nested tables, merged cells or pictures in cells |
| Tags | done | Eleven tags with Ctrl+1…9; Tags Summary |
| Links | done | Web, page and paragraph links; `[[wiki links]]` |
| Spelling | partial | As you type, with suggestions, a Spelling pane and a personal dictionary. English is built in; other languages need a Hunspell dictionary on the computer (the desktop) |
| Editing | done | Format Painter, rich clipboard, Find and Replace, Insert Space, picture rotate, flip, crop and alt text, arrange, page templates |
| Ink | done | Pen, pencil, highlighter with pressure; stroke eraser; lasso select; rotate and flip; Ink to Shape |
| Shapes | done | Draw › Shapes: lines, arrows, basic shapes and graph axes, stored as ink |
| Pictures, file printouts, attachments | done | Insert pictures; insert a PDF as page pictures and ink over it; attach any file and open or save it again |
| Search | done | Page titles and text in a section, section group, notebook or everywhere; Ctrl+F on the page |
| Page setup | done | Page colour, rule lines and grids, hide the page title |
| Native storage | done | Notebook folder with `Open Notebook.onetoc2` and one `.one` per section; autosave |
| `.onepkg` | done | Import and export |
| Export and printing | done | HTML, Markdown, text, PDF; File › Print through the system spooler (desktop) or the browser |
| Fonts | done | Installed fonts, fonts you add, and the craft-fonts faces bundled in release builds |
| Updates | done | Checks GitHub for a newer release (desktop) |
| Light and dark mode | done | |
| Interface languages | partial | Wired through craft-i18n; builds are English only until its catalogs ship with the release builds |
| Command line | done | `notecraft-cli info`, `dump`, `convert`, `commands` |
| Web build | partial | Runs in the browser; notebooks are kept in the browser's storage (export them to keep a copy) or travel through a hosting page (`craft-web-host`); no system printer list or update checks |

## Next

### Interoperability (the most important open question)
- **Verify `.one` / `.onetoc2` writing against real Microsoft OneNote** (2016/2019 desktop and the
  Windows 10/11 app): open NoteCraft-written notebooks in OneNote, round-trip them, and fix what it
  rejects. That includes tables, tags, alignment, the recycle bin and page versions (written from
  the specifications, never checked in OneNote). Until then this is **experimental and
  unverified**.
- Read more of what real OneNote files contain: nested tables and merged cells, pictures in table
  cells, equations (math), audio and video recordings, section passwords (read-only detection).
  Today these are kept in the file and shown as placeholders.

### Notes
- Tables: nested tables, merged cells, pictures in cells.
- Equations.
- Handwriting recognition (Ink to Text) and better handwriting: smoothing, pressure curves,
  stroke-part erasing everywhere.
- A ruler; filled shapes.
- Audio and video notes.

### App
- Interface translations: ship craft-i18n's catalogs in the release builds.
- More spelling languages built in (the SCOWL dictionaries are embedded only when a build sets
  `CRAFT_I18N_DIR`).
- AppStream screenshots.

### Packaging
- Signed and notarized macOS builds, signed Windows installers, signed Linux packages (the release
  workflow already supports the secrets; NoteCraft doesn't have certificates yet).
- Flathub submission (the from-source manifest is in `packaging/linux/flatpak/`).

## Not planned
- Cloud sync or accounts. NoteCraft is local-first; sync notebooks with any file-sync tool.
- Dock to Desktop (a Windows-only OneNote window mode).
- Anything copied from Microsoft's or another vendor's software, artwork or code.

Ideas and requests are welcome in [issues](https://github.com/Ideademic/NoteCraft/issues).
