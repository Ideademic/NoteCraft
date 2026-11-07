//! Notebooks: new, open, close, rename, colour.

use notecraft_doc::{Color, Notebook, Page, Section, SectionEntry};
use serde_json::{Value, json};

use super::{has_notebook, id_or};
use crate::ids::{NOTEBOOK_CLOSE, NOTEBOOK_COLOR, NOTEBOOK_NEW, NOTEBOOK_OPEN, NOTEBOOK_RENAME};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Origin, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(NOTEBOOK_NEW, "New Notebook", new).params(r##"{"name"?: string}"##),
        CommandSpec::new(NOTEBOOK_OPEN, "Open Notebook", open).pure().key("Mod+O").params(r##"{"path": string}"##),
        CommandSpec::new(NOTEBOOK_CLOSE, "Close This Notebook", close).pure().when(has_notebook).params(r##"{"id"?: guid}"##),
        CommandSpec::new(NOTEBOOK_RENAME, "Rename Notebook", rename).when(has_notebook).params(r##"{"id"?: guid, "name": string}"##),
        CommandSpec::new(NOTEBOOK_COLOR, "Notebook Color", color).when(has_notebook).params(r##"{"id"?: guid, "color": "#rrggbb"}"##),
    ]
}

/// A new notebook with one section and one empty page, made current.
fn new(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::name(params, "name")?.unwrap_or_else(|| "New Notebook".to_owned());
    let mut nb = Notebook::new(name);
    nb.color = Color::section_color(s.notebooks.len() + 4);
    let mut section = Section::new("New Section 1", Color::section_color(0));
    let page = Page::new("");
    let loc = Location { notebook: Some(nb.id), section: Some(section.id), page: Some(page.id) };
    section.pages.push(page);
    nb.sections.push(SectionEntry::Section(section));
    s.origins.insert(nb.id, Origin::Scratch);
    s.notebooks.push(nb);
    s.current = loc;
    Ok(CmdOutput::value(json!({"id": loc.notebook, "section": loc.section, "page": loc.page})))
}

/// Opens a notebook folder, `.onetoc2`, `.onepkg` or `.one` from a path (desktop). Clears undo.
fn open(s: &mut Session, params: &Value) -> CmdResult {
    let path = p::req_str(params, "path")?;
    open_path(s, path)
}

#[cfg(not(target_arch = "wasm32"))]
fn open_path(s: &mut Session, path: &str) -> CmdResult {
    let path = std::path::Path::new(path);
    if let Some(open) = s.notebooks.iter().find(|n| s.origins.get(&n.id).is_some_and(|o| *o == crate::io::origin_of(path))) {
        let id = open.id;
        s.current = Location { notebook: Some(id), section: None, page: None };
        return Ok(CmdOutput::query(json!({"id": id, "alreadyOpen": true})));
    }
    let nb = crate::io::open_path(path)?;
    let id = s.add_notebook(nb, crate::io::origin_of(path));
    Ok(CmdOutput::query(json!({"id": id})))
}

#[cfg(target_arch = "wasm32")]
fn open_path(_: &mut Session, _: &str) -> CmdResult {
    Err(CmdError::Disabled("the web app opens files through the browser".into()))
}

/// Closes a notebook (saving it first when storage is set). Clears undo.
fn close(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.notebook, "notebook")?;
    if s.notebook(id).is_none() {
        return Err(CmdError::Params(format!("no notebook {id}")));
    }
    if s.notebook_is_dirty(id) && s.storage.is_some() {
        s.save_dirty()?;
    }
    let sections: Vec<_> = s.notebook(id).map(|nb| nb.all_sections().iter().map(|sec| sec.id).collect()).unwrap_or_default();
    for sec in sections {
        s.dirty.remove(&sec);
    }
    s.dirty_tocs.remove(&id);
    s.notebooks.retain(|n| n.id != id);
    s.origins.remove(&id);
    s.history.clear();
    s.revision += 1;
    if s.current.notebook == Some(id) {
        s.current = Location::default();
    }
    Ok(CmdOutput::query(Value::Null))
}

fn rename(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.notebook, "notebook")?;
    let name = p::name(params, "name")?.ok_or_else(|| CmdError::Params("`name` is required".into()))?;
    let nb = s.notebook_mut(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    if nb.name == name {
        return Ok(CmdOutput::unchanged());
    }
    nb.name = name;
    Ok(CmdOutput::ok())
}

fn color(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.notebook, "notebook")?;
    let c = p::req_color(params, "color")?;
    let nb = s.notebook_mut(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    if nb.color == c {
        return Ok(CmdOutput::unchanged());
    }
    nb.color = c;
    Ok(CmdOutput::ok())
}
