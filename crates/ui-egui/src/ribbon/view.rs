//! View: Rule Lines, Page Color, Zoom, Dark Mode.

use crate::NoteApp;
use crate::tl;

pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    if ui.button(tl!("Zoom In")).clicked() {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_in(app.canvas.zoom));
    }
    if ui.button(tl!("Zoom Out")).clicked() {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_out(app.canvas.zoom));
    }
    ui.checkbox(&mut app.ui.dark, tl!("Dark Mode"));
}
