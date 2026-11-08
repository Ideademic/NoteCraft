//! Simplified Insert: Space | Table | File Printout, File Attachment | Pictures | Date & Time▾
//! (Date, Time inside).

use egui::Ui;
use notecraft_engine::ids;
use serde_json::json;

use super::Segment;
use crate::icons::Icon;
use crate::services::OpenPurpose;
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 5] = [
    Segment { key: "space", draw: space },
    Segment { key: "table", draw: table },
    Segment { key: "files", draw: files },
    Segment { key: "images", draw: images },
    Segment { key: "stamp", draw: stamp },
];

fn space(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::InsertSpace, tl!("Space")).tip(tl!("Insert Space")).enabled(false).help(tl!("Coming in a later version of NoteCraft."));
    widgets::small(ui, app, b);
}

fn table(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Table, tl!("Table"))
        .enabled(app.session.current_page().is_some())
        .help(tl!("Insert a table: pick its size, or choose Insert Table… for a bigger one."));
    widgets::small_menu(ui, app, b, crate::ribbon::table::picker);
}

fn files(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Printout, tl!("Printout"))
        .tip(tl!("File Printout"))
        .help(tl!("Put the pages of a PDF on the page as pictures you can write on."))
        .for_command(app, ids::INSERT_PRINTOUT);
    if widgets::small(ui, app, b) {
        crate::app::files::pick_and_insert(app, OpenPurpose::Printout);
    }
    let b = Btn::new(Icon::Attach, tl!("Attachment"))
        .tip(tl!("File Attachment"))
        .help(tl!("Attach a copy of a file to the page."))
        .for_command(app, ids::INSERT_FILE);
    if widgets::small(ui, app, b) {
        crate::app::files::pick_and_insert(app, OpenPurpose::Attachment);
    }
}

fn images(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Picture, tl!("Pictures")).help(tl!("Insert a picture from a file.")).for_command(app, ids::INSERT_IMAGE);
    if widgets::small(ui, app, b) {
        crate::app::files::pick_and_insert(app, OpenPurpose::Picture);
    }
}

/// Types a date, a time or both at the caret (or in a new note container), as Insert › Time
/// Stamp does.
fn insert_stamp(app: &mut NoteApp, what: &str) {
    let extra = json!({ "what": what, "utcOffsetMinutes": app.canvas.utc_offset_minutes });
    let params = if app.canvas.editing.is_some() { widgets::selection_params(app, extra) } else { extra };
    if let Ok(out) = app.run(ids::INSERT_DATE_TIME, params)
        && let Some(pos) = out.value.get("caret").and_then(notecraft_engine::TextPos::from_json)
        && let Some(c) = app.canvas.caret.as_mut()
    {
        // The caret goes after the inserted stamp.
        c.head = pos;
        c.anchor = pos;
    }
}

fn stamp(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::DateTime, tl!("Date & Time")).for_command(app, ids::INSERT_DATE_TIME);
    if super::labeled_split(ui, app, b, |ui, app| {
        for (icon, label, what) in
            [(Icon::Date, tl!("Date"), "date"), (Icon::Time, tl!("Time"), "time"), (Icon::DateTime, tl!("Date & Time"), "dateTime")]
        {
            if widgets::menu_item(ui, Some(icon), label, true) {
                insert_stamp(app, what);
            }
        }
    }) {
        insert_stamp(app, "dateTime");
    }
}
