# notecraft-one: `.one`, `.onetoc2` and notebook folders

The MS-ONE mapping between OneNote's revision store (`notecraft-onestore`) and NoteCraft's model
(`notecraft-doc`), plus `.onetoc2` tables of contents and notebook folders. `.onepkg` packages
(cabinets of the same files) live in `notecraft-onepkg`.

Clean room: written only from Microsoft's public Open Specifications ([MS-ONESTORE], [MS-ONE],
[MS-OSHARED] for MsoCrc32, the Ink Serialized Format spec for ink integers) and black-box
inspection of sample files (Apache Tika's Apache-2.0 test documents, fetched by
`scripts/fetch-corpus.sh` into the gitignored `corpus/`). No other implementation's source was
read.

## Status: reading is solid, writing is experimental

NoteCraft's own files round-trip exactly (tests). **Whether OneNote accepts the files NoteCraft
writes has not been verified**: the project has no Windows/OneNote to test with. The writer
follows the specs' MUSTs (see below), but OneNote may expect things the specs don't say.

### Read (OneNote 2010–2016/2019 desktop files)

- Sections: name (display name, else the file name), colour, id (`guidFile`), page series and
  page order, subpage levels.
- Pages: title, created / modified times, id, level.
- Outlines: position (offsets in half-inches from the page margin origin), width, outline
  element nesting → paragraph indent (outline groups included).
- Rich text: UTF-16 or extended-ASCII text, text runs (bold, italic, underline, strike,
  highlight, font colour, font, size, hyperlinks from OneNote's hidden `HYPERLINK` field runs),
  paragraph styles (`p`, `h1`–`h6`, `PageTitle`, `blockquote`/`cite`, `code`).
- Bullets and numbering (NumberListFormat with or without U+FFFD).
- Note tags: To Do (every checkable shape, checked from ActionItemStatus), Important (star
  shapes), Question (question shapes); task tags read as To Do.
- Pictures (page-level and inside outlines, which become page-level pictures), with size and
  alt text; embedded files.
- Tables: flattened to one read-only paragraph per row (cells separated by tabs); the section is
  flagged `unsupported_content`.
- Ink (see below).
- Anything else on a page is kept as `ElementKind::Opaque` (its objects captured and re-emitted
  on save) and flags `unsupported_content`.
- `.onetoc2`: NoteCraft's own entries exactly; other tables of contents heuristically.
- Notebook folders: every `.one` file (named after its file), section group subfolders,
  ordered by the table of contents; `OneNote_RecycleBin/`, `*.bak`, dot files and NoteCraft's
  temporary save files are skipped; unreadable sections are skipped and logged.

Not read: the OneDrive/SharePoint (FSSHTTP-packaged) variant is detected and refused with a
clear `Unsupported` error; password-protected sections likewise. Files kept in a `onefiles`
folder (`<file>` references) aren't loaded. Conflict pages and version history are ignored.

### Write

A fresh file every time ([MS-ONESTORE]): one revision per object space, one transaction, no
free chunk list, exact `cbExpectedFileLength`, `crcName` for the file name, new
`guidFileVersion` / `guidDenyReadFileVersion`, `guidAncestor` set to the table of contents' id
when written as part of a notebook, legacy fields at their required values, the RFC 3309 CRC for
`.one` and MsoCrc32 for `.onetoc2`. Objects go in one object group per page with a global id
table; read-only objects carry their MD5; reference counts are computed.

Content follows MS-ONE's structures and its §3.1 example: section node + metadata (display
name, colour, schema revisions), page series with metadata copies (salted GUIDs), page manifest
→ page node, title node with title text and date/time outlines, outlines with nested outline
elements, rich text with shared formatting objects and paragraph style objects, number lists,
note tag definitions and states, pictures (file data store objects), embedded files (with a
generated icon, which MS-ONE requires), printouts (an embedded file plus its page pictures,
tagged so NoteCraft can put them back together), ink, kept opaque content.

