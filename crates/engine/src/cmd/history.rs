//! History tab: the notebook recycle bin, page versions, unread pages and recent edits.
//!
//! - **Recycle bin**: `page.delete` and `section.delete` move what they delete into the
//!   notebook's [`notecraft_doc::RecycleBin`] (undo brings it straight back). From there
//!   `history.restore` puts it back where it was (or into a chosen section / group),
//!   `history.deleteForever` and `history.emptyRecycleBin` remove it for good. Entries older than
//!   60 days go when the notebook is opened.
//! - **Page versions**: the session keeps them automatically while pages are edited
//!   ([`crate::Session`]); `history.viewVersion` opens one read-only, `history.restoreVersion`
//!   brings it back (the current page becomes a version).
//! - **Unread**: [`crate::unread`]. These commands change the user's read marks, not the
//!   notebooks, so they don't touch undo.

use chrono::{DateTime, Duration, FixedOffset, Offset, TimeZone, Utc};
use notecraft_doc::{DeletedPage, DeletedSection, Guid, Notebook, PageVersion, SectionEntry};
use serde_json::{Value, json};

use super::page::normalize_levels;
use super::{has_notebook, has_page};
use crate::ids::{
    HISTORY_DELETE_ALL_VERSIONS, HISTORY_DELETE_FOREVER, HISTORY_DELETE_VERSION, HISTORY_DISABLE, HISTORY_EMPTY_RECYCLE_BIN, HISTORY_MARK_READ,
    HISTORY_MARK_UNREAD, HISTORY_NEXT_UNREAD, HISTORY_RECENT_EDITS, HISTORY_RECYCLE_BIN, HISTORY_RESTORE, HISTORY_RESTORE_VERSION,
    HISTORY_SHOW_VERSIONS, HISTORY_TOGGLE_READ, HISTORY_VERSIONS, HISTORY_VIEW_VERSION,
};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Session, p};

