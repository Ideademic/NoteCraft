//! History tab: the recycle bin, page versions, unread pages and recent edits, through commands
//! and through the real storage paths (`.one` folders, `.onepkg`, JSON).

use chrono::{DateTime, Duration, Utc};
use notecraft_doc::{Guid, Notebook, SectionEntry, SectionGroup};
use serde_json::{Value, json};

use crate::io::safe_save::tests::temp_dir;
use crate::io::{FolderStorage, MemoryFormat, MemoryStorage, Origin};
use crate::{Session, ids};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap()
}

fn gid(v: &Value, k: &str) -> Guid {
    Guid::parse(v[k].as_str().unwrap()).unwrap()
}

/// A session at a fixed time, on the welcome page.
fn session() -> Session {
    let mut s = Session::new_default();
    s.clock = Some(at(1_800_000_000));
    s
}

fn bin_len(s: &Session) -> usize {
    s.notebooks[0].recycle_bin.len()
}

#[test]
fn deleted_pages_go_to_the_bin_and_come_back() {
    let mut s = session();
    let sec = s.current.section.unwrap();
    let a = gid(&s.run(ids::PAGE_NEW, &json!({"title": "A"})).unwrap().value, "id");
    let b = gid(&s.run(ids::PAGE_NEW, &json!({"title": "B"})).unwrap().value, "id");
    s.run(ids::PAGE_DELETE, &json!({"id": a.to_string()})).unwrap();
    assert!(s.current_section().unwrap().page(a).is_none());
    assert!(s.dirty_bins.contains(&s.notebooks[0].id), "the bin needs saving");
    let list = s.run(ids::HISTORY_RECYCLE_BIN, &Value::Null).unwrap().value;
    assert_eq!(list["pages"][0]["title"], "A");
    assert_eq!(list["pages"][0]["sectionName"], "Quick Notes");
    // Undo brings it straight back, redo deletes it again.
    assert!(s.undo());
    assert_eq!(bin_len(&s), 0);
    assert!(s.current_section().unwrap().page(a).is_some());
    assert!(s.redo());
    assert_eq!(bin_len(&s), 1);
    // Restore: back where it was (index 1, after the welcome page), and current.
    let out = s.run(ids::HISTORY_RESTORE, &json!({"id": a.to_string()})).unwrap().value;
    assert_eq!(gid(&out, "page"), a);
    assert_eq!(s.current.page, Some(a));
    let titles: Vec<&str> = s.current_section().unwrap().pages.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, vec!["Welcome to NoteCraft", "A", "B"]);
    assert_eq!(bin_len(&s), 0);
    // Restore into another section.
    let other = gid(&s.run(ids::SECTION_NEW, &json!({"name": "Other"})).unwrap().value, "id");
    s.run(ids::PAGE_DELETE, &json!({"id": b.to_string()})).unwrap();
    s.run(ids::HISTORY_RESTORE, &json!({"id": b.to_string(), "to": other.to_string()})).unwrap();
    assert!(s.notebooks[0].section(other).unwrap().page(b).is_some());
    assert!(s.notebooks[0].section(sec).unwrap().page(b).is_none());
    // Junk ids and targets are errors that change nothing.
    assert!(s.run(ids::HISTORY_RESTORE, &json!({"id": Guid::new().to_string()})).is_err());
    s.run(ids::PAGE_DELETE, &json!({"id": b.to_string()})).unwrap();
    assert!(s.run(ids::HISTORY_RESTORE, &json!({"id": b.to_string(), "to": Guid::new().to_string()})).is_err());
    assert_eq!(bin_len(&s), 1);
    // A page whose section is gone goes to the first section.
    s.run(ids::SECTION_DELETE, &json!({"id": other.to_string()})).unwrap();
    s.run(ids::HISTORY_DELETE_FOREVER, &json!({"id": other.to_string()})).unwrap();
    s.run(ids::HISTORY_RESTORE, &json!({"id": b.to_string()})).unwrap();
    assert_eq!(s.notebooks[0].section_of_page(b), Some(sec));
}

