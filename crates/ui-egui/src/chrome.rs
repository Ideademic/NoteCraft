//! Window chrome: the custom title bar (quick access toolbar, title, window buttons) and the
//! status bar. Phase 2 (shell agent): QAT buttons, window controls, drag to move.

use egui::{Align, Layout, RichText};

use crate::NoteApp;
use crate::tl;

/// The title bar: "<page> - <notebook> - NoteCraft".
pub fn title_bar(ui: &mut egui::Ui, app: &mut NoteApp) {
    let page = app.session.current_page().map(|p| p.display_title().to_owned());
    let nb = app.session.current_notebook().map(|n| n.name.clone());
    let title = match (page, nb) {
        (Some(p), Some(n)) => format!("{p} - {n} - NoteCraft"),
        (None, Some(n)) => format!("{n} - NoteCraft"),
        _ => "NoteCraft".to_owned(),
    };
    ui.with_layout(Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
        ui.label(RichText::new(title).color(app.tokens.on_title_bar));
    });
}

/// The status bar: the last message, or nothing.
pub fn status_bar(ui: &mut egui::Ui, app: &mut NoteApp) {
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        let text = app.ui.status.clone().unwrap_or_else(|| tl!("Ready").to_owned());
        ui.label(RichText::new(text).small().color(app.tokens.text_dim));
    });
}
