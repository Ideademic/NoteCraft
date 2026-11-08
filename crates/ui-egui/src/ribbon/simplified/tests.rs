//! The simplified ribbon and the ribbon display options, in the whole window (headless).

use egui::{Modifiers, PointerButton, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::Prefs;
use crate::ribbon::display::{OPTIONS_LABEL, REVEAL_LABEL, RibbonDisplay, RibbonLayout};

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

fn harness(app: NoteApp) -> Harness<'static, NoteApp> {
    harness_sized(app, 1440.0, 900.0)
}

fn app() -> NoteApp {
    NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default())
}

fn simplified_app() -> NoteApp {
    let mut a = app();
    a.ui.ribbon_layout = RibbonLayout::Simplified;
    a
}

fn click_at(h: &mut Harness<'static, NoteApp>, p: egui::Pos2, button: PointerButton) {
    h.event(egui::Event::PointerMoved(p));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    h.run_steps(3);
}

/// A point on the page, well away from the ribbon.
fn on_the_page(h: &Harness<'static, NoteApp>) -> egui::Pos2 {
    h.state().canvas_rect.map_or(pos2(700.0, 600.0), |r| r.center() + egui::vec2(0.0, 150.0))
}

fn canvas_top(h: &Harness<'static, NoteApp>) -> f32 {
    h.state().canvas_rect.map_or(0.0, |r| r.min.y)
}

#[test]
fn right_click_on_empty_ribbon_switches_to_the_simplified_ribbon() {
    let mut h = harness(app());
    h.run_steps(3);
    let classic_top = canvas_top(&h);
    // Empty ribbon, right of the Tags group.
    let y = h.get_by_label("Home").rect().max.y + 40.0;
    click_at(&mut h, pos2(1250.0, y), PointerButton::Secondary);
    let _ = h.get_by_label("Classic ribbon");
    let _ = h.get_by_label("Collapse the Ribbon");
    h.get_by_label("Simplified ribbon").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.ribbon_layout, RibbonLayout::Simplified);
    assert!(crate::ribbon::body_h(h.state()) < 50.0);
    let saved = classic_top - canvas_top(&h);
    assert!(saved >= crate::ribbon::BODY_H - 50.0, "the page moved up by {saved}");
    // The simplified row has the Home commands.
    let _ = h.get_by_label("Bold");
    let _ = h.get_by_label("Paste");
    // Right-clicking the empty tab band offers the same menu; switch back.
    let band = h.get_by_label("View").rect();
    click_at(&mut h, pos2(1100.0, band.center().y), PointerButton::Secondary);
    h.get_by_label("Classic ribbon").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.ribbon_layout, RibbonLayout::Classic);
    assert!((canvas_top(&h) - classic_top).abs() < 1.0);
}

#[test]
fn the_bottom_right_arrow_opens_the_display_options() {
    for layout in [RibbonLayout::Classic, RibbonLayout::Simplified] {
        let mut a = app();
        a.ui.ribbon_layout = layout;
        let mut h = harness(a);
        h.run_steps(3);
        let arrow = h.get_by_label(OPTIONS_LABEL);
        let r = arrow.rect();
        assert!(r.max.x > 1400.0, "{layout:?}: at the right end, {r:?}");
        assert!(r.min.y > h.get_by_label("Home").rect().max.y, "{layout:?}: on the ribbon, under the tabs");
        arrow.click();
        h.run_steps(3);
        for item in ["Classic ribbon", "Simplified ribbon", "Full-screen mode", "Show tabs only", "Always show ribbon"] {
            let _ = h.get_by_label(item);
        }
        h.get_by_label("Show tabs only").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::TabsOnly);
    }
}