#[test]
fn deleted_sections_and_groups_restore_into_their_groups() {
    let mut s = session();
    let nb = s.current.notebook.unwrap();
    let g = gid(&s.run(ids::SECTION_GROUP_NEW, &json!({"name": "Work"})).unwrap().value, "id");
    let plans = gid(&s.run(ids::SECTION_NEW, &json!({"name": "Plans", "group": g.to_string()})).unwrap().value, "id");
    let notes = gid(&s.run(ids::SECTION_NEW, &json!({"name": "Notes", "group": g.to_string()})).unwrap().value, "id");
    s.run(ids::SECTION_DELETE, &json!({"id": plans.to_string()})).unwrap();
    let d = s.notebooks[0].recycle_bin.section(plans).unwrap();
    assert_eq!((d.group_path.clone(), d.index), (vec![g], 0));
    s.run(ids::HISTORY_RESTORE, &json!({"id": plans.to_string()})).unwrap();
    let names: Vec<&str> = s.notebooks[0].group(g).unwrap().entries.iter().map(SectionEntry::name).collect();
    assert_eq!(names, vec!["Plans", "Notes"], "back in its group, at its place");
    assert_eq!(s.current.section, Some(plans));
    // Deleting the group puts each section in the bin; with the group gone they restore at the top.
    s.run(ids::SECTION_DELETE, &json!({"id": g.to_string()})).unwrap();
    assert_eq!(bin_len(&s), 2);
    assert!(s.notebooks[0].group(g).is_none());
    s.run(ids::HISTORY_RESTORE, &json!({"id": notes.to_string()})).unwrap();
    assert!(matches!(s.notebooks[0].sections.last(), Some(SectionEntry::Section(x)) if x.id == notes));
    // …or into a chosen group (or the notebook itself).
    let g2 = gid(&s.run(ids::SECTION_GROUP_NEW, &json!({"name": "Archive"})).unwrap().value, "id");
    s.run(ids::HISTORY_RESTORE, &json!({"id": plans.to_string(), "to": g2.to_string()})).unwrap();
    assert!(s.notebooks[0].group(g2).unwrap().entries.iter().any(|e| e.id() == plans));
    // A page of a deleted section restores on its own.
    let quick = s.notebooks[0].first_section().unwrap().id;
    let welcome = s.notebooks[0].first_section().unwrap().pages[0].id;
    s.run(ids::SECTION_DELETE, &json!({"id": quick.to_string()})).unwrap();
    s.run(ids::HISTORY_RESTORE, &json!({"id": welcome.to_string()})).unwrap();
    assert!(s.notebooks[0].find_page(welcome).is_some());
    assert!(s.notebooks[0].recycle_bin.section(quick).unwrap().section.pages.is_empty());
    // Empty Recycle Bin: everything goes, undoably; an empty bin is a no-op.
    s.run(ids::HISTORY_EMPTY_RECYCLE_BIN, &json!({"notebook": nb.to_string()})).unwrap();
    assert_eq!(bin_len(&s), 0);
    assert!(!s.run(ids::HISTORY_EMPTY_RECYCLE_BIN, &Value::Null).unwrap().changed);
    assert!(s.undo());
    assert_eq!(bin_len(&s), 1);
}

#[test]
fn the_bin_expires_after_sixty_days_on_open() {
    let mut s = session();
    let p = gid(&s.run(ids::PAGE_NEW, &json!({"title": "Old"})).unwrap().value, "id");
    s.run(ids::PAGE_DELETE, &json!({"id": p.to_string()})).unwrap();
    let q = gid(&s.run(ids::PAGE_NEW, &json!({"title": "Recent"})).unwrap().value, "id");
    s.clock = Some(at(1_800_000_000) + Duration::days(30));
    s.run(ids::PAGE_DELETE, &json!({"id": q.to_string()})).unwrap();
    let nb = s.notebooks[0].clone();
    let mut later = Session::empty();
    later.clock = Some(at(1_800_000_000) + Duration::days(61));
    later.add_notebook(nb, Origin::Scratch);
    let bin = &later.notebooks[0].recycle_bin;
    assert_eq!(bin.pages.len(), 1);
    assert_eq!(bin.pages[0].page.id, q);
    assert!(later.dirty_bins.contains(&later.notebooks[0].id), "the expired files go at the next save");
}

