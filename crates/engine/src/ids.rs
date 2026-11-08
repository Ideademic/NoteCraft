//! Command ids. Stable: scripts, shortcuts and saved settings name commands by these.

pub const NOTEBOOK_NEW: &str = "notebook.new";
pub const NOTEBOOK_OPEN: &str = "notebook.open";
pub const NOTEBOOK_CLOSE: &str = "notebook.close";
pub const NOTEBOOK_RENAME: &str = "notebook.rename";
pub const NOTEBOOK_COLOR: &str = "notebook.color";

pub const SECTION_NEW: &str = "section.new";
pub const SECTION_RENAME: &str = "section.rename";
pub const SECTION_DELETE: &str = "section.delete";
pub const SECTION_COLOR: &str = "section.color";
pub const SECTION_MOVE: &str = "section.move";
pub const SECTION_GROUP_NEW: &str = "sectionGroup.new";

pub const PAGE_NEW: &str = "page.new";
pub const PAGE_NEW_SUBPAGE: &str = "page.newSubpage";
pub const PAGE_DELETE: &str = "page.delete";
pub const PAGE_RENAME: &str = "page.rename";
pub const PAGE_MOVE: &str = "page.move";
pub const PAGE_INDENT: &str = "page.indent";
pub const PAGE_OUTDENT: &str = "page.outdent";

pub const OUTLINE_NEW: &str = "outline.new";
pub const OUTLINE_MOVE: &str = "outline.move";
pub const OUTLINE_RESIZE: &str = "outline.resize";
pub const OUTLINE_DELETE: &str = "outline.delete";

pub const TEXT_SET_PARAGRAPHS: &str = "text.setParagraphs";
pub const TEXT_INSERT: &str = "text.insert";
pub const TEXT_DELETE: &str = "text.delete";
pub const TEXT_SPLIT_PARAGRAPH: &str = "text.splitParagraph";
pub const TEXT_MERGE_PARAGRAPHS: &str = "text.mergeParagraphs";
pub const TEXT_FORMAT_AT: &str = "text.formatAt";

pub const FORMAT_BOLD: &str = "format.bold";
pub const FORMAT_ITALIC: &str = "format.italic";
pub const FORMAT_UNDERLINE: &str = "format.underline";
pub const FORMAT_STRIKE: &str = "format.strike";
pub const FORMAT_HIGHLIGHT: &str = "format.highlight";
pub const FORMAT_COLOR: &str = "format.color";
pub const FORMAT_SIZE: &str = "format.size";
pub const FORMAT_FONT: &str = "format.font";
pub const FORMAT_STYLE: &str = "format.style";
pub const FORMAT_BULLETS: &str = "format.bullets";
pub const FORMAT_NUMBERING: &str = "format.numbering";
pub const FORMAT_INDENT: &str = "format.indent";
pub const FORMAT_OUTDENT: &str = "format.outdent";
pub const FORMAT_TAG_TODO: &str = "format.tag.todo";
pub const FORMAT_TOGGLE_TODO: &str = "format.toggleTodo";
pub const FORMAT_CLEAR: &str = "format.clear";

pub const INSERT_IMAGE: &str = "insert.image";
pub const INSERT_PRINTOUT: &str = "insert.printout";
pub const INSERT_FILE: &str = "insert.file";
pub const INSERT_DATE_TIME: &str = "insert.dateTime";

pub const DRAW_STROKE: &str = "draw.stroke";
pub const DRAW_ERASE_STROKE: &str = "draw.eraseStroke";
pub const DRAW_ERASE_AT: &str = "draw.eraseAt";
pub const DRAW_LASSO_SELECT: &str = "draw.lassoSelect";
pub const DRAW_MOVE_SELECTION: &str = "draw.moveSelection";
pub const DRAW_DELETE_SELECTION: &str = "draw.deleteSelection";
pub const DRAW_RECOLOR: &str = "draw.recolor";
pub const DRAW_INK_TO_SHAPE: &str = "draw.inkToShape";
pub const DRAW_SHAPE: &str = "draw.shape";

pub const VIEW_RULE_LINES: &str = "view.ruleLines";
pub const VIEW_PAGE_COLOR: &str = "view.pageColor";

pub const EDIT_UNDO: &str = "edit.undo";
pub const EDIT_REDO: &str = "edit.redo";

pub const FILE_EXPORT_ONEPKG: &str = "file.exportOnepkg";
pub const FILE_EXPORT_HTML: &str = "file.exportHtml";
pub const FILE_EXPORT_MARKDOWN: &str = "file.exportMarkdown";
pub const FILE_EXPORT_TEXT: &str = "file.exportText";
pub const FILE_EXPORT_PDF: &str = "file.exportPdf";
pub const FILE_IMPORT_ONEPKG: &str = "file.importOnepkg";
pub const FILE_IMPORT_ONE: &str = "file.importOne";

