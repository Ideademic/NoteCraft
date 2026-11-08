//! The page list on the right: "+ Add Page" at the top, then the section's pages (subpages
//! indented), tinted with the section's colour, the current page in a stronger tint. Drag a page
//! to reorder (its subpages come along); right-click for New Page, New Subpage, Rename, Delete,
//! Make Subpage and Promote Subpage. Unread pages (changed since last seen) are bold; with History ▸
//! Page Versions on, each page's versions follow it in a lighter style. It collapses to a thin
//! bar.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::Guid;
use notecraft_engine::ids;
use serde_json::json;

use crate::app::RenameTarget;
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, fit_text, paint_strong, regular};
use crate::{NoteApp, tl, widgets};

const ROW_H: f32 = 30.0;
const HEADER_H: f32 = 36.0;
/// Indent per subpage level.
const LEVEL_INDENT: f32 = 16.0;

fn drag_id() -> egui::Id {
    egui::Id::unique("nc_page_drag")
}

/// One row of the list.
#[derive(Clone, Debug)]
struct Row {
    id: Guid,
    title: String,
    level: u8,
    when: String,
}

/// The page list (expanded).
pub fn page_list(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    let color = app.session.current_section().map(|s| s.color);
    let bg = color.map_or(t.page_list, |c| t.section_tint(c));
    ui.painter().rect_filled(full, 0.0, bg);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    // Header: + Add Page, and the button that folds the list away.
    let header = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    let add = Rect::from_min_max(header.min + vec2(6.0, 5.0), pos2(header.max.x - 34.0, header.max.y - 5.0));
    let can = app.can(ids::PAGE_NEW);
    let resp = ui.interact(add, ui.scope_id().with("add_page"), if can { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can, tl!("Add Page")));
    if can && resp.hovered() {
        ui.painter().rect_filled(add, RADIUS_SMALL, color.map_or(t.hover, |c| t.section_selected(c)));
    }
    let ink = if can { t.accent_text } else { t.text_disabled };
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(add.min.x + 14.0, add.center().y), vec2(13.0, 13.0)),
        Icon::Plus,
        icons::Style::mono(ink),
    );
    paint_strong(ui.painter(), pos2(add.min.x + 28.0, add.center().y), Align2::LEFT_CENTER, tl!("Add Page"), regular(13.0), ink);
    let key = widgets::shortcut_text("Mod+N");
    if resp.on_hover_text(format!("{} ({key})", tl!("Add Page"))).clicked() && can {
        app.act(ids::PAGE_NEW, json!({}));
    }
    let fold = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, fold, Icon::ChevronRight, tl!("Collapse Page Tabs")) {
        app.ui.page_list_open = false;
    }
    ui.painter()
        .line_segment([pos2(full.min.x, header.max.y), pos2(full.max.x, header.max.y)], Stroke::new(1.0, crate::theme::mix(bg, t.text, 0.12)));
    let rows: Vec<Row> = app
        .session
        .current_section()
        .map(|s| {
            s.pages.iter().map(|p| Row { id: p.id, title: p.title.clone(), level: p.level, when: p.created.format("%d/%m/%Y").to_string() }).collect()
        })
        .unwrap_or_default();
    let list = Rect::from_min_max(pos2(full.min.x + 1.0, header.max.y + 1.0), full.max);
    let mut row_rects: Vec<Rect> = Vec::new();
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_page_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for (i, row) in rows.iter().enumerate() {
                row_rects.push(draw_row(ui, app, row, i, &rows));
                super::history::version_rows(ui, app, row.id, row.level);
            }
            if rows.is_empty() {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| ui.label(egui::RichText::new(tl!("No pages in this section")).font(regular(12.0)).color(t.text_dim)));
            }
        });
    });
    drag_and_drop(ui, app, &rows, &row_rects);
    // The panel keeps the width its content used (see `notebook_pane`): claim all of it.
    ui.expand_to_include_rect(full);
}

