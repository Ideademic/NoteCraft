//! Home tab formatting. Not implemented yet (phase 2): each registers its id and parameters.
//!
//! Selection-based formatting takes the outline and a character range:
//! `{"id": outline guid, "para": n, "start": char, "end": char}` (or `"paras": [a, b]` for
//! paragraph-level commands) plus the command's own value; the canvas passes its selection.

use super::has_page;
use crate::CommandSpec;
use crate::ids::{
    FORMAT_BOLD, FORMAT_BULLETS, FORMAT_COLOR, FORMAT_FONT, FORMAT_HIGHLIGHT, FORMAT_INDENT, FORMAT_ITALIC, FORMAT_NUMBERING, FORMAT_OUTDENT,
    FORMAT_SIZE, FORMAT_STRIKE, FORMAT_STYLE, FORMAT_TAG_TODO, FORMAT_TOGGLE_TODO, FORMAT_UNDERLINE,
};

const RANGE: &str = r##"{"id": guid, "para": number, "start": number, "end": number}"##;
const PARAS: &str = r##"{"id": guid, "paras": [first, last]}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::stub(FORMAT_BOLD, "Bold").key("Mod+B").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_ITALIC, "Italic").key("Mod+I").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_UNDERLINE, "Underline").key("Mod+U").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_STRIKE, "Strikethrough").key("Mod+-").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_HIGHLIGHT, "Text Highlight Color").key("Mod+Alt+H").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_COLOR, "Font Color").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_SIZE, "Font Size").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_FONT, "Font").when(has_page).params(RANGE),
        CommandSpec::stub(FORMAT_STYLE, "Styles").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_BULLETS, "Bullets").key("Mod+.").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_NUMBERING, "Numbering").key("Mod+/").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_INDENT, "Increase Indent").key("Alt+Shift+Right").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_OUTDENT, "Decrease Indent").key("Alt+Shift+Left").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_TAG_TODO, "To Do Tag").key("Mod+1").when(has_page).params(PARAS),
        CommandSpec::stub(FORMAT_TOGGLE_TODO, "Check To Do").when(has_page).params(r##"{"id": guid, "para": number}"##),
    ]
}
