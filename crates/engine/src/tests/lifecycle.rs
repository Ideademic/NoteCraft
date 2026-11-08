//! Session lifecycle: autosave through storages, folder saving through the session, crash
//! recovery, session blobs, recent notebooks.

use std::collections::HashSet;
use std::path::PathBuf;

use notecraft_doc::{Guid, Notebook};
use serde_json::json;

use crate::io::folder::tests::json_codec;
use crate::io::recovery::RECOVERY_INTERVAL_SECS;
use crate::io::safe_save::tests::temp_dir;
use crate::io::{IoError, MemoryFormat, MemoryStorage, Origin, SaveJob, SaveOutcome, Storage};
use crate::{AUTOSAVE_DELAY_SECS, AUTOSAVE_MAX_WAIT_SECS, Session, ids};

#[test]
fn autosave_debounces_with_memory_storage() {
    let mut s = Session::new_default();
    let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let w = written.clone();
    let mut m = MemoryStorage { format: MemoryFormat::Json, ..MemoryStorage::default() };
    m.on_write = Some(Box::new(move |name, bytes| {
        assert!(serde_json::from_slice::<Notebook>(bytes).is_ok());
        w.borrow_mut().push(name.to_owned());
    }));
    s.storage = Some(Box::new(m));
    s.run(ids::PAGE_RENAME, &json!({"title": "a"})).unwrap();
    assert!(s.tick(100.0).is_none());
    s.run(ids::PAGE_RENAME, &json!({"title": "ab"})).unwrap();
    assert!(s.tick(100.0 + AUTOSAVE_DELAY_SECS * 0.9).is_none(), "a new change restarts the delay");
    assert!(s.tick(100.0 + AUTOSAVE_DELAY_SECS * 1.5).is_none());
    assert_eq!(s.tick(100.0 + AUTOSAVE_DELAY_SECS * 2.5), Some(Ok(1)));
    assert_eq!(written.borrow().as_slice(), ["My Notebook.json"]);
    assert!(!s.is_dirty());
    assert!(s.tick(200.0).is_none(), "nothing more to save");
    // Typing without pause still saves within the maximum wait.
    let mut t = 300.0;
    let mut saves = 0;
    while t < 300.0 + AUTOSAVE_MAX_WAIT_SECS * 2.0 {
        s.run(ids::PAGE_RENAME, &json!({"title": format!("{t}")})).unwrap();
        saves += usize::from(s.tick(t).is_some());
        t += AUTOSAVE_DELAY_SECS / 2.0;
    }
    assert!(saves >= 1, "{saves}");
    // A JSON-saved notebook opens again.
    let mut m = MemoryStorage { format: MemoryFormat::Json, ..MemoryStorage::default() };
    let nb = s.notebooks[0].clone();
    m.save(&nb, &Origin::Scratch, &HashSet::new(), true).unwrap();
    let bytes = m.files.get("My Notebook.json").unwrap();
    assert_eq!(crate::io::open_bytes("My Notebook.json", bytes).unwrap(), nb);
}

/// Folder saving through the session, with JSON standing in for `.one`.
struct JsonFolder(PathBuf);

impl Storage for JsonFolder {
    fn save(&mut self, nb: &Notebook, origin: &Origin, dirty: &HashSet<Guid>, toc: bool) -> Result<Option<Origin>, IoError> {
        let job = SaveJob { notebook: nb, origin, dirty_sections: dirty, toc_dirty: toc, bin_dirty: true, previous: None };
        Ok(self.save_notebook(&job)?.origin)
    }
    fn save_notebook(&mut self, job: &SaveJob<'_>) -> Result<SaveOutcome, IoError> {
        crate::io::folder::save(job, &self.0, &json_codec())
    }
}

fn files(dir: &std::path::Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    out
}

