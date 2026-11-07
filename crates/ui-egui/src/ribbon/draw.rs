//! Draw: Select / Type, Eraser, Lasso, pens (pen, pencil, highlighter) with colour and width.
//! Sets the tool through [`crate::canvas::CanvasState::set_tool`].

use crate::NoteApp;
use crate::canvas::Tool;
use crate::tl;

pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    for (tool, label) in [
        (Tool::Type, tl!("Type")),
        (Tool::Select, tl!("Select")),
        (Tool::Lasso, tl!("Lasso Select")),
        (Tool::Eraser, tl!("Eraser")),
        (Tool::Pen, tl!("Pen")),
        (Tool::Pencil, tl!("Pencil")),
        (Tool::Highlighter, tl!("Highlighter")),
    ] {
        if ui.selectable_label(app.canvas.tool == tool, label).clicked() {
            app.canvas.set_tool(tool);
        }
    }
}