NoteCraft-only data uses private PropertyIDs (ids from `0x3FC0000`, see `src/ids.rs`), which
[MS-ONE] §2.1.12 tells readers to ignore and preserve: the writer's name (how NoteCraft knows a
file is its own → `authored_by_notecraft`), page colour, rule lines, exact stroke data,
printout grouping, colour alpha, fractional font sizes.

## What is uncertain

- **Ink.** MS-ONE doesn't document ink. The structure (ink node `0x00060014` on the page →
  ink data `0x0002003B` → strokes `0x00020047` → drawing attributes `0x00120048`), the
  PropertyIDs, the path encoding (a multi-byte count, then signed multi-byte deltas, one run per
  packet property: X, Y[, pressure]), colour (COLORREF) and width (HIMETRIC) were inferred from
  the sample files. NoteCraft writes ink that way — one ink node per stroke, X/Y/NormalPressure
  packet properties — and also stores every stroke exactly in a private property, so NoteCraft
  always reads back what it wrote. Unknown to us: several ink node properties OneNote writes
  (`0x0c001d4f`, `0x14001d4e`, `0x1c001daa`, `0x1c001dac`), how OneNote marks highlighter strokes
  (OneNote strokes read as pens), and whether OneNote accepts a pressure packet property
  described this way.
- **Tables of contents.** MS-ONE doesn't number the TOC properties (TOCEntryIndex_OidIndex,
  FolderChildFilename, NotebookElementOrderingID) and the corpus has no `.onetoc2`. NoteCraft
  writes its entries in private properties next to the documented FileIdentityGuid and the
  NotebookColor seen in section files; OneNote will probably see an empty table of contents and
  rebuild it from the folder (unverified).
- Some ids MS-ONE names without numbering were taken from the samples: NotebookColor
  `0x14001CBE` (a COLORREF), SchemaRevisionInOrderToWrite `0x1400348B`; NoteTagStates is
  `0x40003489` (MS-ONE's table prints `0x04003489`, a typo by its own §2.2.88).
- MsoCrc32 is implemented exactly as [MS-OSHARED] §2.4.3 writes it, including the `0xFFFF` mask
  of its cache step; only the (SHOULD) transaction-log sentinel of `.onetoc2` files uses it.

## Manual test steps for someone with Windows and OneNote 2016/2019

1. `cargo run -p notecraft-cli -- convert <some notebook folder or .onepkg> out.onepkg` (or save
   a notebook from NoteCraft into a folder), and copy the result to the Windows machine.
2. In OneNote: File ▸ Open ▸ Browse, pick the notebook folder's `Open Notebook.onetoc2` (or
   double-click the `.onepkg` and unpack it to a new folder). Note any error or "repair" prompt.
3. Check, page by page: titles and dates; text and its formatting (bold, italic, underline,
   strike, colours, highlight, fonts, sizes); bullets and numbering and their indents; To Do
   (checked and unchecked), Important and Question tags; hyperlinks open; pictures at the right
   place and size; embedded files open; printouts (file icon plus page pictures); ink strokes
   (position, colour, width, pressure); subpages indented in the page list; section colours and
   order; section groups.
4. Edit a page in OneNote, close OneNote, reopen the notebook in NoteCraft: the edit shows and
   nothing else changed (NoteCraft then sees the file as OneNote's: `authored_by_notecraft`
   false, so its first save keeps a `.bak`).
5. Open a single section file written by NoteCraft directly (double-click the `.one`).
6. Report: OneNote's version and build, any error text, screenshots of differences. A copy of
   the notebook OneNote wrote back is the most useful thing to send (it shows the properties
   OneNote expects, which can be inspected with
   `cargo run -p notecraft-onestore --example dump -- file.one`).

## Development aids

- `cargo run -p notecraft-onestore --example dump -- file.one`: every object and property.
- `cargo run -p notecraft-one --example summary -- file.one`: what NoteCraft reads.
- `scripts/fetch-corpus.sh`, then `cargo test -p notecraft-one corpus -- --nocapture`.
