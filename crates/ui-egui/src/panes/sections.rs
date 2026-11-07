//! The section tabs across the top of the page area: one coloured tab per section of the level
//! being shown (the notebook's own sections, or a section group's), section group tabs, an "up"
//! tab inside a group, a "+" tab, an overflow menu, and the search box at the right end. The
//! current tab joins the band in its colour that runs over the page. Drag a tab to reorder;
//! right-click for Rename, Delete, Section Color, Move or Copy, New Section, New Section Group.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::{Guid, SectionEntry};
use notecraft_engine::ids;
use serde_json::json;

use crate::app::RenameTarget;
use crate::icons::{self, Icon};
use crate::theme::{fit_text, on, paint_strong, regular, text_width};
use crate::{NoteApp, tl};

/// Height of the strip, and of the band under it.
pub const STRIP_H: f32 = 34.0;
pub const BAND_H: f32 = 4.0;
const TAB_H: f32 = 24.0;
const SEARCH_W: f32 = 230.0;

/// One tab of the strip.
#[derive(Clone, Debug)]
struct Tab {
    id: Guid,
    name: String,
    /// `None` for a section group.
    color: Option<notecraft_doc::Color>,
    /// Its place in the shown entry list.
    index: usize,
    width: f32,
}

/// The group whose entries the strip shows (`None`: the notebook's own), from where the current
/// section sits.
fn shown_group(app: &NoteApp) -> Option<Guid> {
    let nb = app.session.current_notebook()?;
    let sec = app.session.current.section?;
    nb.path_of(sec)?.last().copied()
}

fn tabs(app: &NoteApp, ctx: &egui::Context) -> Vec<Tab> {
    let Some(nb) = app.session.current_notebook() else { return Vec::new() };
    let entries = match shown_group(app) {
        Some(g) => nb.group(g).map(|g| g.entries.as_slice()).unwrap_or(&[]),
        None => nb.sections.as_slice(),
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, e)| {
            let (color, extra) = match e {
                SectionEntry::Section(s) => (Some(s.color), 0.0),
                SectionEntry::Group(_) => (None, 20.0),
            };
            let width = (text_width(ctx, e.name(), regular(12.5)) + 24.0 + extra).clamp(48.0, 200.0);
            Tab { id: e.id(), name: e.name().to_owned(), color, index, width }
        })
        .collect()
}

fn drag_id() -> egui::Id {
    egui::Id::unique("nc_section_tab_drag")
}

