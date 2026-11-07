//! Simplified View: Normal View, Full Page View | Page Color▾, Rule Lines▾, Hide Page Title |
//! Zoom Out, Zoom In, 100%, Page Width | Dark Mode.

use egui::Ui;
use notecraft_doc::RuleLines;
use notecraft_engine::ids;
use serde_json::json;

use super::Segment;
use crate::icons::Icon;
use crate::ribbon::view::{fit_page_width, rule_label};
use crate::theme::{Appearance, color32};
use crate::widgets::{self, Btn, color};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 4] = [
    Segment { key: "views", draw: views },
    Segment { key: "page", draw: page_setup },
    Segment { key: "zoom", draw: zoom },
    Segment { key: "appearance", draw: appearance },
];

fn views(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::NormalView, tl!("Normal View")).icon_only().checked(!app.ui.full_page_view);
    if widgets::small(ui, app, b) {
        app.ui.full_page_view = false;
    }
    let b = Btn::new(Icon::FullPageView, tl!("Full Page View"))
        .shortcut("F11")
        .help(tl!("Show just the page: no ribbon, panes or tabs."))
        .checked(app.ui.full_page_view);
    if widgets::small(ui, app, b) {
        app.ui.full_page_view = !app.ui.full_page_view;
    }
}

fn page_setup(ui: &mut Ui, app: &mut NoteApp) {
    let has_page = app.can(ids::VIEW_PAGE_COLOR);
    let b = Btn::new(Icon::PageColor, tl!("Page Color")).enabled(has_page);
    widgets::small_menu(ui, app, b, page_color_menu);
    let b = Btn::new(Icon::RuleLines, tl!("Rule Lines")).enabled(app.can(ids::VIEW_RULE_LINES));
    widgets::small_menu(ui, app, b, rule_lines_menu);
    let b = Btn::new(Icon::HideTitle, tl!("Hide Page Title")).icon_only().checked(app.ui.hide_page_title).enabled(has_page);
    if widgets::small(ui, app, b) {
        app.ui.hide_page_title = !app.ui.hide_page_title;
    }
}

fn zoom(ui: &mut Ui, app: &mut NoteApp) {
    if widgets::small(ui, app, Btn::new(Icon::ZoomOut, tl!("Zoom Out")).icon_only()) {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_out(app.canvas.zoom));
    }
    if widgets::small(ui, app, Btn::new(Icon::ZoomIn, tl!("Zoom In")).icon_only()) {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_in(app.canvas.zoom));
    }
    if widgets::small(ui, app, Btn::new(Icon::Zoom100, tl!("100%")).icon_only()) {
        app.canvas.set_zoom(1.0);
    }
    if widgets::small(ui, app, Btn::new(Icon::PageWidth, tl!("Page Width"))) {
        fit_page_width(app);
    }
}

fn appearance(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::DarkMode, tl!("Dark Mode"))
        .help(tl!("Switch between the light and dark themes. File › Options can follow the system."))
        .checked(app.ui.dark);
    if widgets::small(ui, app, b) {
        app.ui.prefs.appearance = if app.ui.dark { Appearance::Light } else { Appearance::Dark };
    }
}

fn page_color_menu(ui: &mut Ui, app: &mut NoteApp) {
    let current = app.session.current_page().and_then(|p| p.background.color).map(color32);
    if widgets::check_item(ui, current.is_none(), tl!("No Color")) {
        app.act(ids::VIEW_PAGE_COLOR, json!({ "color": null }));
    }
    ui.separator();
    if let Some(c) = color::grid(ui, &color::PAGE, 4, current) {
        app.act(ids::VIEW_PAGE_COLOR, json!({ "color": color::hex(c) }));
    }
}

fn rule_lines_menu(ui: &mut Ui, app: &mut NoteApp) {
    let current = app.session.current_page().map(|p| p.background.rule).unwrap_or_default();
    widgets::menu_heading(ui, tl!("Rule Lines"));
    for r in [RuleLines::Narrow, RuleLines::College, RuleLines::Standard, RuleLines::Wide] {
        if widgets::check_item(ui, current == r, rule_label(r)) {
            app.act(ids::VIEW_RULE_LINES, json!({ "rule": r.name() }));
        }
    }
    widgets::menu_heading(ui, tl!("Grid Lines"));
    for r in [RuleLines::GridSmall, RuleLines::GridMedium, RuleLines::GridLarge] {
        if widgets::check_item(ui, current == r, rule_label(r)) {
            app.act(ids::VIEW_RULE_LINES, json!({ "rule": r.name() }));
        }
    }
    ui.separator();
    if widgets::check_item(ui, current == RuleLines::None, rule_label(RuleLines::None)) {
        app.act(ids::VIEW_RULE_LINES, json!({ "rule": "none" }));
    }
}