#[test]
fn versions_are_kept_while_editing_and_restore() {
    let mut s = session();
    let page = s.current.page.unwrap();
    let original = s.current_page().unwrap().clone();
    s.run(ids::PAGE_RENAME, &json!({"title": "Edited once"})).unwrap();
    let v = &s.current_page().unwrap().versions;
    assert_eq!(v.len(), 1, "the first change keeps the page as it was");
    assert_eq!(v[0].page.title, original.title);
    // More edits within five minutes: no new version.
    s.clock = Some(at(1_800_000_000 + 60));
    s.run(ids::PAGE_RENAME, &json!({"title": "Edited twice"})).unwrap();
    s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 300, "text": "more"})).unwrap();
    assert_eq!(s.current_page().unwrap().versions.len(), 1);
    // Later: another version, of the page as it was then.
    s.clock = Some(at(1_800_000_000 + 400));
    s.run(ids::PAGE_RENAME, &json!({"title": "Edited later"})).unwrap();
    let vs = s.run(ids::HISTORY_VERSIONS, &Value::Null).unwrap().value;
    assert_eq!(vs.as_array().unwrap().len(), 2);
    assert_eq!(vs[0]["title"], "Edited twice", "newest first");
    let v1 = gid(&vs[1], "id");
    // Undo takes the version away with the edit.
    assert!(s.undo());
    assert_eq!(s.current_page().unwrap().versions.len(), 1);
    assert!(s.redo());
    // View one read-only, then restore it: the current page becomes a version.
    s.run(ids::HISTORY_VIEW_VERSION, &json!({"page": page.to_string(), "version": v1.to_string()})).unwrap();
    assert_eq!(s.viewing_version, Some((page, v1)));
    s.run(ids::HISTORY_RESTORE_VERSION, &Value::Null).unwrap();
    let p = s.current_page().unwrap();
    assert_eq!(p.title, original.title);
    assert_eq!(p.elements, original.elements);
    assert_eq!(p.versions.len(), 2);
    assert_eq!(p.versions.last().unwrap().page.title, "Edited later");
    assert!(p.versions.iter().all(|v| v.id != v1), "the restored version is the page now");
    assert_eq!(s.viewing_version, None);
    assert!(s.undo());
    assert_eq!(s.current_page().unwrap().title, "Edited later");
    assert!(s.redo());
    // Delete one, then all in the section.
    let last = s.current_page().unwrap().versions[0].id;
    s.run(ids::HISTORY_DELETE_VERSION, &json!({"version": last.to_string()})).unwrap();
    assert_eq!(s.current_page().unwrap().versions.len(), 1);
    assert!(s.run(ids::HISTORY_DELETE_VERSION, &json!({"version": Guid::new().to_string()})).is_err());
    let out = s.run(ids::HISTORY_DELETE_ALL_VERSIONS, &json!({"scope": "notebook"})).unwrap();
    assert_eq!(out.value["deleted"], 1);
    assert!(!s.run(ids::HISTORY_DELETE_ALL_VERSIONS, &json!({"scope": "section"})).unwrap().changed);
    assert!(s.run(ids::HISTORY_DELETE_ALL_VERSIONS, &json!({"scope": "galaxy"})).is_err());
}

#[test]
fn versions_follow_the_rules() {
    let mut s = session();
    // Navigation and versions of blank pages: none.
    let blank = gid(&s.run(ids::PAGE_NEW, &json!({})).unwrap().value, "id");
    s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "hello"})).unwrap();
    assert!(s.notebooks[0].find_page(blank).unwrap().1.versions.is_empty(), "a new, blank page has no earlier version");
    // Show / hide, view and leave.
    assert!(!s.show_versions);
    assert_eq!(s.run(ids::HISTORY_SHOW_VERSIONS, &Value::Null).unwrap().value["value"], true);
    assert!(s.show_versions);
    s.run(ids::HISTORY_SHOW_VERSIONS, &json!({"value": false})).unwrap();
    assert!(!s.show_versions);
    assert!(s.run(ids::HISTORY_SHOW_VERSIONS, &json!({"value": "yes"})).is_err());
    s.clock = Some(at(1_800_000_000 + 1000));
    s.run(ids::PAGE_RENAME, &json!({"title": "x"})).unwrap();
    let v = s.current_page().unwrap().versions[0].id;
    s.run(ids::HISTORY_VIEW_VERSION, &json!({"version": v.to_string()})).unwrap();
    assert!(s.viewing_version.is_some());
    let welcome = s.notebooks[0].first_section().unwrap().pages[0].id;
    s.run(ids::NAV_PAGE, &json!({"id": welcome.to_string()})).unwrap();
    assert_eq!(s.viewing_version, None, "going elsewhere closes the version");
    assert!(s.run(ids::HISTORY_VIEW_VERSION, &json!({"page": blank.to_string(), "version": Guid::new().to_string()})).is_err());
    // History disabled for the notebook: no versions; it is a table-of-contents change.
    s.dirty_tocs.clear();
    s.run(ids::HISTORY_DISABLE, &json!({"value": true})).unwrap();
    assert!(s.notebooks[0].history_disabled);
    assert!(s.dirty_tocs.contains(&s.notebooks[0].id));
    s.clock = Some(at(1_800_000_000 + 5000));
    s.run(ids::PAGE_RENAME, &json!({"title": "no history"})).unwrap();
    assert!(s.current_page().unwrap().versions.is_empty());
    assert!(!s.run(ids::HISTORY_DISABLE, &json!({"value": true})).unwrap().changed);
}

