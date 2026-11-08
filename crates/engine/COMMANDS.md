# NoteCraft commands

Everything the user can do is a command, run by id through `Session::run(id, &params)` (the
ribbon, shortcuts, `notecraft-cli run --cmd`, tests). This file lists every command; the same
parameter strings are in each `CommandSpec::params` and in `notecraft-cli commands --json`.
`crates/engine/src/tests.rs::every_id_is_registered_once` keeps the registry and `ids.rs` in step;
update this file when you add or change a command.

## Conventions

- **Params** are a JSON object. `?` marks optional keys. `guid` is a GUID string (`{…}` or bare),
  colours are `"#rrggbb"`. Unknown keys are ignored; a present but malformed value is an error
  (`CmdError::Params`). `page` (where listed) targets another page than the current one.
- **Output** is `CmdOutput { value, changed }`; `→ {…}` below is `value`. `changed: false` means
  nothing happened and no undo step was recorded.
- **Undo**: commands marked *yes* are atomic (an error restores the notebooks) and record one
  undo step. Passing the same `"mergeKey": string` on consecutive commands folds them into one
  step (typing in an outline: use the outline id; dragging ink: a drag id). Any command without
  that key, or `Session::break_undo_merge()`, ends the merge.
- **Positions** on a page are points from its top-left corner, y down.
- **Files** come as `{"bytesBase64": string, "name"?: string}` (web, scripts, drag and drop) or
  `{"path": string}` (desktop only). Base64 input is capped before decoding (512 MB).
- **Exports** return `{"fileName", "mime", "bytesBase64", "size"}` for the front end to save or
  download, or, with `"path"` (desktop), write the file there with a safe save and return
  `{"path", "size"}`.

## The text selection model (canvas)