/// The tab strip with the search box.
pub fn section_tabs(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), STRIP_H), Sense::hover());
    ui.painter().rect_filled(strip, 0.0, t.section_strip);
    let current = app.session.current.section;
    let current_color = app.session.current_section().map(|s| s.color);
    let band = Rect::from_min_max(pos2(strip.min.x, strip.max.y - BAND_H), strip.max);
    ui.painter().rect_filled(band, 0.0, current_color.map_or(t.border, |c| t.section_fill(c)));
    // Search at the right end.
    let search_w = SEARCH_W.min((strip.width() * 0.3).max(120.0));
    let search = Rect::from_min_size(pos2(strip.max.x - search_w - 8.0, strip.min.y + 4.0), vec2(search_w, TAB_H));
    super::search::search_box(ui, app, search);
    let ctx = ui.ctx().clone();
    let mut x = strip.min.x + 6.0;
    let tab_bottom = band.min.y;
    // Inside a section group: the "up" tab goes back to the level above.
    let group = shown_group(app);
    if let Some(g) = group {
        let r = Rect::from_min_max(pos2(x, tab_bottom - TAB_H), pos2(x + 30.0, tab_bottom));
        x += 32.0;
        let resp = ui.interact(r, ui.scope_id().with("tab_up"), Sense::click());
        let parent_name = app.session.current_notebook().and_then(|nb| nb.group(g)).map(|gr| gr.name.clone()).unwrap_or_default();
        let tip = crate::i18n::fmt_plain(tl!("Back from \u{201c}{group}\u{201d}"), &[("group", &parent_name)]);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &tip));
        let fill = if resp.hovered() { t.hover } else { t.panel };
        ui.painter().rect(r, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, fill, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(14.0, 14.0)), Icon::Back, icons::Style::mono(t.accent_text));
        if resp.on_hover_text(tip).clicked() {
            go_up(&ctx, app, g);
        }
    }
    let all = tabs(app, &ctx);
    let plus_w = 30.0;
    let more_w = 26.0;
    let room = (search.min.x - 10.0 - x - plus_w).max(0.0);
    let visible = fit(&all, room, more_w, current);
    let dragging = ui.data(|d| d.get_temp::<Guid>(drag_id()));
    let mut slots: Vec<(f32, usize)> = Vec::new();
    for tab in all.iter().filter(|tb| visible.contains(&tb.id)) {
        let is_current = current == Some(tab.id);
        let r = if is_current {
            Rect::from_min_max(pos2(x, tab_bottom - TAB_H - 3.0), pos2(x + tab.width, band.max.y))
        } else {
            Rect::from_min_max(pos2(x, tab_bottom - TAB_H), pos2(x + tab.width, tab_bottom))
        };
        slots.push((x, tab.index));
        x += tab.width + 2.0;
        draw_tab(ui, app, tab, r, is_current, dragging == Some(tab.id));
    }
    slots.push((x, all.len()));
    // The overflow menu.
    if visible.len() < all.len() {
        let r = Rect::from_min_max(pos2(x, tab_bottom - TAB_H), pos2(x + more_w, tab_bottom));
        x += more_w + 2.0;
        let resp = ui.interact(r, ui.scope_id().with("tab_more"), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("More sections")));
        if resp.hovered() {
            ui.painter().rect_filled(r, 2.0, t.hover);
        }
        icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(12.0, 12.0)), Icon::ChevronDown, icons::Style::mono(t.icon));
        let resp = resp.on_hover_text(tl!("More sections"));
        let hidden: Vec<Tab> = all.iter().filter(|tb| !visible.contains(&tb.id)).cloned().collect();
        egui::Popup::menu(&resp).show(|ui| {
            for tab in hidden {
                let icon = if tab.color.is_some() { Icon::NewSection } else { Icon::SectionGroup };
                if crate::widgets::menu_item(ui, Some(icon), &tab.name, true) {
                    open_tab(&ctx, app, &tab);
                }
            }
        });
    }
    // The "+" tab: a new section at this level.
    let r = Rect::from_min_max(pos2(x, tab_bottom - TAB_H), pos2(x + plus_w, tab_bottom));
    let resp = ui.interact(r, ui.scope_id().with("tab_plus"), Sense::click());
    let can = app.can(ids::SECTION_NEW);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, tl!("Create a New Section")));
    let fill = if resp.hovered() && can { t.hover } else { t.panel };
    ui.painter().rect(r, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, fill, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(r.center(), vec2(12.0, 12.0)),
        Icon::Plus,
        icons::Style::mono(if can { t.accent_text } else { t.text_disabled }),
    );
    let shortcut = crate::widgets::shortcut_text("Mod+T");
    if resp.on_hover_text(format!("{} ({shortcut})", tl!("Create a New Section"))).clicked() && can {
        app.act(ids::SECTION_NEW, json!({ "group": group }));
    }
    // Dragging a tab: show where it lands, move it there on release.
    if let Some(id) = dragging {
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let released = ui.input(|i| i.pointer.any_released());
        let slot = pointer.map(|p| drop_slot(&slots, p.x));
        if let Some(i) = slot
            && let Some((sx, _)) = slots.get(i)
        {
            ui.painter().rect_filled(Rect::from_min_max(pos2(sx - 2.0, tab_bottom - TAB_H), pos2(sx + 1.0, tab_bottom)), 0.0, t.accent_text);
        }
        if released {
            ui.data_mut(|d| d.remove::<Guid>(drag_id()));
            let from = all.iter().find(|tb| tb.id == id).map(|tb| tb.index);
            let to = slot.and_then(|i| slots.get(i)).map(|(_, idx)| *idx);
            if let (Some(from), Some(to)) = (from, to)
                && let Some(index) = moved_index(from, to)
            {
                app.act(ids::SECTION_MOVE, json!({ "id": id, "index": index, "group": group }));
            }
        }
    }
}

/// The slot (gap before tab `i`, or after the last) nearest to `x`.
fn drop_slot(slots: &[(f32, usize)], x: f32) -> usize {
    let mut best = 0;
    let mut dist = f32::INFINITY;
    for (i, (sx, _)) in slots.iter().enumerate() {
        let d = (sx - x).abs();
        if d < dist {
            dist = d;
            best = i;
        }
    }
    best
}

/// The `index` `section.move` takes (the list without the moved entry) to put the entry at `from`
/// before the entry now at `to`; `None` when it stays where it is.
pub fn moved_index(from: usize, to: usize) -> Option<usize> {
    let index = if to > from { to - 1 } else { to };
    (index != from).then_some(index)
}

/// The tabs that fit in `room` (keeping the current one shown), in order.
fn fit(all: &[Tab], room: f32, more_w: f32, current: Option<Guid>) -> Vec<Guid> {
    let total: f32 = all.iter().map(|t| t.width + 2.0).sum();
    if total <= room {
        return all.iter().map(|t| t.id).collect();
    }
    let room = room - more_w - 2.0;
    let mut shown = Vec::new();
    let mut used = 0.0;
    for t in all {
        if used + t.width + 2.0 > room {
            break;
        }
        used += t.width + 2.0;
        shown.push(t.id);
    }
    if let Some(cur) = current
        && !shown.contains(&cur)
        && let Some(ct) = all.iter().find(|t| t.id == cur)
    {
        while !shown.is_empty() && used + ct.width + 2.0 > room {
            if let Some(last) = shown.pop() {
                used -= all.iter().find(|t| t.id == last).map_or(0.0, |t| t.width + 2.0);
            }
        }
        shown.push(cur);
    }
    shown
}