#[test]
fn folder_saves_follow_renames_and_warn_once() {
    let root = temp_dir("session-folder");
    let mut nb = Notebook::new_default();
    if let Some(sec) = nb.all_sections_mut().into_iter().next() {
        sec.source.authored_by_notecraft = false;
        sec.source.unsupported_content = true;
    }
    let dir = root.join("Mine");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Quick Notes.one"), b"the original").unwrap();
    let mut s = Session::new(nb, Origin::Folder(dir.clone()));
    let opened_notices = s.notices.len();
    s.storage = Some(Box::new(JsonFolder(root.clone())));
    s.run(ids::PAGE_RENAME, &json!({"title": "changed"})).unwrap();
    assert_eq!(s.save_dirty(), Ok(1));
    assert_eq!(files(&dir), vec!["Open Notebook.onetoc2", "Quick Notes.one", "Quick Notes.one.bak"]);
    assert_eq!(std::fs::read(dir.join("Quick Notes.one.bak")).unwrap(), b"the original");
    assert_eq!(s.notices.len(), opened_notices + 1, "{:?}", s.notices);
    assert!(s.notices.last().unwrap().contains("backup"));
    s.run(ids::PAGE_RENAME, &json!({"title": "again"})).unwrap();
    s.save_dirty().unwrap();
    assert_eq!(s.notices.len(), opened_notices + 1, "warned once");
    // Renaming the section renames its file; the original goes to the recycle bin.
    s.run(ids::SECTION_RENAME, &json!({"name": "Inbox"})).unwrap();
    s.save_dirty().unwrap();
    assert!(dir.join("Inbox.one").is_file());
    assert!(!dir.join("Quick Notes.one").exists());
    assert!(dir.join(crate::io::RECYCLE_BIN).join("Quick Notes.one.bak").is_file(), "kept, as a backup");
    // A new notebook goes to its own folder under the root and into the recent list.
    s.run(ids::NOTEBOOK_NEW, &json!({"name": "Work"})).unwrap();
    s.save_dirty().unwrap();
    assert!(root.join("Work").join("New Section 1.one").is_file());
    assert_eq!(s.recent.entries.first().map(|e| e.name.as_str()), Some("Work"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn new_notebooks_use_the_default_location() {
    let root = temp_dir("default-loc");
    let mut s = Session::new_default();
    s.default_location = Some(root.clone());
    std::fs::create_dir_all(root.join("Plans")).unwrap();
    let out = s.run(ids::NOTEBOOK_NEW, &json!({"name": "Plans"})).unwrap();
    let id = Guid::parse(out.value["id"].as_str().unwrap()).unwrap();
    assert_eq!(s.origins.get(&id), Some(&Origin::Folder(root.join("Plans (2)"))), "an existing folder isn't reused");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn recovery_round_trip() {
    let dir = temp_dir("recovery");
    let mut s = Session::new_default();
    s.enable_recovery(dir.clone()).unwrap();
    assert!(s.recovery_candidates().is_empty());
    s.run(ids::PAGE_RENAME, &json!({"title": "Unsaved work"})).unwrap();
    s.tick(10.0);
    s.end_recovery();
    let nb_before = s.notebooks.clone();
    // A later change is snapshotted only after the interval.
    s.run(ids::PAGE_RENAME, &json!({"title": "Newer"})).unwrap();
    s.tick(10.0 + RECOVERY_INTERVAL_SECS / 2.0);
    s.end_recovery();
    // The "crash": a new session finds the snapshot (its own isn't listed).
    let mut next = Session::empty();
    next.enable_recovery(dir.clone()).unwrap();
    let found = next.recovery_candidates();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].notebooks, vec!["My Notebook".to_owned()]);
    assert!(s.recovery_candidates().is_empty(), "a session doesn't offer its own snapshot");
    let ids = next.restore_recovery(&found[0].path).unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(next.notebooks, nb_before);
    assert!(next.is_dirty(), "restored changes still need saving");
    assert_eq!(next.current_page().map(|p| p.title.as_str()), Some("Unsaved work"));
    assert!(!found[0].path.exists(), "the old snapshot is gone once restored");
    // Once everything is saved, the session's own snapshot goes too.
    next.dirty.clear();
    next.dirty_tocs.clear();
    next.tick(100.0);
    next.end_recovery();
    assert!(Session::empty().recovery_candidates_in(&dir).is_empty());
    // Discarding.
    let mut third = Session::new_default();
    third.enable_recovery(dir.clone()).unwrap();
    third.run(ids::PAGE_NEW, &json!({})).unwrap();
    third.write_recovery_now().unwrap();
    let left = Session::empty().recovery_candidates_in(&dir);
    assert_eq!(left.len(), 1);
    Session::discard_recovery(&left[0].path).unwrap();
    assert!(Session::empty().recovery_candidates_in(&dir).is_empty());
    assert!(Session::discard_recovery(&dir.join("notes.txt")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn session_blob_round_trip() {
    let mut s = Session::new_default();
    s.run(ids::NOTEBOOK_NEW, &json!({"name": "Second"})).unwrap();
    s.run(ids::DRAW_STROKE, &json!({"points": [[1, 2, 0.5], [3, 4]]})).unwrap();
    let blob = s.to_blob().unwrap();
    let back = Session::from_blob(&blob).unwrap();
    assert_eq!(back.notebooks, s.notebooks);
    assert_eq!(back.current, s.current);
    assert!(back.is_dirty());
    assert!(!back.can_undo());
    assert!(Session::from_blob(b"{}").is_err());
    assert!(Session::from_blob(br#"{"format":"other","version":1,"savedAt":"2020-01-01T00:00:00Z","notebooks":[]}"#).is_err());
    // Hostile values in a blob are bounded.
    let mut v: serde_json::Value = serde_json::from_slice(&blob).unwrap();
    v["notebooks"][0]["notebook"]["sections"][0]["pages"][0]["level"] = json!(200);
    let back = Session::from_blob(&serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(back.notebooks[0].first_section().unwrap().pages[0].level <= notecraft_doc::page::MAX_PAGE_LEVEL);
}