/// Most rows `history.recentEdits` returns.
pub const MAX_RECENT_EDITS: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(HISTORY_RECYCLE_BIN, "Notebook Recycle Bin", recycle_bin).pure().when(has_notebook).params(r##"{"notebook"?: guid}"##),
        CommandSpec::new(HISTORY_RESTORE, "Restore", restore).when(has_notebook).params(r##"{"id": guid, "to"?: guid}"##),
        CommandSpec::new(HISTORY_DELETE_FOREVER, "Delete Forever", delete_forever).when(has_notebook).params(r##"{"id": guid}"##),
        CommandSpec::new(HISTORY_EMPTY_RECYCLE_BIN, "Empty Recycle Bin", empty_bin).when(has_notebook).params(r##"{"notebook"?: guid}"##),
        CommandSpec::new(HISTORY_VERSIONS, "Page Versions", versions).pure().when(has_page).params(r##"{"page"?: guid}"##),
        CommandSpec::new(HISTORY_VIEW_VERSION, "View Page Version", view_version)
            .pure()
            .when(has_page)
            .params(r##"{"page"?: guid, "version": guid | null}"##),
        CommandSpec::new(HISTORY_RESTORE_VERSION, "Restore Version", restore_version).when(has_page).params(r##"{"page"?: guid, "version"?: guid}"##),
        CommandSpec::new(HISTORY_DELETE_VERSION, "Delete Version", delete_version).when(has_page).params(r##"{"page"?: guid, "version"?: guid}"##),
        CommandSpec::new(HISTORY_DELETE_ALL_VERSIONS, "Delete All Versions", delete_all_versions)
            .when(has_notebook)
            .params(r##"{"scope"?: "page"|"section"|"notebook", "id"?: guid}"##),
        CommandSpec::new(HISTORY_SHOW_VERSIONS, "Show Page Versions", show_versions).pure().params(r##"{"value"?: bool}"##),
        CommandSpec::new(HISTORY_DISABLE, "Disable History for this Notebook", disable_history)
            .when(has_notebook)
            .params(r##"{"notebook"?: guid, "value"?: bool}"##),
        CommandSpec::new(HISTORY_MARK_READ, "Mark as Read", mark_read)
            .pure()
            .when(has_notebook)
            .params(r##"{"scope"?: "page"|"section"|"notebook", "id"?: guid}"##),
        CommandSpec::new(HISTORY_MARK_UNREAD, "Mark as Unread", mark_unread)
            .pure()
            .when(has_notebook)
            .params(r##"{"scope"?: "page"|"section"|"notebook", "id"?: guid}"##),
        CommandSpec::new(HISTORY_TOGGLE_READ, "Mark as Read", toggle_read).pure().when(has_page).params(r##"{"page"?: guid}"##),
        CommandSpec::new(HISTORY_NEXT_UNREAD, "Next Unread", next_unread).pure().when(has_notebook),
        CommandSpec::new(HISTORY_RECENT_EDITS, "Recent Edits", recent_edits)
            .pure()
            .params(r##"{"since"?: "today"|"yesterday"|"7d"|"14d"|"30d"|"6m"|"all", "utcOffsetMinutes"?: number}"##),
    ]
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339()
}

/// `params.notebook`, else the current notebook.
fn notebook_id(s: &Session, params: &Value) -> Result<Guid, CmdError> {
    let id = p::guid(params, "notebook")?.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
    s.notebook(id).map(|n| n.id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))
}

// --- recycle bin ---------------------------------------------------------------------------

fn recycle_bin(s: &mut Session, params: &Value) -> CmdResult {
    let id = notebook_id(s, params)?;
    let nb = s.notebook(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    let bin = &nb.recycle_bin;
    let sections: Vec<Value> = bin
        .sections
        .iter()
        .rev()
        .map(|d| {
            let pages: Vec<Value> = d.section.pages.iter().map(|p| json!({"id": p.id, "title": p.display_title(), "level": p.level})).collect();
            json!({"id": d.section.id, "name": d.section.name, "color": d.section.color.to_hex(), "deleted": ts(d.deleted), "pages": pages})
        })
        .collect();
    let pages: Vec<Value> = bin
        .pages
        .iter()
        .rev()
        .map(|d| {
            json!({"id": d.page.id, "title": d.page.display_title(), "deleted": ts(d.deleted), "section": d.section, "sectionName": d.section_name})
        })
        .collect();
    Ok(CmdOutput::query(json!({"notebook": id, "sections": sections, "pages": pages})))
}

/// The notebook whose recycle bin holds `id` (a deleted page or section, or a page of a
/// deleted section).
fn bin_owner(s: &Session, id: Guid) -> Option<Guid> {
    s.notebooks.iter().find(|n| n.recycle_bin.contains(id) || n.recycle_bin.sections.iter().any(|d| d.section.page(id).is_some())).map(|n| n.id)
}

fn restore(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let to = p::guid(params, "to")?;
    let nb_id = bin_owner(s, id).ok_or_else(|| CmdError::Params(format!("{id} is not in a recycle bin")))?;
    let nb = s.notebook_mut(nb_id).ok_or_else(|| CmdError::Params(format!("no notebook {nb_id}")))?;
    let location = if let Some(i) = nb.recycle_bin.pages.iter().position(|d| d.page.id == id) {
        let d = nb.recycle_bin.pages.remove(i);
        restore_page(nb, d, to)?
    } else if let Some(i) = nb.recycle_bin.sections.iter().position(|d| d.section.id == id) {
        let d = nb.recycle_bin.sections.remove(i);
        restore_section(nb, d, to)?
    } else {
        // A page inside a deleted section: restored on its own.
        let (si, pi) = nb
            .recycle_bin
            .sections
            .iter()
            .enumerate()
            .find_map(|(si, d)| d.section.page_index(id).map(|pi| (si, pi)))
            .ok_or_else(|| CmdError::Params(format!("{id} is not in a recycle bin")))?;
        let d = nb.recycle_bin.sections.get_mut(si).ok_or_else(|| CmdError::Failed("recycle bin changed".into()))?;
        let page = d.section.pages.remove(pi);
        normalize_levels(&mut d.section);
        let dp = DeletedPage { page, deleted: d.deleted, section: d.section.id, section_name: d.section.name.clone(), index: pi };
        restore_page(nb, dp, to)?
    };
    s.current = location;
    s.viewing_version = None;
    Ok(CmdOutput::value(json!({"notebook": location.notebook, "section": location.section, "page": location.page})))
}

/// Puts a deleted page back into `to`, else the section it came from, else the first section
/// (a new "Restored Pages" section when the notebook has none).
fn restore_page(nb: &mut Notebook, d: DeletedPage, to: Option<Guid>) -> Result<Location, CmdError> {
    if let Some(t) = to
        && nb.section(t).is_none()
    {
        return Err(CmdError::Params(format!("no section {t} in this notebook")));
    }
    let target = to.or_else(|| nb.section(d.section).map(|s| s.id)).or_else(|| nb.first_section().map(|s| s.id));
    let target = match target {
        Some(t) => t,
        None => {
            let sec = notecraft_doc::Section::new("Restored Pages", notecraft_doc::Color::section_color(nb.all_sections().len()));
            let id = sec.id;
            nb.sections.push(SectionEntry::Section(sec));
            id
        }
    };
    let sec = nb.section_mut(target).ok_or_else(|| CmdError::Params(format!("no section {target}")))?;
    let mut page = d.page;
    if sec.page(page.id).is_some() {
        page.id = Guid::new();
    }
    let pid = page.id;
    let at = if to.is_some() && to != Some(d.section) { sec.pages.len() } else { d.index.min(sec.pages.len()) };
    sec.pages.insert(at, page);
    normalize_levels(sec);
    Ok(Location { notebook: Some(nb.id), section: Some(target), page: Some(pid) })
}

/// Puts a deleted section back into `to` (a section group or the notebook itself), else the
/// group it came from (as far down that path as still exists).
fn restore_section(nb: &mut Notebook, d: DeletedSection, to: Option<Guid>) -> Result<Location, CmdError> {
    let group = match to {
        Some(t) if t == nb.id => None,
        Some(t) if nb.group(t).is_some() => Some(t),
        Some(t) => return Err(CmdError::Params(format!("no section group {t} in this notebook"))),
        None => d.group_path.iter().rev().find(|g| nb.group(**g).is_some()).copied(),
    };
    let mut section = d.section;
    // Back in the notebook it is named and filed by its name again.
    section.source.file_name = None;
    if nb.section(section.id).is_some() || nb.group(section.id).is_some() {
        section.id = Guid::new();
    }
    let sid = section.id;
    let first_page = section.pages.first().map(|p| p.id);
    let exact = to.is_none() && group == d.group_path.last().copied();
    let entries = nb.entries_mut(group).ok_or_else(|| CmdError::Params("the section group is gone".into()))?;
    let at = if exact { d.index.min(entries.len()) } else { entries.len() };
    entries.insert(at, SectionEntry::Section(section));
    Ok(Location { notebook: Some(nb.id), section: Some(sid), page: first_page })
}

fn delete_forever(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let nb_id = bin_owner(s, id).ok_or_else(|| CmdError::Params(format!("{id} is not in a recycle bin")))?;
    let nb = s.notebook_mut(nb_id).ok_or_else(|| CmdError::Params(format!("no notebook {nb_id}")))?;
    let bin = &mut nb.recycle_bin;
    let before = bin.len();
    bin.pages.retain(|d| d.page.id != id);
    bin.sections.retain(|d| d.section.id != id);
    if bin.len() == before {
        for d in &mut bin.sections {
            if let Some(i) = d.section.page_index(id) {
                d.section.pages.remove(i);
                normalize_levels(&mut d.section);
            }
        }
    }
    Ok(CmdOutput::ok())
}

fn empty_bin(s: &mut Session, params: &Value) -> CmdResult {
    let id = notebook_id(s, params)?;
    let nb = s.notebook_mut(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    if nb.recycle_bin.is_empty() {
        return Ok(CmdOutput::unchanged());
    }
    nb.recycle_bin = Default::default();
    Ok(CmdOutput::ok())
}

// --- page versions -------------------------------------------------------------------------

/// `params.page`, else the current page.
fn page_id(s: &Session, params: &Value) -> Result<Guid, CmdError> {
    let id = p::guid(params, "page")?.or(s.current.page).ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
    if s.notebooks.iter().any(|n| n.find_page(id).is_some()) { Ok(id) } else { Err(CmdError::Params(format!("no page {id}"))) }
}

/// `params.version`, else the version open in the canvas (for that page).
fn version_id(s: &Session, params: &Value, page: Guid) -> Result<Guid, CmdError> {
    p::guid(params, "version")?
        .or(s.viewing_version.filter(|(p, _)| *p == page).map(|(_, v)| v))
        .ok_or_else(|| CmdError::Params("`version` (GUID) is required".into()))
}

fn versions(s: &mut Session, params: &Value) -> CmdResult {
    let id = page_id(s, params)?;
    let page = s.notebooks.iter().find_map(|n| n.find_page(id)).map(|(_, p)| p).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let list: Vec<Value> = page.versions.iter().rev().map(|v| json!({"id": v.id, "saved": ts(v.saved), "title": v.page.display_title()})).collect();
    Ok(CmdOutput::query(Value::Array(list)))
}

fn view_version(s: &mut Session, params: &Value) -> CmdResult {
    let id = page_id(s, params)?;
    let Some(version) = p::guid(params, "version")? else {
        s.viewing_version = None;
        return Ok(CmdOutput::unchanged());
    };
    let (nb, sec, page) = s
        .notebooks
        .iter()
        .find_map(|n| n.find_page(id).map(|(sec, p)| (n.id, sec.id, p)))
        .ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    if !page.versions.iter().any(|v| v.id == version) {
        return Err(CmdError::Params(format!("no version {version}")));
    }
    s.current = Location { notebook: Some(nb), section: Some(sec), page: Some(id) };
    s.viewing_version = Some((id, version));
    Ok(CmdOutput::unchanged())
}

fn restore_version(s: &mut Session, params: &Value) -> CmdResult {
    let id = page_id(s, params)?;
    let version = version_id(s, params, id)?;
    let now = s.now();
    let page = s.page_mut(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let i = page.versions.iter().position(|v| v.id == version).ok_or_else(|| CmdError::Params(format!("no version {version}")))?;
    let v = page.versions.remove(i);
    // The page as it is now becomes a version.
    let current = PageVersion::of(page);
    let old = (*v.page).clone();
    page.title = old.title;
    page.background = old.background;
    page.elements = old.elements;
    page.ink = old.ink;
    page.modified = now;
    if !crate::session::same_content(&current.page, page) {
        page.versions.push(current);
    }
    notecraft_doc::history::trim_versions(&mut page.versions);
    s.viewing_version = None;
    Ok(CmdOutput::ok())
}

fn delete_version(s: &mut Session, params: &Value) -> CmdResult {
    let id = page_id(s, params)?;
    let version = version_id(s, params, id)?;
    let page = s.page_mut(id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    let i = page.versions.iter().position(|v| v.id == version).ok_or_else(|| CmdError::Params(format!("no version {version}")))?;
    page.versions.remove(i);
    if s.viewing_version == Some((id, version)) {
        s.viewing_version = None;
    }
    Ok(CmdOutput::ok())
}

fn delete_all_versions(s: &mut Session, params: &Value) -> CmdResult {
    let scope = p::str(params, "scope").unwrap_or("section");
    let id = p::guid(params, "id")?;
    let mut n = 0usize;
    let mut clear = |pages: &mut Vec<notecraft_doc::Page>| {
        for p in pages {
            n += p.versions.len();
            p.versions.clear();
        }
    };
    match scope {
        "page" => {
            let pid = id.or(s.current.page).ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
            let page = s.page_mut(pid).ok_or_else(|| CmdError::Params(format!("no page {pid}")))?;
            n = page.versions.len();
            page.versions.clear();
        }
        "section" => {
            let sid = id.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
            let sec = s.section_mut(sid).ok_or_else(|| CmdError::Params(format!("no section {sid}")))?;
            clear(&mut sec.pages);
        }
        "notebook" => {
            let nid = id.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
            let nb = s.notebook_mut(nid).ok_or_else(|| CmdError::Params(format!("no notebook {nid}")))?;
            for sec in nb.all_sections_mut() {
                clear(&mut sec.pages);
            }
        }
        other => return Err(CmdError::Params(format!("unknown `scope` `{}`", other.chars().take(20).collect::<String>()))),
    }
    s.viewing_version = None;
    if n == 0 { Ok(CmdOutput::unchanged()) } else { Ok(CmdOutput::value(json!({"deleted": n}))) }
}

fn show_versions(s: &mut Session, params: &Value) -> CmdResult {
    let value = match params.get("value") {
        None | Some(Value::Null) => !s.show_versions,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(CmdError::Params("`value` must be true or false".into())),
    };
    s.show_versions = value;
    if !value {
        s.viewing_version = None;
    }
    s.revision += 1;
    Ok(CmdOutput::query(json!({"value": value})))
}

fn disable_history(s: &mut Session, params: &Value) -> CmdResult {
    let id = notebook_id(s, params)?;
    let nb = s.notebook_mut(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    let value = match params.get("value") {
        None | Some(Value::Null) => !nb.history_disabled,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(CmdError::Params("`value` must be true or false".into())),
    };
    if nb.history_disabled == value {
        return Ok(CmdOutput::unchanged());
    }
    nb.history_disabled = value;
    Ok(CmdOutput::value(json!({"value": value})))
}

// --- unread --------------------------------------------------------------------------------

/// The (notebook, pages) a read-mark command applies to.
fn scope_pages(s: &Session, params: &Value) -> Result<Vec<(Guid, notecraft_doc::Page)>, CmdError> {
    let scope = p::str(params, "scope").unwrap_or("page");
    let id = p::guid(params, "id")?;
    let mut out = Vec::new();
    match scope {
        "page" => {
            let pid = id.or(s.current.page).ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
            let (nb, page) = s
                .notebooks
                .iter()
                .find_map(|n| n.find_page(pid).map(|(_, p)| (n.id, p.clone())))
                .ok_or_else(|| CmdError::Params(format!("no page {pid}")))?;
            out.push((nb, page));
        }
        "section" => {
            let sid = id.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
            let nb = s.notebook_of_section(sid).ok_or_else(|| CmdError::Params(format!("no section {sid}")))?;
            let sec = nb.section(sid).ok_or_else(|| CmdError::Params(format!("no section {sid}")))?;
            out.extend(sec.pages.iter().map(|p| (nb.id, p.clone())));
        }
        "notebook" => {
            let nid = id.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
            let nb = s.notebook(nid).ok_or_else(|| CmdError::Params(format!("no notebook {nid}")))?;
            out.extend(nb.all_sections().iter().flat_map(|sec| sec.pages.iter().map(|p| (nb.id, p.clone()))));
        }
        other => return Err(CmdError::Params(format!("unknown `scope` `{}`", other.chars().take(20).collect::<String>()))),
    }
    Ok(out)
}

fn mark_read(s: &mut Session, params: &Value) -> CmdResult {
    for (_, p) in scope_pages(s, params)? {
        s.read_state.mark_read(&p);
    }
    s.revision += 1;
    Ok(CmdOutput::unchanged())
}

fn mark_unread(s: &mut Session, params: &Value) -> CmdResult {
    for (nb, p) in scope_pages(s, params)? {
        if let Some(n) = s.notebook(nb) {
            let n = n.clone();
            s.read_state.adopt(&n);
        }
        s.read_state.mark_unread(p.id);
    }
    s.revision += 1;
    Ok(CmdOutput::unchanged())
}

fn toggle_read(s: &mut Session, params: &Value) -> CmdResult {
    let id = page_id(s, params)?;
    let unread = s.is_unread(id);
    let params = json!({"scope": "page", "id": id});
    if unread {
        mark_read(s, &params)?
    } else {
        mark_unread(s, &params)?
    };
    Ok(CmdOutput::query(json!({"unread": !unread})))
}

/// Goes to the next unread page after the current one (all open notebooks, wrapping round).
fn next_unread(s: &mut Session, _: &Value) -> CmdResult {
    let rs = &s.read_state;
    let all: Vec<(Guid, Guid, Guid, bool)> = s
        .notebooks
        .iter()
        .flat_map(|n| n.all_sections().into_iter().flat_map(move |sec| sec.pages.iter().map(move |p| (n.id, sec.id, p.id, rs.is_unread(n.id, p)))))
        .collect();
    let here = s.current.page.and_then(|c| all.iter().position(|(_, _, p, _)| *p == c)).map_or(0, |i| i + 1);
    let n = all.len();
    let found = (0..n).filter_map(|k| all.get((here + k) % n.max(1))).find(|(_, _, p, unread)| *unread && Some(*p) != s.current.page).copied();
    match found {
        Some((nb, sec, page, _)) => {
            s.current = Location { notebook: Some(nb), section: Some(sec), page: Some(page) };
            s.viewing_version = None;
            Ok(CmdOutput::query(json!({"page": page})))
        }
        None => Ok(CmdOutput::query(Value::Null)),
    }
}

// --- recent edits --------------------------------------------------------------------------

/// The earliest change `since` covers, in the user's time zone; `None` for everything.
pub fn since_cutoff(since: &str, now: DateTime<Utc>, offset_minutes: i32) -> Result<Option<DateTime<Utc>>, CmdError> {
    let offset = FixedOffset::east_opt(offset_minutes.clamp(-14 * 60, 14 * 60) * 60).unwrap_or_else(|| Utc.fix());
    let local = now.with_timezone(&offset);
    let midnight = local.date_naive().and_hms_opt(0, 0, 0).and_then(|m| offset.from_local_datetime(&m).single()).map(|m| m.with_timezone(&Utc));
    let days = |d: i64| now.checked_sub_signed(Duration::days(d));
    Ok(match since {
        "today" => midnight,
        "yesterday" => midnight.and_then(|m| m.checked_sub_signed(Duration::days(1))),
        "7d" => days(7),
        "14d" => days(14),
        "30d" => days(30),
        "6m" => days(183),
        "all" => None,
        other => return Err(CmdError::Params(format!("unknown `since` `{}`", other.chars().take(20).collect::<String>()))),
    })
}

fn recent_edits(s: &mut Session, params: &Value) -> CmdResult {
    let since = p::str(params, "since").unwrap_or("7d");
    let offset = p::f32(params, "utcOffsetMinutes").unwrap_or(0.0).clamp(-14.0 * 60.0, 14.0 * 60.0) as i32;
    let cutoff = since_cutoff(since, s.now(), offset)?;
    let mut rows: Vec<(DateTime<Utc>, Value)> = Vec::new();
    for nb in &s.notebooks {
        for sec in nb.all_sections() {
            for p in &sec.pages {
                if cutoff.is_some_and(|c| p.modified < c) {
                    continue;
                }
                rows.push((
                    p.modified,
                    json!({"page": p.id, "title": p.display_title(), "section": sec.id, "sectionName": sec.name, "notebook": nb.id,
                           "notebookName": nb.name, "modified": ts(p.modified), "unread": s.read_state.is_unread(nb.id, p)}),
                ));
            }
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    rows.truncate(MAX_RECENT_EDITS);
    Ok(CmdOutput::query(Value::Array(rows.into_iter().map(|(_, v)| v).collect())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoffs_follow_the_local_day() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap(); // 2027-01-15 08:00 UTC
        let today = since_cutoff("today", now, 0).unwrap().unwrap();
        assert_eq!(today.timestamp() % 86_400, 0);
        assert!(today <= now && now.signed_duration_since(today) < Duration::days(1));
        let west = since_cutoff("today", now, -10 * 60).unwrap().unwrap();
        assert_ne!(west, today, "the day starts later west of Greenwich");
        assert_eq!(since_cutoff("yesterday", now, 0).unwrap().unwrap(), today - Duration::days(1));
        assert_eq!(since_cutoff("7d", now, 0).unwrap().unwrap(), now - Duration::days(7));
        assert_eq!(since_cutoff("all", now, 0).unwrap(), None);
        assert!(since_cutoff("fortnight", now, 0).is_err());
    }
}