#[test]
fn full_screen_mode_hides_tabs_and_ribbon_until_the_strip_is_clicked() {
    let mut h = harness(app());
    h.run_steps(3);
    let normal_top = canvas_top(&h);
    h.get_by_label(OPTIONS_LABEL).click();
    h.run_steps(3);
    h.get_by_label("Full-screen mode").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::FullScreen);
    assert!(h.query_by_label("Home").is_none(), "tabs hidden");
    assert!(h.query_by_label("Bold").is_none(), "ribbon hidden");
    assert!(h.query_by_label("Undo").is_none(), "the Quick Access Toolbar row is a thin strip");
    let full_top = canvas_top(&h);
    assert!(normal_top - full_top > 100.0, "the page gains the room: {normal_top} → {full_top}");
    // The strip's "⋯" shows the tabs and ribbon.
    h.get_by_label(REVEAL_LABEL).click();
    h.run_steps(4);
    assert!(h.state().ui.ribbon_peek);
    let _ = h.get_by_label("Home");
    let _ = h.get_by_label("Bold");
    // A click on the page hides them again; the mode stays.
    let p = on_the_page(&h);
    click_at(&mut h, p, PointerButton::Primary);
    assert!(!h.state().ui.ribbon_peek);
    assert!(h.query_by_label("Home").is_none());
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::FullScreen);
    // Shown again, then Esc hides them.
    h.get_by_label(REVEAL_LABEL).click();
    h.run_steps(3);
    assert!(h.state().ui.ribbon_peek);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(!h.state().ui.ribbon_peek);
    // From the revealed ribbon, the menu leaves full-screen mode.
    h.get_by_label(REVEAL_LABEL).click();
    h.run_steps(3);
    h.get_by_label(OPTIONS_LABEL).click();
    h.run_steps(3);
    h.get_by_label("Always show ribbon").click();
    h.run_steps(4);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always);
    assert!((canvas_top(&h) - normal_top).abs() < 1.0);
}

#[test]
fn esc_leaves_full_screen_mode() {
    let mut a = app();
    a.ui.ribbon_display = RibbonDisplay::FullScreen;
    let mut h = harness(a);
    h.run_steps(3);
    assert!(h.query_by_label("Home").is_none());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always);
    let _ = h.get_by_label("Home");
}

#[test]
fn show_tabs_only_returns_to_always_show_when_a_tab_is_clicked() {
    for layout in [RibbonLayout::Classic, RibbonLayout::Simplified] {
        let mut a = app();
        a.ui.ribbon_layout = layout;
        a.ui.ribbon_display = RibbonDisplay::TabsOnly;
        let mut h = harness(a);
        h.run_steps(3);
        assert!(h.query_by_label("Pictures").is_none());
        // The options arrow sits on the tab row while only the tabs show.
        let _ = h.get_by_label(OPTIONS_LABEL);
        h.get_by_label("Insert").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always, "{layout:?}: the click pins the ribbon");
        assert!(!h.state().ui.ribbon_peek, "{layout:?}: no peek");
        let _ = h.get_by_label("Pictures");
        // Using the ribbon and clicking the page leave it showing.
        h.get_by_label("View").click();
        h.run_steps(3);
        let _ = h.get_by_label("Page Width");
        h.get_by_label("Page Width").click();
        h.run_steps(3);
        let p = on_the_page(&h);
        click_at(&mut h, p, PointerButton::Primary);
        let _ = h.get_by_label("Page Width");
        assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always, "{layout:?}");
    }
}

#[test]
fn a_keytip_that_opens_a_tab_also_returns_to_always_show() {
    let mut a = app();
    a.ui.ribbon_display = RibbonDisplay::TabsOnly;
    let mut h = harness(a);
    h.run_steps(3);
    // F10 shows the KeyTips; N opens Insert.
    h.key_press(egui::Key::F10);
    h.run_steps(2);
    assert!(h.state().ui.keytips.active());
    h.key_press(egui::Key::N);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_tab, RibbonTab::Insert);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always);
}

#[test]
fn double_clicking_a_tab_does_not_collapse_the_ribbon() {
    for layout in [RibbonLayout::Classic, RibbonLayout::Simplified] {
        let mut a = app();
        a.ui.ribbon_layout = layout;
        // Short frames, so the two clicks are well within the double-click delay.
        let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).with_step_dt(0.02).build_ui_state(
            |ui, app: &mut NoteApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        );
        h.run_steps(3);
        let p = h.get_by_label("Draw").rect().center();
        h.event(egui::Event::PointerMoved(p));
        h.step();
        for _ in 0..2 {
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
                h.step();
            }
        }
        h.run_steps(3);
        assert_eq!(h.state().ui.ribbon_tab, RibbonTab::Draw, "{layout:?}");
        assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always, "{layout:?}: still showing");
        let _ = h.get_by_label("Lasso Select");
    }
}

