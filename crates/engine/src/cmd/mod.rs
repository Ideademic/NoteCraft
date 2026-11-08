//! The commands, by ribbon area. Each module's `specs()` lists its commands; [`all`] joins them.

pub mod draw;
pub mod edit;
pub mod file;
pub mod format;
pub mod ink_shape;
pub mod insert;
pub mod nav;
pub mod notebook;
pub mod outline;
pub mod page;
pub mod pdf;
pub mod search;
pub mod section;
pub mod shapes;
pub mod text;
pub mod view;

use base64::Engine as _;
use notecraft_doc::page::CONTENT_ORIGIN;
use notecraft_doc::{Guid, Page, Point};
use serde_json::Value;

use crate::{CmdError, CommandSpec, Session, p};

/// Every command, in menu order.
pub fn all() -> Vec<CommandSpec> {
    let mut v = Vec::new();
    v.extend(notebook::specs());
    v.extend(section::specs());
    v.extend(page::specs());
    v.extend(outline::specs());
    v.extend(text::specs());
    v.extend(format::specs());
    v.extend(insert::specs());
    v.extend(draw::specs());
    v.extend(view::specs());
    v.extend(edit::specs());
    v.extend(file::specs());
    v.extend(pdf::specs());
    v.extend(nav::specs());
    v.extend(search::specs());
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

/// Largest file a command reads or decodes, bytes.
pub const MAX_FILE_BYTES: usize = 512 * 1024 * 1024;

/// A file handed to a command: `bytesBase64` (+ `name`), or `path` on the desktop.
pub(crate) struct FileInput {
    /// The file name (no folders), `file` when unknown.
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Reads `params.bytesBase64` or `params.path`, at most `max` bytes.
pub(crate) fn file_input(params: &Value, max: usize) -> Result<FileInput, CmdError> {
    let given =
        p::str(params, "name").map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n).chars().filter(|c| !c.is_control()).take(255).collect::<String>());
    if let Some(b64) = p::str(params, "bytesBase64") {
        if b64.len() / 4 * 3 > max.saturating_add(3) {
            return Err(CmdError::Params(format!("the file is larger than {} MB", max / 1_048_576)));
        }
        let bytes =
            base64::engine::general_purpose::STANDARD.decode(b64.trim().as_bytes()).map_err(|e| CmdError::Params(format!("`bytesBase64`: {e}")))?;
        let name = given.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "file".to_owned());
        return Ok(FileInput { name, bytes });
    }
    let path = p::str(params, "path").ok_or_else(|| CmdError::Params("`bytesBase64` or `path` is required".into()))?;
    read_path(path, given, max)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_path(path: &str, given: Option<String>, max: usize) -> Result<FileInput, CmdError> {
    use std::io::Read as _;
    let p = std::path::Path::new(path);
    let file = std::fs::File::open(p).map_err(|e| CmdError::Failed(format!("{}: {e}", p.display())))?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(CmdError::Params(format!("{} is not a file", p.display())));
    }
    if len > max as u64 {
        return Err(CmdError::Params(format!("the file is larger than {} MB", max / 1_048_576)));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut bytes).map_err(|e| CmdError::Failed(format!("{}: {e}", p.display())))?;
    if bytes.len() > max {
        return Err(CmdError::Params(format!("the file is larger than {} MB", max / 1_048_576)));
    }
    let name = given
        .filter(|n| !n.trim().is_empty())
        .or_else(|| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "file".to_owned());
    Ok(FileInput { name, bytes })
}

#[cfg(target_arch = "wasm32")]
fn read_path(_: &str, _: Option<String>, _: usize) -> Result<FileInput, CmdError> {
    Err(CmdError::Disabled("the web app passes files as `bytesBase64`".into()))
}

/// Where inserted content goes: `params.x`/`params.y`, else at the left margin below what is
/// already on the page.
pub(crate) fn insert_pos(params: &Value, page: &Page) -> Point {
    match (p::f32(params, "x"), p::f32(params, "y")) {
        (Some(x), Some(y)) => Point::new(x, y).sanitized(),
        _ => {
            let below = page.content_bounds().map_or(CONTENT_ORIGIN.y, |b| (b.bottom() + 18.0).max(CONTENT_ORIGIN.y));
            Point::new(CONTENT_ORIGIN.x, below).sanitized()
        }
    }
}
