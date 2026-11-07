mod commands;
mod lifecycle;
mod roundtrip;

use std::collections::HashSet;

use notecraft_doc::{Guid, Notebook, Paragraph, RuleLines};
use serde_json::{Value, json};

use crate::io::{IoError, Origin, Storage};
use crate::{CmdError, MAX_UNDO, Session, ids, registry};

fn id_of(v: &Value) -> Guid {
    Guid::parse(v["id"].as_str().unwrap()).unwrap()
}

#[test]
fn every_id_is_registered_once() {
    let reg = registry();
    let mut seen = HashSet::new();
    for spec in reg.all() {
        assert!(seen.insert(spec.id), "duplicate {}", spec.id);
    }
    for id in ids::ALL {
        assert!(reg.get(id).is_some(), "{id} not registered");
    }
    assert_eq!(reg.all().len(), ids::ALL.len());
    assert_eq!(reg.by_shortcut("mod+z").map(|s| s.id), Some(ids::EDIT_UNDO));
    assert_eq!(reg.by_shortcut("Mod+Shift+Z").map(|s| s.id), Some(ids::EDIT_REDO));
    assert!(reg.describe().as_array().is_some_and(|a| a.len() == ids::ALL.len()));
}

#[test]
fn hostile_params_never_panic() {
    let junk = [
        Value::Null,
        json!({}),
        json!([1, 2, 3]),
        json!("text"),
        json!({"id": "nope", "x": "a", "y": null, "points": [[1e40, "x"], {"x": 1}], "paragraphs": [{"runs": 5}], "index": -1}),
        json!({"id": Guid::new().to_string(), "x": f64::MAX, "y": -1e300, "index": u64::MAX, "name": "\u{0}\u{1}", "color": "#zzz", "points": [], "rule": "x"}),
        json!({"points": [[0, 0, 7], [1, 1]], "tool": "highlighter", "width": 1e30, "title": "x".repeat(5000)}),
        json!({"outline": Guid::new().to_string(), "start": {"para": u64::MAX, "char": -5}, "end": [1e300, "x"], "text": "a\nb\r\n", "style": {"size": "big", "color": 7}}),
        json!({"bytesBase64": "!!!not base64", "name": "../../etc/passwd", "path": "/definitely/not/here.png", "dpi": -1, "format": "%Q%", "what": "never"}),
        json!({"bytesBase64": "JVBERi0xLjcKZ2FyYmFnZQ==", "name": "x.pdf", "scope": "notebook", "query": "", "limit": 0, "radius": 1e9}),
        json!({"ids": ["x", 1], "dx": f64::NAN, "dy": 1, "color": "#fff", "paras": [9, -1], "para": 3, "start": 1e9, "end": 2, "unit": "word", "count": 1e12}),
        json!({"at": {"para": 0, "char": 999999}, "backward": false, "location": "\u{0}", "positioned": "yes", "checked": 2, "value": "no"}),
        json!({"kind": "axes3", "x": 1e30, "y": -1e30, "w": f64::MAX, "h": -1e300, "constrain": true, "width": -5, "color": "#00ff00"}),
        json!({"kind": "\u{0}ellipse", "x": 0, "y": 0, "w": 10, "h": 10, "constrain": "yes", "value": 1}),
        json!({"kind": 7, "x": "a", "w": null, "ids": [], "value": true}),
    ];
    for spec in registry().all() {
        // A notebook.open path that doesn't exist is fine to try; skip nothing.
        // `Session::run` turns a panic into this error; seeing it means a command panicked.
        let panicked = |r: &Result<crate::CmdOutput, CmdError>| matches!(r, Err(CmdError::Failed(m)) if m.starts_with("internal error"));
        for params in &junk {
            let mut s = Session::new_default();
            let r = s.run(spec.id, params);
            assert!(!panicked(&r), "{} panicked with {params}", spec.id);
            s.normalize_location();
        }
        let mut empty = Session::empty();
        for params in &junk {
            let r = empty.run(spec.id, params);
            assert!(!panicked(&r), "{} panicked with {params} on an empty session", spec.id);
        }
    }
}

