//! Files dropped on the page: pictures become pictures (`insert.image`), PDFs become file
//! printouts (`insert.printout`), anything else an attached file (`insert.file`), placed where
//! they were dropped (several files step down the page). While files hover over the canvas it
//! shows a drop frame.
//!
//! Desktop: the bytes come from the drop itself, else from `Services::read_file` with the path.
//! Web: egui's dropped files only offer their bytes asynchronously, so the web host delivers
//! drops through `Services::inbox` instead (the shell inserts those).

use base64::Engine;
use egui::{Painter, Rect, Stroke, StrokeKind, Ui};
use notecraft_engine::{Session, ids};
use serde_json::json;

use super::view::View;
use super::{CanvasState, run};
use crate::services::Services;
use crate::theme::Tokens;

/// Picture extensions `insert.image` takes.
const PICTURES: [&str; 7] = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "jfif"];

/// The command for a dropped file named `name`.
pub fn command_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if ext == "pdf" {
        ids::INSERT_PRINTOUT
    } else if PICTURES.contains(&ext.as_str()) {
        ids::INSERT_IMAGE
    } else {
        ids::INSERT_FILE
    }
}

/// Inserts `bytes` as the file `name` at page point (`x`, `y`).
pub fn insert(state: &mut CanvasState, session: &mut Session, name: &str, bytes: &[u8], x: f32, y: f32) {
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let id = command_for(name);
    if let Some(out) = run(state, session, id, json!({"bytesBase64": b64, "name": name, "x": x, "y": y, "page": state.page}))
        && let Some(el) = out.value.get("id").and_then(|v| v.as_str()).and_then(notecraft_doc::Guid::parse)
        && id == ids::INSERT_IMAGE
    {
        state.selection.elements = vec![el];
    }
}

/// Takes this frame's dropped files.
pub fn handle(ui: &Ui, state: &mut CanvasState, session: &mut Session, services: &Services, v: &View) {
    let (files, at) = ui.input(|i| (i.raw.dropped_files.clone(), i.pointer.latest_pos()));
    if files.is_empty() {
        return;
    }
    let at = at.filter(|p| v.clip.contains(*p)).unwrap_or(v.clip.min + egui::vec2(60.0, 120.0));
    let p = v.to_page(at);
    let (mut x, mut y) = (p.x.max(0.0), p.y.max(0.0));
    for f in files {
        let path = f.path().to_path_buf();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Dropped file".to_owned());
        let bytes = read(&*f, &path, services);
        match bytes {
            Some(b) if !b.is_empty() => {
                insert(state, session, &name, &b, x, y);
                x += 24.0;
                y += 24.0;
            }
            _ => state.messages.push(format!("\u{201c}{name}\u{201d} couldn't be read.")),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read(f: &dyn egui::DroppedFile, path: &std::path::Path, services: &Services) -> Option<Vec<u8>> {
    match f.bytes() {
        Ok(b) if !b.is_empty() => Some(b),
        _ => services.read_file.as_ref().and_then(|r| r(path).ok()),
    }
}

#[cfg(target_arch = "wasm32")]
fn read(_f: &dyn egui::DroppedFile, path: &std::path::Path, services: &Services) -> Option<Vec<u8>> {
    services.read_file.as_ref().and_then(|r| r(path).ok())
}

/// The drop frame while files hover over the canvas.
pub fn paint_hover(ui: &Ui, painter: &Painter, tokens: &Tokens, rect: Rect) {
    let hovering = ui.input(|i| !i.raw.hovered_files.is_empty() && i.pointer.latest_pos().is_none_or(|p| rect.contains(p)));
    if hovering {
        painter.rect_stroke(rect.shrink(3.0), 0.0, Stroke::new(2.0, tokens.accent), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropped_files_pick_their_command() {
        assert_eq!(command_for("scan.PDF"), ids::INSERT_PRINTOUT);
        assert_eq!(command_for("cat.jpeg"), ids::INSERT_IMAGE);
        assert_eq!(command_for("notes.docx"), ids::INSERT_FILE);
        assert_eq!(command_for("README"), ids::INSERT_FILE);
    }
}
