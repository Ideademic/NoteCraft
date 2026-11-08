//! Tables: the Insert › Table picker (a 10 × 8 grid to size the table, and Insert Table… for
//! more), and the contextual Table tab (Table Tools › Layout) shown while the caret is in a
//! table: Select, Delete, Insert, Format (Hide Borders, Shading), Alignment, Data (Sort) and
//! Convert (to text; OneNote's Convert to Spreadsheet is left out).

use egui::{Rect, Sense, Ui, vec2};
use notecraft_doc::{Align, Color};
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::icons::Icon;
use crate::theme::regular;
use crate::widgets::{self, Btn, color};
use crate::{NoteApp, tl};

/// The grid picker's size: columns × rows.
pub const GRID_COLS: usize = 10;
pub const GRID_ROWS: usize = 8;
/// Most rows or columns the Insert Table dialog offers.
pub const DIALOG_MAX: u32 = 64;

/// Whether the caret is in a table (the Table tab shows).
pub fn in_table(app: &NoteApp) -> bool {
    crate::canvas::table_context(&app.canvas).is_some()
}

/// Inserts a `rows` × `cols` table at the caret (or below the page's content) and shows the
/// Table tab.
pub fn insert(app: &mut NoteApp, rows: usize, cols: usize) {
    match crate::canvas::insert_table(&mut app.canvas, &mut app.session, rows, cols) {
        Ok(_) => {
            if app.ui.ribbon_tab == super::RibbonTab::Insert {
                app.ui.ribbon_tab = super::RibbonTab::Table;
            }
        }
        Err(e) => app.set_status(e.to_string(), true),
    }
}

/// Runs a Table tab command on the caret's table.
pub fn act(app: &mut NoteApp, id: &str, extra: Value) {
    match crate::canvas::table_command(&mut app.canvas, &mut app.session, id, extra) {
        Some(Ok(_)) => {
            if app.ui.status_error {
                app.ui.status = None;
                app.ui.status_error = false;
            }
        }
        Some(Err(_)) => {}
        None => app.set_status(tl!("Click in a table first."), false),
    }
}

/// Insert › Table's drop-down: hover the grid to size the table, click to insert it.
pub fn picker(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let id = egui::Id::unique("nc_table_picker_hover");
    let hover: (usize, usize) = ui.data(|d| d.get_temp(id)).unwrap_or((0, 0));
    let caption = if hover.0 > 0 {
        crate::i18n::fmt_plain(tl!("{cols}×{rows} Table"), &[("cols", &hover.1.to_string()), ("rows", &hover.0.to_string())])
    } else {
        tl!("Insert Table").to_owned()
    };
    ui.label(egui::RichText::new(caption).font(regular(12.5)).color(t.text));
    let mut new_hover = (0, 0);
    let mut clicked = None;
    let cell = 16.0;
    let gap = 2.0;
    let size = vec2(GRID_COLS as f32 * (cell + gap) - gap, GRID_ROWS as f32 * (cell + gap) - gap);
    // The menu is as wide as the grid (its separator and item would stretch it otherwise).
    ui.set_max_width(size.x);
    let (area, _) = ui.allocate_exact_size(size, Sense::hover());
    for r in 1..=GRID_ROWS {
        for c in 1..=GRID_COLS {
            let min = area.min + vec2((c - 1) as f32 * (cell + gap), (r - 1) as f32 * (cell + gap));
            let rect = Rect::from_min_size(min, vec2(cell, cell));
            let resp = ui.interact(rect, id.with((r, c)), Sense::click());
            let on = r <= hover.0 && c <= hover.1;
            ui.painter().rect(
                rect,
                1.0,
                if on { t.selection } else { t.menu },
                egui::Stroke::new(1.0, if on { t.accent } else { t.border_strong }),
                egui::StrokeKind::Inside,
            );
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{c}×{r}")));
            if resp.hovered() {
                new_hover = (r, c);
            }
            if resp.clicked() {
                clicked = Some((r, c));
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(id, new_hover));
    if new_hover != hover {
        ui.ctx().request_repaint();
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Table), tl!("Insert Table…"), true) {
        ui.data_mut(|d| d.insert_temp(dialog_id(), Some((3u32, 3u32))));
        ui.close();
    }
    if let Some((r, c)) = clicked {
        insert(app, r, c);
        ui.close();
    }
}

fn dialog_id() -> egui::Id {
    egui::Id::unique("nc_insert_table_dialog")
}

/// Opens the Insert Table dialog (rows and columns).
pub fn open_dialog(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(dialog_id(), Some((3u32, 3u32))));
}

