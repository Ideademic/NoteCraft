//! Home: Clipboard, Basic Text, Styles, Tags.

use serde_json::Value;

use crate::NoteApp;
use crate::tl;

pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    if ui.button(tl!("New Page")).clicked() {
        let _ = app.run(notecraft_engine::ids::PAGE_NEW, Value::Null);
    }
    if ui.add_enabled(app.session.can_undo(), egui::Button::new(tl!("Undo"))).clicked() {
        let _ = app.run(notecraft_engine::ids::EDIT_UNDO, Value::Null);
    }
}
