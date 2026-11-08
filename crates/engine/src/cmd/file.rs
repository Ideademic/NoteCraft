//! File tab: export (a notebook as `.onepkg`; a page or section as HTML, Markdown or text) and
//! import (`.onepkg` as a new notebook, `.one` as a section of the current notebook).
//!
//! Exports return the file for the front end to save or download:
//! `{"fileName": string, "mime": string, "bytesBase64": string, "size": number}`. With `path`
//! (desktop) the file is written there instead (safely: a temporary file, then renamed) and the
//! output is `{"path": string, "size": number}`.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::{Guid, Notebook, Page, Section, SectionEntry};
use serde_json::{Value, json};

use super::{MAX_FILE_BYTES, file_input, has_notebook, has_page};
use crate::ids::{FILE_EXPORT_HTML, FILE_EXPORT_MARKDOWN, FILE_EXPORT_ONEPKG, FILE_EXPORT_TEXT, FILE_IMPORT_ONE, FILE_IMPORT_ONEPKG};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Origin, Session, p};

const EXPORT_PAGE: &str = r##"{"scope"?: "page" (default)|"section", "id"?: page or section guid (default: the current one), "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} | {"path", "size"}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(FILE_EXPORT_ONEPKG, "Export Notebook (.onepkg)", export_onepkg)
            .pure()
            .when(has_notebook)
            .params(r##"{"id"?: notebook guid (default: the current one), "path"?: string} -> {"fileName", "mime", "bytesBase64", "size"} | {"path", "size"}"##),
        CommandSpec::new(FILE_EXPORT_HTML, "Export as HTML", export_html).pure().when(has_page).params(concat!(
            r##"{"scope"?: "page" (default)|"section", "id"?: page or section guid (default: the current one), "path"?: string"##,
            r##", "positioned"?: bool (default true: keep the page layout)} -> {"fileName", "mime", "bytesBase64", "size"} | {"path", "size"}"##
        )),
        CommandSpec::new(FILE_EXPORT_MARKDOWN, "Export as Markdown", export_markdown).pure().when(has_page).params(EXPORT_PAGE),
        CommandSpec::new(FILE_EXPORT_TEXT, "Export as Plain Text", export_text).pure().when(has_page).params(EXPORT_PAGE),
        CommandSpec::new(FILE_IMPORT_ONEPKG, "Import Notebook (.onepkg)", import_onepkg)
            .pure()
            .params(r##"{"bytesBase64": string, "name"?: string} | {"path": string} -> {"id": notebook guid}"##),
        CommandSpec::new(FILE_IMPORT_ONE, "Import Section (.one)", import_one)
            .when(has_notebook)
            .params(r##"{"bytesBase64": string, "name"?: string} | {"path": string}, "notebook"?: guid, "group"?: guid -> {"id": section guid}"##),
    ]
}

/// Hands an exported file back: bytes as base64, or written to `params.path`.
pub(crate) fn deliver(params: &Value, file_name: String, mime: &str, bytes: Vec<u8>) -> CmdResult {
    let size = bytes.len();
    if let Some(path) = p::str(params, "path") {
        return write_to(path, &bytes).map(|()| CmdOutput::query(json!({"path": path, "size": size})));
    }
    Ok(CmdOutput::query(json!({"fileName": file_name, "mime": mime, "bytesBase64": STANDARD.encode(&bytes), "size": size})))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_to(path: &str, bytes: &[u8]) -> Result<(), CmdError> {
    crate::io::safe_save::write(std::path::Path::new(path), bytes).map_err(|e| CmdError::Failed(format!("{path}: {e}")))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn write_to(_: &str, _: &[u8]) -> Result<(), CmdError> {
    Err(CmdError::Disabled("the web app downloads exports".into()))
}

fn stem(name: &str) -> String {
    notecraft_one::safe_file_stem(name)
}

fn export_onepkg(s: &mut Session, params: &Value) -> CmdResult {
    let id = super::id_or(params, s.current.notebook, "notebook")?;
    let nb = s.notebook(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    let bytes = notecraft_onepkg::write(nb).map_err(|e| CmdError::Io(e.into()))?;
    deliver(params, format!("{}.onepkg", stem(&nb.name)), "application/onenote", bytes)
}

/// What a page export covers.
enum Scope<'a> {
    Page(&'a Page),
    Section(&'a Section),
}

fn scope<'a>(s: &'a Session, params: &Value) -> Result<Scope<'a>, CmdError> {
    let id = p::guid(params, "id")?;
    match p::str(params, "scope").unwrap_or("page") {
        "page" => {
            let pid = id.or(s.current.page).ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
            s.notebooks.iter().find_map(|n| n.find_page(pid)).map(|(_, pg)| Scope::Page(pg)).ok_or_else(|| CmdError::Params(format!("no page {pid}")))
        }
        "section" => {
            let sid = id.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
            s.notebooks.iter().find_map(|n| n.section(sid)).map(Scope::Section).ok_or_else(|| CmdError::Params(format!("no section {sid}")))
        }
        other => Err(CmdError::Params(format!("unknown scope `{}`", other.chars().take(20).collect::<String>()))),
    }
}

fn failed(e: notecraft_formats::Error) -> CmdError {
    CmdError::Failed(e.to_string())
}

fn export_html(s: &mut Session, params: &Value) -> CmdResult {
    let opts = notecraft_formats::HtmlOptions { positioned: p::bool(params, "positioned").unwrap_or(true) };
    let (name, html) = match scope(s, params)? {
        Scope::Page(pg) => (pg.display_title().to_owned(), notecraft_formats::page_to_html_with(pg, opts)),
        Scope::Section(sec) => (sec.name.clone(), notecraft_formats::section_to_html_with(sec, opts)),
    };
    deliver(params, format!("{}.html", stem(&name)), "text/html", html.into_bytes())
}

/// A section's pages joined, each by `one`, separated by `sep`.
fn join_pages(sec: &Section, sep: &str, one: fn(&Page) -> notecraft_formats::Result<String>) -> Result<String, CmdError> {
    let parts = sec.pages.iter().map(one).collect::<Result<Vec<_>, _>>().map_err(failed)?;
    Ok(parts.join(sep))
}

fn export_markdown(s: &mut Session, params: &Value) -> CmdResult {
    let (name, md) = match scope(s, params)? {
        Scope::Page(pg) => (pg.display_title().to_owned(), notecraft_formats::page_to_markdown(pg).map_err(failed)?),
        Scope::Section(sec) => (sec.name.clone(), join_pages(sec, "\n\n---\n\n", notecraft_formats::page_to_markdown)?),
    };
    deliver(params, format!("{}.md", stem(&name)), "text/markdown", md.into_bytes())
}

fn export_text(s: &mut Session, params: &Value) -> CmdResult {
    let (name, txt) = match scope(s, params)? {
        Scope::Page(pg) => (pg.display_title().to_owned(), notecraft_formats::page_to_text(pg).map_err(failed)?),
        Scope::Section(sec) => (sec.name.clone(), join_pages(sec, "\n\n", notecraft_formats::page_to_text)?),
    };
    deliver(params, format!("{}.txt", stem(&name)), "text/plain", txt.into_bytes())
}

/// Ids already used by the open notebooks (notebooks, groups, sections, pages).
fn used_ids(s: &Session) -> std::collections::HashSet<Guid> {
    let mut ids = std::collections::HashSet::new();
    for nb in &s.notebooks {
        ids.insert(nb.id);
        collect_ids(&nb.sections, 0, &mut ids);
    }
    ids
}

fn collect_ids(entries: &[SectionEntry], depth: usize, ids: &mut std::collections::HashSet<Guid>) {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        ids.insert(e.id());
        match e {
            SectionEntry::Section(sec) => ids.extend(sec.pages.iter().map(|pg| pg.id)),
            SectionEntry::Group(g) => collect_ids(&g.entries, depth + 1, ids),
        }
    }
}

/// Gives anything whose id is already open a new one (importing the same notebook twice).
fn reid_entries(entries: &mut [SectionEntry], used: &std::collections::HashSet<Guid>, depth: usize) {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(sec) => reid_section(sec, used),
            SectionEntry::Group(g) => {
                if used.contains(&g.id) {
                    g.id = Guid::new();
                }
                reid_entries(&mut g.entries, used, depth + 1);
            }
        }
    }
}

