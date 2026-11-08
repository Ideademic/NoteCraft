//! The History tab through the window: a deleted page comes back from the Notebook Recycle Bin;
//! versions appear after edits and one can be restored; unread pages and Recent Edits.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use notecraft_engine::ids;
use serde_json::json;

use super::*;

fn harness(app: NoteApp) -> Harness<'static, NoteApp> {
    Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut NoteApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

fn new_page(app: &mut NoteApp, title: &str) -> notecraft_doc::Guid {
    let out = app.run(ids::PAGE_NEW, json!({ "title": title })).unwrap();
    notecraft_doc::Guid::parse(out.value["id"].as_str().unwrap()).unwrap()
}

#[test]
fn a_deleted_page_comes_back_from_the_recycle_bin() {
    let mut app = NoteApp::new(Session::new_default(), Services::default());
    let page = new_page(&mut app, "Trip ideas");
    app.run(ids::OUTLINE_NEW, json!({ "x": 36, "y": 108, "text": "Lisbon in May" })).unwrap();
    app.run(ids::PAGE_DELETE, json!({ "id": page })).unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label("Trip ideas").is_none(), "gone from the page list");
    h.get_by_label("History").click();
    h.run_steps(3);
    h.get_by_label("Notebook Recycle Bin").click();
    h.run_steps(4);
    assert!(h.state().ui.bin_view.is_some());
    let _ = h.get_by_label("Deleted Pages");
    // The deleted page is listed and shows read-only.
    h.get_by_label("Trip ideas").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.bin_view.and_then(|v| v.page), Some(page));
    h.get_by_label("Restore").click();
    h.run_steps(4);
    let s = &h.state().session;
    assert!(h.state().ui.bin_view.is_none(), "restoring leaves the bin");
    assert_eq!(s.current.page, Some(page));
    assert!(s.current_section().is_some_and(|sec| sec.page(page).is_some()));
    assert!(s.current_notebook().is_some_and(|n| n.recycle_bin.is_empty()));
    let _ = h.get_by_label("Trip ideas");
}

#[test]
fn empty_recycle_bin_asks_first() {
    let mut app = NoteApp::new(Session::new_default(), Services::default());
    let page = new_page(&mut app, "Scratch");
    app.run(ids::PAGE_DELETE, json!({ "id": page })).unwrap();
    crate::panes::history::open_bin(&mut app);
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Empty Recycle Bin").click();
    h.run_steps(3);
    assert!(matches!(h.state().ui.prompt, Some(Prompt::Confirm { .. })));
    assert!(h.state().session.current_notebook().is_some_and(|n| !n.recycle_bin.is_empty()), "nothing gone before the answer");
    h.get_by_label("Delete").click();
    h.run_steps(4);
    assert!(h.state().session.current_notebook().is_some_and(|n| n.recycle_bin.is_empty()));
    assert!(h.state().ui.bin_view.is_some(), "the empty bin stays open");
}

#[test]
fn versions_appear_after_edits_and_restore() {
    let mut app = NoteApp::new(Session::new_default(), Services::default());
    app.ui.ribbon_tab = RibbonTab::History;
    let page = app.session.current.page.unwrap();
    app.run(ids::PAGE_RENAME, json!({ "title": "Renamed" })).unwrap();
    let saved = app.session.current_page().unwrap().versions[0].saved;
    let label = crate::panes::history::when(saved, app.canvas.utc_offset_minutes);
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.query_by_label(&label).is_none(), "versions are hidden until asked for");
    h.get_by_label("Page Versions").click();
    h.run_steps(3);
    assert!(h.state().session.show_versions);
    h.get_by_label(&label).click();
    h.run_steps(4);
    assert!(h.state().session.viewing_version.is_some_and(|(p, _)| p == page));
    let _ = h.get_by_label("Restore Version");
    // The read-only page shows the old title; the notebook still has the new one.
    assert_eq!(h.state().session.current_page().unwrap().title, "Renamed");
    h.get_by_label("Restore Version").click();
    h.run_steps(4);
    let p = h.state().session.current_page().unwrap();
    assert_eq!(p.title, "Welcome to NoteCraft");
    assert_eq!(p.versions.len(), 1);
    assert_eq!(p.versions[0].page.title, "Renamed", "what it was became a version");
    assert!(h.state().session.viewing_version.is_none());
}

#[test]
fn unread_pages_and_recent_edits() {
    let mut app = NoteApp::new(Session::new_default(), Services::default());
    let other = new_page(&mut app, "Edited elsewhere");
    let first = app.session.current_section().unwrap().pages[0].id;
    app.run(ids::NAV_PAGE, json!({ "id": first })).unwrap();
    app.session.read_state.mark_unread(other);
    app.ui.ribbon_tab = RibbonTab::History;
    let mut h = harness(app);
    h.run_steps(3);
    assert!(h.state().session.is_unread(other));
    h.get_by_label("Next Unread").click();
    h.run_steps(3);
    assert_eq!(h.state().session.current.page, Some(other));
    assert!(!h.state().session.is_unread(other));
    h.get_by_label("Recent Edits").click();
    h.run_steps(3);
    h.get_by_label("Last 7 Days").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.recent_edits.as_deref(), Some("7d"));
    // Both pages are listed (rows are buttons named by page title).
    assert_eq!(h.get_all_by_label("Edited elsewhere").count(), 2, "page list and Recent Edits");
    // The read marks survive a restart through the host's `read.json`.
    let stored = h.state().read_state_json();
    let mut again = NoteApp::new(Session::new_default(), Services::default());
    again.load_read_state(&stored);
    let fresh = again.session.notebooks[0].clone();
    assert!(!again.session.read_state.is_unread(fresh.id, &fresh.first_section().unwrap().pages[0]), "a notebook new to the state is read");
    let mut back = notecraft_engine::unread::ReadState::from_json(&stored);
    back.adopt(&fresh);
    assert_eq!(again.session.read_state, back);
}