A text position is `{"para": n, "char": n}` (or `[para, char]`): the paragraph's index in the
outline and a **character** index (Unicode scalar values, as egui's `CCursor` counts them) into
its text (all runs joined). Text and formatting commands take:

```json
{"page"?: guid, "outline": guid, "start": {"para": 0, "char": 3}, "end": {"para": 1, "char": 0}}
```

- `start`/`end` may come in either order and are clamped to the outline; `end` defaults to
  `start`. Also accepted: `"id"` for `"outline"`, `"at"` for a caret, `"para": n` (that whole
  paragraph), `"paras": [first, last]`, the older `{"para": n, "start": c0, "end": c1}`.
  No range at all means the whole outline.
- **Typing**: `text.insert` (`text`, optional `style` for the pending format), `text.delete`
  (Backspace/Delete/word), `text.splitParagraph` (Enter), `text.mergeParagraphs`. They return the
  new caret as `{"caret": {"para", "char"}}`. Pass `"mergeKey": <outline id>` while typing.
  `text.setParagraphs` (replace the whole outline) still works for bulk edits.
- **Character formatting** (`format.bold` … `format.font`, `format.clear`) covers `start..end`.
  A caret inside a word formats that word; a caret elsewhere changes nothing and returns
  `{"collapsed": true}`: keep the format as "pending" in the canvas and pass it as
  `text.insert`'s `style`.
- **Toggles** (bold, italic, underline, strike, highlight without a colour, bullets, numbering,
  To Do tag) work like Word: if everything selected already has it, it is removed, otherwise it
  is applied to all. `"value": true|false` sets instead of toggling.
- **Paragraph formatting** (style, bullets, numbering, indent, tags) covers every paragraph the
  range touches.
- `text.formatAt` gives the ribbon its toggle states for the selection.
- **Tables.** An outline holds blocks: paragraphs and tables. A table is one block (`para`
  indexes blocks); nothing is typed into a table's block, paragraphs don't merge with it, and a
  range that spans it deletes it. Its cells are flows of their own: add
  `"cell": {"block": b, "row": r, "col": c}` (or `[b, r, c]`) to any text or formatting command
  and `start`/`end`/`at` are in that cell's paragraphs. Formatting commands also take
  `"cells": {"block": b, "rows": [r0, r1], "cols": [c0, c1]}` (whole cells), and a range in the
  outline's own blocks (or no range) covers every table it spans. Commands that move the caret
  into another flow return `{"cell": cell | null, "caret": pos}`.

## Ink selection

`draw.lassoSelect` sets `Session::ink_selection` (stroke ids on the current page; cleared when
the page changes or on undo). `draw.moveSelection`, `draw.deleteSelection`, `draw.recolor` and
`draw.inkToShape` act on it (the last two also take `ids`). `draw.shape` selects the strokes it
inserts.

## Ink to Shape and shapes

- **Ink to Shape is a mode** (`Session::ink_to_shape()`, the ribbon's checked toggle).
  `draw.inkToShape {"value": bool}` sets it; `{}` toggles it, except that with `ids` or a lasso
  selection it converts those strokes instead. It never converts the whole page by itself.
- While the mode is on, `draw.stroke` redraws a stroke that looks like a line, arrow, rectangle
  (square), ellipse (circle), triangle or polygon of up to 8 corners as a clean polyline, in the
  same undo step, keeping its id, colour, width and tool (and its mean pressure). A line followed
  by an arrowhead at one of its ends (or the head first) becomes one arrow. Highlighter strokes and
  strokes smaller than 20 pt (handwriting) are never converted. The result says what the stroke
  became (`"shape"`).
- Converting `ids` or the selection reports `{converted, kept, shapes}`; when nothing is
  recognised it changes nothing and adds a notice to `Session::notices`.
- **Shapes are ink**: `draw.shape` adds one or more pen strokes (polylines; arrowheads and graph
  axes are separate strokes) in one undo step, so they save to `.one` / `.onepkg` like any ink.
  `x`, `y` is where the drag started and `w`, `h` how far it went (may be negative): lines and
  arrows run from start to end, other shapes fill the box. `constrain` (Shift) makes the box square
  or snaps a line to 45°. Sides are clamped to 5000 pt.

## Commands

### Notebooks

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `notebook.new` |  | yes | `{"name"?: string, "location"?: folder path (desktop; default: Session::default_location)} -> {"id", "section", "page"}` | New notebook with one section and an empty page, made current. Saved under `location`/`Session::default_location` (a free folder name), else wherever the storage puts scratch notebooks. |
| `notebook.open` | `Mod+O` | — | `{"path": string}` | Opens a notebook folder, `Open Notebook.onetoc2` (its folder), `.onepkg` or a single `.one` (desktop). Already open: goes to it (`alreadyOpen: true`). Adds to `Session::recent`. Clears undo. |
| `notebook.close` |  | — | `{"id"?: guid}` | Saves (when storage is set) and closes. Clears undo. |
| `notebook.rename` |  | yes | `{"id"?: guid, "name": string}` | Renames. |
| `notebook.color` |  | yes | `{"id"?: guid, "color": "#rrggbb"}` | Sets the notebook colour. |

### Sections

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `section.new` | `Mod+T` | yes | `{"name"?: string, "group"?: guid, "notebook"?: guid}` | New section (one empty page) at the end of the notebook or `group`; made current. → `{id, page}` |
| `section.rename` |  | yes | `{"id"?: guid, "name": string}` | Renames a section or a section group. A renamed section's file follows the new name on the next save. |
| `section.delete` |  | yes | `{"id"?: guid}` | Deletes a section or group into the notebook's recycle bin (a group's sections one by one, remembering where they were); the next section becomes current. On disk it moves to `OneNote_RecycleBin/` on save. |
| `section.color` |  | yes | `{"id"?: guid, "color": "#rrggbb"}` | Sets the section colour. |
| `section.move` |  | yes | `{"id"?: guid, "index": number, "group"?: guid \| null, "notebook"?: guid}` | Moves a section or group to `index` in `group` (null: top level) of `notebook`. |
| `sectionGroup.new` |  | yes | `{"name"?: string, "parent"?: guid}` | New section group. → `{id}` |

### Pages

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `page.new` | `Mod+N` | yes | `{"section"?: guid, "title"?: string, "blank"?: bool}` | New page after the current page's family; made current. A section with a default template (`section.defaultTemplate`) starts the page from it unless `blank` is true. → `{id}` |
| `page.newSubpage` | `Mod+Shift+Alt+N` | yes | `{"title"?: string}` | New subpage under the current page. → `{id}` |
| `page.delete` |  | yes | `{"id"?: guid}` | Deletes a page into the notebook's recycle bin (its subpages move up a level where needed). |
| `page.rename` |  | yes | `{"id"?: guid, "title": string}` | Sets the page title. |
| `page.move` |  | yes | `{"id"?: guid, "index": number, "section"?: guid}` | Moves a page and its subpages to `index` in `section`. |
| `page.indent` | `Mod+Alt+]` | yes | `{"id"?: guid}` | Makes the page (and its subpages) a subpage. |
| `page.outdent` | `Mod+Alt+[` | yes | `{"id"?: guid}` | Promotes a subpage. |

### Note containers

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `outline.new` |  | yes | `{"x": number, "y": number, "width"?: number, "text"?: string, "page"?: guid}` | New note container at (x, y). → `{id}` |
| `outline.move` |  | yes | `{"id": guid, "x": number, "y": number, "page"?: guid}` | Moves any element (outline, picture, printout, file) so its top-left is at (x, y). |
| `outline.resize` |  | yes | `{"id": guid, "width": number, "height"?: number, "page"?: guid}` | Sets an outline's width or a picture's size. |
| `outline.delete` |  | yes | `{"id": guid, "page"?: guid}` | Deletes any element. |

### Typing

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `text.setParagraphs` |  | yes | `{"id": guid, "paragraphs": [Paragraph], "mergeKey"?: string, "page"?: guid}` | Replaces all of an outline's paragraphs (whole-outline editing). Same `mergeKey` while typing = one undo step. |
| `text.insert` |  | yes | `{"outline": guid, "at"?: {"para","char"} (default: end), "start"?/"end"?: pos (typing over a selection), "text": string, "style"?: {"bold"?, "italic"?, "underline"?, "strike"?: bool, "highlight"?, "color"?: "#rrggbb"\|null, "font"?: string\|null, "size"?: number\|null, "link"?: string\|null}, "mergeKey"?: string, "page"?: guid}` | Types `text` at the caret (or over `start`..`end`). `\n` starts new paragraphs (they inherit list/indent/style; headings are followed by normal text; To Do starts unchecked). Formatting comes from the character before the caret, overridden by `style`. → `{caret}` |
| `text.delete` |  | yes | `{"outline": guid, "start": pos, "end": pos} \| {"outline": guid, "at": pos, "backward"?: bool (default true), "unit"?: "char"\|"word", "count"?: number}, "mergeKey"?: string, "page"?: guid` | Deletes `start`..`end`, or one step from `at`: Backspace (`backward`, default) / Delete, `count` characters or a `unit: "word"`. Backspace at a paragraph start removes its bullet/tag, then one indent level, then joins it to the previous paragraph. → `{caret}` |
| `text.splitParagraph` |  | yes | `{"outline": guid, "at"?: pos (default: end), "start"?/"end"?: pos (replaced first), "mergeKey"?: string, "page"?: guid}` | Enter: splits the paragraph at the caret (replacing a selection first). Enter on an empty bullet/To Do paragraph ends the list instead. → `{caret}` |
| `text.mergeParagraphs` |  | yes | `{"outline": guid, "para": number (joined with the next one), "mergeKey"?: string, "page"?: guid}` | Joins paragraph `para` with the next. → `{caret}` (unchanged when there is no next paragraph) |
| `text.formatAt` |  | — | `{"outline": guid, "start"?: pos, "end"?: pos, "page"?: guid}` | Query for the ribbon: formatting of the selection, or what a caret would type with. Each key is the common value, or `"mixed"`: `{bold, italic, underline, strike, highlight, color, font, size, link, style, list, indent, tag}`. |

### Formatting (Home)

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `format.bold` | `Mod+B` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggle (see conventions). |
| `format.italic` | `Mod+I` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggle. |
| `format.underline` | `Mod+U` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggle. |
| `format.strike` | `Mod+-` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggle. |
| `format.highlight` | `Mod+Alt+H` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "color"?: "#rrggbb" (default yellow; toggles off when all already have it) \| null (remove)}` | Highlights with `color` (default yellow). Without `color` it toggles: everything already highlighted that colour loses the highlight. `null` removes. |
| `format.color` |  | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "color": "#rrggbb" \| null (automatic)}` | Font colour; `null` = automatic. |
| `format.size` |  | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "size": number (1-400) \| null (style default)}` | Font size in points (clamped 1–400); `null` = the style's size. |
| `format.font` |  | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "font": string \| null (default font)}` | Font family; `null` = default font. |
| `format.clear` | `Mod+Shift+N` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid}` | Clear All Formatting: plain runs (links kept), paragraphs back to Normal (lists and tags kept). |
| `format.style` |  | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "style": "normal"\|"heading1".."heading6"\|"title"\|"quote"\|"code"}` | Paragraph style of every paragraph the range touches. |
| `format.bullets` | `Mod+.` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggles bullets on the paragraphs (all bulleted → off; else on, replacing numbering). |
| `format.numbering` | `Mod+/` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggles numbering likewise. |
| `format.indent` | `Alt+Shift+Right` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid}` | Indent +1 (max 8) for the paragraphs. |
| `format.outdent` | `Alt+Shift+Left` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid}` | Indent −1. |
| `format.tag.todo` | `Mod+1` | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "value"?: bool}` | Toggles a To Do tag: adds an unchecked one to untagged paragraphs (checked ones stay checked); all already To Do → removes. |
| `format.toggleTodo` |  | yes | `{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid, "checked"?: bool}` | Checks/unchecks the To Do boxes in the range (click on a box: pass `para`). Without `checked`: all become checked unless all already are. Paragraphs without a To Do are left alone. |

### Insert

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `insert.image` |  | yes | `{"bytesBase64": string, "name"?: string} \| {"path": string}, "x"?: number, "y"?: number, "page"?: guid, "alt"?: string -> {"id": guid}` | Inserts a picture (PNG/JPEG/GIF kept; BMP/WebP stored as PNG; ≤ 64 MB, ≤ 100 Mpx), sized at 96 dpi and fitted to 540 × 720 pt. → `{id}` |
| `insert.printout` |  | yes | `{"bytesBase64": string, "name"?: string} \| {"path": string}, "x"?: number, "y"?: number, "page"?: guid, "dpi"?: number (24-600, default 150) -> {"id": guid, "pages": number}` | File printout: a PDF's pages (≤ 200) rendered at `dpi` and stacked down the page at their real size (≤ 1440 pt wide); a picture becomes a one-page printout. The original file is kept. → `{id, pages}` |
| `insert.file` |  | yes | `{"bytesBase64": string, "name"?: string} \| {"path": string}, "x"?: number, "y"?: number, "page"?: guid` | Attaches a file (≤ 100 MB) as an icon with its name. → `{id}` |
| `insert.dateTime` | `Alt+Shift+F` | yes | `{"what"?: "date"\|"time"\|"dateTime" (default), "format"?: strftime string, "utcOffsetMinutes"?: number, "outline"?: guid, "at"?: {"para","char"} (insert at the caret), "x"?: number, "y"?: number (else a new note container), "page"?: guid} -> {"id"?: guid, "caret"?: pos, "text": string}` | The date and/or time (UTC shifted by `utcOffsetMinutes`; `format` is a checked chrono strftime string): typed at the caret when `outline` is given (→ `{caret, text}`), else in a new note container (→ `{id, text}`). |

### Draw

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `draw.stroke` |  | yes | `{"points": [[x, y, pressure?]] \| [{"x","y","pressure"?}], "tool"?: "pen"\|"pencil"\|"highlighter", "color"?: "#rrggbb", "width"?: number, "page"?: guid} -> {"id": guid, "shape": string\|null}` | Adds an ink stroke (page points; pressure 0–1). With Ink to Shape on, a stroke that looks like a shape is redrawn as it (`shape` says which). → `{id, shape}` |
| `draw.eraseStroke` |  | yes | `{"id"?: guid, "ids"?: [guid], "page"?: guid}` | Removes strokes by id. |
| `draw.eraseAt` |  | yes | `{"x": number, "y": number} \| {"points": [[x, y]]} (an eraser drag), "radius"?: number (default 6), "mergeKey"?: string, "page"?: guid} -> {"erased": [guid]}` | Stroke eraser: removes every stroke within `radius` of the point or of the drag path (sampled every radius/2). Pass the same `mergeKey` during one drag for one undo step. → `{erased}` |
| `draw.lassoSelect` |  | — | `{"points": [[x, y]] (the lasso polygon; fewer than 3 points clears) \| "ids": [guid] \| "all": true, "add"?: bool} -> {"ids": [guid]}` | Sets `Session::ink_selection` (current page): strokes with at least half their points inside the polygon, or `ids`, or `all`; `add` extends it. Not undoable. → `{ids}` |
| `draw.moveSelection` |  | yes | `{"dx": number, "dy": number, "mergeKey"?: string}` | Moves the selected strokes by (dx, dy). Same `mergeKey` during a drag = one undo step. |
| `draw.deleteSelection` |  | yes | `{}` | Deletes the selected strokes. |
| `draw.recolor` |  | yes | `{"color"?: "#rrggbb", "width"?: number, "ids"?: [guid] (default: the selection)}` | Sets colour and/or width (clamped 0.1–100) of the selected strokes (or `ids`). |
| `draw.inkToShape` |  | yes (converting) | `{"value"?: bool, "ids"?: [guid], "page"?: guid} -> {"value": bool} \| {"converted": number, "kept": number, "shapes": [string]}` | `value` sets the Ink to Shape mode; with `ids` or a lasso selection converts those strokes (a notice when none is recognised); else toggles the mode. Mode changes are not undo steps. |
| `draw.shape` |  | yes | `{"kind": "line"\|"arrow"\|"doubleArrow"\|"rectangle"\|"roundedRectangle"\|"ellipse"\|"triangle"\|"rightTriangle"\|"parallelogram"\|"trapezoid"\|"diamond"\|"pentagon"\|"hexagon"\|"axes2"\|"axes2Arrows"\|"axes3", "x": number, "y": number, "w": number, "h": number, "constrain"?: bool, "color"?: "#rrggbb", "width"?: number, "page"?: guid} -> {"ids": [guid], "kind": string}` | Inserts a shape as pen strokes (one undo step) and selects them. → `{ids, kind}` |

### View

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `view.ruleLines` |  | yes | `{"rule": "none"\|"narrow"\|"college"\|"standard"\|"wide"\|"gridSmall"\|"gridMedium"\|"gridLarge", "page"?: guid}` | Rule lines / grid of the page. |
| `view.pageColor` |  | yes | `{"color": "#rrggbb" \| null, "page"?: guid}` | Page colour; `null` = none. |
| `view.hidePageTitle` |  | yes | `{"value"?: bool (default: toggle), "page"?: guid} -> {"value": bool}` | Hide Page Title, kept with the page (`Page::hide_title`; a private property in `.one`). |

### Undo

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `edit.undo` | `Mod+Z` | — | `{}` | Undo. |
| `edit.redo` | `Mod+Y / Mod+Shift+Z` | — | `{}` | Redo. |

### File (export / import)

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `file.exportOnepkg` |  | — | `{"id"?: notebook guid (default: the current one), "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} \| {"path", "size"}` | The notebook as a `.onepkg` (see conventions: bytes or `path`). |
| `file.exportHtml` |  | — | `{"scope"?: "page" (default)\|"section", "id"?: page or section guid (default: the current one), "path"?: string, "positioned"?: bool (default true: keep the page layout)} -> {"fileName", "mime", "bytesBase64", "size"} \| {"path", "size"}` | The page, or the section with a table of contents, as one HTML file (pictures inline, ink as SVG). |
| `file.exportMarkdown` |  | — | `{"scope"?: "page" (default)\|"section", "id"?: page or section guid (default: the current one), "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} \| {"path", "size"}` | The page (or the section's pages separated by `---`) as Markdown. |
| `file.exportText` |  | — | `{"scope"?: "page" (default)\|"section", "id"?: page or section guid (default: the current one), "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} \| {"path", "size"}` | The page (or section) as plain text. |
| `file.exportPdf` |  | — | `{"scope"?: "page" (default)\|"section"\|"notebook", "id"?: page, section or notebook guid (default: the current one), "paper"?: "fit" (default)\|"letter"\|"a4"\|"legal"\|"a5", "landscape"?: bool, "fitWidth"?: bool (default true), "header"?: bool (default true), "pageNumbers"?: bool (default true on paper), "pages"?: "1-3, 5", "utcOffset"?: minutes east of UTC, "fonts"?: "system" (default)\|"bundled", "path"?: string} -> {"fileName", "mime", "bytesBase64", "size", "sheets"} \| {"path", "size", "sheets"}` | The page, section or notebook as a PDF: vector ink, pictures, the text in embedded fonts laid out as on the canvas, links, and a bookmark per page. `fit` makes each page one sheet its content's size; a paper size shrinks wide pages to its width and breaks long ones between lines (`pages` keeps only those sheets, a print range). Also the file the Print page prints. |
| `file.importOnepkg` |  | — | `{"bytesBase64": string, "name"?: string} \| {"path": string} -> {"id": notebook guid}` | Opens a `.onepkg` as a new notebook (fresh ids if it is already open); everything is saved on the next autosave. Clears undo. → `{id}` |
| `file.importOne` |  | yes | `{"bytesBase64": string, "name"?: string} \| {"path": string}, "notebook"?: guid, "group"?: guid -> {"id": section guid}` | Adds a `.one` section to the current notebook (or `notebook`/`group`) and goes to it. Undoable. → `{id}` |

### Navigation and search

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `nav.notebook` |  | — | `{"id": guid}` | Goes to a notebook. |
| `nav.section` |  | — | `{"id": guid}` | Goes to a section. |
| `nav.page` |  | — | `{"id": guid}` | Goes to a page. |
| `search` | `Mod+E` | — | `{"query": string, "scope"?: "all" (default)\|"notebook"\|"sectionGroup"\|"section"\|"page", "limit"?: number} -> {"hits": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "snippet"}], "truncated": bool}` | Searches page titles and text in the open notebooks (case-insensitive; one hit per page). `sectionGroup` is the innermost group holding the current section (the notebook when it is in none); `page` the current page. |

### History

The notebook recycle bin, page versions, unread pages and recent edits. Deleted pages and sections stay in the recycle bin for 60 days (expired when the notebook is opened). Page versions are kept automatically: the first time a page changes in a session (the page as it was), then at most one every 5 minutes while it is edited; at most 20 per page. Read marks are the user's, not the notebook's: they don't touch undo.

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `history.recycleBin` |  | — | `{"notebook"?: guid} -> {"notebook", "sections": [{"id", "name", "color", "deleted", "pages": [{"id", "title", "level"}]}], "pages": [{"id", "title", "deleted", "section", "sectionName"}]}` | Lists the recycle bin, newest deletion first (`deleted`: RFC 3339). |
| `history.restore` |  | yes | `{"id": guid, "to"?: guid} -> {"notebook", "section", "page"}` | Restores a deleted page (into `to`, else the section it came from, else the first section) or section (into `to`: a section group or the notebook id, else the group it was in), or one page of a deleted section; goes to it. |
| `history.deleteForever` |  | yes | `{"id": guid}` | Removes a page or section from the recycle bin for good (undo still brings it back this session). |
| `history.emptyRecycleBin` |  | yes | `{"notebook"?: guid}` | Empties the notebook's recycle bin (the UI asks first). |
| `history.versions` |  | — | `{"page"?: guid} -> [{"id", "saved", "title"}]` | The page's versions, newest first. |
| `history.viewVersion` |  | — | `{"page"?: guid, "version": guid \| null}` | Opens a version read-only (`Session::viewing_version`), going to its page; `null` closes it. Going to another page closes it too. |
| `history.restoreVersion` |  | yes | `{"page"?: guid, "version"?: guid (default: the one open)}` | Makes the page what the version was; the page as it was becomes a version. |
| `history.deleteVersion` |  | yes | `{"page"?: guid, "version"?: guid (default: the one open)}` | Deletes one version. |
| `history.deleteAllVersions` |  | yes | `{"scope"?: "page"\|"section" (default)\|"notebook", "id"?: guid} -> {"deleted": number}` | Deletes every version in the page, section or notebook. |
| `history.showVersions` |  | — | `{"value"?: bool (default: toggle)} -> {"value"}` | Page Versions / Hide Page Versions: whether the page list shows versions under their pages. |
| `history.disableHistory` |  | yes | `{"notebook"?: guid, "value"?: bool (default: toggle)}` | Disable History for this Notebook: no versions are kept (stored as MS-ONE EnableHistory in the table of contents). Existing versions stay. |
| `history.markRead` |  | — | `{"scope"?: "page" (default)\|"section"\|"notebook", "id"?: guid}` | Marks pages read. |
| `history.markUnread` |  | — | `{"scope"?: "page" (default)\|"section"\|"notebook", "id"?: guid}` | Marks pages unread. |
| `history.toggleRead` |  | — | `{"page"?: guid} -> {"unread": bool}` | Mark as Read / Unread for one page. |
| `history.nextUnread` |  | — | `{} -> {"page"} \| null` | Goes to the next unread page after the current one (all open notebooks, wrapping round). |
| `history.recentEdits` |  | — | `{"since"?: "today"\|"yesterday"\|"7d" (default)\|"14d"\|"30d"\|"6m"\|"all", "utcOffsetMinutes"?: number} -> [{"page", "title", "section", "sectionName", "notebook", "notebookName", "modified", "unread"}]` | Pages changed in the period, newest first (at most 500). "today" and "yesterday" follow the local day. |

### Tables

Insert › Table and the contextual Table tab. Tables hold rows of cells; a cell holds paragraphs (no nested tables). Caps: 2,000 rows, 64 columns, 10,000 cells; columns 18–2,000 pt wide. Commands name the table by `cell` (the caret's), `cells` (a selection) or `block` (the whole table); with `cells` the row and column commands act on every selected row or column.

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `insert.table` |  | yes | `{"rows": number, "cols": number, "colWidth"?: points, "outline"?: guid, "cell"?: cell, "at"?/"start"?: pos, "x"?: number, "y"?: number, "page"?: guid}` | A `rows` × `cols` table (columns 72 pt): at the caret in `outline` (replacing an empty paragraph, or splitting the caret's paragraph; from a cell, after its table; an empty paragraph follows a table that ends the outline), else in a new note container at `x`/`y` or below the content. → `{id, cell, caret}` |
| `table.fromText` |  | yes | `{"outline": guid, "start"?/"end"?/"para"?/"paras"?, "addColumn"?: bool, "page"?: guid}` | The range's paragraphs become rows, split into cells at tabs (formatting kept). `addColumn` adds an empty column (typing Tab after text). → `{cell, caret}` |
| `table.insertRowAbove` |  | yes | table target | A row above the first selected row (shading copied). → `{cell, caret}` |
| `table.insertRowBelow` | `Mod+Enter` | yes | table target | A row below the last selected row. → `{cell, caret}` |
| `table.insertColumnLeft` | `Mod+Alt+E` | yes | table target | A column left of the first selected column, as wide as it. → `{cell, caret}` |
| `table.insertColumnRight` | `Mod+Alt+R` | yes | table target | A column right of the last selected column. → `{cell, caret}` |
| `table.deleteRow` |  | yes | table target | Deletes the selected rows (all of them: the table). → `{cell, caret}` |
| `table.deleteColumn` |  | yes | table target | Deletes the selected columns (all of them: the table). → `{cell, caret}` |
| `table.deleteTable` |  | yes | table target | Deletes the table; an empty paragraph is left if nothing else is. → `{cell: null, caret}` |
| `table.selectCell` / `selectRow` / `selectColumn` / `selectTable` |  | — | table target | The cells to select. → `{cells}` |
| `table.setColumnWidth` |  | yes | table target + `"col"?: number (default: the cell's), "width": points` | Sets (and locks) a column's width. Pass a `mergeKey` while dragging a border. |
| `table.shading` |  | yes | table target + `"color": "#rrggbb" \| null` | Shades the cell, the selected cells, or (`block` alone) every cell. |
| `table.borders` |  | yes | table target + `"value"?: bool (default: toggle)` | Shows or hides the borders (Hide Borders). → `{borders}` |
| `table.sort` |  | yes | table target + `"col"?: number, "descending"?: bool, "header"?: bool` | Sorts the rows by a column: numbers by value, then text without case, empty cells last; `header` keeps the first row on top. |
| `table.convertToText` |  | yes | table target | A paragraph per row, cells separated by tabs (formatting kept). → `{cell: null, caret}` |

A *table target* is `{"outline": guid, "cell"?: cell, "cells"?: cell range, "block"?: number, "page"?: guid}`.

### Editing: Format Painter, clipboard, find, pictures, arrange, space, tags, templates

**Format Painter.** `format.painterPick` reads the formatting at the start of a range (or what a
caret would type with); the canvas keeps it while the brush is on and hands it to
`format.painterApply`. Text formatting (font, size, colour, highlight, bold, italic, underline,
strike) replaces the painted characters' (links stay); paragraph formatting (style, alignment,
list) goes onto the paragraphs the range covers whole. A caret paints the word it is in. Tags and
links are never painted.

**Clipboard.** `edit.copy` returns a `Clip` (NoteCraft JSON: `{"paragraphs": [Paragraph],
"elements": [PageElement], "ink": [InkStroke]}`, the front end's internal clipboard) with its
plain `text` and an `html` fragment for other apps. `edit.paste` takes a clip, `html` (basic
formatting: b/i/u/s, headings, lists, links, line breaks, colours, fonts and sizes) or `text`;
`plain` keeps the text only, in the formatting at the caret. Each paste is one undo step.

**Pictures** are never re-encoded: rotation, flips and crop are `ImageElement::transform`
(`PictureTransform`: quarter turns clockwise, flips in the source image before the turn, crop in
fractions of the source), drawn by the canvas, baked into PDF and HTML exports, and kept in
`.one` as a private property. `pos`/`size` are the visible (cropped, turned) picture.

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `format.painterPick` |  | — | `{"outline": guid, "start"?/"end"?/"at"?: pos, "page"?: guid} -> {"run": {"bold", "italic", "underline", "strike": bool, "highlight", "color": "#rrggbb"\|null, "font": string\|null, "size": number\|null}, "para": {"style", "align", "list": "bullet"\|"numbered"\|null, "listStyle": string\|null}}` | The formatting to paint. |
| `format.painterApply` |  | yes | `{"outline": guid, "start": pos, "end": pos, "format": painterPick's output, "page"?: guid} -> {"caret": pos} \| {"collapsed": true}` | Paints the format (see above); a caret outside a word changes nothing (`collapsed`). |
| `edit.copy` |  | — | `{"outline": guid, "start": pos, "end": pos} \| {"elements": [guid], "strokes"?: [guid], "ink"?: true (the lasso selection)}, "page"?: guid -> {"clip": Clip, "text": string, "html": string}` | Copies a text range (partial runs and paragraph formatting kept) or whole items and ink. An empty selection is an error. |
| `edit.cut` |  | yes | as `edit.copy` `-> {"clip", "text", "html", "caret"?}` | Copies, then deletes what was copied in the same step. |
| `edit.paste` |  | yes | `{"clip"?: Clip, "html"?: string, "text"?: string, "plain"?: bool, "outline"?: guid, "start"?/"end"?/"at"?: pos, "x"?: number, "y"?: number, "page"?: guid} -> {"caret"?: pos, "outline"?: guid, "ids"?: [guid]}` | Precedence clip > html > text. With `outline`, the paragraphs go in at the range: the first joins the caret's paragraph (an empty line takes its formatting), the last takes the text after the caret. Without it, a new note container at `x`,`y` (default: below the content). A clip's items and ink get new ids and go with their top-left corner at `x`,`y`, else 18 pt from the originals; pasted ink becomes the lasso selection. Caps: 64 MB of clip, the page's item, ink and paragraph limits. |
| `edit.pastePicture` |  | yes | `{"bytesBase64": string, "name"?: string, "x"?: number, "y"?: number, "page"?: guid} -> {"id": guid}` | A picture from the clipboard, as `insert.image`. |
| `text.find` | `Mod+F` | — | `{"query": string, "matchCase"?: bool, "wholeWord"?: bool, "page"?: guid} -> {"matches": [{"outline": guid\|null (null: the title), "para", "start", "end"}]}` | Matches on the page (title first, then the note containers in page order; character offsets; at most 10 000). |
| `text.replace` |  | yes | `{"query": string, "with": string, "matchCase"?: bool, "wholeWord"?: bool, "at"?: a text.find match, "page"?: guid} -> {"replaced": number}` | Replaces every match on the page in one undo step (or only `at`); the new text takes the replaced text's formatting. No match: `changed: false`. |
| `picture.rotate` |  | yes | `{"element": guid (or "id"), "degrees": 90\|-90\|180\|270, "page"?: guid} -> {"rotation": 0..3}` | Turns a picture clockwise by quarter turns about its centre (width and height swap). |
| `picture.flip` |  | yes | `{"element": guid, "axis": "horizontal"\|"vertical", "page"?: guid}` | Flips the picture as it looks on the page. |
| `picture.crop` |  | yes | `{"element": guid, "rect": {"x", "y", "w", "h"}} \| {"element", "crop": [left, top, right, bottom]} \| {"element", "reset": true}, "page"?: guid` | Keeps the part of the picture in `rect` (page points, clamped to the whole picture's rect) or the `crop` fractions of the original; `reset` shows the whole picture again at the same scale. |
| `picture.altText` |  | yes | `{"element": guid, "text": string, "page"?: guid}` | Alt text (≤ 4096 characters; kept in `.one` as MS-ONE ImageAltText). |
| `picture.reset` |  | yes | `{"element": guid, "page"?: guid}` | No rotation, flip or crop; the whole picture at its current scale. |
| `arrange.bringToFront` |  | yes | `{"element"?: guid, "elements"?: [guid], "strokes"?: [guid], "ink"?: true, "page"?: guid}` | Moves items (or ink strokes) in front of all others. Ink is always drawn over the items, so the two are ordered separately. |
| `arrange.bringForward` |  | yes | as above | In front of the next item above that they overlap (one step when none does). |
| `arrange.sendBackward` |  | yes | as above | Behind the next item below that they overlap (one step when none does). |
| `arrange.sendToBack` |  | yes | as above | Behind all others. |
| `ink.rotate` |  | yes | `{"degrees": 90\|-90\|180\|270, "ids"?: [guid] (default: the lasso selection), "page"?: guid} -> {"ids"}` | Turns the strokes clockwise about the centre of their bounding box; they stay selected. |
| `ink.flip` |  | yes | `{"axis": "horizontal"\|"vertical", "ids"?: [guid], "page"?: guid} -> {"ids"}` | Mirrors the strokes about their centre. |
| `page.insertSpace` |  | yes | `{"y": number, "dy": number, "page"?: guid} -> {"moved": number}` | Insert Space: moves every note container, picture, printout, file and stroke whose top is at or below `y` down by `dy` (up when negative, never above the top of the page; ±100 000 pt). |
| `tags.summary` |  | — | `{"scope"?: "page"\|"section"\|"sectionGroup"\|"notebook" (default)\|"all", "uncheckedOnly"?: bool, "limit"?: number (default 2000, max 10 000)} -> {"items": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "outline", "para", "tag", "label", "checked": bool\|null, "text", "modified"}], "truncated": bool}` | Find Tags: every tagged paragraph in the scope, in notebook order (`checked` for To Do only; `uncheckedOnly` keeps unticked To Dos). |
| `template.list` |  | — | `{} -> {"templates": [{"id", "name", "category", "builtin": bool}]}` | NoteCraft's own templates (ids `nc:…`: blank pages in several colours and rule lines, meeting notes, to-do list, lecture notes, project overview, weekly planner, journal), then the user's (`Session::templates`, category "My Templates"). |
| `template.saveCurrent` |  | — | `{"name": string, "page"?: guid} -> {"template": {"id", "name", "category", "page"}}` | Saves the page as a user template (`user:…`; the same name replaces one; at most 64, 16 MB each). Changes `Session::templates`, not the notebooks: the app stores them. |
| `template.delete` |  | — | `{"id": string}` | Deletes a user template. |
| `page.newFromTemplate` |  | yes | `{"template": string, "title"?: string, "section"?: guid} -> {"id": guid}` | A new page after the current page's family, copied from the template (new ids); made current. |
| `section.defaultTemplate` |  | yes | `{"template": string \| null, "section"?: guid}` | The template new pages in the section start from (`page.new`); `null` for none. Kept in `.one` as a private property. |
### Review (spelling, language, word count)

Spelling comes from craft-proof: English always (SCOWL when the build has craft-i18n's language data, else the public-domain Moby list), other languages when a Hunspell dictionary is installed. Each paragraph is checked in its own language (`Paragraph.lang`, kept in `.one` as RichEditTextLangID / LanguageID) or the default (`proof.setDefaultLanguage`; the app sets the system's). Misspellings are `{"outline", "para", "start", "end", "word", "lang"}` with character offsets in the paragraph. Ignore, Ignore All and the user dictionary are the user's, not the notebook's: they don't touch undo.

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `proof.check` | F7 | — | `{"page"?: guid, "outline"?: guid, "limit"?: number} -> {"issues": [issue], "more": bool}` | The page's misspellings in reading order (at most 500, or `limit` up to 5000). F7 opens the Spelling pane, which uses this. |
| `proof.suggestions` |  | — | `{"outline": guid, "at": pos, "page"?: guid, "max"?: number} -> null \| issue + {"suggestions": [string]}` | The misspelling at a caret, with suggestions (the right-click menu). |
| `proof.apply` |  | yes | `{"outline": guid, "para", "start", "end": number, "text": string, "word"?: string, "page"?: guid} -> {"caret"}` | Change: replaces the word, keeping its formatting. With `word`, fails when the text there changed. |
| `proof.changeAll` |  | yes | `{"word": string, "text": string, "page"?: guid} -> {"changed": number}` | Change All on the page. |
| `proof.ignore` |  | — | issue | Ignore (once): that word there, while its paragraph's text is the same. |
| `proof.ignoreAll` |  | — | `{"word": string}` | Ignore All, until NoteCraft closes. |
| `proof.addToDictionary` |  | — | `{"word": string} -> {"word", "words": [string]}` | Adds to the user dictionary (the app keeps it in its settings). |
| `proof.removeFromDictionary` |  | — | `{"word": string} -> {"removed": bool, "words"}` | Takes a word out again (File › Options). |
| `proof.dictionary` |  | — | `{} -> {"words": [string]}` | The user dictionary. |
| `proof.setLanguage` |  | yes | `{"lang": string\|null, "outline"?: guid, "start"?/"end"?: pos, "page"?: guid} -> {"paragraphs": number}` | Sets the proofing language of the selected paragraphs (the container, or the whole page without `outline`). `"zxx"`: don't check; `null`: the default. |
| `proof.setDefaultLanguage` |  | — | `{"lang": string} -> {"lang"}` | The language of paragraphs without one. |
| `proof.languages` |  | — | `{} -> {"default", "languages": [{"tag", "name", "dictionary": bool}]}` | The languages offered. |
| `review.wordCount` |  | — | `{"page"?: guid, "outline"?: guid, "start"?/"end"?: pos} -> {"scope", "words", "characters", "charactersNoSpaces", "paragraphs"}` | Counts the selection, else the page (title included). |

### Links and attachments

Link addresses are checked before they are kept: web and mail links as the PDF export allows them, `notecraft:` links when they parse, `file:` and `onenote:` links as they are; `www.example.com` becomes `https://…`, `a@b.org` a `mailto:` link, a path a `file:` link. NoteCraft's own links are `notecraft:page?id=<page>&section=<section>&notebook=<notebook>&title=<title>[&outline=<outline>&para=<n>]` (see `notecraft_doc::link`); they resolve by page id, else by title. Links are kept in `.one` files (HYPERLINK field runs), HTML exports (a section export points them at its own pages) and PDF exports (links to pages of the same PDF become page destinations).

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `link.set` | Mod+K | yes | `{"outline": guid, "start"?/"end"?/"at"?: pos, "url": string, "text"?: string, "page"?: guid} -> {"caret", "start", "end", "url"}` | Links the selection (with `text`, replaced by it); at a caret, inserts the text (default: the page title or the address) as a link. |
| `link.remove` |  | yes | `{"outline": guid, "start"?/"end"?/"at"?: pos, "page"?: guid}` | Removes the links in the selection, or the link around a caret. |
| `link.get` |  | — | `{"outline": guid, "at": pos, "page"?: guid} -> null \| {"url", "text", "start", "end"}` | The link at a caret. |
| `link.follow` |  | — | `{"url": string} -> {"internal": true, "page", "outline"?, "para"?} \| {"external": string}` | Goes to a `notecraft:` link's page; other links come back checked for the front end to open. |
| `link.toPage` |  | — | `{"id"?: guid} -> {"url"}` | Copy Link to Page. |
| `link.toParagraph` |  | — | `{"outline": guid, "para"?: number \| "at"?: pos, "page"?: guid} -> {"url"}` | Copy Link to Paragraph. |
| `link.wiki` |  | yes | `{"outline": guid, "at": pos, "page"?: guid} -> null \| {"caret", "page", "created": bool}` | `[[Page title]]` just before `at` becomes a link to that page of the notebook (any case), made in the section when there is none. The canvas runs it after `]` is typed. |
| `file.exportAttachment` |  | — | `{"element": guid, "page"?: guid, "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} \| {"path", "size"}` | The file of an attached file, a picture or a printout (its original PDF), for Open and Save As. |

## Session lifecycle (Rust API, not commands)

For the apps; see the rustdoc of `notecraft_engine::io` for details.

| What | API |
|---|---|
| Open by path / bytes | `io::open_path(path)` (folder, `.onetoc2` → its folder, `.onepkg`, `.one`), `io::origin_of(path)`, `io::open_bytes(name, bytes)` (`.one`, `.onepkg`, NoteCraft `.json`); then `Session::add_notebook(nb, origin)`, or the `notebook.open` command (also updates `recent`). |
| Desktop saving | `session.storage = Some(Box::new(io::FolderStorage::new(root)))`, `root` = `io::default_notebook_root(documents)`; also set `session.default_location = Some(root)`. Each changed section is a safe save (temp file + rename); a file NoteCraft didn't write is copied to `<file>.bak` the first time; a section with content NoteCraft can't edit adds one notice to `session.notices`; deleted sections go to `OneNote_RecycleBin/`. `.onepkg` and single `.one` origins save back in place. |
| Web saving | `io::MemoryStorage { format: MemoryFormat::Onepkg \| Json, on_write, .. }`; or keep the whole session in browser storage with `Session::to_blob()` / `Session::from_blob(bytes)` (JSON; unsaved state kept). |
| Autosave | call `session.tick(now_secs)` every frame or timer (monotonic seconds): saves 2 s after the last change, at most 30 s after the first unsaved one; failures go to `notices` and retry after another delay. `session.save_dirty()` saves now (quit). |
| Crash recovery (desktop) | at start: `session.enable_recovery(dir)`, then `session.recovery_candidates()` → `[RecoveryInfo {path, savedAt, notebooks}]`; offer `session.restore_recovery(&path)` (reopens them, unsaved changes marked) or `Session::discard_recovery(&path)`. `tick` keeps a snapshot of unsaved notebooks (worker thread, at most every 5 s) and removes it once saved. On a clean exit: `save_dirty()` then `end_recovery()`. |
| Unread pages | `session.read_state` (`unread::ReadState`): load with `ReadState::from_json`, store `to_json()` in the app's settings. A notebook opened for the first time counts as read; `session.is_unread(page)` afterwards says whether a page changed since it was last seen. |
| Recent notebooks | `session.recent` (`RecentNotebooks`): load with `RecentNotebooks::from_json`, store `to_json()` in the app's settings; updated by `notebook.open` and by the first save of a new notebook. |
| Notices | `session.notices`: messages for the user, oldest first; the UI shows and drains them. |
