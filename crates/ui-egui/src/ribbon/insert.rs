//! Insert: Pictures, File Printout, File Attachment, Date & Time.

use crate::NoteApp;
use crate::tl;

pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    let _ = app;
    ui.label(tl!("Insert"));
}