pub const NAV_NOTEBOOK: &str = "nav.notebook";
pub const NAV_SECTION: &str = "nav.section";
pub const NAV_PAGE: &str = "nav.page";

pub const SEARCH: &str = "search";

pub const HISTORY_RECYCLE_BIN: &str = "history.recycleBin";
pub const HISTORY_RESTORE: &str = "history.restore";
pub const HISTORY_DELETE_FOREVER: &str = "history.deleteForever";
pub const HISTORY_EMPTY_RECYCLE_BIN: &str = "history.emptyRecycleBin";
pub const HISTORY_VERSIONS: &str = "history.versions";
pub const HISTORY_VIEW_VERSION: &str = "history.viewVersion";
pub const HISTORY_RESTORE_VERSION: &str = "history.restoreVersion";
pub const HISTORY_DELETE_VERSION: &str = "history.deleteVersion";
pub const HISTORY_DELETE_ALL_VERSIONS: &str = "history.deleteAllVersions";
pub const HISTORY_SHOW_VERSIONS: &str = "history.showVersions";
pub const HISTORY_DISABLE: &str = "history.disableHistory";
pub const HISTORY_MARK_READ: &str = "history.markRead";
pub const HISTORY_MARK_UNREAD: &str = "history.markUnread";
pub const HISTORY_TOGGLE_READ: &str = "history.toggleRead";
pub const HISTORY_NEXT_UNREAD: &str = "history.nextUnread";
pub const HISTORY_RECENT_EDITS: &str = "history.recentEdits";

/// Every id above, for tests and tooling.
pub const ALL: &[&str] = &[
    NOTEBOOK_NEW,
    NOTEBOOK_OPEN,
    NOTEBOOK_CLOSE,
    NOTEBOOK_RENAME,
    NOTEBOOK_COLOR,
    SECTION_NEW,
    SECTION_RENAME,
    SECTION_DELETE,
    SECTION_COLOR,
    SECTION_MOVE,
    SECTION_GROUP_NEW,
    PAGE_NEW,
    PAGE_NEW_SUBPAGE,
    PAGE_DELETE,
    PAGE_RENAME,
    PAGE_MOVE,
    PAGE_INDENT,
    PAGE_OUTDENT,
    OUTLINE_NEW,
    OUTLINE_MOVE,
    OUTLINE_RESIZE,
    OUTLINE_DELETE,
    TEXT_SET_PARAGRAPHS,
    TEXT_INSERT,
    TEXT_DELETE,
    TEXT_SPLIT_PARAGRAPH,
    TEXT_MERGE_PARAGRAPHS,
    TEXT_FORMAT_AT,
    FORMAT_BOLD,
    FORMAT_ITALIC,
    FORMAT_UNDERLINE,
    FORMAT_STRIKE,
    FORMAT_HIGHLIGHT,
    FORMAT_COLOR,
    FORMAT_SIZE,
    FORMAT_FONT,
    FORMAT_STYLE,
    FORMAT_BULLETS,
    FORMAT_NUMBERING,
    FORMAT_INDENT,
    FORMAT_OUTDENT,
    FORMAT_TAG_TODO,
    FORMAT_TOGGLE_TODO,
    FORMAT_CLEAR,
    INSERT_IMAGE,
    INSERT_PRINTOUT,
    INSERT_FILE,
    INSERT_DATE_TIME,
    DRAW_STROKE,
    DRAW_ERASE_STROKE,
    DRAW_ERASE_AT,
    DRAW_LASSO_SELECT,
    DRAW_MOVE_SELECTION,
    DRAW_DELETE_SELECTION,
    DRAW_RECOLOR,
    DRAW_INK_TO_SHAPE,
    DRAW_SHAPE,
    VIEW_RULE_LINES,
    VIEW_PAGE_COLOR,
    EDIT_UNDO,
    EDIT_REDO,
    FILE_EXPORT_ONEPKG,
    FILE_EXPORT_HTML,
    FILE_EXPORT_MARKDOWN,
    FILE_EXPORT_TEXT,
    FILE_EXPORT_PDF,
    FILE_IMPORT_ONEPKG,
    FILE_IMPORT_ONE,
    NAV_NOTEBOOK,
    NAV_SECTION,
    NAV_PAGE,
    SEARCH,
    HISTORY_RECYCLE_BIN,
    HISTORY_RESTORE,
    HISTORY_DELETE_FOREVER,
    HISTORY_EMPTY_RECYCLE_BIN,
    HISTORY_VERSIONS,
    HISTORY_VIEW_VERSION,
    HISTORY_RESTORE_VERSION,
    HISTORY_DELETE_VERSION,
    HISTORY_DELETE_ALL_VERSIONS,
    HISTORY_SHOW_VERSIONS,
    HISTORY_DISABLE,
    HISTORY_MARK_READ,
    HISTORY_MARK_UNREAD,
    HISTORY_TOGGLE_READ,
    HISTORY_NEXT_UNREAD,
    HISTORY_RECENT_EDITS,
];