fn draw_row(ui: &mut Ui, app: &mut NoteApp, row: &Row, index: usize, rows: &[Row]) -> Rect {
    let t = app.tokens.clone();
    let color = app.session.current_section().map(|s| s.color);
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
    // While one of its versions is open, the version row is the selected one.
    let current = app.session.current.page == Some(row.id) && app.session.viewing_version.is_none();
    let title = if row.title.trim().is_empty() { tl!("Untitled page") } else { row.title.as_str() };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, title));
    if current {
        let fill = color.map_or(t.page_list_selected, |c| t.section_selected(c));
        ui.painter().rect_filled(r, 0.0, fill);
        let bar = Rect::from_min_max(r.min, pos2(r.min.x + 3.0, r.max.y));
        ui.painter().rect_filled(bar, 0.0, color.map_or(t.accent, |c| crate::theme::mix(t.section_fill(c), egui::Color32::BLACK, 0.25)));
    } else if resp.hovered() {
        let fill = color.map_or(t.hover, |c| crate::theme::mix(t.section_tint(c), t.section_selected(c), 0.5));
        ui.painter().rect_filled(r, 0.0, fill);
    }
    let x = r.min.x + 12.0 + f32::from(row.level) * LEVEL_INDENT;
    let text_rect = Rect::from_min_max(pos2(x - 4.0, r.min.y + 3.0), pos2(r.max.x - 6.0, r.max.y - 3.0));
    if !super::rename_box(ui, app, RenameTarget::Page(row.id), text_rect) {
        let font = regular(if row.level == 0 { 13.0 } else { 12.5 });
        let shown = fit_text(ui.ctx(), title, &font, (r.max.x - x - 8.0).max(0.0));
        let color = if row.title.trim().is_empty() { t.text_dim } else { t.text };
        // Bold: the current page, and pages changed since the user last saw them (History).
        if current || app.session.is_unread(row.id) {
            paint_strong(ui.painter(), pos2(x, r.center().y), Align2::LEFT_CENTER, &shown, font, color);
        } else {
            ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, &shown, font, color);
        }
    }
    if resp.drag_started() {
        ui.data_mut(|d| d.insert_temp(drag_id(), row.id));
    }
    if resp.clicked() {
        app.act(ids::NAV_PAGE, json!({ "id": row.id }));
    }
    if resp.double_clicked() {
        super::start_rename(app, RenameTarget::Page(row.id));
    }
    let resp = resp.on_hover_text(format!("{title}\n{}", row.when));
    let id = row.id;
    let can_indent =
        index > 0 && rows.get(index.wrapping_sub(1)).is_some_and(|p| p.level >= row.level) && row.level < notecraft_doc::page::MAX_PAGE_LEVEL;
    let level = row.level;
    resp.context_menu(|ui| page_menu(ui, app, id, can_indent, level > 0));
    r
}

fn page_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid, can_indent: bool, can_promote: bool) {
    let go = |app: &mut NoteApp| {
        if app.session.current.page != Some(id) {
            app.act(ids::NAV_PAGE, json!({ "id": id }));
        }
    };
    if widgets::menu_item(ui, Some(Icon::NewPage), tl!("New Page"), true) {
        go(app);
        app.act(ids::PAGE_NEW, json!({}));
    }
    if widgets::menu_item(ui, Some(Icon::NewPage), tl!("New Subpage"), true) {
        go(app);
        app.act(ids::PAGE_NEW_SUBPAGE, json!({}));
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Rename), tl!("Rename"), true) {
        super::start_rename(app, RenameTarget::Page(id));
    }
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete"), true) {
        app.act(ids::PAGE_DELETE, json!({ "id": id }));
    }
    if widgets::menu_item(ui, Some(Icon::Link), tl!("Copy Link to Page"), true)
        && let Ok(out) = app.run(ids::LINK_TO_PAGE, json!({ "id": id }))
        && let Some(url) = out.value.get("url").and_then(serde_json::Value::as_str)
    {
        ui.ctx().copy_text(url.to_owned());
        app.set_status(tl!("Link copied."), false);
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Indent), tl!("Make Subpage"), can_indent) {
        app.act(ids::PAGE_INDENT, json!({ "id": id }));
    }
    if widgets::menu_item(ui, Some(Icon::Outdent), tl!("Promote Subpage"), can_promote) {
        app.act(ids::PAGE_OUTDENT, json!({ "id": id }));
    }
}