#[test]
fn unknown_and_bad_params_report_errors() {
    let mut s = Session::new_default();
    assert!(matches!(s.run("no.such", &Value::Null), Err(CmdError::Unknown(_))));
    assert!(matches!(s.run(ids::FORMAT_BOLD, &json!({})), Err(CmdError::Params(_))));
    assert!(!s.can_undo());
    assert!(matches!(s.run(ids::EDIT_UNDO, &Value::Null), Err(CmdError::Disabled(_))));
}

#[test]
fn default_session_points_at_the_welcome_page() {
    let s = Session::new_default();
    assert_eq!(s.current_page().map(|p| p.title.as_str()), Some("Welcome to NoteCraft"));
    assert_eq!(s.current_section().map(|p| p.name.as_str()), Some("Quick Notes"));
    assert!(!s.is_dirty());
}

#[test]
fn page_crud_with_undo_redo_and_dirty() {
    let mut s = Session::new_default();
    let sec = s.current.section.unwrap();
    let out = s.run(ids::PAGE_NEW, &json!({"title": "Groceries"})).unwrap();
    let pid = id_of(&out.value);
    assert_eq!(s.current.page, Some(pid));
    assert!(s.dirty.contains(&sec));
    assert!(!s.dirty_tocs.is_empty() || s.dirty_tocs.is_empty()); // TOC unaffected by pages: either is allowed
    s.run(ids::PAGE_RENAME, &json!({"title": "Shopping"})).unwrap();
    assert_eq!(s.current_page().unwrap().title, "Shopping");
    let sub = id_of(&s.run(ids::PAGE_NEW_SUBPAGE, &json!({})).unwrap().value);
    assert_eq!(s.current_page().unwrap().level, 1);
    s.run(ids::PAGE_OUTDENT, &json!({"id": sub.to_string()})).unwrap();
    assert_eq!(s.current_page().unwrap().level, 0);
    s.run(ids::PAGE_INDENT, &json!({"id": sub.to_string()})).unwrap();
    assert_eq!(s.current_page().unwrap().level, 1);
    assert_eq!(s.current_section().unwrap().pages.len(), 3);
    s.run(ids::PAGE_DELETE, &json!({"id": pid.to_string()})).unwrap();
    // The subpage is promoted: no orphan levels.
    assert_eq!(s.current_section().unwrap().pages.iter().map(|p| p.level).collect::<Vec<_>>(), vec![0, 1]);
    assert!(s.undo());
    assert_eq!(s.current_section().unwrap().pages.len(), 3);
    assert!(s.redo());
    assert_eq!(s.current_section().unwrap().pages.len(), 2);
    s.run(ids::EDIT_UNDO, &Value::Null).unwrap();
    s.run(ids::EDIT_REDO, &Value::Null).unwrap();
    assert_eq!(s.current_section().unwrap().pages.len(), 2);
}

#[test]
fn sections_groups_and_moves() {
    let mut s = Session::new_default();
    let nb = s.current.notebook.unwrap();
    let first = s.current.section.unwrap();
    let out = s.run(ids::SECTION_NEW, &json!({"name": "Work"})).unwrap();
    let work = id_of(&out.value);
    assert_eq!(s.current.section, Some(work));
    assert!(s.dirty_tocs.contains(&nb));
    let g = id_of(&s.run(ids::SECTION_GROUP_NEW, &json!({"name": "Archive"})).unwrap().value);
    s.run(ids::SECTION_MOVE, &json!({"id": work.to_string(), "group": g.to_string(), "index": 0})).unwrap();
    assert_eq!(s.notebook(nb).unwrap().path_of(work), Some(vec![g]));
    // A group can't move into itself: the command fails and changes nothing.
    let before = s.notebooks.clone();
    assert!(s.run(ids::SECTION_MOVE, &json!({"id": g.to_string(), "group": g.to_string(), "index": 0})).is_err());
    assert_eq!(s.notebooks, before);
    s.run(ids::SECTION_COLOR, &json!({"id": first.to_string(), "color": "#112233"})).unwrap();
    s.run(ids::SECTION_RENAME, &json!({"id": g.to_string(), "name": "Old"})).unwrap();
    assert_eq!(s.notebook(nb).unwrap().group(g).unwrap().name, "Old");
    s.run(ids::NAV_SECTION, &json!({"id": work.to_string()})).unwrap();
    s.run(ids::SECTION_DELETE, &json!({"id": g.to_string()})).unwrap();
    assert_eq!(s.current.section, Some(first));
    assert!(s.current.page.is_some());
}

