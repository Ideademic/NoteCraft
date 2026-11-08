//! Copy, cut and paste on the page.
//!
//! **Copy** (Ctrl+C) and **Cut** (Ctrl+X) take the text selection with its formatting, or the
//! selected note containers, pictures, printouts, files and lasso-selected ink (`edit.copy` /
//! `edit.cut`). The result is kept in two places: NoteCraft's own JSON ([`Clip`]) on the internal
//! clipboard, for pasting back with everything intact, and HTML plus plain text on the system
//! clipboard for other apps ([`crate::Services::set_clipboard_rich`]; a lone picture goes there as
//! a picture).
//!
//! **Paste** (Ctrl+V) uses the internal clipboard when the system clipboard still holds what
//! NoteCraft put there, else HTML from another app (basic formatting kept), else plain text; a
//! picture on the clipboard (`Event::PasteImage`, the web's pasted files) becomes a picture.
//! Text goes in at the caret, or as a new note container where the caret waits on empty paper
//! or at the pointer; items and ink go where the pointer is. **Paste as plain text**
//! (Ctrl+Shift+V, Paste › Keep Text Only) keeps the words only. Each paste is one undo step.

use std::sync::Arc;

use base64::Engine as _;
use egui::Pos2;
use notecraft_doc::{ElementKind, Guid};
use notecraft_engine::cmd::clip::Clip;
use notecraft_engine::{Session, TextPos, ids};
use serde_json::{Value, json};

use super::view::View;
use super::{CanvasState, TextCaret, run};
use crate::services::{ClipboardImage, Services};

/// What NoteCraft last copied: the clip and the plain text that went with it to the system
/// clipboard (a paste whose text still matches came from here).
#[derive(Clone, Debug, PartialEq)]
pub struct Internal {
    pub clip: Clip,
    pub text: String,
}

fn internal_id() -> egui::Id {
    egui::Id::unique("nc_internal_clipboard")
}

/// The internal clipboard.
pub fn internal(ctx: &egui::Context) -> Option<Arc<Internal>> {
    ctx.data(|d| d.get_temp::<Arc<Internal>>(internal_id()))
}

fn set_internal(ctx: &egui::Context, clip: Clip, text: String) {
    ctx.data_mut(|d| d.insert_temp(internal_id(), Arc::new(Internal { clip, text })));
}

/// Line ends and trailing space don't decide whether two texts are the same copy.
fn same_text(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace("\r\n", "\n").replace('\r', "\n").trim_end().to_owned();
    norm(a) == norm(b)
}

/// What a copy would take: the text selection (in its table cell, or the selected cells), else
/// the selected items and ink.
fn copy_params(state: &CanvasState, session: &Session) -> Option<Value> {
    if let Some(c) = state.caret.as_ref().filter(|c| !c.is_collapsed() || c.cells.is_some()) {
        return Some(super::edit::range_params(state, c));
    }
    if state.caret.is_some() || state.pending.is_some() {
        return None;
    }
    let ink = !session.ink_selection.is_empty();
    if state.selection.elements.is_empty() && !ink {
        return None;
    }
    Some(json!({"elements": state.selection.elements, "ink": ink, "page": state.page}))
}

/// Whether Copy and Cut have something to take.
pub fn can_copy(state: &CanvasState, session: &Session) -> bool {
    copy_params(state, session).is_some()
}

/// Ctrl+C (or Ctrl+X with `cut`). `false` when nothing is selected.
pub fn copy(ctx: &egui::Context, state: &mut CanvasState, session: &mut Session, services: &Services, cut: bool) -> bool {
    let Some(params) = copy_params(state, session) else { return false };
    let id = if cut { ids::EDIT_CUT } else { ids::EDIT_COPY };
    if cut {
        session.break_undo_merge();
    }
    let Some(out) = run(state, session, id, params) else { return false };
    let clip: Clip = out.value.get("clip").and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default();
    let text = out.value.get("text").and_then(Value::as_str).unwrap_or("").to_owned();
    let html = out.value.get("html").and_then(Value::as_str).unwrap_or("").to_owned();
    to_system(ctx, services, &clip, &text, &html);
    set_internal(ctx, clip, text);
    if cut {
        if let Some(pos) = out.value.get("caret").and_then(TextPos::from_json)
            && let Some(c) = state.caret.as_mut()
        {
            c.anchor = pos;
            c.head = pos;
        }
        if let Some(c) = state.caret.as_mut() {
            c.cells = None;
        }
        state.selection.clear();
        session.break_undo_merge();
    }
    true
}

