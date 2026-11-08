//! Whole-window tests of the editing features: Find & Replace on the page, the Tags Summary
//! pane, page templates, Hide Page Title kept per page, the saved zoom and the ribbon buttons
//! that were disabled before.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use notecraft_doc::Guid;
use notecraft_engine::ids;
use serde_json::{Value, json};

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

fn app() -> NoteApp {
    NoteApp::new(Session::new_default(), Services::default())
}

fn id_of(v: &Value) -> Guid {
    v["id"].as_str().and_then(Guid::parse).unwrap()
}

/// A page titled `title` with one note container holding `paras`; returns (page, outline).
fn page_with(app: &mut NoteApp, title: &str, paras: Value) -> (Guid, Guid) {
    let page = id_of(&app.run(ids::PAGE_NEW, json!({"title": title, "blank": true})).unwrap().value);
    let outline = id_of(&app.run(ids::OUTLINE_NEW, json!({"x": 40, "y": 120})).unwrap().value);
    app.run(ids::TEXT_SET_PARAGRAPHS, json!({"id": outline, "paragraphs": paras})).unwrap();
    (page, outline)
}

#[test]
fn ctrl_f_finds_on_the_page_and_replace_all_is_one_undo_step() {
    let mut a = app();
    let (_, outline) = page_with(&mut a, "Pets", json!([{"runs": [{"text": "cat and Cat and category"}]}, {"runs": [{"text": "one cat"}]}]));
    let mut h = harness(a);
    h.run_steps(3);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run_steps(2);
    assert!(h.state().ui.find.is_some(), "Ctrl+F opens the find bar");
    h.state_mut().ui.find.as_mut().unwrap().query = "cat".into();
    h.run_steps(2);
    let found = h.state().canvas.find.as_ref().map(|f| f.matches.len());
    assert_eq!(found, Some(4), "every match is highlighted (case ignored)");
    h.state_mut().ui.find.as_mut().unwrap().whole_word = true;
    h.run_steps(2);
    assert_eq!(h.state().canvas.find.as_ref().map(|f| f.matches.len()), Some(3), "whole words only");
    let _ = h.get_by_label("Next match (Enter)");
    let depth = h.state().session.undo_depth();
    {
        let app = h.state_mut();
        app.ui.find.as_mut().unwrap().replace = "dog".into();
        crate::dialogs::find::replace(app, true);
    }
    h.run_steps(2);
    let text = h.state().session.current_page().unwrap().outline(outline).unwrap().text();
    assert_eq!(text, "dog and dog and category\none dog");
    assert_eq!(h.state().session.undo_depth(), depth + 1, "Replace All is one undo step");
    assert!(h.state().ui.status.as_deref().is_some_and(|s| s.contains('3')));
    h.state_mut().session.undo();
    let back = h.state().session.current_page().unwrap().outline(outline).unwrap().text();
    assert_eq!(back, "cat and Cat and category\none cat");
    // Esc in the box, or the close button, closes it and its highlights.
    crate::dialogs::find::close(h.state_mut());
    h.run_steps(2);
    assert!(h.state().canvas.find.is_none());
}

#[test]
fn tags_summary_lists_tags_and_jumps_to_them() {
    let mut a = app();
    let (tagged, outline) = page_with(
        &mut a,
        "Plans",
        json!([
            {"runs": [{"text": "intro"}]},
            {"runs": [{"text": "Call the bank"}], "tag": {"type": "toDo", "checked": false}},
            {"runs": [{"text": "Booked"}], "tag": {"type": "toDo", "checked": true}},
            {"runs": [{"text": "Big idea"}], "tag": {"type": "idea"}}
        ]),
    );
    a.run(ids::PAGE_NEW, json!({"title": "Elsewhere", "blank": true})).unwrap();
    let mut h = harness(a);
    h.run_steps(2);
    crate::panes::tags::toggle(h.state_mut());
    h.run_steps(3);
    let pane = h.state().ui.tags_pane.clone().unwrap();
    let here: Vec<_> = pane.items.iter().filter(|i| i.page == tagged).cloned().collect();
    assert_eq!(here.len(), 3, "the page's three tagged notes (the welcome pages have their own)");
    let groups = crate::panes::tags::grouped(&here, crate::panes::tags::GroupBy::Tag, 0);
    assert_eq!(groups.len(), 2, "To Do and Idea: {groups:?}");
    // Only unchecked To Do items.
    {
        let app = h.state_mut();
        let mut p = app.ui.tags_pane.clone().unwrap();
        p.unchecked_only = true;
        crate::panes::tags::refresh(app, &mut p);
        let here: Vec<_> = p.items.iter().filter(|i| i.page == tagged).collect();
        assert_eq!(here.len(), 1);
        assert_eq!(here[0].text, "Call the bank");
        app.ui.tags_pane = Some(p);
    }
    h.run_steps(2);
    h.get_by_label("Call the bank").click();
    h.run_steps(4);
    assert_eq!(h.state().session.current.page, Some(tagged), "the click goes to the page");
    let caret = h.state().canvas.caret.clone().expect("with the caret in the note");
    assert_eq!((caret.outline, caret.head.para), (outline, 1));
}

