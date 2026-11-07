//! File ▸ (Backstage): Info, New, Open, Export, Import, Options, About. Phase 2 (shell agent).

use crate::NoteApp;
use crate::tl;

pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    if ui.button(tl!("Back")).clicked() {
        app.ui.backstage = false;
    }
    ui.heading(tl!("Notebook Information"));
}