#[test]
fn notebooks_new_rename_close() {
    let mut s = Session::new_default();
    let out = s.run(ids::NOTEBOOK_NEW, &json!({"name": "Recipes"})).unwrap();
    let nb = id_of(&out.value);
    assert_eq!(s.notebooks.len(), 2);
    assert_eq!(s.current.notebook, Some(nb));
    assert!(s.current_page().is_some());
    s.run(ids::NOTEBOOK_RENAME, &json!({"name": "Cooking"})).unwrap();
    s.run(ids::NOTEBOOK_COLOR, &json!({"color": "#c44df0"})).unwrap();
    assert!(s.undo());
    assert!(s.undo());
    assert_eq!(s.notebook(nb).unwrap().name, "Recipes");
    s.run(ids::NOTEBOOK_CLOSE, &json!({"id": nb.to_string()})).unwrap();
    assert_eq!(s.notebooks.len(), 1);
    assert!(!s.can_undo(), "closing clears undo");
    assert_eq!(s.current_page().unwrap().title, "Welcome to NoteCraft");
}

#[test]
fn outlines_and_typing_merge() {
    let mut s = Session::new_default();
    let out = s.run(ids::OUTLINE_NEW, &json!({"x": 100, "y": 200, "text": "a"})).unwrap();
    let o = id_of(&out.value);
    let depth = s.undo_depth();
    for text in ["ab", "abc", "abcd"] {
        let paras = serde_json::to_value(vec![Paragraph::plain(text)]).unwrap();
        s.run(ids::TEXT_SET_PARAGRAPHS, &json!({"id": o.to_string(), "paragraphs": paras, "mergeKey": o.to_string()})).unwrap();
    }
    assert_eq!(s.undo_depth(), depth + 1, "typing merges into one step");
    assert_eq!(s.current_page().unwrap().outline(o).unwrap().text(), "abcd");
    assert!(s.undo());
    assert_eq!(s.current_page().unwrap().outline(o).unwrap().text(), "a");
    s.run(ids::OUTLINE_MOVE, &json!({"id": o.to_string(), "x": 10, "y": 20})).unwrap();
    s.run(ids::OUTLINE_RESIZE, &json!({"id": o.to_string(), "width": 1})).unwrap();
    let page = s.current_page().unwrap();
    let ol = page.outline(o).unwrap();
    assert_eq!((ol.pos.x, ol.pos.y, ol.width), (10.0, 20.0, crate::cmd::outline::MIN_OUTLINE_WIDTH));
    s.run(ids::OUTLINE_DELETE, &json!({"id": o.to_string()})).unwrap();
    assert!(s.current_page().unwrap().outline(o).is_none());
}

