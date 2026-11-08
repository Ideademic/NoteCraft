//! Ribbon display options, as in current Office versions but in the 2016/2019 skin: the ribbon's
//! layout (Classic: tall groups with large buttons and captions; Simplified: one thin row) and
//! how much of it shows (Always; Show tabs only, where a click on a tab brings the ribbon back
//! for good; Full-screen mode, where tabs and ribbon hide behind a thin strip at the top of the window that
//! shows them while needed).
//!
//! The choices are offered from the right-click menu on empty ribbon or tab-strip space and from
//! the small arrow at the ribbon's bottom-right corner ([`options_button`]), and in File ›
//! Options ([`options`]). They persist in [`crate::Prefs`].

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::icons::{self, Icon};
use crate::theme::regular;
use crate::widgets;
use crate::{NoteApp, tl};

/// How the ribbon lays out its commands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RibbonLayout {
    /// Groups of large and small buttons with captions (OneNote 2016/2019).
    #[default]
    Classic,
    /// One thin row of small buttons; related commands fold into drop-downs.
    Simplified,
}

/// How much of the ribbon shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RibbonDisplay {
    /// Tabs and ribbon hide; a thin strip at the top shows them while needed.
    FullScreen,
    /// Only the tabs show; a click on one (or a KeyTip that opens one) switches back to
    /// [`RibbonDisplay::Always`].
    TabsOnly,
    /// Tabs and ribbon always show.
    #[default]
    Always,
}

/// Height of the full-screen strip that stands in for the title bar.
pub const FULL_SCREEN_BAR_H: f32 = 18.0;
/// Height of the strip under an app-drawn title bar (it keeps its window buttons) in full-screen
/// mode.
pub const REVEAL_STRIP_H: f32 = 10.0;

/// The label of the bottom-right arrow (and of its menu).
pub const OPTIONS_LABEL: &str = "Ribbon Display Options";
/// The label of the full-screen strip's button that shows the ribbon.
pub const REVEAL_LABEL: &str = "Show the Ribbon";

/// Chooses how much of the ribbon shows (a ribbon shown once goes away).
pub fn set_display(app: &mut NoteApp, d: RibbonDisplay) {
    app.ui.ribbon_display = d;
    app.ui.ribbon_peek = false;
}

/// Chooses the ribbon's layout.
pub fn set_layout(app: &mut NoteApp, l: RibbonLayout) {
    app.ui.ribbon_layout = l;
}

/// Ctrl+F1: Show tabs only, or Always show again. (A double-click on a tab doesn't do this: it was
/// too easy to hide the ribbon by accident.)
pub fn toggle_tabs_only(app: &mut NoteApp) {
    let next = if app.ui.ribbon_display == RibbonDisplay::Always { RibbonDisplay::TabsOnly } else { RibbonDisplay::Always };
    set_display(app, next);
}

/// Whether the tab row shows this frame.
pub fn tabs_visible(app: &NoteApp) -> bool {
    app.ui.ribbon_display != RibbonDisplay::FullScreen || app.ui.ribbon_peek
}

/// Whether the ribbon body shows this frame.
pub fn body_visible(app: &NoteApp) -> bool {
    app.ui.ribbon_display == RibbonDisplay::Always || app.ui.ribbon_peek
}

/// Whether the full-screen strip stands in for the title bar this frame.
pub fn full_screen_bar(app: &NoteApp) -> bool {
    app.ui.ribbon_display == RibbonDisplay::FullScreen && !app.ui.ribbon_peek
}

