# Glossary

Words NoteCraft uses in its code, interface and docs. The names of OneNote features are used only
to describe what a feature does; the file-format terms come from Microsoft's Open Specifications
[MS-ONESTORE] and [MS-ONE].

| Term | Meaning |
|---|---|
| **Notebook** | The top-level container: a name, a colour and an ordered list of sections and section groups. On disk it is a folder holding `Open Notebook.onetoc2` and one `.one` file per section. Model: `notecraft_doc::Notebook`. |
| **Section** | A tab of pages inside a notebook, with its own colour. One `.one` file. Model: `Section`. |
| **Section group** | A named folder of sections (and further section groups) inside a notebook; a subfolder on disk. Nesting depth is bounded (`MAX_GROUP_DEPTH`). Model: `SectionGroup`. |
| **Page** | One free-form canvas in a section, with a title, a creation date and time, a page colour and optional rule lines or grid. Model: `Page`. |
| **Subpage** | A page indented under the page before it in the page list (levels 1 and 2), so related pages collapse and move together. It is still a page; only its level differs. |
| **Note container** (outline) | A box you click anywhere on a page to start typing: rich text, paragraph styles, bullets, numbering and note tags, which grows as you type and can be dragged. [MS-ONE] calls it an *outline*. Model: a `PageElement`. |
| **Paragraph** / **outline element** | One line of text in a note container, with an indent level, list formatting and tags. |
| **Note tag** | A marker on a paragraph: To Do (a checkbox), Important or Question. |
| **Ink** | Strokes drawn with the pen, pencil or highlighter: points with optional pressure, colour, width and a highlighter flag. Ink lives on the page itself (`Page::ink`), not inside a note container. Bounded by `MAX_POINTS` and `MAX_INK_PER_PAGE`. Model: `InkStroke`. |
| **Lasso** | The tool that selects ink strokes (and other page content) by drawing a loop around them. |
| **Eraser** | Removes whole ink strokes, or the part of a stroke it passes over. |
| **Picture** | An image placed on a page, at any position and size. |
| **File printout** | The pages of a PDF inserted as pictures, one below the other, so you can write and draw over them. Rendered with craft-pdf. |
| **Rule lines** | Page lines or a grid drawn behind the page content, as on lined or squared paper. Not part of the content. |
| **Autosave** | NoteCraft has no Save button: every change marks the section dirty and is written to its `.one` file shortly after, and on exit. |
| **`.one`** | A OneNote section file: a revision store ([MS-ONESTORE]) whose objects describe pages, outlines, ink and images ([MS-ONE]). NoteCraft's native format. Writing `.one` files that Microsoft OneNote opens is **experimental**. |
| **`.onetoc2`** | A notebook's table of contents (`Open Notebook.onetoc2`): the order and colours of its sections and section groups. Also a revision store. |
| **`.onepkg`** | A whole notebook packed into one file for sharing or backup: a Microsoft Cabinet (`.cab`, [MS-CAB]) archive of the `.onetoc2` and `.one` files. NoteCraft imports and exports it. |
| **Revision store** | The container format of `.one` and `.onetoc2` ([MS-ONESTORE]): a header, file node lists, object spaces, revisions, property sets, a transaction log and CRCs. Crate: `notecraft-onestore`. |
| **Half-inch** | The layout unit of [MS-ONE]: 36 points. Positions on a page are stored in half-inches. |
| **Command** | Every user-visible action, with an id (e.g. from `crates/engine/src/ids.rs`). The ribbon, shortcuts, the CLI and tests all dispatch commands through `Session::run`. |
| **Session** | The engine's state: the open notebooks, the current section and page, undo history and storage. |
| **Craft app** | One of the ArtCraft team's open-source Crafting Apps (WordCraft, DeckCraft, GridCraft…). NoteCraft is a *community* Craft app: made to fit in with them, not an official one. |
