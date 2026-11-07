# Roadmap

NoteCraft is in **early alpha** (v0.1.0). It is usable for taking notes, but expect rough edges,
and keep backups of notebooks that matter: until the items under *Interoperability* are done,
treat `.one` files written by NoteCraft as NoteCraft's own.

Status words: **done**, **partial**, **planned**.

## v0.1.0: the foundation (now)

| Area | Status | Notes |
|---|---|---|
| Notebooks, section groups, sections, pages, subpages | done | Create, rename, reorder, colour, delete; page list with subpages |
| Note containers | done | Click anywhere and type; rich text, paragraph styles, bullets and numbering, To Do / Important / Question tags |
| Ink | done | Pen, pencil, highlighter with pressure; stroke eraser; lasso select |
| Pictures and file printouts | done | Insert pictures; insert a PDF as page pictures and ink over it |
| Page setup | done | Page colour, rule lines and grids |
| Native storage | done | Notebook folder with `Open Notebook.onetoc2` and one `.one` per section; autosave |
| `.onepkg` | done | Import and export |
| Export | done | HTML and Markdown (and plain text, JSON from the CLI) |
| Light and dark mode | done | |
| Command line | done | `notecraft-cli info`, `dump`, `convert`, `commands` |
| Web build | partial | Runs in the browser; notebooks live in the page until you download them |

## Next

### Interoperability (the most important open question)
- **Verify `.one` / `.onetoc2` writing against real Microsoft OneNote** (2016/2019 desktop and the
  Windows 10/11 app): open NoteCraft-written notebooks in OneNote, round-trip them, and fix what it
  rejects. Until then this is **experimental and unverified**.
- Read more of what real OneNote files contain: tables, embedded files, audio/video placeholders,
  math, hyperlinks, page templates, section passwords (read-only detection), version history.
- A sample-file corpus in CI (gitignored inputs, e.g. Apache Tika's Apache-2.0 test files) with
  "never crash" fuzzing of every reader.

### Notes
- Search across notebooks (text and page titles), with results by section.
- Tables in note containers.
- More note tags, and a tags summary.
- Page templates, page versions, recycle bin.
- Linked pages (`[[wiki links]]`) and links to paragraphs.
- Shape and straight-line tools, ink-to-shape, ruler.
- Better handwriting: smoothing, pressure curves, stroke-part erasing everywhere.

### App
- Printing, and PDF export of pages and sections.
- Interface translations through craft-i18n.
- Spelling through craft-proof.
- Web: the `craft-web-host` bridge (`?host=parent` and same-document host mode) and browser
  storage for notebooks.
- Screenshots in the README and AppStream metainfo.

### Packaging
- Signed and notarized macOS builds, signed Windows installers, signed Linux packages (the release
  workflow already supports the secrets; NoteCraft doesn't have certificates yet).
- Flathub submission (the from-source manifest is in `packaging/linux/flatpak/`).

## Not planned
- Cloud sync or accounts. NoteCraft is local-first; sync notebooks with any file-sync tool.
- Anything copied from Microsoft's or another vendor's software, artwork or code.

Ideas and requests are welcome in [issues](https://github.com/Ideademic/NoteCraft/issues).
