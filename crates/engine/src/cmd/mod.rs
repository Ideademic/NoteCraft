//! The commands, by ribbon area. Each module's `specs()` lists its commands; [`all`] joins them.

pub mod draw;
pub mod edit;
pub mod file;
pub mod format;
pub mod insert;
pub mod nav;
pub mod notebook;
pub mod outline;
pub mod page;
pub mod section;
pub mod view;

use notecraft_doc::{Guid, Page};
use serde_json::Value;

use crate::{CmdError, CommandSpec, Session, p};

/// Every command, in menu order.
pub fn all() -> Vec<CommandSpec> {
    let mut v = Vec::new();
    v.extend(notebook::specs());
    v.extend(section::specs());
    v.extend(page::specs());
    v.extend(outline::specs());
    v.extend(format::specs());
    v.extend(insert::specs());
    v.extend(draw::specs());
    v.extend(view::specs());
    v.extend(edit::specs());
    v.extend(file::specs());
    v.extend(nav::specs());
    v
}

// --- `enabled` predicates ----------------------------------------------------------------

pub(crate) fn has_notebook(s: &Session) -> bool {
    s.current_notebook().is_some()
}

pub(crate) fn has_section(s: &Session) -> bool {
    s.current_section().is_some()
}

pub(crate) fn has_page(s: &Session) -> bool {
    s.current_page().is_some()
}

// --- shared helpers ----------------------------------------------------------------------

/// The page named by `params.page`, else the current page.
pub(crate) fn target_page<'a>(s: &'a mut Session, params: &Value) -> Result<&'a mut Page, CmdError> {
    match p::guid(params, "page")? {
        Some(id) => s.page_mut(id).ok_or_else(|| CmdError::Params(format!("no page {id}"))),
        None => s.current_page_mut().ok_or_else(|| CmdError::Disabled("no page is open".into())),
    }
}

/// `params.id`, else the current notebook / section / page id from `current`.
pub(crate) fn id_or(params: &Value, current: Option<Guid>, what: &str) -> Result<Guid, CmdError> {
    p::guid(params, "id")?.or(current).ok_or_else(|| CmdError::Disabled(format!("no {what} is open")))
}
