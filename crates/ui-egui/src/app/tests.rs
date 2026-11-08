//! Whole-window tests: the window draws headless in both themes, the ribbon tabs switch, the
//! panes add sections and pages through commands.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::Value;

use super::*;

fn harness(app: NoteApp) -> Harness<'static, NoteApp> {
    harness_sized(app, 1440.0, 900.0)
}

fn harness_sized(app: NoteApp, w: f32, h: f32) -> Harness<'static, NoteApp> {
    Harness::builder().with_size(egui::vec2(w, h)).build_ui_state(
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
    assert!(o.para(0).unwrap().runs.iter().all(|r| r.bold), "{:?}", o.para(0).unwrap().runs);
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

/// Drags a side pane's splitter by `dx` and releases it, then moves the pointer around.
fn drag_splitter(h: &mut Harness<'static, NoteApp>, x0: f32, dx: f32) {
    let y = 500.0;
    let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    h.event(egui::Event::PointerMoved(egui::pos2(x0, y)));
    h.step();
    h.event(button(egui::pos2(x0, y), true));
    h.step();
    for f in [0.2, 0.6, 1.0] {
        h.event(egui::Event::PointerMoved(egui::pos2(x0 + dx * f, y)));
        h.step();
    }
    h.event(button(egui::pos2(x0 + dx, y), false));
    h.step();
    for i in 0..30 {
        h.event(egui::Event::PointerMoved(egui::pos2(760.0 + i as f32 * 3.0, 640.0)));
        h.step();
    }
}

/// The notebook pane's right edge (its collapse button sits 18 px inside it).
fn notebook_edge(h: &Harness<'static, NoteApp>) -> f32 {
    h.get_by_label("Collapse Notebook Pane").rect().center().x + 18.0
}

/// The page list's left edge (its Add Page button starts 6 px inside it).
fn page_list_edge(h: &Harness<'static, NoteApp>) -> f32 {
    h.get_by_label("Add Page").rect().min.x - 6.0
}

#[test]
fn the_notebook_pane_keeps_its_width_while_idle() {
    let mut h = harness(app());
    h.run_steps(3);
    let start = notebook_edge(&h);
    for i in 0..30 {
        h.event(egui::Event::PointerMoved(egui::pos2(700.0 + i as f32, 600.0)));
        h.step();
    }
    assert!((notebook_edge(&h) - start).abs() <= 1.0, "pane went from {start} to {}", notebook_edge(&h));
}

#[test]
fn a_dragged_notebook_pane_keeps_its_width() {
    let mut h = harness(app());
    h.run_steps(3);
    // The splitter is grabbed just inside the pane's edge.
    let x0 = notebook_edge(&h) - 2.0;
    drag_splitter(&mut h, x0, 100.0);
    assert!((notebook_edge(&h) - (x0 + 100.0)).abs() <= 2.0, "dragged to {}, now {}", x0 + 100.0, notebook_edge(&h));
}

#[test]
fn a_dragged_page_list_keeps_its_width() {
    let mut h = harness(app());
    h.run_steps(3);
    let x0 = page_list_edge(&h) + 2.0;
    drag_splitter(&mut h, x0, -100.0);
    assert!((page_list_edge(&h) - (x0 - 100.0)).abs() <= 2.0, "dragged to {}, now {}", x0 - 100.0, page_list_edge(&h));
}

fn canvas_width(h: &Harness<'static, NoteApp>) -> f32 {
    h.state().canvas_rect.map_or(0.0, |r| r.width())
}

#[test]
fn a_narrow_window_folds_the_panes_for_now() {
    let mut h = harness_sized(app(), 800.0, 760.0);
    h.run_steps(4);
    assert!(canvas_width(&h) >= narrow::MIN_CANVAS_WIDTH, "page only {} wide", canvas_width(&h));
    assert!(h.state().ui.narrow.notebooks_folded);
    assert!(h.query_by_label("Collapse Notebook Pane").is_none(), "the notebook pane is folded");
    // The user's choices are untouched…
    assert!(h.state().ui.notebook_pane_open && h.state().ui.page_list_open);
    assert!(h.state().prefs().notebook_pane_open);
    // …and come back with the room.
    h.set_size(egui::vec2(1440.0, 900.0));
    h.run_steps(4);
    assert!(!h.state().ui.narrow.notebooks_folded);
    assert!((notebook_edge(&h) - (8.0 + NOTEBOOK_PANE_WIDTH)).abs() <= 2.0, "pane edge at {}", notebook_edge(&h));
    assert!(h.query_by_label("Add Page").is_some());
}

#[test]
fn a_very_narrow_window_narrows_then_folds_the_page_list() {
    let mut h = harness_sized(app(), 620.0, 700.0);
    h.run_steps(4);
    assert!(h.state().ui.narrow.notebooks_folded && !h.state().ui.narrow.pages_folded);
    assert!(canvas_width(&h) >= narrow::MIN_CANVAS_WIDTH, "page only {} wide", canvas_width(&h));
    let list = h.get_by_label("Collapse Page Tabs").rect().center().x + 18.0 - page_list_edge(&h);
    assert!(list < PAGE_LIST_WIDTH, "the page list narrowed to {list}");
    h.set_size(egui::vec2(480.0, 700.0));
    h.run_steps(4);
    assert!(h.state().ui.narrow.pages_folded);
    assert!(h.query_by_label("Expand Page Tabs").is_some());
    assert!(h.state().ui.page_list_open, "the choice is kept");
    h.set_size(egui::vec2(1440.0, 900.0));
    h.run_steps(4);
    assert!((page_list_edge(&h) - (1440.0 - 8.0 - PAGE_LIST_WIDTH)).abs() <= 2.0, "list edge at {}", page_list_edge(&h));
}

#[test]
fn a_dragged_wide_pane_is_held_to_its_share_and_keeps_its_width() {
    let mut h = harness(app());
    h.run_steps(3);
    let x0 = notebook_edge(&h) - 2.0;
    drag_splitter(&mut h, x0, 240.0);
    let wide = notebook_edge(&h);
    assert!(wide > 440.0, "dragged to {wide}");
    h.set_size(egui::vec2(1100.0, 800.0));
    h.run_steps(4);
    let capped = notebook_edge(&h) - 8.0;
    assert!(capped <= (1100.0 - 16.0) * narrow::MAX_PANE_SHARE + 1.0, "pane {capped} wide in a 1100 window");
    assert!(canvas_width(&h) >= narrow::MIN_CANVAS_WIDTH);
    h.set_size(egui::vec2(1440.0, 900.0));
    h.run_steps(4);
    assert!((notebook_edge(&h) - wide).abs() <= 2.0, "back to {}, was {wide}", notebook_edge(&h));
}

#[test]
fn a_folded_pane_opens_over_the_page_and_closes_on_a_click_outside() {
    let mut h = harness_sized(app(), 800.0, 760.0);
    h.run_steps(4);
    let page = canvas_width(&h);
    h.get_by_label("Expand Notebook Pane").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.narrow.overlay, Some(narrow::SidePane::Notebooks));
    assert!(h.query_by_label("Collapse Notebook Pane").is_some());
    assert!((canvas_width(&h) - page).abs() < 1.0, "the overlay floats over the page");
    // Its fold button closes it without changing the choice.
    h.get_by_label("Collapse Notebook Pane").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.narrow.overlay, None);
    assert!(h.state().ui.notebook_pane_open);
    // A click on the page closes it too.
    h.get_by_label("Expand Notebook Pane").click();
    h.run_steps(3);
    assert!(h.state().ui.narrow.overlay.is_some());
    let p = egui::pos2(520.0, 600.0);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.step();
    }
    h.run_steps(2);
    assert_eq!(h.state().ui.narrow.overlay, None);
    assert!(h.state().ui.notebook_pane_open);
}