/// The layout and display choices, with check marks on the current ones.
pub fn menu_body(ui: &mut Ui, app: &mut NoteApp) {
    // Menu items fill the width there is: keep the menu to a menu's width.
    ui.set_min_width(220.0);
    ui.set_max_width(220.0);
    widgets::menu_heading(ui, tl!("Ribbon layout"));
    for (l, text) in [(RibbonLayout::Classic, tl!("Classic ribbon")), (RibbonLayout::Simplified, tl!("Simplified ribbon"))] {
        if widgets::check_item(ui, app.ui.ribbon_layout == l, text) {
            set_layout(app, l);
        }
    }
    ui.separator();
    widgets::menu_heading(ui, tl!("Show ribbon"));
    for (d, text) in [
        (RibbonDisplay::FullScreen, tl!("Full-screen mode")),
        (RibbonDisplay::TabsOnly, tl!("Show tabs only")),
        (RibbonDisplay::Always, tl!("Always show ribbon")),
    ] {
        if widgets::check_item(ui, app.ui.ribbon_display == d, text) {
            set_display(app, d);
        }
    }
}

/// The right-click menu on empty ribbon or tab-strip space (and on a tab).
pub fn context_menu(ui: &mut Ui, app: &mut NoteApp) {
    menu_body(ui, app);
    ui.separator();
    if app.ui.ribbon_display == RibbonDisplay::Always {
        if widgets::menu_item(ui, None, tl!("Collapse the Ribbon"), true) {
            set_display(app, RibbonDisplay::TabsOnly);
        }
    } else if widgets::menu_item(ui, None, tl!("Pin the Ribbon"), true) {
        set_display(app, RibbonDisplay::Always);
    }
}

/// The small arrow that opens the display options, in `r`. `on_band`: drawn on the accent tab
/// strip (Show tabs only) rather than on the ribbon.
pub fn options_button(ui: &mut Ui, app: &mut NoteApp, r: Rect, on_band: bool) {
    let t = app.tokens.clone();
    let resp = ui.interact(r, ui.scope_id().with("nc_ribbon_display_options"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::t(OPTIONS_LABEL)));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let (hover, pressed, ink) = if on_band { (t.title_hover, t.title_pressed, t.tab_text) } else { (t.hover, t.pressed, t.icon) };
    if open || resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(r, 2.0, pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, hover);
    }
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(10.0, 10.0)), Icon::ChevronDown, icons::Style::mono(ink));
    let resp = resp.on_hover_text(tl!("Ribbon Display Options (Ctrl+F1 shows only the tabs)"));
    egui::Popup::menu(&resp).show(|ui| menu_body(ui, app));
}

/// Full-screen mode's strip in place of the title bar: the window title, small, and a "⋯" button
/// at the right. A click on the strip (or the button) shows the tabs and ribbon; dragging it moves
/// the window.
pub fn full_screen_strip(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.title_bar);
    let strip = craft_window::title_bar::empty_area(ui, full);
    let left = full.min.x + if app.integrated_titlebar { crate::app::MAC_CONTENT_LEFT } else { 8.0 };
    let title = crate::chrome::window_title(app);
    let font = regular(if full.height() >= 24.0 { 12.0 } else { 10.5 });
    let room = (full.width() - (left - full.min.x) - 60.0).max(0.0);
    let shown = crate::theme::fit_text(ui.ctx(), &title, &font, room.min(full.width() * 0.6));
    if !shown.is_empty() {
        ui.painter().text(full.center(), Align2::CENTER_CENTER, shown, font, t.on_title_bar);
    }
    let more = Rect::from_center_size(pos2(full.max.x - 22.0, full.center().y), vec2(32.0, full.height()));
    let resp = reveal_button(ui, app, more);
    if strip.clicked() || resp {
        app.ui.ribbon_peek = true;
    }
}

/// The thin strip under an app-drawn title bar in full-screen mode: a click shows the ribbon.
pub fn reveal_strip(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.tab_strip);
    let resp = ui.interact(full, ui.scope_id().with("nc_reveal_strip"), Sense::click());
    let more = Rect::from_center_size(pos2(full.max.x - 22.0, full.center().y), vec2(32.0, full.height()));
    if reveal_button(ui, app, more) || resp.clicked() {
        app.ui.ribbon_peek = true;
    }
}