/// The Insert Table dialog, while it is open (the frame calls this after the panels).
pub fn dialog(ctx: &egui::Context, app: &mut NoteApp) {
    let Some((mut rows, mut cols)) = ctx.data(|d| d.get_temp::<Option<(u32, u32)>>(dialog_id())).flatten() else { return };
    let t = app.tokens.clone();
    let mut answer: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_insert_table_modal")).backdrop_color(egui::Color32::TRANSPARENT).frame(
        egui::Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(20.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(260.0);
        ui.label(egui::RichText::new(tl!("Insert Table")).font(regular(17.0)).color(t.text));
        ui.add_space(10.0);
        egui::Grid::new("nc_insert_table_grid").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Number of columns:")).font(regular(13.0)).color(t.text));
            ui.add(egui::DragValue::new(&mut cols).range(1..=DIALOG_MAX).speed(0.1));
            ui.end_row();
            ui.label(egui::RichText::new(tl!("Number of rows:")).font(regular(13.0)).color(t.text));
            ui.add(egui::DragValue::new(&mut rows).range(1..=DIALOG_MAX).speed(0.1));
            ui.end_row();
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if widgets::flat_button(ui, tl!("OK"), true, true) {
                answer = Some(true);
            }
            if widgets::flat_button(ui, tl!("Cancel"), false, true) {
                answer = Some(false);
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            answer = Some(true);
        }
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    let (rows, cols) = (rows.clamp(1, DIALOG_MAX), cols.clamp(1, DIALOG_MAX));
    match answer {
        None => {
            ctx.data_mut(|d| d.insert_temp(dialog_id(), Some((rows, cols))));
        }
        Some(yes) => {
            ctx.data_mut(|d| d.insert_temp::<Option<(u32, u32)>>(dialog_id(), None));
            if yes {
                insert(app, rows as usize, cols as usize);
            }
        }
    }
}

/// Whether the caret's table has its borders showing (`true` outside a table).
pub fn borders_shown(app: &NoteApp) -> bool {
    let Some((outline, cell, _)) = crate::canvas::table_context(&app.canvas) else { return true };
    app.session.current_page().and_then(|p| p.outline(outline)).and_then(|o| o.table(cell.block)).is_none_or(|t| t.borders)
}

/// Shading▾: the colours, and No Color.
pub fn shading_menu(ui: &mut Ui, app: &mut NoteApp) {
    ui.set_min_width(180.0);
    if let Some(c) = color::grid(ui, &color::HIGHLIGHT, 5, None) {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        act(app, ids::TABLE_SHADING, json!({ "color": Color::rgb(r, g, b).to_hex() }));
        ui.close();
    }
    ui.separator();
    if widgets::menu_item(ui, None, tl!("No Color"), true) {
        act(app, ids::TABLE_SHADING, json!({ "color": null }));
    }
}

/// Sort▾: ascending or descending by the caret's column, with or without a header row.
pub fn sort_menu(ui: &mut Ui, app: &mut NoteApp) {
    let header_id = egui::Id::unique("nc_table_sort_header");
    let mut header = ui.data(|d| d.get_temp::<bool>(header_id)).unwrap_or(false);
    if widgets::menu_item(ui, Some(Icon::SortAscending), tl!("Sort Ascending"), true) {
        act(app, ids::TABLE_SORT, json!({ "descending": false, "header": header }));
    }
    if widgets::menu_item(ui, Some(Icon::SortDescending), tl!("Sort Descending"), true) {
        act(app, ids::TABLE_SORT, json!({ "descending": true, "header": header }));
    }
    ui.separator();
    if widgets::check_item(ui, header, tl!("Include Header Row")) {
        header = !header;
        ui.data_mut(|d| d.insert_temp(header_id, header));
    }
}

/// The classic Table tab.
pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    let on = in_table(app);
    widgets::group(ui, app, tl!("Select"), Icon::SelectTable, |ui, app| {
        let b = Btn::new(Icon::SelectTable, tl!("Select\nTable")).tip(tl!("Select Table")).enabled(on);
        if widgets::big(ui, app, b) {
            act(app, ids::TABLE_SELECT_TABLE, json!({}));
        }
        widgets::rows(ui, app, |ui, app| {
            for (icon, label, id) in [
                (Icon::SelectColumn, tl!("Select Columns"), ids::TABLE_SELECT_COLUMN),
                (Icon::SelectRow, tl!("Select Rows"), ids::TABLE_SELECT_ROW),
                (Icon::SelectCell, tl!("Select Cell"), ids::TABLE_SELECT_CELL),
            ] {
                if widgets::small(ui, app, Btn::new(icon, label).enabled(on)) {
                    act(app, id, json!({}));
                }
            }
        });
    });
    widgets::group(ui, app, tl!("Delete"), Icon::DeleteTable, |ui, app| {
        let b = Btn::new(Icon::DeleteTable, tl!("Delete\nTable")).tip(tl!("Delete Table")).enabled(on);
        if widgets::big(ui, app, b) {
            act(app, ids::TABLE_DELETE, json!({}));
        }
        widgets::rows(ui, app, |ui, app| {
            if widgets::small(ui, app, Btn::new(Icon::DeleteColumns, tl!("Delete Columns")).enabled(on)) {
                act(app, ids::TABLE_DELETE_COLUMN, json!({}));
            }
            if widgets::small(ui, app, Btn::new(Icon::DeleteRows, tl!("Delete Rows")).enabled(on)) {
                act(app, ids::TABLE_DELETE_ROW, json!({}));
            }
        });
    });
    widgets::group(ui, app, tl!("Insert"), Icon::InsertBelow, |ui, app| {
        for (icon, label, tip, id) in [
            (Icon::InsertAbove, tl!("Insert\nAbove"), tl!("Insert Above"), ids::TABLE_INSERT_ROW_ABOVE),
            (Icon::InsertBelow, tl!("Insert\nBelow"), tl!("Insert Below"), ids::TABLE_INSERT_ROW_BELOW),
            (Icon::InsertLeft, tl!("Insert\nLeft"), tl!("Insert Left"), ids::TABLE_INSERT_COLUMN_LEFT),
            (Icon::InsertRight, tl!("Insert\nRight"), tl!("Insert Right"), ids::TABLE_INSERT_COLUMN_RIGHT),
        ] {
            let b = Btn::new(icon, label).tip(tip).for_command(app, id).enabled(on);
            if widgets::big(ui, app, b) {
                act(app, id, json!({}));
            }
        }
    });
    widgets::group(ui, app, tl!("Format"), Icon::Borders, |ui, app| {
        let hidden = !borders_shown(app);
        let b = Btn::new(Icon::Borders, tl!("Hide\nBorders"))
            .tip(tl!("Hide Borders"))
            .help(tl!("Hide the table's lines. They show dashed while you work in the table."))
            .checked(on && hidden)
            .enabled(on);
        if widgets::big(ui, app, b) {
            act(app, ids::TABLE_BORDERS, json!({ "value": hidden }));
        }
        let b = Btn::new(Icon::Shading, tl!("Shading")).help(tl!("Colour the background of the selected cells.")).enabled(on);
        widgets::big_menu(ui, app, b, shading_menu);
    });
    widgets::group(ui, app, tl!("Alignment"), Icon::AlignLeft, |ui, app| {
        widgets::rows(ui, app, |ui, app| {
            let current = super::home::format_state(app).and_then(|f| f.align);
            for a in [Align::Left, Align::Center, Align::Right] {
                let (icon, label) = super::paragraph::align_item(a);
                let b = Btn::new(icon, label).checked(on && current == Some(a)).enabled(on);
                if widgets::small(ui, app, b) {
                    super::home::format(app, ids::FORMAT_ALIGN, json!({ "align": a.name() }));
                }
            }
        });
    });
    widgets::group(ui, app, tl!("Data"), Icon::SortAscending, |ui, app| {
        let b = Btn::new(Icon::SortAscending, tl!("Sort")).help(tl!("Sort the rows by the column the caret is in.")).enabled(on);
        widgets::big_menu(ui, app, b, sort_menu);
    });
    widgets::group(ui, app, tl!("Convert"), Icon::ConvertToText, |ui, app| {
        let b = Btn::new(Icon::ConvertToText, tl!("Convert\nto Text"))
            .tip(tl!("Convert to Text"))
            .help(tl!("Turn the table into paragraphs, one per row, with the cells separated by tabs."))
            .enabled(on);
        if widgets::big(ui, app, b) {
            act(app, ids::TABLE_CONVERT_TO_TEXT, json!({}));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_matches_wordcraft() {
        assert_eq!((GRID_COLS, GRID_ROWS), (10, 8));
        assert!(DIALOG_MAX as usize <= notecraft_doc::table::MAX_TABLE_COLS);
    }
}