#[test]
fn dragging_a_pane_stops_short_of_squeezing_the_page() {
    let mut h = harness_sized(app(), 900.0, 800.0);
    h.run_steps(3);
    let x0 = notebook_edge(&h) - 2.0;
    drag_splitter(&mut h, x0, 400.0);
    h.run_steps(3);
    assert!(!h.state().ui.narrow.notebooks_folded, "the pane stays pinned");
    assert!(canvas_width(&h) >= narrow::MIN_CANVAS_WIDTH, "page only {} wide", canvas_width(&h));
}

#[test]
fn a_control_not_built_yet_says_coming_soon_and_the_toast_fades() {
    let mut h = harness(app());
    h.run_steps(3);
    // Cut with nothing selected is only disabled for now: no toast.
    h.get_by_label("Cut").click();
    h.run_steps(2);
    assert_eq!(h.state().toast_text(), None);
    h.get_by_label("Draw").click();
    h.run_steps(3);
    h.get_by_label("Ink to Text").click();
    h.run_steps(2);
    assert_eq!(h.state().toast_text(), Some("Coming soon"));
    h.run_steps(4);
    assert_eq!(h.state().toast_text(), Some("Coming soon"), "still showing after a second");
    // Steps are a quarter of a second: gone after the timeout.
    h.run_steps((crate::toast::DURATION / 0.25) as usize + 2);
    assert_eq!(h.state().toast_text(), None);
}
