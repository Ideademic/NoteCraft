# Changelog

All notable changes to NoteCraft are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-11-08

### Added
- **Tables:** Insert › Table (a 10 × 8 grid picker, or a dialog for any size up to the limits),
  and a contextual Table tab in the classic and simplified ribbons: insert and delete rows and
  columns, select cells, rows, columns or the table, shading, Hide Borders, cell alignment, sort
  and Convert to Text. Tab, Enter, Ctrl+Enter and the arrow keys move through cells; drag a
  column border to resize it. Tables are saved as real MS-ONE tables in `.one` files (column
  widths, locks, borders and shading) and appear in HTML, Markdown, text and PDF output. Cells hold
  formatted paragraphs; nested tables, merged cells and pictures in cells are not supported yet.
- **Real fonts:** the font box lists every installed font (each name drawn in its own face), the
  fonts NoteCraft bundles and fonts you add (TTF, OTF, WOFF, WOFF2; on the web they are kept in
  the browser). Text is drawn with the real face, including real bold and italic. Release builds
  embed the metric-compatible Carlito, Caladea and Liberation faces from craft-fonts, which stand
  in for Calibri, Cambria, Arial, Times New Roman and Courier New when those aren't installed.
- **Paragraphs:** alignment (left, centre, right, justify); bullet styles (disc, circle, square,
  dash, arrow) and number styles (1. a. A. i. I.).
- **Tags:** Remember for later, Definition, Highlight, Contact, Address, Phone number, Idea and
  Critical, with Ctrl+1…Ctrl+9 and Remove Tag (Ctrl+0); a Tags Summary pane grouped by tag,
  section, title or date, with a search scope and "unchecked only".
- **History tab:** a notebook Recycle Bin (deleted pages and sections are kept for 60 days and can
  be restored or deleted forever), page versions (kept while you edit, viewable read-only and
  restorable; saved in `.one` files as MS-ONE version history), unread pages in bold with Next
  Unread and Mark as Read, and Recent Edits.
- **Review tab:** spelling checked as you type with suggestions on right-click, Change All, Ignore
  All, a personal dictionary and an F7 Spelling pane; Word Count; a proofing language per
  paragraph. English is built in; other languages use a Hunspell dictionary installed on the
  computer.
- **Links:** Ctrl+K links to web addresses or to pages, Copy Link to Page and Copy Link to
  Paragraph, `[[Page title]]` wiki links (the page is made when it doesn't exist), Ctrl+click to
  follow. Links survive `.one`, HTML and PDF.
- **Attachments:** Insert › File Attachment; open or save an attached file from its right-click
  menu.
- **Right-click menus** for text, pictures, attached files, printouts, ink, note containers, the
  page and content NoteCraft keeps but can't show.
- **Editing:** Format Painter; a rich clipboard (copy and paste formatted text, pictures, note
  containers and ink inside NoteCraft; HTML and pictures to and from other apps; Ctrl+Shift+V
  pastes plain text); Insert Space; Find and Replace on the page (Ctrl+F) with highlights; picture
  Rotate, Flip, Crop and Alt Text; Arrange (bring forward, send back); rotate and flip ink; Hide
  Page Title; undo that takes typing back word by word.
- **Page templates:** a Templates pane with blank, business, academic and planner templates, your
  own templates, and a default template per section.
- **Printing and PDF:** File › Print with a preview, paper size, orientation and a sheet range;
  Save as PDF and File › Export › PDF. PDFs are vector pages with real embedded fonts, ink,
  pictures, links and a bookmark per page. The desktop app prints through the system spooler; the
  web app opens the browser's print dialog.
- **Updates:** NoteCraft checks GitHub for a newer release when it starts (you can turn this off
  in File › Options) and from File › About › Check for Updates. Desktop only.
- **Web:** host mode (`?host=parent` or `data-host="page"`): a hosting page can open a notebook in
  NoteCraft and receives it back as a `.onepkg` on every save.
- **Interface languages:** File › Options › Language, through craft-i18n. Builds made without the
  craft-i18n catalogs, including this release, are English only.

### Changed
- The ribbon starts in Always show. In Show tabs only, clicking a tab (or a KeyTip that opens
  one) brings the whole ribbon back instead of showing it for one click, and double-clicking a tab
  no longer collapses the ribbon; Ctrl+F1 and Ribbon Display Options do that.
- File dialogs no longer block the window while they are open.
- The welcome page's pen underline and highlighter sweep line up with the default font.
- Dates in the page list, Recent Edits, the Recycle Bin and the Tags Summary are written out
  (Nov 8, 2026) instead of as day/month/year numbers.

### Fixed
- Release builds now embed the bundled fallback fonts; 0.1.x builds shipped without them.
- Draw › Delete also deletes a selected picture or note container, not only selected ink.
- File › Print is disabled, with the reason shown, where the host gives no way to print.

## [0.1.1] - 2026-11-07

### Added
- **Shapes:** a Draw › Shapes gallery with 16 shapes (line, arrows, rectangles, ellipse,
  triangles, parallelogram, trapezoid, diamond, pentagon, hexagon and graph axes). Drag one out on
  the page; hold Shift for squares, circles and 45° lines. Shapes are stored as ink, so they save
  to `.one` unchanged.
- **Simplified ribbon:** a thinner, single-row ribbon with related commands grouped into
  dropdowns and a More menu for whatever doesn't fit.
- **Ribbon display options:** Classic or Simplified layout, Full-screen mode, Show tabs only and
  Always show ribbon. Right-click the ribbon or use the arrow at its bottom-right; they're also in
  File › Options.

### Fixed
- **Ink to Shape** now works: it's a toggle, and while it's on each pen stroke that looks like a
  line, arrow, rectangle, circle, ellipse or polygon is redrawn cleanly in the same undo step.
  The recognizer handles tilted, overshooting and overtraced shapes, and leaves handwriting and
  highlighter strokes alone.
- The notebook pane no longer shrinks back after you drag it wider.
- Icons: new, cleaner icons throughout, drawn after DeckCraft's set. Icons in the File menu, the
  title bar and on disabled buttons no longer turn into solid blobs.

## [0.1.0] - 2026-11-07

First release: notebooks, section groups, sections, pages and subpages; note containers with rich
text, lists and To Do tags; pen, pencil, highlighter, eraser and lasso with pressure; pictures and
PDF printouts to write on; `.one`/`.onetoc2` as the native format with `.onepkg` import and export;
HTML, Markdown and text export; autosave and crash recovery; light and dark themes; desktop, web
and `notecraft-cli` builds.

[0.2.0]: https://github.com/Ideademic/NoteCraft/releases/tag/v0.2.0
[0.1.1]: https://github.com/Ideademic/NoteCraft/releases/tag/v0.1.1
[0.1.0]: https://github.com/Ideademic/NoteCraft/releases/tag/v0.1.0
