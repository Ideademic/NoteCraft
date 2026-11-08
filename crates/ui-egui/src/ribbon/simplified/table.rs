//! Simplified Table tab (Table Tools › Layout): Select▾ | Delete▾ | Insert Above, Below, Left,
//! Right | Hide Borders, Shading▾ | alignment | Sort▾ | Convert to Text.

use egui::Ui;
use notecraft_doc::Align;
use notecraft_engine::ids;
use serde_json::json;

use super::Segment;
use crate::icons::Icon;
use crate::ribbon::table::{act, borders_shown, in_table, shading_menu, sort_menu};
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 6] = [
    Segment { key: "select", draw: select },
    Segment { key: "insert", draw: insert },
    Segment { key: "format", draw: format },
    Segment { key: "align", draw: align },
    Segment { key: "sort", draw: sort },
    Segment { key: "convert", draw: convert },
];

fn select(ui: &mut Ui, app: &mut NoteApp) {
    let on = in_table(app);
    let b = Btn::new(Icon::SelectTable, tl!("Select")).help(tl!("Select the table, its columns or rows, or the cell.")).enabled(on);
    widgets::small_menu(ui, app, b, |ui, app| {
        for (icon, label, id) in [
            (Icon::SelectTable, tl!("Select Table"), ids::TABLE_SELECT_TABLE),
            (Icon::SelectColumn, tl!("Select Columns"), ids::TABLE_SELECT_COLUMN),
            (Icon::SelectRow, tl!("Select Rows"), ids::TABLE_SELECT_ROW),
            (Icon::SelectCell, tl!("Select Cell"), ids::TABLE_SELECT_CELL),
        ] {
            if widgets::menu_item(ui, Some(icon), label, true) {
                act(app, id, json!({}));
            }
        }
    });
    let b = Btn::new(Icon::DeleteTable, tl!("Delete")).help(tl!("Delete the table, or its selected columns or rows.")).enabled(on);
    widgets::small_menu(ui, app, b, |ui, app| {
        for (icon, label, id) in [
            (Icon::DeleteTable, tl!("Delete Table"), ids::TABLE_DELETE),
            (Icon::DeleteColumns, tl!("Delete Columns"), ids::TABLE_DELETE_COLUMN),
            (Icon::DeleteRows, tl!("Delete Rows"), ids::TABLE_DELETE_ROW),
        ] {
            if widgets::menu_item(ui, Some(icon), label, true) {
                act(app, id, json!({}));
            }
        }
    });
}

fn insert(ui: &mut Ui, app: &mut NoteApp) {
    let on = in_table(app);
    for (icon, label, tip, id) in [
        (Icon::InsertAbove, tl!("Above"), tl!("Insert Above"), ids::TABLE_INSERT_ROW_ABOVE),
        (Icon::InsertBelow, tl!("Below"), tl!("Insert Below"), ids::TABLE_INSERT_ROW_BELOW),
        (Icon::InsertLeft, tl!("Left"), tl!("Insert Left"), ids::TABLE_INSERT_COLUMN_LEFT),
        (Icon::InsertRight, tl!("Right"), tl!("Insert Right"), ids::TABLE_INSERT_COLUMN_RIGHT),
    ] {
        let b = Btn::new(icon, label).tip(tip).for_command(app, id).enabled(on);
        if widgets::small(ui, app, b) {
            act(app, id, json!({}));
        }
    }
}

fn format(ui: &mut Ui, app: &mut NoteApp) {
    let on = in_table(app);
    let hidden = !borders_shown(app);
    let b = Btn::new(Icon::Borders, tl!("Hide Borders")).checked(on && hidden).enabled(on);
    if widgets::small(ui, app, b) {
        act(app, ids::TABLE_BORDERS, json!({ "value": hidden }));
    }
    let b = Btn::new(Icon::Shading, tl!("Shading")).enabled(on);
    widgets::small_menu(ui, app, b, shading_menu);
}

fn align(ui: &mut Ui, app: &mut NoteApp) {
    let on = in_table(app);
    let current = crate::ribbon::home::format_state(app).and_then(|f| f.align);
    for a in [Align::Left, Align::Center, Align::Right] {
        let (icon, label) = crate::ribbon::paragraph::align_item(a);
        let b = Btn::new(icon, label).icon_only().checked(on && current == Some(a)).enabled(on);
        if widgets::small(ui, app, b) {
            crate::ribbon::home::format(app, ids::FORMAT_ALIGN, json!({ "align": a.name() }));
        }
    }
}

fn sort(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::SortAscending, tl!("Sort")).enabled(in_table(app));
    widgets::small_menu(ui, app, b, sort_menu);
}

fn convert(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::ConvertToText, tl!("Convert to Text")).enabled(in_table(app));
    if widgets::small(ui, app, b) {
        act(app, ids::TABLE_CONVERT_TO_TEXT, json!({}));
    }
}
