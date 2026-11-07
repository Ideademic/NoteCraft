//! File tab: export and import. Not implemented yet (phase 2).

use super::{has_notebook, has_page};
use crate::CommandSpec;
use crate::ids::{FILE_EXPORT_HTML, FILE_EXPORT_MARKDOWN, FILE_EXPORT_ONEPKG, FILE_IMPORT_ONE, FILE_IMPORT_ONEPKG};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::stub(FILE_EXPORT_ONEPKG, "Export Notebook (.onepkg)").pure().when(has_notebook).params(r##"{"path": string, "id"?: guid}"##),
        CommandSpec::stub(FILE_EXPORT_HTML, "Export as HTML").pure().when(has_page).params(r##"{"path": string, "scope"?: "page"|"section"}"##),
        CommandSpec::stub(FILE_EXPORT_MARKDOWN, "Export as Markdown").pure().when(has_page).params(r##"{"path": string}"##),
        CommandSpec::stub(FILE_IMPORT_ONEPKG, "Import Notebook (.onepkg)")
            .pure()
            .params(r##"{"path"?: string, "name"?: string, "bytesBase64"?: string}"##),
        CommandSpec::stub(FILE_IMPORT_ONE, "Import Section (.one)")
            .when(has_notebook)
            .params(r##"{"path"?: string, "name"?: string, "bytesBase64"?: string}"##),
    ]
}
