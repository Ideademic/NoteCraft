//! Pages: new, new subpage, delete (to the recycle bin), rename, move, indent / outdent (make
//! subpage).

use notecraft_doc::{DeletedPage, Page, Section, page::MAX_PAGE_LEVEL};
use serde_json::{Value, json};

use super::{has_page, has_section, id_or};
use crate::ids::{PAGE_DELETE, PAGE_INDENT, PAGE_MOVE, PAGE_NEW, PAGE_NEW_SUBPAGE, PAGE_OUTDENT, PAGE_RENAME};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(PAGE_NEW, "New Page", new)
            .key("Mod+N")
            .when(has_section)
            .params(r##"{"section"?: guid, "title"?: string, "blank"?: bool (ignore the section's default template)}"##),
        CommandSpec::new(PAGE_NEW_SUBPAGE, "New Subpage", new_subpage).key("Mod+Shift+Alt+N").when(has_page).params(r##"{"title"?: string}"##),
        CommandSpec::new(PAGE_DELETE, "Delete Page", delete).when(has_page).params(r##"{"id"?: guid}"##),
        CommandSpec::new(PAGE_RENAME, "Rename Page", rename).when(has_page).params(r##"{"id"?: guid, "title": string}"##),
        CommandSpec::new(PAGE_MOVE, "Move Page", move_page).when(has_page).params(r##"{"id"?: guid, "index": number, "section"?: guid}"##),
        CommandSpec::new(PAGE_INDENT, "Make Subpage", indent).key("Mod+Alt+]").when(has_page).params(r##"{"id"?: guid}"##),
        CommandSpec::new(PAGE_OUTDENT, "Promote Subpage", outdent).key("Mod+Alt+[").when(has_page).params(r##"{"id"?: guid}"##),
    ]
}

/// Titles keep to one line and 1000 characters.
pub(crate) fn clean_title(t: &str) -> String {
    t.chars().map(|c| if c.is_control() { ' ' } else { c }).take(1000).collect()
}

/// Makes levels consistent: the first page is a page, and no page is more than one level below
/// the one above it.
pub(crate) fn normalize_levels(section: &mut Section) {
    let mut prev: Option<u8> = None;
    for pg in &mut section.pages {
        let max = prev.map_or(0, |l| (l + 1).min(MAX_PAGE_LEVEL));
        pg.level = pg.level.min(max);
        prev = Some(pg.level);
    }
}

/// Inserts a new page with `level` after the current page's family (or at the end), makes it
/// current.
fn insert_page(s: &mut Session, section_id: Option<notecraft_doc::Guid>, title: String, sub: bool) -> CmdResult {
    insert_built_page(s, section_id, Page::new(title), sub)
}

/// [`insert_page`] with the page already made (from a template).
pub(crate) fn insert_built_page(s: &mut Session, section_id: Option<notecraft_doc::Guid>, mut page: Page, sub: bool) -> CmdResult {
    let cur_page = s.current.page;
    let sec_id = section_id.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
    let sec = s.section_mut(sec_id).ok_or_else(|| CmdError::Params(format!("no section {sec_id}")))?;
    let at = match cur_page.and_then(|id| sec.page_index(id)) {
        Some(i) => {
            if sub {
                page.set_level(sec.pages.get(i).map_or(0, |p| p.level + 1));
            }
            sec.page_family(i).end
        }
        None => sec.pages.len(),
    };
    let id = page.id;
    sec.pages.insert(at.min(sec.pages.len()), page);
    normalize_levels(sec);
    s.current.section = Some(sec_id);
    s.current.page = Some(id);
    Ok(CmdOutput::value(json!({"id": id})))
}

fn new(s: &mut Session, params: &Value) -> CmdResult {
    let title = p::str(params, "title").map(clean_title);
    let section = p::guid(params, "section")?;
    // The section's default template (Insert › Page Templates), unless asked for a blank page.
    let template = match p::bool(params, "blank") {
        Some(true) => None,
        _ => section
            .or(s.current.section)
            .and_then(|id| s.notebooks.iter().find_map(|n| n.section(id)))
            .and_then(|sec| sec.default_template.clone())
            .and_then(|id| super::template::find(s, &id)),
    };
    match template {
        Some(t) => {
            let mut page = t.instantiate();
            if let Some(title) = title {
                page.title = title;
            }
            insert_built_page(s, section, page, false)
        }
        None => insert_page(s, section, title.unwrap_or_default(), false),
    }
}

fn new_subpage(s: &mut Session, params: &Value) -> CmdResult {
    let title = clean_title(p::str(params, "title").unwrap_or(""));
    insert_page(s, None, title, true)
}

/// Deletes one page (its subpages stay, one level up where needed) into the notebook's recycle
/// bin (History ▸ Notebook Recycle Bin).
fn delete(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.page, "page")?;
    let now = s.now();
    let nb = s.notebooks.iter_mut().find(|n| n.find_page(id).is_some()).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let sec = nb.all_sections_mut().into_iter().find(|x| x.page(id).is_some()).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let i = sec.page_index(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let page = sec.pages.remove(i);
    normalize_levels(sec);
    let deleted = DeletedPage { page, deleted: now, section: sec.id, section_name: sec.name.clone(), index: i };
    let next = sec.pages.get(i).or_else(|| i.checked_sub(1).and_then(|j| sec.pages.get(j))).map(|p| p.id);
    nb.recycle_bin.pages.push(deleted);
    nb.recycle_bin.cap();
    if s.current.page == Some(id) {
        s.current.page = next;
    }
    Ok(CmdOutput::ok())
}

fn rename(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.page, "page")?;
    let title = clean_title(p::req_str(params, "title")?);
    let pg = s.page_mut(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    if pg.title == title {
        return Ok(CmdOutput::unchanged());
    }
    pg.title = title;
    pg.touch();
    Ok(CmdOutput::ok())
}

/// Moves a page and its subpages to `index` (in the list without them) of `section` (default:
/// its own).
fn move_page(s: &mut Session, params: &Value) -> CmdResult {
    let id = id_or(params, s.current.page, "page")?;
    let index = p::index(params, "index").ok_or_else(|| CmdError::Params("`index` (number) is required".into()))?;
    let from = s.notebooks.iter().find_map(|n| n.section_of_page(id)).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let to = p::guid(params, "section")?.unwrap_or(from);
    if s.notebooks.iter().all(|n| n.section(to).is_none()) {
        return Err(CmdError::Params(format!("no section {to}")));
    }
    let src = s.section_mut(from).ok_or_else(|| CmdError::Params(format!("no section {from}")))?;
    let i = src.page_index(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let family: Vec<Page> = src.pages.drain(src.page_family(i)).collect();
    normalize_levels(src);
    let dst = s.section_mut(to).ok_or_else(|| CmdError::Params(format!("no section {to}")))?;
    let at = index.min(dst.pages.len());
    for (k, pg) in family.into_iter().enumerate() {
        dst.pages.insert(at + k, pg);
    }
    normalize_levels(dst);
    if s.current.page == Some(id) {
        s.current.section = Some(to);
        if let Some(nb) = s.notebooks.iter().find(|n| n.section(to).is_some()) {
            s.current.notebook = Some(nb.id);
        }
    }
    Ok(CmdOutput::ok())
}

fn shift_level(s: &mut Session, params: &Value, deeper: bool) -> CmdResult {
    let id = id_or(params, s.current.page, "page")?;
    let sec = s.notebooks.iter_mut().find_map(|n| n.all_sections_mut().into_iter().find(|x| x.page(id).is_some()));
    let sec = sec.ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let i = sec.page_index(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let range = sec.page_family(i);
    let level = sec.pages.get(i).map_or(0, |p| p.level);
    if deeper {
        let prev = i.checked_sub(1).and_then(|j| sec.pages.get(j)).map(|p| p.level);
        if level >= MAX_PAGE_LEVEL || prev.is_none_or(|pl| pl < level) {
            return Err(CmdError::Disabled("this page can't be made a subpage".into()));
        }
    } else if level == 0 {
        return Ok(CmdOutput::unchanged());
    }
    for pg in sec.pages.get_mut(range).into_iter().flatten() {
        let l = if deeper { pg.level.saturating_add(1) } else { pg.level.saturating_sub(1) };
        pg.set_level(l);
    }
    normalize_levels(sec);
    Ok(CmdOutput::ok())
}

fn indent(s: &mut Session, params: &Value) -> CmdResult {
    shift_level(s, params, true)
}

fn outdent(s: &mut Session, params: &Value) -> CmdResult {
    shift_level(s, params, false)
}
