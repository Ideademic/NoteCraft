//! Insert tab. Not implemented yet (phase 2).
//!
//! Files arrive as a path (desktop) or as base64 bytes with a name (web, scripts).

use super::has_page;
use crate::CommandSpec;
use crate::ids::{INSERT_DATE_TIME, INSERT_FILE, INSERT_IMAGE, INSERT_PRINTOUT};

const FILE: &str = r##"{"path"?: string, "name"?: string, "bytesBase64"?: string, "x"?: number, "y"?: number}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::stub(INSERT_IMAGE, "Pictures").when(has_page).params(FILE),
        CommandSpec::stub(INSERT_PRINTOUT, "File Printout").when(has_page).params(FILE),
        CommandSpec::stub(INSERT_FILE, "Attach File").when(has_page).params(FILE),
        CommandSpec::stub(INSERT_DATE_TIME, "Date & Time").key("Alt+Shift+F").when(has_page).params(r##"{"id"?: guid, "x"?: number, "y"?: number}"##),
    ]
}