/// Puts a copy on the system clipboard: a lone picture as a picture, else HTML and text.
fn to_system(ctx: &egui::Context, services: &Services, clip: &Clip, text: &str, html: &str) {
    if clip.blocks.is_empty()
        && clip.ink.is_empty()
        && let [el] = clip.elements.as_slice()
        && let ElementKind::Image(img) = &el.kind
        && let Some(set) = &services.set_clipboard_image
    {
        let bytes = notecraft_formats::picture::bake(img).unwrap_or_else(|| img.data.to_vec());
        if let Some(pixels) = rgba(&bytes)
            && set(&pixels)
        {
            return;
        }
    }
    if let Some(set) = &services.set_clipboard_rich
        && set(&notecraft_formats::clip::document(html), text)
    {
        return;
    }
    ctx.copy_text(text.to_owned());
}

/// A picture file as RGBA pixels (the system clipboard's picture format).
fn rgba(bytes: &[u8]) -> Option<ClipboardImage> {
    let img = image::load_from_memory(bytes).ok()?;
    if u64::from(img.width()) * u64::from(img.height()) > 64_000_000 {
        return None;
    }
    let rgba = img.to_rgba8();
    Some(ClipboardImage { width: usize::try_from(rgba.width()).ok()?, height: usize::try_from(rgba.height()).ok()?, rgba: rgba.into_raw() })
}

/// Where pasted items go: the pointer over the page, else `None` (beside the originals, or
/// below what is on the page).
fn pointer_spot(ctx: &egui::Context, state: &CanvasState, v: Option<&View>) -> Option<notecraft_doc::Point> {
    let v = v?;
    let p: Pos2 = ctx.input(|i| i.pointer.latest_pos())?;
    let inside = state.last_rect.is_some_and(|r| r.contains(p)) && v.clip.contains(p);
    inside.then(|| v.to_page(p)).filter(|p| p.x >= 0.0 && p.y >= 0.0)
}

/// Ctrl+V with the system clipboard's text `text` (`None`: unknown, as for the ribbon's Paste
/// without clipboard access). `plain` keeps the words only.
pub fn paste(
    ctx: &egui::Context,
    state: &mut CanvasState,
    session: &mut Session,
    services: &Services,
    v: Option<&View>,
    text: Option<&str>,
    plain: bool,
) {
    let plain = plain || std::mem::take(&mut state.paste_plain);
    let mut params = serde_json::Map::new();
    let internal = internal(ctx).filter(|i| text.is_none_or(|t| same_text(t, &i.text)));
    match (&internal, plain) {
        (Some(i), false) => {
            let mut clip = i.clip.clone();
            if state.caret.is_some() {
                flatten_outlines(&mut clip);
            }
            params.insert("clip".into(), serde_json::to_value(&clip).unwrap_or(Value::Null));
        }
        (Some(i), true) => {
            params.insert("text".into(), Value::String(text.map_or_else(|| i.clip.plain_text(), str::to_owned)));
        }
        (None, _) => {
            let Some(t) = text.map(str::to_owned).or_else(|| services.clipboard_text.as_ref().and_then(|f| f())) else { return };
            if !plain && let Some(h) = services.clipboard_html.as_ref().and_then(|f| f()).filter(|h| !h.trim().is_empty()) {
                params.insert("html".into(), Value::String(h));
            }
            params.insert("text".into(), Value::String(t));
        }
    }
    if plain {
        params.insert("plain".into(), Value::Bool(true));
    }
    params.insert("page".into(), json!(state.page));
    // Where it goes: the caret, the waiting container's spot, or the pointer.
    if let Some(c) = state.caret.clone() {
        let (a, b) = c.range();
        params.insert("outline".into(), json!(c.outline));
        params.insert("start".into(), a.to_json());
        params.insert("end".into(), b.to_json());
        if let Some(cell) = c.cell {
            params.insert("cell".into(), cell.to_json());
        }
    } else if let Some(p) = state.pending.take() {
        params.insert("x".into(), json!(p.at.x));
        params.insert("y".into(), json!(p.at.y));
    } else if let Some(p) = pointer_spot(ctx, state, v) {
        params.insert("x".into(), json!(p.x));
        params.insert("y".into(), json!(p.y));
    }
    session.break_undo_merge();
    let Some(out) = run(state, session, ids::EDIT_PASTE, Value::Object(params)) else { return };
    session.break_undo_merge();
    land(state, session, &out.value);
}

