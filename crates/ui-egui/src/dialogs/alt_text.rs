//! A picture's Alt Text: a short description read out by screen readers and kept in exports
//! (`picture.altText`). Draw › Alt Text (or the picture's context menu) opens it for the
//! selected picture; OK saves, Cancel or Esc leaves the text as it was.

use egui::{Frame, Stroke};
use notecraft_doc::{ElementKind, Guid};
use notecraft_engine::ids;
use serde_json::json;

use crate::theme::regular;
use crate::widgets::flat_button;
use crate::{NoteApp, tl};

/// Most characters kept.
pub const MAX_CHARS: usize = 4096;

/// The one selected picture, if one is.
pub fn selected_picture(app: &NoteApp) -> Option<Guid> {
    let [id] = app.canvas.selection.elements.as_slice() else { return None };
    let page = app.session.current_page()?;
    matches!(page.element(*id)?.kind, ElementKind::Image(_)).then_some(*id)
}

/// Opens the dialog for picture `id` with its current text.
pub fn open(app: &mut NoteApp, id: Guid) {
    let text = app
        .session
        .current_page()
        .and_then(|p| p.element(id))
        .and_then(|e| match &e.kind {
            ElementKind::Image(i) => Some(i.alt.clone()),
            _ => None,
        })
        .unwrap_or_default();
    app.ui.alt_text = Some((id, text));
}

/// Draws the dialog while it is open.
pub fn show(ctx: &egui::Context, app: &mut NoteApp) {
    let Some((id, mut text)) = app.ui.alt_text.clone() else { return };
    let t = app.tokens.clone();
    let mut answer: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_alt_text"))
        .backdrop_color(egui::Color32::TRANSPARENT)
        .frame(Frame::NONE.fill(t.menu).stroke(Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(20.0));
    let resp = modal.show(ctx, |ui| {
        ui.set_width(400.0);
        ui.label(egui::RichText::new(tl!("Alt Text")).font(regular(17.0)).color(t.text));
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(tl!("Describe the picture for people who can't see it. Screen readers read this, and exports keep it."))
                .font(regular(12.5))
                .color(t.text_dim),
        );
        ui.add_space(10.0);
        let edit =
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(4).desired_width(f32::INFINITY).font(regular(13.0)).char_limit(MAX_CHARS));
        if !edit.has_focus() && ui.memory(|m| m.focused().is_none()) {
            edit.request_focus();
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("OK"), true, true) {
                answer = Some(true);
            }
            if flat_button(ui, tl!("Cancel"), false, true) {
                answer = Some(false);
            }
        });
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    match answer {
        None => app.ui.alt_text = Some((id, text)),
        Some(save) => {
            app.ui.alt_text = None;
            if save {
                app.act(ids::PICTURE_ALT_TEXT, json!({"element": id, "text": text}));
            }
        }
    }
}
