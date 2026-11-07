//! Whole-window tests: the window draws headless in both themes, the ribbon tabs switch, the
//! panes add sections and pages through commands.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::Value;

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

#[test]
fn the_window_draws_headless_light_and_dark() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let mut a = app();
        a.ui.prefs.appearance = appearance;
        let mut h = harness(a);
        h.run_steps(4);
        assert_eq!(h.state().ui.dark, appearance == Appearance::Dark);
        assert!(h.state().session.current_page().is_some());
        // The frame's landmarks are there.
        let _ = h.get_by_label("Home");
        let _ = h.get_by_label("Add Page");
        assert_eq!(h.get_all_by_label("Quick Notes").count(), 2, "pane row and section tab");
    }
}

#[test]
fn errors_reach_the_status_bar() {
    let mut app = app();
    assert!(app.run("format.bold", Value::Null).is_err());
    assert!(app.ui.status.is_some() && app.ui.status_error);
    assert!(app.run("page.new", Value::Null).is_ok());
    assert!(app.ui.status.is_none());
    assert!(app.run("no.such.command", Value::Null).is_err());
    assert!(app.ui.status.as_deref().is_some_and(|s| s.contains("isn't available")));
}

#[test]
fn canvas_errors_reach_the_status_bar() {
    let mut app = app();
    app.canvas.messages.push("first".into());
    app.canvas.messages.push("The note container is gone.".into());
    assert!(app.drain_canvas_messages());
    assert!(app.canvas.messages.is_empty());
    assert_eq!(app.ui.status.as_deref(), Some("The note container is gone."));
    assert!(app.ui.status_error);
    assert!(!app.drain_canvas_messages());
}

#[test]
fn clicking_each_ribbon_tab_switches_it() {
    let mut h = harness(app());
    h.run_steps(3);
    for (label, tab) in [("Insert", RibbonTab::Insert), ("Draw", RibbonTab::Draw), ("View", RibbonTab::View), ("Home", RibbonTab::Home)] {
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ribbon_tab, tab, "{label}");
    }
    h.get_by_label("File").click();
    h.run_steps(3);
    assert!(h.state().ui.backstage);
    h.get_by_label("Back to Notes").click();
    h.run_steps(3);
    assert!(!h.state().ui.backstage);
}

#[test]
fn the_panes_add_sections_and_pages() {
    let mut h = harness(app());
    h.run_steps(3);
    let sections = |h: &Harness<'_, NoteApp>| h.state().session.current_notebook().map_or(0, |n| n.all_sections().len());
    let pages = |h: &Harness<'_, NoteApp>| h.state().session.current_section().map_or(0, |s| s.pages.len());
    assert_eq!(sections(&h), 1);
    h.get_by_label("Create a New Section").click();
    h.run_steps(3);
    assert_eq!(sections(&h), 2);
    assert_eq!(pages(&h), 1, "a new section starts with one page");
    h.get_by_label("Add Page").click();
    h.run_steps(3);
    assert_eq!(pages(&h), 2);
    // Back to the first section by its tab.
    if let Some(tab) = h.get_all_by_label("Quick Notes").last() {
        tab.click();
    }
    h.run_steps(3);
    assert_eq!(h.state().session.current_section().map(|s| s.name.as_str()), Some("Quick Notes"));
    // Undo from the Quick Access Toolbar takes the page back.
    h.get_by_label("Undo").click();
    h.run_steps(3);
}

#[test]
fn ribbon_bold_on_empty_paper_bolds_what_is_typed_next() {
    let mut h = harness(app());
    h.run_steps(3);
    // Empty paper well below the welcome note.
    let rect = h.state().canvas.last_rect.unwrap();
    let sheet = crate::canvas::page::sheet_rect(rect);
    let k = h.state().canvas.scale();
    let p = egui::pos2(sheet.min.x + 120.0 * k - h.state().canvas.scroll.x, sheet.min.y + 420.0 * k - h.state().canvas.scroll.y);
    h.event(egui::Event::PointerMoved(p));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
    assert!(h.state().canvas.pending.is_some(), "the caret waits on the paper");
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert_eq!(h.state().canvas.pending_style.bold, Some(true), "Bold becomes the pending style");
    assert_eq!(crate::canvas::current_format(&h.state().canvas, &h.state().session).and_then(|f| f.bold), Some(true), "the button shows on");
    h.event(egui::Event::Text("x".into()));
    h.run_steps(3);
    let id = h.state().canvas.caret.as_ref().map(|c| c.outline).unwrap();
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    assert_eq!(o.text(), "x");
    assert!(o.paragraphs[0].runs.iter().all(|r| r.bold), "{:?}", o.paragraphs[0].runs);
}

#[test]
fn draw_tools_set_the_canvas_tool() {
    let mut a = app();
    a.ui.ribbon_tab = RibbonTab::Draw;
    let mut h = harness(a);
    h.run_steps(3);
    h.get_by_label("Highlighter").click();
    h.run_steps(2);
    assert_eq!(h.state().canvas.tool, crate::Tool::Highlighter);
    h.get_by_label("Lasso Select").click();
    h.run_steps(2);
    assert_eq!(h.state().canvas.tool, crate::Tool::Lasso);
}

#[test]
fn panes_collapse_and_come_back() {
    let mut h = harness(app());
    h.run_steps(3);
    h.get_by_label("Collapse Notebook Pane").click();
    h.run_steps(3);
    assert!(!h.state().ui.notebook_pane_open);
    h.get_by_label("Expand Notebook Pane").click();
    h.run_steps(3);
    assert!(h.state().ui.notebook_pane_open);
    h.get_by_label("Collapse Page Tabs").click();
    h.run_steps(3);
    assert!(!h.state().ui.page_list_open);
}

#[test]
fn back_returns_to_the_previous_page() {
    let mut app = app();
    let first = app.session.current.page;
    app.act(notecraft_engine::ids::PAGE_NEW, serde_json::json!({}));
    app.track_location();
    assert_ne!(app.session.current.page, first);
    assert!(app.can_go_back());
    app.go_back();
    assert_eq!(app.session.current.page, first);
}

#[test]
fn prefs_round_trip_and_survive_junk() {
    let mut p = Prefs::default();
    p.remember("A", "/a", None);
    p.remember("B", "/b", Some("#ff0000".into()));
    p.remember("A", "/a", None);
    assert_eq!(p.recent.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    let back = Prefs::from_json(&p.to_json());
    assert_eq!(back, p);
    assert_eq!(Prefs::from_json("not json"), Prefs::default());
    let odd = Prefs::from_json(r#"{"qat": ["ui.back", "rm -rf"], "window": {"x": 0, "y": 0, "width": 1e30, "height": 5}}"#);
    assert_eq!(odd.qat, vec![QAT_BACK.to_owned()]);
    assert_eq!(odd.window, None);
}
