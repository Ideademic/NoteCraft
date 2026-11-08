//! The file behind a page item: an attached file, a picture, or a printout's original file, for
//! right-click › Open and Save As.

use notecraft_doc::ElementKind;
use serde_json::Value;

use super::file::deliver;
use super::{has_page, target_page};
use crate::ids::FILE_EXPORT_ATTACHMENT;
use crate::{CmdError, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(FILE_EXPORT_ATTACHMENT, "Save Attachment As", export).pure().when(has_page).params(concat!(
        r##"{"element": guid (an attached file, picture or printout; also "id"), "page"?: guid, "path"?: string}"##,
        r##" -> {"fileName", "mime", "bytesBase64", "size"} | {"path", "size"}"##
    ))]
}

fn export(s: &mut Session, params: &Value) -> CmdResult {
    let id = match p::guid(params, "element")? {
        Some(id) => id,
        None => p::req_guid(params, "id")?,
    };
    let page = target_page(s, params)?;
    let el = page.element(id).ok_or_else(|| CmdError::Params(format!("no item {id} on the page")))?;
    let (name, mime, bytes) = match &el.kind {
        ElementKind::EmbeddedFile { name, bytes, .. } => {
            let n = if name.trim().is_empty() { "Attachment".to_owned() } else { name.clone() };
            (n, "application/octet-stream", bytes.as_ref().clone())
        }
        ElementKind::Image(img) => {
            let stem = if img.alt.trim().is_empty() { "Picture".to_owned() } else { notecraft_one::safe_file_stem(&img.alt) };
            (format!("{stem}.{}", img.format.extension()), img.format.mime(), img.data.as_ref().clone())
        }
        ElementKind::Printout(pr) if !pr.file_bytes.is_empty() => (pr.file_name.clone(), "application/pdf", pr.file_bytes.as_ref().clone()),
        _ => return Err(CmdError::Params("that item has no file to save".into())),
    };
    let name: String = name.rsplit(['/', '\\']).next().unwrap_or("Attachment").chars().filter(|c| !c.is_control()).take(255).collect();
    deliver(params, name, mime, bytes)
}
