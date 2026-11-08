//! View: Views (Normal View, Full Page View, Dock to Desktop), Page Setup (Page Color, Rule
//! Lines, Hide Page Title), Zoom (Zoom Out, Zoom In, 100 %, Page Width), Appearance.

use egui::Ui;
use notecraft_doc::RuleLines;
use notecraft_engine::ids;
use serde_json::json;

use crate::icons::Icon;
use crate::theme::{Appearance, color32};
use crate::widgets::{self, Btn, color};
use crate::{NoteApp, tl};

/// The page width Zoom › Page Width fits when the page is emptier than this (points).
const MIN_PAGE_WIDTH: f32 = 600.0;

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Views"), Icon::NormalView, |ui, app| {
        let b = Btn::new(Icon::NormalView, tl!("Normal\nView")).tip(tl!("Normal View")).checked(!app.ui.full_page_view);
        if widgets::big(ui, app, b) {
            app.ui.full_page_view = false;
        }
        let b = Btn::new(Icon::FullPageView, tl!("Full Page\nView"))
            .tip(tl!("Full Page View"))
            .shortcut("F11")
            .help(tl!("Show just the page: no ribbon, panes or tabs."))
            .checked(app.ui.full_page_view);
        if widgets::big(ui, app, b) {
            app.ui.full_page_view = !app.ui.full_page_view;
        }
        let b =
            Btn::new(Icon::DockDesktop, tl!("Dock to\nDesktop")).tip(tl!("Dock to Desktop")).enabled(false).help(tl!("Not available in NoteCraft."));
        widgets::big(ui, app, b);
    });
    widgets::group(ui, app, tl!("Page Setup"), Icon::PageColor, |ui, app| {
        let has_page = app.can(ids::VIEW_PAGE_COLOR);
        let b = Btn::new(Icon::PageColor, tl!("Page\nColor")).tip(tl!("Page Color")).enabled(has_page);
        widgets::big_menu(ui, app, b, page_color_menu);
        let b = Btn::new(Icon::RuleLines, tl!("Rule\nLines")).tip(tl!("Rule Lines")).enabled(app.can(ids::VIEW_RULE_LINES));
        widgets::big_menu(ui, app, b, rule_lines_menu);
        let b = Btn::new(Icon::HideTitle, tl!("Hide\nPage Title")).tip(tl!("Hide Page Title")).checked(app.ui.hide_page_title).enabled(has_page);
        if widgets::big(ui, app, b) {
            app.ui.hide_page_title = !app.ui.hide_page_title;
        }
    });
    widgets::group(ui, app, tl!("Zoom"), Icon::ZoomIn, |ui, app| {
        let b = Btn::new(Icon::ZoomOut, tl!("Zoom\nOut")).tip(tl!("Zoom Out"));
        if widgets::big(ui, app, b) {
            app.canvas.set_zoom(notecraft_geom::zoom::zoom_out(app.canvas.zoom));
        }
        let b = Btn::new(Icon::ZoomIn, tl!("Zoom\nIn")).tip(tl!("Zoom In"));
        if widgets::big(ui, app, b) {
            app.canvas.set_zoom(notecraft_geom::zoom::zoom_in(app.canvas.zoom));
        }
        widgets::rows(ui, app, |ui, app| {
            ui.add_space(4.0);
            if widgets::small(ui, app, Btn::new(Icon::Zoom100, tl!("100%"))) {
                app.canvas.set_zoom(1.0);
            }
            if widgets::small(ui, app, Btn::new(Icon::PageWidth, tl!("Page Width"))) {
                fit_page_width(app);
            }
        });
    });
    widgets::group(ui, app, tl!("Appearance"), Icon::DarkMode, |ui, app| {
        let b = Btn::new(Icon::DarkMode, tl!("Dark\nMode"))
            .tip(tl!("Dark Mode"))
            .help(tl!("Switch between the light and dark themes. File › Options can follow the system."))
            .checked(app.ui.dark);
        if widgets::big(ui, app, b) {
            app.ui.prefs.appearance = if app.ui.dark { Appearance::Light } else { Appearance::Dark };
        }
    });
}

/// Zoom so the page's content (at least [`MIN_PAGE_WIDTH`]) fills the page area's width. A page
/// point is [`crate::canvas::PX_PER_PT`] pixels at 100 %.
pub fn fit_page_width(app: &mut NoteApp) {
    let Some(area) = app.canvas_rect.or(app.canvas.last_rect) else { return };
    let content = app.session.current_page().and_then(|p| p.content_bounds()).map_or(0.0, |b| b.x + b.w);
    let page_w = content.max(MIN_PAGE_WIDTH) + 72.0;
    if area.width() > 40.0 && page_w.is_finite() && page_w > 0.0 {
        app.canvas.set_zoom((area.width() - 40.0) / (page_w * crate::canvas::PX_PER_PT));
    }
}

pub fn page_color_menu(ui: &mut Ui, app: &mut NoteApp) {
    let current = app.session.current_page().and_then(|p| p.background.color).map(color32);
    if crate::widgets::check_item(ui, current.is_none(), tl!("No Color")) {
        app.act(ids::VIEW_PAGE_COLOR, json!({ "color": null }));
    }
    ui.separator();
    if let Some(c) = color::grid(ui, &color::PAGE, 4, current) {
        app.act(ids::VIEW_PAGE_COLOR, json!({ "color": color::hex(c) }));
    }
}

pub fn rule_label(r: RuleLines) -> &'static str {
    match r {
        RuleLines::None => tl!("No Rule Lines"),
        RuleLines::Narrow => tl!("Narrow Ruled"),
        RuleLines::College => tl!("College Ruled"),
        RuleLines::Standard => tl!("Standard Ruled"),
        RuleLines::Wide => tl!("Wide Ruled"),
        RuleLines::GridSmall => tl!("Small Grid"),
        RuleLines::GridMedium => tl!("Medium Grid"),
        RuleLines::GridLarge => tl!("Large Grid"),
    }
}

pub fn rule_lines_menu(ui: &mut Ui, app: &mut NoteApp) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_width_zooms_to_fit() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        app.canvas_rect = Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1400.0, 800.0)));
        fit_page_width(&mut app);
        assert!(app.canvas.zoom > 1.0 && app.canvas.zoom <= notecraft_geom::zoom::MAX);
        // The page width (672 pt with the margin) fills the 1360 px left over, in pixels.
        let px = (MIN_PAGE_WIDTH + 72.0) * app.canvas.scale();
        assert!((px - 1360.0).abs() < 2.0, "{px}");
        for r in RuleLines::ALL {
            assert!(!rule_label(r).is_empty());
        }
    }
}
