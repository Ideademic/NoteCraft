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
| `section.delete` |  | yes | `{"id"?: guid}` | Deletes a section or group; the next section becomes current. Its file goes to `OneNote_RecycleBin/` on save. |
| `section.color` |  | yes | `{"id"?: guid, "color": "#rrggbb"}` | Sets the section colour. |
| `section.move` |  | yes | `{"id"?: guid, "index": number, "group"?: guid \| null, "notebook"?: guid}` | Moves a section or group to `index` in `group` (null: top level) of `notebook`. |
| `sectionGroup.new` |  | yes | `{"name"?: string, "parent"?: guid}` | New section group. → `{id}` |

### Pages

| id | shortcut | undo | params | effect / output |
|---|---|---|---|---|
| `page.new` | `Mod+N` | yes | `{"section"?: guid, "title"?: string}` | New page after the current page's family; made current. → `{id}` |
| `page.newSubpage` | `Mod+Shift+Alt+N` | yes | `{"title"?: string}` | New subpage under the current page. → `{id}` |
| `page.delete` |  | yes | `{"id"?: guid}` | Deletes a page (its subpages move up a level where needed). |
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
| `search` | `Mod+E` | — | `{"query": string, "scope"?: "all" (default)\|"notebook"\|"section", "limit"?: number} -> {"hits": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "snippet"}], "truncated": bool}` | Searches page titles and text in the open notebooks (case-insensitive; one hit per page). |

## Session lifecycle (Rust API, not commands)

For the apps; see the rustdoc of `notecraft_engine::io` for details.

| What | API |
|---|---|
| Open by path / bytes | `io::open_path(path)` (folder, `.onetoc2` → its folder, `.onepkg`, `.one`), `io::origin_of(path)`, `io::open_bytes(name, bytes)` (`.one`, `.onepkg`, NoteCraft `.json`); then `Session::add_notebook(nb, origin)`, or the `notebook.open` command (also updates `recent`). |
| Desktop saving | `session.storage = Some(Box::new(io::FolderStorage::new(root)))`, `root` = `io::default_notebook_root(documents)`; also set `session.default_location = Some(root)`. Each changed section is a safe save (temp file + rename); a file NoteCraft didn't write is copied to `<file>.bak` the first time; a section with content NoteCraft can't edit adds one notice to `session.notices`; deleted sections go to `OneNote_RecycleBin/`. `.onepkg` and single `.one` origins save back in place. |
| Web saving | `io::MemoryStorage { format: MemoryFormat::Onepkg \| Json, on_write, .. }`; or keep the whole session in browser storage with `Session::to_blob()` / `Session::from_blob(bytes)` (JSON; unsaved state kept). |
| Autosave | call `session.tick(now_secs)` every frame or timer (monotonic seconds): saves 2 s after the last change, at most 30 s after the first unsaved one; failures go to `notices` and retry after another delay. `session.save_dirty()` saves now (quit). |
| Crash recovery (desktop) | at start: `session.enable_recovery(dir)`, then `session.recovery_candidates()` → `[RecoveryInfo {path, savedAt, notebooks}]`; offer `session.restore_recovery(&path)` (reopens them, unsaved changes marked) or `Session::discard_recovery(&path)`. `tick` keeps a snapshot of unsaved notebooks (worker thread, at most every 5 s) and removes it once saved. On a clean exit: `save_dirty()` then `end_recovery()`. |
| Recent notebooks | `session.recent` (`RecentNotebooks`): load with `RecentNotebooks::from_json`, store `to_json()` in the app's settings; updated by `notebook.open` and by the first save of a new notebook. |
| Notices | `session.notices`: messages for the user, oldest first; the UI shows and drains them. |
