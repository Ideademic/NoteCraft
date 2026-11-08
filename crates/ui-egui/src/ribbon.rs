//! The ribbon: the tab row (File, Home, Insert, Draw, View) on the accent band under the title
//! bar, and the selected tab's commands on a light body. File opens the backstage.
//!
//! The body has two layouts ([`display::RibbonLayout`]): Classic, groups of large and small
//! buttons with captions (OneNote 2016/2019), and Simplified, one thin row of small buttons with
//! related commands in drop-downs ([`simplified`]). How much of the ribbon shows is a display
//! option too ([`display::RibbonDisplay`]): always, tabs only (Ctrl+F1 or the display options;
//! a click on a tab then shows the ribbon for good), or full-screen mode. Both are chosen from the
//! right-click menu on empty ribbon space or the arrow at the ribbon's bottom-right corner. A
//! double-click on a tab doesn't hide the ribbon: that was too easy to do by accident.

pub mod display;
pub mod draw;
pub mod font_box;
pub mod home;
pub mod insert;
pub mod paragraph;
pub mod simplified;
pub mod view;

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::{paint_strong, regular, text_width};
use crate::widgets::{CONTENT_H, LABEL_H, SMALL_H};
use crate::{NoteApp, tl};
use display::{RibbonDisplay, RibbonLayout};

/// Height of the tab row.
pub const TAB_STRIP_H: f32 = 26.0;
/// Height of the classic ribbon body (content, group titles, margins).
pub const BODY_H: f32 = CONTENT_H + LABEL_H + 8.0;
/// Height of the simplified ribbon body: one row of small buttons.
pub const SIMPLIFIED_H: f32 = SMALL_H + 14.0;

/// Ribbon tabs. File opens the backstage instead of a tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RibbonTab {
    File,
    #[default]
    Home,
    Insert,
    Draw,
    View,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 5] = [RibbonTab::File, RibbonTab::Home, RibbonTab::Insert, RibbonTab::Draw, RibbonTab::View];

    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::File => tl!("File"),
            RibbonTab::Home => tl!("Home"),
            RibbonTab::Insert => tl!("Insert"),
            RibbonTab::Draw => tl!("Draw"),
            RibbonTab::View => tl!("View"),
        }
    }

    /// The key the fit planner keeps this tab's group widths under.
    fn key(self) -> &'static str {
        match self {
            RibbonTab::File => "file",
            RibbonTab::Home => "home",
            RibbonTab::Insert => "insert",
            RibbonTab::Draw => "draw",
            RibbonTab::View => "view",
        }
    }
}

/// Opens a tab (File opens the backstage). With only the tabs showing, opening one pins the
/// ribbon again (Always show): a ribbon that hid itself after every button was more surprising
/// than useful. In full-screen mode the tab shows while the tabs and ribbon are revealed.
pub fn select(app: &mut NoteApp, tab: RibbonTab) {
    if tab == RibbonTab::File {
        app.ui.backstage = true;
        app.ui.keytips.off();
        return;
    }
    match app.ui.ribbon_display {
        RibbonDisplay::Always => {}
        RibbonDisplay::TabsOnly => display::set_display(app, RibbonDisplay::Always),
        RibbonDisplay::FullScreen => app.ui.ribbon_peek = !(app.ui.ribbon_peek && app.ui.ribbon_tab == tab),
    }
    app.ui.ribbon_tab = tab;
}

/// Ctrl+F1: shows only the tabs, or the whole ribbon again.
pub fn toggle_collapsed(app: &mut NoteApp) {
    display::toggle_tabs_only(app);
}

/// Height of the ribbon body in the current layout.
pub fn body_h(app: &NoteApp) -> f32 {
    match app.ui.ribbon_layout {
        RibbonLayout::Classic => BODY_H,
        RibbonLayout::Simplified => SIMPLIFIED_H,
    }
}