/// Pasting whole note containers into text pastes their paragraphs (as OneNote does); pictures
/// and ink still go on the page.
fn flatten_outlines(clip: &mut Clip) {
    let mut keep = Vec::new();
    for el in std::mem::take(&mut clip.elements) {
        match el.kind {
            ElementKind::Outline(o) => clip.blocks.extend(o.blocks),
            _ => keep.push(el),
        }
    }
    clip.elements = keep;
}

/// After a paste: the caret goes after the pasted text, pasted items are selected.
fn land(state: &mut CanvasState, session: &Session, v: &Value) {
    let caret = v.get("caret").and_then(TextPos::from_json);
    let outline = v.get("outline").and_then(Value::as_str).and_then(Guid::parse);
    let pasted: Vec<Guid> = super::pointer::ids_of(v);
    if let (Some(id), Some(pos)) = (outline, caret) {
        let key = state.caret.as_ref().filter(|c| c.outline == id).map_or_else(|| id.to_string(), |c| c.merge_key.clone());
        let cell = v.get("cell").and_then(notecraft_engine::text::CellRef::from_json);
        state.caret = Some(TextCaret { merge_key: key, cell, ..TextCaret::at(id, pos) });
        state.editing = Some(id);
        state.reveal_caret = true;
        state.selection.elements.clear();
    } else if !pasted.is_empty() {
        let page = session.current_page();
        let elements: Vec<Guid> = pasted.iter().copied().filter(|g| page.is_some_and(|p| p.element(*g).is_some())).collect();
        state.selection.elements = elements;
        state.selection.strokes = session.ink_selection.clone();
    }
}

/// A picture pasted from the clipboard (pixels): a picture on the page, at the caret's
/// container or the pointer.
pub fn paste_image(ctx: &egui::Context, state: &mut CanvasState, session: &mut Session, v: Option<&View>, img: &egui::ColorImage) {
    let [w, h] = img.size;
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let Some(png) = crate::app::files::png_from_rgba(w, h, &rgba) else {
        state.messages.push(crate::tl!("The picture on the clipboard couldn't be read.").to_owned());
        return;
    };
    paste_picture_bytes(ctx, state, session, v, &png);
}

/// A picture file pasted (the web's paste event, the ribbon's Paste): a picture on the page.
pub fn paste_picture_bytes(ctx: &egui::Context, state: &mut CanvasState, session: &mut Session, v: Option<&View>, bytes: &[u8]) {
    let mut params =
        json!({"bytesBase64": base64::engine::general_purpose::STANDARD.encode(bytes), "name": "Pasted picture.png", "page": state.page});
    let spot = state.pending.take().map(|p| p.at).or_else(|| pointer_spot(ctx, state, v));
    if let (Some(p), Value::Object(m)) = (spot, &mut params) {
        m.insert("x".into(), json!(p.x));
        m.insert("y".into(), json!(p.y));
    }
    super::edit::leave(state, session, None);
    session.break_undo_merge();
    if let Some(out) = run(state, session, ids::EDIT_PASTE_PICTURE, params)
        && let Some(id) = out.value.get("id").and_then(Value::as_str).and_then(Guid::parse)
    {
        state.selection.elements = vec![id];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_compare_without_line_end_noise() {
        assert!(same_text("a\r\nb\n", "a\nb"));
        assert!(!same_text("a", "b"));
    }
}