/// The `index` `page.move` takes (the list without the dragged page and its subpages) for a
/// drop before row `to` (`rows.len()`: the end); `None` when nothing moves.
pub fn drop_index(levels: &[u8], from: usize, to: usize) -> Option<usize> {
    let level = *levels.get(from)?;
    let end = levels.iter().skip(from + 1).position(|l| *l <= level).map_or(levels.len(), |n| from + 1 + n);
    if (from..=end).contains(&to) {
        return None;
    }
    Some(if to > end { to - (end - from) } else { to })
}

fn drag_and_drop(ui: &mut Ui, app: &mut NoteApp, rows: &[Row], rects: &[Rect]) {
    let Some(id) = ui.data(|d| d.get_temp::<Guid>(drag_id())) else { return };
    let t = app.tokens.clone();
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let released = ui.input(|i| i.pointer.any_released());
    let to = pointer.map(|p| rects.iter().position(|r| p.y < r.center().y).unwrap_or(rects.len()));
    if let Some(slot) = to {
        let y = rects.get(slot).map(|r| r.min.y).or_else(|| rects.last().map(|r| r.max.y));
        if let (Some(y), Some(first)) = (y, rects.first()) {
            ui.painter().rect_filled(Rect::from_min_max(pos2(first.min.x + 4.0, y - 1.5), pos2(first.max.x - 4.0, y + 1.5)), 1.0, t.accent_text);
        }
    }
    if released {
        ui.data_mut(|d| d.remove::<Guid>(drag_id()));
        let levels: Vec<u8> = rows.iter().map(|r| r.level).collect();
        let from = rows.iter().position(|r| r.id == id);
        if let (Some(from), Some(to)) = (from, to)
            && let Some(index) = drop_index(&levels, from, to)
        {
            app.act(ids::PAGE_MOVE, json!({ "id": id, "index": index }));
        }
    }
}

/// The collapsed page list: a thin bar to bring it back.
pub fn page_bar(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    let bg = app.session.current_section().map_or(t.page_list, |s| t.section_tint(s.color));
    ui.painter().rect_filled(full, 0.0, bg);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let open = Rect::from_center_size(pos2(full.center().x, full.min.y + 16.0), vec2(22.0, 24.0));
    if super::icon_button(ui, app, open, Icon::ChevronLeft, tl!("Expand Page Tabs")) {
        app.ui.page_list_open = true;
    }
    let add = Rect::from_center_size(pos2(full.center().x, full.min.y + 44.0), vec2(22.0, 24.0));
    if app.can(ids::PAGE_NEW) && super::icon_button(ui, app, add, Icon::Plus, tl!("Add Page")) {
        app.act(ids::PAGE_NEW, json!({}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_counts_without_the_family() {
        // Pages: A, A.1, B, C. Drag A (with A.1) before C: index 1 in [B, C].
        let levels = [0, 1, 0, 0];
        assert_eq!(drop_index(&levels, 0, 3), Some(1));
        assert_eq!(drop_index(&levels, 0, 4), Some(2));
        // Onto itself or inside its family: nothing.
        assert_eq!(drop_index(&levels, 0, 0), None);
        assert_eq!(drop_index(&levels, 0, 1), None);
        assert_eq!(drop_index(&levels, 0, 2), None);
        // C to the top.
        assert_eq!(drop_index(&levels, 3, 0), Some(0));
        assert_eq!(drop_index(&levels, 9, 0), None);
    }
}
