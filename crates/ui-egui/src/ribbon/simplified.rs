//! The simplified ribbon: one thin row of small buttons per tab, groups split by thin dividers and
//! no captions, related commands folded into split buttons and drop-downs (Paste▾ holds Cut and
//! Copy, Styles▾ the style gallery…). It wears the same 2016/2019 skin as the classic ribbon.
//!
//! Each tab is a list of [`Segment`]s in drawing order. Whatever doesn't fit the window's width
//! moves, a whole segment at a time from the right, into the "⋯" More commands menu at the row's
//! end ([`crate::ribbon_fit::overflow_fit`] plans it from the widths measured on earlier frames).
//! The buttons call the same engine commands and set the canvas state the same way the classic
//! tabs do.

pub mod draw;
pub mod history;
pub mod home;
pub mod insert;
pub mod review;
pub mod table;
pub mod view;

use std::collections::HashMap;

use egui::{Align, Layout, Rect, Sense, Ui, UiBuilder, pos2, vec2};

use super::RibbonTab;
use super::display;
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, regular};
use crate::widgets::{self, Btn, DROPDOWN, SMALL_H};
use crate::{NoteApp, tl};

/// One group of a simplified tab: drawn in the row, or in the More commands menu when the row is
/// too narrow.
#[derive(Clone, Copy)]
pub struct Segment {
    /// Names the segment among its tab's (stable between frames).
    pub key: &'static str,
    pub draw: fn(&mut Ui, &mut NoteApp),
}

/// The label of the overflow button.
pub const MORE_LABEL: &str = "More commands";
/// Width of the overflow button.
const MORE_W: f32 = 26.0;
/// Width a segment not measured yet is taken to have.
const GUESS_W: f32 = 90.0;
/// Room kept at the row's right end for the display options arrow.
const ARROW_ROOM: f32 = 30.0;
/// Height of the dividers between segments.
const DIVIDER_H: f32 = 20.0;

/// The segments of `tab`.
pub fn segments(tab: RibbonTab) -> &'static [Segment] {
    match tab {
        RibbonTab::File | RibbonTab::Home => &home::SEGMENTS,
        RibbonTab::Insert => &insert::SEGMENTS,
        RibbonTab::Draw => &draw::SEGMENTS,
        RibbonTab::History => &history::SEGMENTS,
        RibbonTab::Review => &review::SEGMENTS,
        RibbonTab::View => &view::SEGMENTS,
        RibbonTab::Table => &table::SEGMENTS,
    }
}

/// Segment widths measured so far, by tab key (in segment order; `None`: not measured yet).
#[derive(Clone, Default)]
struct Widths(HashMap<&'static str, Vec<Option<f32>>>);

fn widths_id() -> egui::Id {
    egui::Id::unique("nc_simplified_widths")
}

/// Draws the selected tab's row in the ribbon body `full`.
pub fn show(ui: &mut Ui, app: &mut NoteApp, full: Rect) {
    let tab = app.ui.ribbon_tab;
    let segs = segments(tab);
    let ctx = ui.ctx().clone();
    // Small buttons keep their labels (nothing here is compact), and nothing counts savings.
    crate::ribbon_fit::set_compact(&ctx, false);
    let key = tab.key();
    let mut widths = ctx.data(|d| d.get_temp::<Widths>(widths_id())).and_then(|w| w.0.get(key).cloned()).unwrap_or_default();
    widths.resize(segs.len(), None);
    let top = full.min.y + ((full.height() - SMALL_H) / 2.0).floor();
    let row = Rect::from_min_max(pos2(full.min.x + 6.0, top), pos2(full.max.x - ARROW_ROOM, top + SMALL_H));
    let fit = crate::ribbon_fit::overflow_fit(&widths, row.width(), MORE_W + 8.0, GUESS_W);
    let mut measured = widths.clone();
    ui.scope_builder(UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.set_clip_rect(row.expand2(vec2(2.0, 6.0)).intersect(full));
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        for (i, seg) in segs.iter().enumerate().take(fit) {
            let x0 = ui.cursor().min.x;
            if i > 0 {
                display::divider(ui, app, DIVIDER_H);
            }
            (seg.draw)(ui, app);
            if let Some(w) = measured.get_mut(i) {
                *w = Some(ui.cursor().min.x - x0);
            }
        }
        if let Some(rest) = segs.get(fit..)
            && !rest.is_empty()
        {
            ui.add_space(4.0);
            more_button(ui, app, rest);
        }
    });
    if measured != widths {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Widths>(widths_id()).0.insert(key, measured);
        });
        // Plan again with the new widths.
        ctx.request_repaint();
    }
}

