//! Sections and section groups: new, rename, delete, colour, move.

use notecraft_doc::{Color, DeletedSection, Page, Section, SectionEntry, SectionGroup};
use serde_json::{Value, json};

use super::{has_notebook, has_section, id_or};
use crate::ids::{SECTION_COLOR, SECTION_DELETE, SECTION_GROUP_NEW, SECTION_MOVE, SECTION_NEW, SECTION_RENAME};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(SECTION_NEW, "New Section", new)
            .key("Mod+T")
            .when(has_notebook)
            .params(r##"{"name"?: string, "group"?: guid, "notebook"?: guid}"##),
        CommandSpec::new(SECTION_RENAME, "Rename Section", rename).when(has_section).params(r##"{"id"?: guid, "name": string}"##),
        CommandSpec::new(SECTION_DELETE, "Delete Section", delete).when(has_section).params(r##"{"id"?: guid}"##),
        CommandSpec::new(SECTION_COLOR, "Section Color", color).when(has_section).params(r##"{"id"?: guid, "color": "#rrggbb"}"##),
        CommandSpec::new(SECTION_MOVE, "Move Section", move_section)
            .when(has_section)
            .params(r##"{"id"?: guid, "index": number, "group"?: guid | null, "notebook"?: guid}"##),
        CommandSpec::new(SECTION_GROUP_NEW, "New Section Group", new_group).when(has_notebook).params(r##"{"name"?: string, "parent"?: guid}"##),
    ]
}

/// A new section (with one empty page) at the end of the notebook or of `group`, made current.
fn new(s: &mut Session, params: &Value) -> CmdResult {
    let nb_id = p::guid(params, "notebook")?.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
    let group = p::guid(params, "group")?;
    let nb = s.notebook_mut(nb_id).ok_or_else(|| CmdError::Params(format!("no notebook {nb_id}")))?;
    let count = nb.all_sections().len();
    let name = match p::name(params, "name")? {
        Some(n) => n,
        None => format!("New Section {}", count + 1),
    };
    let mut section = Section::new(name, Color::section_color(count));
    let page = Page::new("");
    let loc = Location { notebook: Some(nb_id), section: Some(section.id), page: Some(page.id) };
    section.pages.push(page);
    nb.entries_mut(group).ok_or_else(|| CmdError::Params("no such section group".into()))?.push(SectionEntry::Section(section));
    s.current = loc;
    Ok(CmdOutput::value(json!({"id": loc.section, "page": loc.page})))
}

fn rename(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.section, "section")?;
    let name = p::name(params, "name")?.ok_or_else(|| CmdError::Params("`name` is required".into()))?;
    // Groups are renamed by the same command.
    for nb in &mut s.notebooks {
        if let Some(g) = nb.group_mut(id) {
            g.name = name;
            return Ok(CmdOutput::ok());
        }
    }
    let sec = s.section_mut(id).ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    if sec.name == name {
        return Ok(CmdOutput::unchanged());
    }
    sec.name = name;
    // OneNote names a section by its file: the next save moves it to a file with the new name.
    sec.source.file_name = None;
    Ok(CmdOutput::ok())
}

/// Deletes a section or section group into the notebook's recycle bin; the current one moves
/// to the next section.
fn delete(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.section, "section")?;
    let s_now = s.now();
    let nb = s
        .notebooks
        .iter_mut()
        .find(|n| n.section(id).is_some() || n.group(id).is_some())
        .ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    // The section after it in tab order becomes current (or the one before, at the end).
    let order: Vec<_> = nb.all_sections().iter().map(|s| s.id).collect();
    let next = order.iter().position(|x| *x == id).and_then(|i| order.get(i + 1).or_else(|| i.checked_sub(1).and_then(|j| order.get(j)))).copied();
    // Into the recycle bin, with where it was (a group's sections go one by one, remembering
    // the group).
    let path = nb.path_of(id).unwrap_or_default();
    let parent = path.last().copied();
    let index = nb.entries_mut(parent).and_then(|e| e.iter().position(|x| x.id() == id)).unwrap_or(0);
    let now = s_now;
    let _ = index;
    match nb.remove_entry(id) {
        Some(SectionEntry::Section(section)) => {
            nb.recycle_bin.sections.push(DeletedSection { section, deleted: now, group_path: path, index });
        }
        Some(SectionEntry::Group(g)) => {
            let mut stack = vec![(g, path)];
            while let Some((g, path)) = stack.pop() {
                let mut inner = path.clone();
                inner.push(g.id);
                if inner.len() > notecraft_doc::notebook::MAX_GROUP_DEPTH {
                    continue;
                }
                for (i, e) in g.entries.into_iter().enumerate() {
                    match e {
                        SectionEntry::Section(section) => {
                            nb.recycle_bin.sections.push(DeletedSection { section, deleted: now, group_path: inner.clone(), index: i })
                        }
                        SectionEntry::Group(sub) => stack.push((sub, inner.clone())),
                    }
                }
            }
        }
        None => {}
    }
    nb.recycle_bin.cap();
    // The current section was it, or inside the deleted group (`normalize_location` falls back
    // to the first section when `next` went too).
    if s.current.section.is_some_and(|c| !s.notebooks.iter().any(|n| n.section(c).is_some())) {
        s.current.section = next;
        s.current.page = None;
    }
    Ok(CmdOutput::ok())
}

fn color(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.section, "section")?;
    let c = p::req_color(params, "color")?;
    let sec = s.section_mut(id).ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    if sec.color == c {
        return Ok(CmdOutput::unchanged());
    }
    sec.color = c;
    Ok(CmdOutput::ok())
}

/// Moves a section or group to `index` in `group` (the notebook's own list when absent or
/// null), in `notebook` (default: the one it is in).
fn move_section(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.section, "section")?;
    let index = p::index(params, "index").ok_or_else(|| CmdError::Params("`index` (number) is required".into()))?;
    let group = p::guid(params, "group")?;
    let from = s
        .notebooks
        .iter()
        .position(|n| n.section(id).is_some() || n.group(id).is_some())
        .ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    let to_id = p::guid(params, "notebook")?;
    let entry = s.notebooks.get_mut(from).and_then(|n| n.remove_entry(id)).ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    let to = match to_id {
        Some(t) => s.notebooks.iter().position(|n| n.id == t).ok_or_else(|| CmdError::Params(format!("no notebook {t}")))?,
        None => from,
    };
    let list = s
        .notebooks
        .get_mut(to)
        .and_then(|n| n.entries_mut(group))
        .ok_or_else(|| CmdError::Params("no such section group (a group can't move into itself)".into()))?;
    let at = index.min(list.len());
    list.insert(at, entry);
    if let Some(nb) = s.notebooks.get(to)
        && to != from
        && s.current.section == Some(id)
    {
        s.current.notebook = Some(nb.id);
    }
    Ok(CmdOutput::ok())
}

fn new_group(s: &mut Session, params: &Value) -> CmdResult {
    let parent = p::guid(params, "parent")?;
    let name = p::name(params, "name")?.unwrap_or_else(|| "New Section Group".to_owned());
    let g = SectionGroup::new(name);
    let id = g.id;
    let nb = s.current_notebook_mut().ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
    nb.entries_mut(parent).ok_or_else(|| CmdError::Params("no such section group".into()))?.push(SectionEntry::Group(g));
    Ok(CmdOutput::value(json!({"id": id})))
}