/// The tab row (on the accent band).
pub fn tab_strip(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.tab_strip);
    // Empty band: right-click for the display options (registered first: the tabs keep theirs).
    let band = ui.interact(full, ui.scope_id().with("nc_tab_band"), Sense::click());
    let mut x = full.min.x + 6.0;
    for tab in RibbonTab::ALL {
        let label = tab.label();
        let w = text_width(ui.ctx(), label, regular(12.5)) + 26.0;
        let r = Rect::from_min_max(pos2(x, full.min.y + 2.0), pos2(x + w, full.max.y));
        x += w + 2.0;
        let resp = ui.interact(r, ui.scope_id().with(("ribbon_tab", tab.key())), Sense::click());
        let active = tab != RibbonTab::File && app.ui.ribbon_tab == tab && !app.ui.backstage && display::body_visible(app);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, active, label));
        app.ui.keytips.tab(r, crate::keytips::tab_letter(tab));
        let radius = egui::CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 };
        if tab == RibbonTab::File {
            let fill = if resp.hovered() { t.title_pressed } else { t.file_tab };
            ui.painter().rect_filled(r, radius, fill);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, regular(12.5), t.on_nav);
        } else if active {
            ui.painter().rect_filled(r, radius, t.ribbon);
            paint_strong(ui.painter(), r.center(), Align2::CENTER_CENTER, label, regular(12.5), t.tab_selected);
        } else {
            if resp.hovered() {
                ui.painter().rect_filled(r, radius, t.title_hover);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, label, regular(12.5), t.tab_text);
        }
        // A double-click is two clicks: it opens the tab, and never collapses the ribbon.
        if resp.clicked() {
            select(app, tab);
        }
        if tab != RibbonTab::File {
            resp.context_menu(|ui| display::context_menu(ui, app));
        }
    }
    // With only the tabs showing, the display options sit at the right end of the row.
    if !display::body_visible(app) {
        let r = Rect::from_center_size(pos2(full.max.x - 18.0, full.center().y + 1.0), vec2(24.0, 22.0));
        display::options_button(ui, app, r, true);
    }
    band.context_menu(|ui| display::context_menu(ui, app));
}

/// Whether the ribbon body shows this frame.
pub fn body_visible(app: &NoteApp) -> bool {
    display::body_visible(app)
}

/// The selected tab's commands, in the chosen layout, with the display options arrow at the
/// bottom-right corner.
pub fn body(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.ribbon);
    ui.painter().line_segment([pos2(full.min.x, full.max.y - 0.5), pos2(full.max.x, full.max.y - 0.5)], Stroke::new(1.0, t.border));
    // Empty ribbon: right-click for the display options (registered first: controls keep theirs).
    let empty = ui.interact(full, ui.scope_id().with("nc_ribbon_empty"), Sense::click());
    let arrow = match app.ui.ribbon_layout {
        RibbonLayout::Classic => {
            classic(ui, app, full);
            Rect::from_center_size(pos2(full.max.x - 14.0, full.max.y - 11.0), vec2(20.0, 16.0))
        }
        RibbonLayout::Simplified => {
            simplified::show(ui, app, full);
            Rect::from_center_size(pos2(full.max.x - 14.0, full.center().y), vec2(20.0, SMALL_H))
        }
    };
    display::options_button(ui, app, arrow, false);
    empty.context_menu(|ui| display::context_menu(ui, app));
}

/// The classic body: the tab's groups, folding to fit ([`crate::ribbon_fit`]).
fn classic(ui: &mut Ui, app: &mut NoteApp, full: Rect) {
    let tab = app.ui.ribbon_tab;
    // Room for the groups: the margins and the display options arrow.
    crate::ribbon_fit::begin(ui.ctx(), tab.key(), full.width() - 40.0);
    egui::ScrollArea::horizontal()
        .id_salt(("ribbon_scroll", tab.key()))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                match tab {
                    RibbonTab::File | RibbonTab::Home => home::show(ui, app),
                    RibbonTab::Insert => insert::show(ui, app),
                    RibbonTab::Draw => draw::show(ui, app),
                    RibbonTab::View => view::show(ui, app),
                }
            });
        });
    crate::ribbon_fit::end(ui.ctx());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_and_collapse() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        select(&mut app, RibbonTab::Draw);
        assert_eq!(app.ui.ribbon_tab, RibbonTab::Draw);
        select(&mut app, RibbonTab::File);
        assert!(app.ui.backstage);
        assert_eq!(app.ui.ribbon_tab, RibbonTab::Draw, "File doesn't replace the tab");
        toggle_collapsed(&mut app);
        assert!(!body_visible(&app));
        select(&mut app, RibbonTab::Home);
        assert!(app.ui.ribbon_display == RibbonDisplay::Always && body_visible(&app), "a click on a tab pins a collapsed ribbon");
        assert!(!app.ui.ribbon_peek, "no peek that hides again");
        toggle_collapsed(&mut app);
        assert_eq!(app.ui.ribbon_display, RibbonDisplay::TabsOnly);
        // File opens the backstage and leaves the display alone.
        select(&mut app, RibbonTab::File);
        assert_eq!(app.ui.ribbon_display, RibbonDisplay::TabsOnly);
        // Full-screen mode keeps its reveal: a tab shows while revealed, the mode stays.
        display::set_display(&mut app, RibbonDisplay::FullScreen);
        select(&mut app, RibbonTab::Insert);
        assert!(app.ui.ribbon_peek && app.ui.ribbon_display == RibbonDisplay::FullScreen);
    }
}