fn open_tab(ctx: &egui::Context, app: &mut NoteApp, tab: &Tab) {
    if tab.color.is_some() { super::go_section(ctx, app, tab.id) } else { super::go_group(ctx, app, tab.id) }
}

/// Leaves group `g` for the level above: the first section there, or of the group itself.
fn go_up(ctx: &egui::Context, app: &mut NoteApp, g: Guid) {
    let Some(nb) = app.session.current_notebook() else { return };
    let parent_entries = match nb.path_of(g).and_then(|p| p.last().copied()) {
        Some(pg) => nb.group(pg).map(|x| x.entries.clone()).unwrap_or_default(),
        None => nb.sections.clone(),
    };
    let target = parent_entries.iter().find_map(|e| match e {
        SectionEntry::Section(s) => Some(s.id),
        SectionEntry::Group(_) => None,
    });
    match target {
        Some(s) => super::go_section(ctx, app, s),
        None => app.set_status(tl!("There are no sections above this group yet."), false),
    }
}

fn draw_tab(ui: &mut Ui, app: &mut NoteApp, tab: &Tab, r: Rect, is_current: bool, dragging: bool) {
    let t = app.tokens.clone();
    let resp = ui.interact(r, ui.scope_id().with(("section_tab", tab.id)), Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, is_current, &tab.name));
    let radius = CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 };
    let (fill, text) = match tab.color {
        Some(c) => {
            let base = t.section_fill(c);
            let fill = if is_current {
                base
            } else if resp.hovered() {
                crate::theme::mix(base, egui::Color32::WHITE, 0.25)
            } else {
                crate::theme::mix(base, t.section_strip, 0.18)
            };
            (fill, on(fill))
        }
        None => (if resp.hovered() { t.hover } else { t.panel }, t.text),
    };
    ui.painter().rect_filled(r, radius, fill);
    if tab.color.is_none() {
        ui.painter().rect_stroke(r, radius, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    }
    if dragging {
        ui.painter().rect_stroke(r, radius, Stroke::new(1.5, t.accent_text), egui::StrokeKind::Inside);
    }
    let mut tx = r.min.x + 12.0;
    if tab.color.is_none() {
        icons::paint_styled(
            ui.painter(),
            Rect::from_center_size(pos2(tx + 6.0, r.center().y), vec2(14.0, 14.0)),
            Icon::SectionGroup,
            icons::Style::new(t.icon, t.accent),
        );
        tx += 18.0;
    }
    let text_rect = Rect::from_min_max(pos2(tx - 6.0, r.min.y + 2.0), pos2(r.max.x - 4.0, r.max.y - 2.0));
    let target = RenameTarget::Section(tab.id);
    if !super::rename_box(ui, app, target, text_rect) {
        let font = regular(12.5);
        let shown = fit_text(ui.ctx(), &tab.name, &font, (r.max.x - tx - 8.0).max(0.0));
        let cy = if is_current { r.center().y - 1.5 } else { r.center().y };
        if is_current {
            paint_strong(ui.painter(), pos2(tx, cy), Align2::LEFT_CENTER, &shown, font, text);
        } else {
            ui.painter().text(pos2(tx, cy), Align2::LEFT_CENTER, &shown, font, text);
        }
    }
    let ctx = ui.ctx().clone();
    if resp.drag_started() && tab.color.is_some() {
        ui.data_mut(|d| d.insert_temp(drag_id(), tab.id));
    }
    if resp.clicked() {
        open_tab(&ctx, app, tab);
    }
    if resp.double_clicked() {
        super::start_rename(app, target);
    }
    let resp = if tab.name.chars().count() > 24 { resp.on_hover_text(&tab.name) } else { resp };
    let name = tab.name.clone();
    resp.context_menu(|ui| match tab.color {
        Some(c) => super::notebooks::section_menu(ui, app, tab.id, &name, c),
        None => super::notebooks::group_menu(ui, app, tab.id, &name),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_a_tab_counts_without_it() {
        // [A B C D]: A before D lands at 2 in [B C D]; D before A at 0; no-ops stay.
        assert_eq!(moved_index(0, 3), Some(2));
        assert_eq!(moved_index(3, 0), Some(0));
        assert_eq!(moved_index(1, 1), None);
        assert_eq!(moved_index(1, 2), None);
        assert_eq!(moved_index(0, 4), Some(3));
    }

    #[test]
    fn the_current_tab_always_fits() {
        let mk = |i: usize| Tab { id: Guid::new(), name: format!("S{i}"), color: Some(notecraft_doc::Color::WHITE), index: i, width: 100.0 };
        let all: Vec<Tab> = (0..10).map(mk).collect();
        let cur = all.get(9).map(|t| t.id);
        let shown = fit(&all, 450.0, 26.0, cur);
        assert!(shown.len() < all.len());
        assert!(cur.is_some_and(|c| shown.contains(&c)));
        assert_eq!(fit(&all, 10_000.0, 26.0, cur).len(), 10);
        assert!(fit(&all, 0.0, 26.0, None).is_empty());
    }
}
