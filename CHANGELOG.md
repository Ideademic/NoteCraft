# Changelog

All notable changes to NoteCraft are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

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

[0.1.1]: https://github.com/Ideademic/NoteCraft/releases/tag/v0.1.1
[0.1.0]: https://github.com/Ideademic/NoteCraft/releases/tag/v0.1.0