fn reid_section(sec: &mut Section, used: &std::collections::HashSet<Guid>) {
    if used.contains(&sec.id) {
        sec.id = Guid::new();
    }
    for pg in &mut sec.pages {
        if used.contains(&pg.id) {
            pg.id = Guid::new();
        }
    }
}

pub(crate) fn reid_notebook(nb: &mut Notebook, s: &Session) {
    let used = used_ids(s);
    if used.contains(&nb.id) {
        nb.id = Guid::new();
    }
    reid_entries(&mut nb.sections, &used, 0);
}

fn import_onepkg(s: &mut Session, params: &Value) -> CmdResult {
    let file = file_input(params, MAX_FILE_BYTES)?;
    let mut nb = notecraft_onepkg::read(&file.bytes).map_err(|e| CmdError::Io(e.into()))?;
    if nb.name.trim().is_empty() {
        nb.name = file.name.strip_suffix(".onepkg").unwrap_or(&file.name).to_owned();
    }
    reid_notebook(&mut nb, s);
    let id = s.add_notebook(nb, Origin::Scratch);
    // A package is unpacked into a new notebook: everything is saved on the next autosave.
    s.mark_notebook_dirty(id);
    Ok(CmdOutput::query(json!({"id": id})))
}

fn import_one(s: &mut Session, params: &Value) -> CmdResult {
    let file = file_input(params, MAX_FILE_BYTES)?;
    let stem = file.name.rsplit_once('.').map_or(file.name.as_str(), |(stem, _)| stem);
    let mut sec = notecraft_one::read_section_named(&file.bytes, stem).map_err(|e| CmdError::Io(e.into()))?;
    if sec.name.trim().is_empty() {
        sec.name = file.name.strip_suffix(".one").unwrap_or(&file.name).to_owned();
    }
    let used = used_ids(s);
    reid_section(&mut sec, &used);
    let nb_id = p::guid(params, "notebook")?.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
    let group = p::guid(params, "group")?;
    let nb = s.notebook_mut(nb_id).ok_or_else(|| CmdError::Params(format!("no notebook {nb_id}")))?;
    // Saved as a new file named after the section (the save picks a free name).
    sec.source.file_name = None;
    let loc = Location { notebook: Some(nb_id), section: Some(sec.id), page: sec.pages.first().map(|pg| pg.id) };
    let id = sec.id;
    nb.entries_mut(group).ok_or_else(|| CmdError::Params("no such section group".into()))?.push(SectionEntry::Section(sec));
    s.current = loc;
    Ok(CmdOutput::value(json!({"id": id})))
}