#[test]
fn ink_strokes() {
    let mut s = Session::new_default();
    let n = s.current_page().unwrap().ink.len();
    let out = s.run(ids::DRAW_STROKE, &json!({"points": [[0, 0, 0.3], {"x": 10, "y": 5}], "tool": "highlighter"})).unwrap();
    let id = id_of(&out.value);
    let page = s.current_page().unwrap();
    assert_eq!(page.ink.len(), n + 1);
    let st = page.stroke(id).unwrap();
    assert_eq!(st.color, notecraft_doc::InkTool::Highlighter.default_color());
    assert_eq!(st.points[0].pressure, 0.3);
    assert!(s.run(ids::DRAW_STROKE, &json!({"points": []})).is_err());
    s.run(ids::DRAW_ERASE_STROKE, &json!({"ids": [id.to_string()]})).unwrap();
    assert_eq!(s.current_page().unwrap().ink.len(), n);
    // Erasing something that isn't there changes nothing and records no step.
    let depth = s.undo_depth();
    s.run(ids::DRAW_ERASE_STROKE, &json!({"id": id.to_string()})).unwrap();
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn view_background() {
    let mut s = Session::new_default();
    s.run(ids::VIEW_RULE_LINES, &json!({"rule": "college"})).unwrap();
    s.run(ids::VIEW_PAGE_COLOR, &json!({"color": "#fff8e0"})).unwrap();
    let bg = s.current_page().unwrap().background;
    assert_eq!(bg.rule, RuleLines::College);
    assert!(bg.color.is_some());
    s.run(ids::VIEW_PAGE_COLOR, &json!({"color": null})).unwrap();
    assert_eq!(s.current_page().unwrap().background.color, None);
}

#[test]
fn navigation_is_not_undoable() {
    let mut s = Session::new_default();
    let first = s.current.page.unwrap();
    let second = id_of(&s.run(ids::PAGE_NEW, &json!({})).unwrap().value);
    let depth = s.undo_depth();
    s.run(ids::NAV_PAGE, &json!({"id": first.to_string()})).unwrap();
    assert_eq!(s.current.page, Some(first));
    assert_eq!(s.undo_depth(), depth);
    s.run(ids::NAV_PAGE, &json!({"id": second.to_string()})).unwrap();
    assert!(s.run(ids::NAV_PAGE, &json!({"id": Guid::new().to_string()})).is_err());
    assert_eq!(s.current.page, Some(second));
}

#[test]
fn undo_is_capped() {
    let mut s = Session::new_default();
    for i in 0..(MAX_UNDO + 10) {
        s.run(ids::PAGE_RENAME, &json!({"title": format!("t{i}")})).unwrap();
    }
    assert_eq!(s.undo_depth(), MAX_UNDO);
}

/// Storage that records what it was asked to save.
#[derive(Default)]
struct Recorder {
    saves: std::rc::Rc<std::cell::RefCell<Vec<(Guid, usize, bool)>>>,
    fail: bool,
}

impl Storage for Recorder {
    fn save(&mut self, nb: &Notebook, _o: &Origin, dirty: &HashSet<Guid>, toc: bool) -> Result<Option<Origin>, IoError> {
        if self.fail {
            return Err(IoError::Io("disk full".into()));
        }
        self.saves.borrow_mut().push((nb.id, dirty.len(), toc));
        Ok(Some(Origin::Folder("/tmp/x".into())))
    }
}

#[test]
fn autosave_debounces() {
    let mut s = Session::new_default();
    let rec = Recorder::default();
    let saves = rec.saves.clone();
    s.storage = Some(Box::new(rec));
    assert!(s.tick(0.0).is_none(), "nothing to save");
    s.run(ids::PAGE_RENAME, &json!({"title": "x"})).unwrap();
    assert!(s.tick(10.0).is_none(), "just changed");
    assert!(s.tick(11.0).is_none());
    assert_eq!(s.tick(12.5), Some(Ok(1)));
    assert!(!s.is_dirty());
    assert_eq!(saves.borrow().len(), 1);
    assert!(matches!(s.origins.values().next(), Some(Origin::Folder(_))));
    // Steady changes still save within the max wait.
    let mut t = 20.0;
    let mut saved = false;
    while t < 20.0 + crate::AUTOSAVE_MAX_WAIT_SECS + 2.0 {
        s.run(ids::PAGE_RENAME, &json!({"title": format!("{t}")})).unwrap();
        saved |= s.tick(t).is_some();
        t += 1.0;
    }
    assert!(saved);
}

#[test]
fn failed_autosave_keeps_changes_and_tells_the_user() {
    let mut s = Session::new_default();
    s.storage = Some(Box::new(Recorder { fail: true, ..Recorder::default() }));
    s.run(ids::PAGE_RENAME, &json!({"title": "x"})).unwrap();
    s.tick(0.0);
    assert!(matches!(s.tick(5.0), Some(Err(_))));
    assert!(s.is_dirty());
    assert_eq!(s.notices.len(), 1);
    assert!(s.tick(5.5).is_none(), "doesn't retry every frame");
}

#[test]
fn save_without_storage_keeps_dirty() {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({})).unwrap();
    assert_eq!(s.save_dirty(), Ok(0));
    assert!(s.is_dirty());
}