/// The "⋯" that shows the tabs and ribbon in full-screen mode. Returns whether it was clicked.
fn reveal_button(ui: &mut Ui, app: &NoteApp, r: Rect) -> bool {
    let t = &app.tokens;
    let resp = ui.interact(r, ui.scope_id().with("nc_ribbon_reveal"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::t(REVEAL_LABEL)));
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(r, 0.0, t.title_pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, t.title_hover);
    }
    // Three dots, drawn: they stay crisp at any strip height.
    for k in [-1.0, 0.0, 1.0] {
        ui.painter().circle_filled(pos2(r.center().x + k * 5.0, r.center().y), 1.4, t.on_title_bar);
    }
    resp.on_hover_text(tl!("Show the tabs and ribbon (Esc hides them)")).clicked()
}

/// After the ribbon is drawn: a ribbon revealed in full-screen mode goes away
/// after a click outside `area` (the title bar, tabs and ribbon) or Esc; Esc in full-screen mode
/// with nothing else to cancel shows the ribbon again for good.
pub fn end_frame(ui: &Ui, app: &mut NoteApp, area: Rect) {
    let ctx = ui.ctx();
    let popup = egui::Popup::is_any_open(ctx);
    if app.ui.ribbon_peek {
        let click = ui.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None });
        if let Some(p) = click
            && !area.contains(p)
            && !popup
        {
            app.ui.ribbon_peek = false;
        }
    }
    if popup || app.ui.keytips.active() || app.ui.prompt.is_some() {
        return;
    }
    if app.ui.ribbon_peek {
        // Esc puts a shown ribbon away; whatever has the keyboard still gets the key.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.ui.ribbon_peek = false;
        }
        return;
    }
    let idle = app.canvas.editing.is_none() && app.canvas.selection.is_empty() && !ctx.egui_wants_keyboard_input();
    if app.ui.ribbon_display == RibbonDisplay::FullScreen && idle && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        set_display(app, RibbonDisplay::Always);
    }
}

/// File › Options: the same two choices.
pub fn options(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Layout:")).font(regular(12.5)).color(t.text_dim));
        for (l, text) in [(RibbonLayout::Classic, tl!("Classic ribbon")), (RibbonLayout::Simplified, tl!("Simplified ribbon"))] {
            if ui.radio(app.ui.ribbon_layout == l, text).clicked() {
                set_layout(app, l);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Show:")).font(regular(12.5)).color(t.text_dim));
        for (d, text) in [
            (RibbonDisplay::Always, tl!("Always show ribbon")),
            (RibbonDisplay::TabsOnly, tl!("Show tabs only")),
            (RibbonDisplay::FullScreen, tl!("Full-screen mode")),
        ] {
            if ui.radio(app.ui.ribbon_display == d, text).clicked() {
                set_display(app, d);
            }
        }
    });
}

/// A thin divider between simplified-ribbon groups.
pub fn divider(ui: &mut Ui, app: &NoteApp, h: f32) {
    ui.add_space(3.0);
    let (r, _) = ui.allocate_exact_size(vec2(1.0, h), Sense::hover());
    ui.painter().line_segment([pos2(r.center().x, r.min.y), pos2(r.center().x, r.max.y)], Stroke::new(1.0, app.tokens.border));
    ui.add_space(3.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_choices() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        assert_eq!(app.ui.ribbon_display, RibbonDisplay::Always);
        assert!(tabs_visible(&app) && body_visible(&app) && !full_screen_bar(&app));
        set_display(&mut app, RibbonDisplay::FullScreen);
        assert!(!tabs_visible(&app) && !body_visible(&app) && full_screen_bar(&app));
        app.ui.ribbon_peek = true;
        assert!(tabs_visible(&app) && body_visible(&app) && !full_screen_bar(&app));
        toggle_tabs_only(&mut app);
        assert_eq!(app.ui.ribbon_display, RibbonDisplay::Always);
        assert!(!app.ui.ribbon_peek);
        toggle_tabs_only(&mut app);
        assert_eq!(app.ui.ribbon_display, RibbonDisplay::TabsOnly);
        assert!(tabs_visible(&app) && !body_visible(&app));
    }
}
