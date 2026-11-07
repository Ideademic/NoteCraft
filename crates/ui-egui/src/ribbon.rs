//! The ribbon: the tab row (File, Home, Insert, Draw, View) on the accent band under the title
//! bar, and the selected tab's groups on a light body. File opens the backstage. Double-click a
//! tab to collapse the ribbon to its tabs (Ctrl+F1); a click on a tab then shows it once.

pub mod draw;
pub mod home;
pub mod insert;
pub mod view;

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme::{paint_strong, regular, text_width};
use crate::widgets::{CONTENT_H, LABEL_H};
use crate::{NoteApp, tl};

/// Height of the tab row.
pub const TAB_STRIP_H: f32 = 26.0;
/// Height of the ribbon body (content, group titles, margins).
pub const BODY_H: f32 = CONTENT_H + LABEL_H + 8.0;

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

/// Opens a tab (File opens the backstage); a collapsed ribbon shows the tab once.
pub fn select(app: &mut NoteApp, tab: RibbonTab) {
    if tab == RibbonTab::File {
        app.ui.backstage = true;
        app.ui.keytips.off();
        return;
    }
    if app.ui.ribbon_collapsed {
        app.ui.ribbon_peek = !(app.ui.ribbon_peek && app.ui.ribbon_tab == tab);
    }
    app.ui.ribbon_tab = tab;
}

/// Collapses the ribbon to its tabs, or pins it open again.
pub fn toggle_collapsed(app: &mut NoteApp) {
    app.ui.ribbon_collapsed = !app.ui.ribbon_collapsed;
    app.ui.ribbon_peek = false;
}

/// The tab row (on the accent band).
pub fn tab_strip(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.tab_strip);
    let mut x = full.min.x + 6.0;
    for tab in RibbonTab::ALL {
        let label = tab.label();
        let w = text_width(ui.ctx(), label, regular(12.5)) + 26.0;
        let r = Rect::from_min_max(pos2(x, full.min.y + 2.0), pos2(x + w, full.max.y));
        x += w + 2.0;
        let resp = ui.interact(r, ui.scope_id().with(("ribbon_tab", tab.key())), Sense::click());
        let active = tab != RibbonTab::File && app.ui.ribbon_tab == tab && !app.ui.backstage && (!app.ui.ribbon_collapsed || app.ui.ribbon_peek);
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
        if resp.clicked() {
            select(app, tab);
        }
        if resp.double_clicked() && tab != RibbonTab::File {
            toggle_collapsed(app);
        }
        resp.context_menu(|ui| {
            let text = if app.ui.ribbon_collapsed { tl!("Pin the Ribbon") } else { tl!("Collapse the Ribbon") };
            if crate::widgets::menu_item(ui, None, text, true) {
                toggle_collapsed(app);
            }
        });
    }
    // Collapse / pin at the right end of the row.
    let r = Rect::from_center_size(pos2(full.max.x - 18.0, full.center().y + 1.0), vec2(24.0, 22.0));
    let resp = ui.interact(r, ui.scope_id().with("ribbon_pin"), Sense::click());
    let (icon, tip) = if app.ui.ribbon_collapsed {
        (Icon::ChevronDown, tl!("Pin the Ribbon (Ctrl+F1)"))
    } else {
        (Icon::ChevronUp, tl!("Collapse the Ribbon (Ctrl+F1)"))
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, t.title_hover);
    }
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(14.0, 14.0)), icon, icons::Style::mono(t.tab_text));
    if resp.on_hover_text(tip).clicked() {
        toggle_collapsed(app);
    }
}

/// Whether the ribbon body shows this frame.
pub fn body_visible(app: &NoteApp) -> bool {
    !app.ui.ribbon_collapsed || app.ui.ribbon_peek
}

/// The selected tab's groups.
pub fn body(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.ribbon);
    ui.painter().line_segment([pos2(full.min.x, full.max.y - 0.5), pos2(full.max.x, full.max.y - 0.5)], Stroke::new(1.0, t.border));
    let tab = app.ui.ribbon_tab;
    crate::ribbon_fit::begin(ui.ctx(), tab.key(), ui.available_width() - 16.0);
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
    // A collapsed ribbon shown once goes away after a click outside it or a used button.
    if app.ui.ribbon_peek {
        let click = ui.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None });
        if let Some(p) = click
            && !full.contains(p)
            && !egui::Popup::is_any_open(ui.ctx())
        {
            app.ui.ribbon_peek = false;
        }
    }
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
        assert!(body_visible(&app), "a click on a tab shows a collapsed ribbon once");
        toggle_collapsed(&mut app);
        assert!(!app.ui.ribbon_collapsed && body_visible(&app));
    }
}