#[test]
fn bin_and_versions_round_trip_through_one_folders() {
    let mut s = session();
    let root = temp_dir("history-folder");
    s.storage = Some(Box::new(FolderStorage::new(root.clone())));
    s.run(ids::PAGE_RENAME, &json!({"title": "Welcome, edited"})).unwrap();
    let doomed = gid(&s.run(ids::PAGE_NEW, &json!({"title": "Doomed"})).unwrap().value, "id");
    s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "goodbye"})).unwrap();
    s.run(ids::PAGE_DELETE, &json!({"id": doomed.to_string()})).unwrap();
    let gone = gid(&s.run(ids::SECTION_NEW, &json!({"name": "Gone"})).unwrap().value, "id");
    s.run(ids::PAGE_NEW, &json!({"title": "In a deleted section"})).unwrap();
    s.run(ids::SECTION_DELETE, &json!({"id": gone.to_string()})).unwrap();
    s.save_dirty().unwrap();
    let Some(Origin::Folder(dir)) = s.origins.get(&s.notebooks[0].id).cloned() else { panic!("saved to a folder") };
    let bin = dir.join(crate::io::RECYCLE_BIN);
    assert!(bin.join("OneNote_DeletedPages.one").is_file());
    assert!(bin.join("Gone.one").is_file());
    assert!(!dir.join("Gone.one").exists());
    let back = crate::io::open_path(&dir).unwrap();
    assert_eq!(back.all_sections().len(), 1, "deleted sections stay in the bin");
    assert_eq!(back.recycle_bin.pages.len(), 1);
    let dp = &back.recycle_bin.pages[0];
    assert_eq!((dp.page.id, dp.page.title.as_str(), dp.section_name.as_str()), (doomed, "Doomed", "Quick Notes"));
    assert_eq!(dp.deleted, at(1_800_000_000));
    assert!(back.recycle_bin.sections[0].section.pages.iter().any(|p| p.title == "In a deleted section"));
    let welcome = &back.first_section().unwrap().pages[0];
    assert_eq!(welcome.versions.len(), 1);
    assert_eq!(welcome.versions[0].page.title, "Welcome to NoteCraft");
    // Restoring a section moves its file out of the bin; emptying the bin removes the rest.
    s.run(ids::HISTORY_RESTORE, &json!({"id": gone.to_string()})).unwrap();
    s.save_dirty().unwrap();
    assert!(dir.join("Gone.one").is_file());
    assert!(!bin.join("Gone.one").exists());
    s.run(ids::HISTORY_EMPTY_RECYCLE_BIN, &Value::Null).unwrap();
    s.save_dirty().unwrap();
    assert!(!bin.join("OneNote_DeletedPages.one").exists());
    let back = crate::io::open_path(&dir).unwrap();
    assert!(back.recycle_bin.is_empty());
    assert_eq!(back.all_sections().len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bin_and_versions_round_trip_through_packages_and_json() {
    for format in [MemoryFormat::Onepkg, MemoryFormat::Json] {
        let mut s = session();
        s.run(ids::PAGE_RENAME, &json!({"title": "Edited"})).unwrap();
        let p = gid(&s.run(ids::PAGE_NEW, &json!({"title": "Deleted"})).unwrap().value, "id");
        s.run(ids::PAGE_DELETE, &json!({"id": p.to_string()})).unwrap();
        s.storage = Some(Box::new(MemoryStorage { format, ..MemoryStorage::default() }));
        s.save_dirty().unwrap();
        let nb: &Notebook = &s.notebooks[0];
        let bytes = match format {
            MemoryFormat::Onepkg => notecraft_onepkg::write(nb).unwrap(),
            MemoryFormat::Json => serde_json::to_vec(nb).unwrap(),
        };
        let name = if format == MemoryFormat::Onepkg { "b.onepkg" } else { "b.json" };
        let back = crate::io::open_bytes(name, &bytes).unwrap();
        assert_eq!(back.recycle_bin.pages.len(), 1, "{format:?}");
        assert_eq!(back.recycle_bin.pages[0].page.title, "Deleted");
        assert_eq!(back.first_section().unwrap().pages[0].versions.len(), 1, "{format:?}");
        // Restoring works on what was read back.
        let mut again = Session::new(back, Origin::Scratch);
        again.run(ids::HISTORY_RESTORE, &json!({"id": p.to_string()})).unwrap();
        assert!(again.notebooks[0].find_page(p).is_some());
    }
}

#[test]
fn unread_pages_after_outside_changes() {
    let mut nb = Notebook::new_default();
    let mut g = SectionGroup::new("G");
    let mut sec = notecraft_doc::Section::new("Other", notecraft_doc::Color::WHITE);
    sec.pages.push(notecraft_doc::Page::new("Other page"));
    let other = sec.pages[0].id;
    g.entries.push(SectionEntry::Section(sec));
    nb.sections.push(SectionEntry::Group(g));
    let welcome = nb.first_section().unwrap().pages[0].id;
    let s = Session::new(nb.clone(), Origin::Scratch);
    assert!(!s.is_unread(welcome) && !s.is_unread(other), "a first open is all read");
    // The notebook changes elsewhere; opened again, the changed page is unread.
    let state = s.read_state.clone();
    let mut changed = nb.clone();
    let p = changed.page_mut(other).unwrap();
    p.title = "Changed elsewhere".into();
    p.modified += Duration::seconds(30);
    let mut s = Session::new(changed, Origin::Scratch);
    s.read_state = state;
    assert!(s.is_unread(other));
    assert!(!s.is_unread(welcome));
    // Next Unread goes there and marks it read.
    let out = s.run(ids::HISTORY_NEXT_UNREAD, &Value::Null).unwrap();
    assert_eq!(gid(&out.value, "page"), other);
    assert_eq!(s.current.page, Some(other));
    assert!(!s.is_unread(other));
    assert_eq!(s.run(ids::HISTORY_NEXT_UNREAD, &Value::Null).unwrap().value, Value::Null, "nothing else is unread");
    // Mark as Read / Unread, by page, section and notebook, and the toggle.
    let out = s.run(ids::HISTORY_TOGGLE_READ, &Value::Null).unwrap();
    assert_eq!(out.value["unread"], true);
    assert!(s.is_unread(other), "marked unread stays unread while shown");
    s.run(ids::HISTORY_TOGGLE_READ, &Value::Null).unwrap();
    assert!(!s.is_unread(other));
    s.run(ids::HISTORY_MARK_UNREAD, &json!({"scope": "notebook"})).unwrap();
    assert!(s.is_unread(welcome) && s.is_unread(other));
    let quick = s.notebooks[0].first_section().unwrap().id;
    s.run(ids::HISTORY_MARK_READ, &json!({"scope": "section", "id": quick.to_string()})).unwrap();
    assert!(!s.is_unread(welcome));
    s.run(ids::HISTORY_MARK_UNREAD, &json!({"scope": "page", "id": welcome.to_string()})).unwrap();
    assert!(s.is_unread(welcome));
    s.run(ids::HISTORY_MARK_READ, &json!({"scope": "notebook"})).unwrap();
    assert!(!s.is_unread(other));
    assert!(s.run(ids::HISTORY_MARK_READ, &json!({"scope": "shelf"})).is_err());
    // Editing a page in NoteCraft keeps it read.
    s.run(ids::PAGE_RENAME, &json!({"id": other.to_string(), "title": "mine"})).unwrap();
    assert!(!s.is_unread(other));
}

#[test]
fn recent_edits_by_period() {
    let mut s = session();
    let now = at(1_800_000_000);
    let ids_titles = [("Today", now - Duration::hours(1)), ("Last week", now - Duration::days(5)), ("Old", now - Duration::days(400))];
    for (title, when) in ids_titles {
        let id = gid(&s.run(ids::PAGE_NEW, &json!({"title": title})).unwrap().value, "id");
        s.page_mut(id).unwrap().modified = when;
    }
    let titles = |s: &mut Session, since: &str| -> Vec<String> {
        let v = s.run(ids::HISTORY_RECENT_EDITS, &json!({"since": since})).unwrap().value;
        v.as_array().unwrap().iter().map(|r| r["title"].as_str().unwrap().to_owned()).filter(|t| t != "Welcome to NoteCraft").collect()
    };
    assert_eq!(titles(&mut s, "today"), vec!["Today"]);
    assert_eq!(titles(&mut s, "7d"), vec!["Today", "Last week"]);
    assert_eq!(titles(&mut s, "all"), vec!["Today", "Last week", "Old"]);
    assert!(s.run(ids::HISTORY_RECENT_EDITS, &json!({"since": "eons"})).is_err());
    let row = &s.run(ids::HISTORY_RECENT_EDITS, &json!({"since": "today"})).unwrap().value[0];
    assert_eq!(row["sectionName"], "Quick Notes");
    assert!(row["page"].is_string() && row["modified"].is_string());
}