#[test]
fn ctrl_f1_toggles_show_tabs_only() {
    let mut h = harness(simplified_app());
    h.run_steps(3);
    let _ = h.get_by_label("Bold");
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::F1);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::TabsOnly);
    assert!(h.query_by_label("Bold").is_none());
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::F1);
    h.run_steps(3);
    assert_eq!(h.state().ui.ribbon_display, RibbonDisplay::Always);
    let _ = h.get_by_label("Bold");
}

#[test]
fn ribbon_prefs_round_trip() {
    let p = Prefs { ribbon_layout: RibbonLayout::Simplified, ribbon_display: RibbonDisplay::FullScreen, ..Prefs::default() };
    let json = p.to_json();
    assert!(json.contains("\"ribbonLayout\": \"simplified\"") && json.contains("\"ribbonDisplay\": \"fullScreen\""), "{json}");
    assert_eq!(Prefs::from_json(&json), p);
    let d = Prefs::default();
    assert_eq!((d.ribbon_layout, d.ribbon_display), (RibbonLayout::Classic, RibbonDisplay::Always));
    // Older settings: a collapsed ribbon is Show tabs only; junk falls back to the defaults.
    assert_eq!(Prefs::from_json(r#"{"ribbonCollapsed": true}"#).ribbon_display, RibbonDisplay::TabsOnly);
    assert_eq!(Prefs::from_json(r#"{"ribbonCollapsed": true, "ribbonDisplay": "always"}"#).ribbon_display, RibbonDisplay::Always);
    assert_eq!(Prefs::from_json(r#"{"ribbonLayout": "sideways"}"#), Prefs::default());
    // The app takes them and gives them back.
    let mut a = app();
    a.load_prefs(p.clone());
    assert_eq!((a.ui.ribbon_layout, a.ui.ribbon_display), (RibbonLayout::Simplified, RibbonDisplay::FullScreen));
    crate::ribbon::display::set_display(&mut a, RibbonDisplay::TabsOnly);
    let back = a.prefs();
    assert_eq!((back.ribbon_layout, back.ribbon_display), (RibbonLayout::Simplified, RibbonDisplay::TabsOnly));
}

#[test]
fn simplified_bold_on_empty_paper_bolds_what_is_typed_next() {
    let mut h = harness(simplified_app());
    h.run_steps(3);
    // Empty paper well below the welcome note.
    let Some(rect) = h.state().canvas.last_rect else { panic!("the canvas drew") };
    let sheet = crate::canvas::page::sheet_rect(rect);
    let k = h.state().canvas.scale();
    let p = pos2(sheet.min.x + 120.0 * k - h.state().canvas.scroll.x, sheet.min.y + 420.0 * k - h.state().canvas.scroll.y);
    click_at(&mut h, p, PointerButton::Primary);
    assert!(h.state().canvas.pending.is_some(), "the caret waits on the paper");
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert_eq!(h.state().canvas.pending_style.bold, Some(true), "Bold becomes the pending style");
    h.event(egui::Event::Text("x".into()));
    h.run_steps(3);
    let id = h.state().canvas.caret.as_ref().map(|c| c.outline);
    let o = id.and_then(|id| h.state().session.current_page().and_then(|p| p.outline(id)).cloned());
    let Some(o) = o else { panic!("a note container with the typed text") };
    assert_eq!(o.text(), "x");
    assert!(o.paragraphs.first().is_some_and(|p| p.runs.iter().all(|r| r.bold)), "{:?}", o.paragraphs);
}

#[test]
fn simplified_tabs_draw_light_and_dark() {
    for appearance in [crate::Appearance::Light, crate::Appearance::Dark] {
        let mut a = simplified_app();
        a.ui.prefs.appearance = appearance;
        let mut h = harness(a);
        h.run_steps(3);
        for (label, tab, probe) in [
            ("Insert", RibbonTab::Insert, "Pictures"),
            ("Draw", RibbonTab::Draw, "Lasso Select"),
            ("View", RibbonTab::View, "Dark Mode"),
            ("Home", RibbonTab::Home, "Underline"),
        ] {
            h.get_by_label(label).click();
            h.run_steps(4);
            assert_eq!(h.state().ui.ribbon_tab, tab);
            let _ = h.get_by_label(probe);
            // Everything fits a wide window: no More commands.
            assert!(h.query_by_label(MORE_LABEL).is_none(), "{label}");
        }
    }
}

#[test]
fn simplified_draw_tools_set_the_canvas_tool() {
    let mut a = simplified_app();
    a.ui.ribbon_tab = RibbonTab::Draw;
    let mut h = harness(a);
    h.run_steps(3);
    h.get_by_label("Lasso Select").click();
    h.run_steps(2);
    assert_eq!(h.state().canvas.tool, crate::Tool::Lasso);
    // The first pen on the strip is the black pen.
    let first = crate::ribbon::simplified::draw::recent_pens(&h.ctx).first().copied();
    let Some(first) = first else { panic!("pens on the strip") };
    let name = format!("Pen {} ({:.1} pt)", crate::widgets::color::hex(crate::theme::color32(first.color)), first.width);
    h.get_by_label(&name).click();
    h.run_steps(2);
    assert_eq!(h.state().canvas.tool, crate::Tool::Pen);
    h.get_by_label("Ink to Shape").click();
    h.run_steps(2);
    assert!(h.state().session.ink_to_shape());
}

#[test]
fn a_narrow_window_moves_commands_into_more() {
    for width in [400.0, 260.0, 120.0] {
        for tab in [RibbonTab::Home, RibbonTab::Insert, RibbonTab::Draw, RibbonTab::View] {
            let mut a = simplified_app();
            a.ui.ribbon_tab = tab;
            let mut h = harness_sized(a, width, 700.0);
            h.run_steps(8);
            assert!(h.query_by_label(MORE_LABEL).is_some(), "{tab:?} at {width}");
            let more = h.get_by_label(MORE_LABEL);
            assert!(more.rect().max.x <= width, "{tab:?} at {width}: More fits, {:?}", more.rect());
            more.click();
            h.run_steps(4);
            // The last segment is in the menu now.
            let probe = match tab {
                RibbonTab::Home | RibbonTab::File => "To Do Tag",
                RibbonTab::Insert => "Date & Time",
                RibbonTab::Draw => "Delete",
                RibbonTab::View => "Dark Mode",
            };
            let _ = h.get_by_label(probe);
        }
    }
    // Widened again, the row takes everything back.
    let mut h = harness_sized(simplified_app(), 400.0, 700.0);
    h.run_steps(8);
    let _ = h.get_by_label(MORE_LABEL);
    h.set_size(egui::vec2(1440.0, 900.0));
    h.run_steps(8);
    assert!(h.query_by_label(MORE_LABEL).is_none());
}

#[test]
fn every_tab_has_segments_with_distinct_keys() {
    for tab in RibbonTab::ALL {
        let segs = segments(tab);
        assert!(!segs.is_empty());
        for s in segs {
            assert_eq!(segs.iter().filter(|o| o.key == s.key).count(), 1, "{tab:?} {}", s.key);
        }
    }
}

#[test]
fn file_options_choose_the_ribbon_too() {
    let mut a = app();
    crate::backstage::open(&mut a, crate::backstage::BackstagePage::Options);
    let mut h = harness(a);
    h.run_steps(3);
    h.get_by_label("Simplified ribbon").click();
    h.run_steps(2);
    h.get_by_label("Show tabs only").click();
    h.run_steps(2);
    assert_eq!((h.state().ui.ribbon_layout, h.state().ui.ribbon_display), (RibbonLayout::Simplified, RibbonDisplay::TabsOnly));
}