#[test]
fn templates_make_pages_and_can_be_saved_and_made_the_default() {
    let mut a = app();
    let before = a.session.current_section().unwrap().pages.len();
    crate::panes::templates::toggle(&mut a);
    let mut h = harness(a);
    h.run_steps(3);
    h.get_by_label("Simple meeting notes").click();
    h.run_steps(3);
    let s = h.state().session.current_section().unwrap();
    assert_eq!(s.pages.len(), before + 1);
    let page = h.state().session.current_page().unwrap();
    assert_eq!(page.title, "Meeting");
    assert!(!page.elements.is_empty(), "the template's notes are on the new page");
    // Save the page as a template: it is kept for the host to store.
    let rev = h.state().ui.templates_revision;
    assert!(crate::panes::templates::save_current(h.state_mut(), "Standup"));
    assert_ne!(h.state().ui.templates_revision, rev);
    let json = h.state().templates_json();
    let mut other = app();
    other.load_templates(&json);
    assert_eq!(other.session.templates.len(), 1);
    assert_eq!(other.session.templates[0].name, "Standup");
    // Make it the section's default: new pages start from it.
    let id = h.state().session.templates[0].id.clone();
    h.state_mut().act(ids::SECTION_DEFAULT_TEMPLATE, json!({"template": id}));
    h.state_mut().act(ids::PAGE_NEW, json!({}));
    assert_eq!(h.state().session.current_page().unwrap().title, "Meeting");
    other.load_templates("not json");
    assert!(other.session.templates.is_empty(), "junk is ignored");
}

#[test]
fn hide_page_title_is_kept_with_each_page() {
    let a = app();
    let first = a.session.current.page.unwrap();
    let mut h = harness(a);
    h.run_steps(2);
    h.state_mut().ui.ribbon_tab = crate::ribbon::RibbonTab::View;
    h.run_steps(2);
    h.get_by_label("Hide Page Title").click();
    h.run_steps(2);
    assert!(h.state().session.current_page().unwrap().hide_title);
    assert!(h.state().canvas.hide_title);
    h.state_mut().act(ids::PAGE_NEW, json!({"blank": true}));
    h.run_steps(2);
    assert!(!h.state().canvas.hide_title, "another page shows its title");
    h.state_mut().act(ids::NAV_PAGE, json!({"id": first}));
    h.run_steps(2);
    assert!(h.state().canvas.hide_title, "the first page still hides it");
}

#[test]
fn zoom_is_kept_in_the_settings() {
    let mut a = app();
    a.canvas.set_zoom(1.5);
    let json = a.prefs().to_json();
    let mut b = app();
    b.load_prefs(Prefs::from_json(&json));
    assert!((b.canvas.zoom - 1.5).abs() < 1e-4);
    let junk = Prefs::from_json(r#"{"zoom": 1e30}"#);
    assert!(junk.zoom <= notecraft_geom::zoom::MAX);
}

#[test]
fn the_editing_buttons_are_enabled() {
    let mut a = app();
    let (_, outline) = page_with(&mut a, "Buttons", json!([{"runs": [{"text": "hello"}]}]));
    let mut h = harness(a);
    h.run_steps(2);
    // Format Painter wants a caret.
    {
        let app = h.state_mut();
        app.canvas.caret = Some(crate::canvas::TextCaret::at(outline, notecraft_engine::TextPos::new(0, 2)));
        app.canvas.editing = Some(outline);
    }
    h.run_steps(2);
    h.get_by_label("Format Painter").click();
    h.run_steps(2);
    assert!(h.state().canvas.painter.is_some(), "Format Painter arms");
    // Insert Space and Page Templates on Insert.
    h.state_mut().ui.ribbon_tab = crate::ribbon::RibbonTab::Insert;
    h.run_steps(2);
    h.get_by_label("Insert Space").click();
    h.run_steps(2);
    assert_eq!(h.state().canvas.tool, crate::canvas::Tool::InsertSpace);
    h.get_by_label("Page Templates").click();
    h.run_steps(2);
    assert!(h.state().ui.templates_pane.is_some());
    // Find Tags on Home.
    h.state_mut().ui.ribbon_tab = crate::ribbon::RibbonTab::Home;
    h.run_steps(2);
    h.get_by_label("Find Tags").click();
    h.run_steps(2);
    assert!(h.state().ui.tags_pane.is_some());
}

#[test]
fn search_box_scope_is_passed_to_search() {
    let mut a = app();
    a.run(ids::SECTION_NEW, json!({"name": "Other"})).unwrap();
    page_with(&mut a, "Kept here", json!([{"runs": [{"text": "zebra crossing"}]}]));
    let first = a.session.current_notebook().unwrap().first_section().unwrap().id;
    a.act(ids::NAV_SECTION, json!({"id": first}));
    a.ui.search_scope = "section".into();
    assert!(crate::panes::search::find(&mut a, "zebra").is_empty(), "not in this section");
    a.ui.search_scope = "notebook".into();
    assert_eq!(crate::panes::search::find(&mut a, "zebra").len(), 1);
}
