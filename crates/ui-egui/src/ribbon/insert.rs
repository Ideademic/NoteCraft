//! Insert: Insert Space, Tables, Files (File Printout, File Attachment), Images (Pictures),
//! Time Stamp (Date, Time, Date & Time).

use egui::Ui;
use notecraft_engine::ids;
use serde_json::json;

use crate::icons::Icon;
use crate::services::OpenPurpose;
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Insert"), Icon::InsertSpace, |ui, app| {
        let b = Btn::new(Icon::InsertSpace, tl!("Insert\nSpace"))
            .tip(tl!("Insert Space"))
            .enabled(false)
            .help(tl!("Coming in a later version of NoteCraft."));
        widgets::big(ui, app, b);
    });
    widgets::group(ui, app, tl!("Tables"), Icon::Table, |ui, app| {
        let b = Btn::new(Icon::Table, tl!("Table"))
            .enabled(app.session.current_page().is_some())
            .help(tl!("Insert a table: pick its size, or choose Insert Table… for a bigger one."));
        widgets::big_menu(ui, app, b, super::table::picker);
    });
    widgets::group(ui, app, tl!("Files"), Icon::Printout, |ui, app| {
        let b = Btn::new(Icon::Printout, tl!("File\nPrintout"))
            .tip(tl!("File Printout"))
            .help(tl!("Put the pages of a PDF on the page as pictures you can write on."))
            .for_command(app, ids::INSERT_PRINTOUT);
        if widgets::big(ui, app, b) {
            crate::app::files::pick_and_insert(app, OpenPurpose::Printout);
        }
        let b = Btn::new(Icon::Attach, tl!("File\nAttachment"))
            .tip(tl!("File Attachment"))
            .help(tl!("Attach a copy of a file to the page."))
            .for_command(app, ids::INSERT_FILE);
        if widgets::big(ui, app, b) {
            crate::app::files::pick_and_insert(app, OpenPurpose::Attachment);
        }
    });
    widgets::group(ui, app, tl!("Images"), Icon::Picture, |ui, app| {
        let b = Btn::new(Icon::Picture, tl!("Pictures")).help(tl!("Insert a picture from a file.")).for_command(app, ids::INSERT_IMAGE);
        if widgets::big(ui, app, b) {
            crate::app::files::pick_and_insert(app, OpenPurpose::Picture);
        }
    });
    widgets::group(ui, app, tl!("Time Stamp"), Icon::DateTime, |ui, app| {
        for (icon, label, what) in
            [(Icon::Date, tl!("Date"), "date"), (Icon::Time, tl!("Time"), "time"), (Icon::DateTime, tl!("Date &\nTime"), "dateTime")]
        {
            let tip = label.replace('\n', " ");
            let b = Btn::new(icon, label).tip(&tip).for_command(app, ids::INSERT_DATE_TIME);
            if widgets::big(ui, app, b) {
                // Typed at the caret while a note is being edited, else in a new note container.
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
        }
    });
}
