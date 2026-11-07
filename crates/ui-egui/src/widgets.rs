//! Ribbon and pane widgets: large and small ribbon buttons, split buttons, colour pickers, the
//! search box. Phase 2 (shell agent). Use `craft_ui::ComboBox`, never `egui::ComboBox`
//! (clippy.toml).

use crate::icons::{self, Icon};
use crate::theme::Tokens;

/// A ribbon button with an icon above its label.
pub fn ribbon_button(ui: &mut egui::Ui, icon: Icon, label: &str, tokens: &Tokens) -> egui::Response {
    let resp = ui.button(label);
    let r = resp.rect;
    icons::paint(ui.painter(), egui::Rect::from_min_size(r.min, egui::vec2(r.height(), r.height())), icon, tokens.icon);
    resp
}