/// The "⋯" button holding the segments that don't fit.
fn more_button(ui: &mut Ui, app: &mut NoteApp, rest: &[Segment]) {
    let t = app.tokens.clone();
    let (r, resp) = ui.allocate_exact_size(vec2(MORE_W, SMALL_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, crate::i18n::t(MORE_LABEL)));
    let id = egui::Popup::default_response_id(&resp);
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    widgets::bg(ui, r, &resp, open, true, &t);
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(14.0, 14.0)), Icon::More, icons::Style::mono(t.icon));
    if app.ui.keytips.control(r) {
        egui::Popup::open_id(ui.ctx(), id);
    }
    let resp = resp.on_hover_text(crate::i18n::t(MORE_LABEL));
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 4.0);
        for (i, seg) in rest.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.set_height(SMALL_H);
                ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
                (seg.draw)(ui, app);
            });
        }
    });
}

/// A small split button with its label: the icon and label run the button, the arrow opens
/// `menu`. Returns whether the button part was used.
pub fn labeled_split(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>, menu: impl FnOnce(&mut Ui, &mut NoteApp)) -> bool {
    let t = app.tokens.clone();
    let text_w = crate::theme::text_width(ui.ctx(), b.label, regular(11.5)) + 6.0;
    let sense = if b.enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(24.0 + text_w, SMALL_H), sense);
    let (ar, aresp) = ui.allocate_exact_size(vec2(13.0, SMALL_H), sense);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, b.enabled, b.checked, b.label));
    widgets::bg(ui, r, &resp, b.checked, b.enabled, &t);
    widgets::bg(ui, ar, &aresp, false, b.enabled, &t);
    if b.enabled && (resp.hovered() || aresp.hovered()) {
        ui.painter().rect_stroke(r.union(ar), RADIUS_SMALL, egui::Stroke::new(1.0, t.checked_border), egui::StrokeKind::Inside);
    }
    let style = if b.enabled { icons::Style::new(t.icon, t.accent) } else { icons::Style::disabled(t.icon) };
    icons::paint_styled(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(16.0, 16.0)), b.icon, style);
    let color = if b.enabled { t.text } else { t.text_disabled };
    ui.painter().text(pos2(r.min.x + 24.0, r.center().y), egui::Align2::LEFT_CENTER, b.label, regular(11.5), color);
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(ar.center(), vec2(DROPDOWN, DROPDOWN)),
        Icon::Dropdown,
        icons::Style::mono(if b.enabled { t.icon } else { t.text_disabled }),
    );
    let keyed = app.ui.keytips.control(r);
    let go = b.enabled && (resp.clicked() || keyed);
    widgets::tooltip(resp, &b);
    let aresp = aresp.on_hover_text(crate::i18n::fmt_plain(tl!("{name} options"), &[("name", b.label)]));
    if b.enabled {
        widgets::popup_menu(&aresp, app, menu);
    }
    go
}

/// A drop-down that shows text (no icon) and a marker: Styles▾.
pub fn text_menu(ui: &mut Ui, app: &mut NoteApp, text: &str, tip: &str, enabled: bool, menu: impl FnOnce(&mut Ui, &mut NoteApp)) {
    let t = app.tokens.clone();
    let text_w = crate::theme::text_width(ui.ctx(), text, regular(11.5));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(text_w + DROPDOWN + 18.0, SMALL_H), sense);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tip));
    let id = egui::Popup::default_response_id(&resp);
    let open = egui::Popup::is_id_open(ui.ctx(), id);
    widgets::bg(ui, r, &resp, open, enabled, &t);
    let color = if enabled { t.text } else { t.text_disabled };
    ui.painter().text(pos2(r.min.x + 7.0, r.center().y), egui::Align2::LEFT_CENTER, text, regular(11.5), color);
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(r.max.x - 8.0, r.center().y), vec2(DROPDOWN, DROPDOWN)),
        Icon::Dropdown,
        icons::Style::mono(color),
    );
    if app.ui.keytips.control(r) && enabled {
        egui::Popup::open_id(ui.ctx(), id);
    }
    let resp = resp.on_hover_text(tip);
    if enabled {
        widgets::popup_menu(&resp, app, menu);
    }
}

#[cfg(test)]
mod tests;
