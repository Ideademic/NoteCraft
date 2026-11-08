//! Cropping a picture with handles (Draw › Crop, or the picture's context menu running
//! `picture.crop`). While cropping, the whole picture shows with what will be cut off dimmed;
//! eight black handles (corners and edge middles) move the crop's edges, a drag inside moves the
//! crop over the picture. Enter, or a click off the picture, keeps the crop (`picture.crop` with
//! the new visible rect: one undo step); Esc leaves it as it was.

use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use notecraft_doc::{ElementKind, Guid, Point};
use notecraft_engine::{Session, ids};
use serde_json::json;

use super::view::View;
use super::{CanvasState, run};
use crate::theme::Tokens;

/// Smallest crop, points.
const MIN_SIDE: f32 = 8.0;
/// Handle grab size, pixels.
const GRAB: f32 = 14.0;

/// A crop being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct CropEdit {
    pub id: Guid,
    /// Where the whole picture is (page points).
    pub full: notecraft_doc::Rect,
    /// The part kept so far (page points, inside `full`).
    pub rect: notecraft_doc::Rect,
    /// A handle being dragged: which (0 top-left, clockwise to 7 left; 8 the whole crop), the
    /// crop when the drag started and where it started.
    pub drag: Option<(u8, notecraft_doc::Rect, Point)>,
}

/// Starts cropping picture `id` on the current page. `false` when it isn't a picture there.
pub fn begin(state: &mut CanvasState, session: &mut Session, id: Guid) -> bool {
    let Some(img) = session.current_page().and_then(|p| p.element(id)).and_then(|e| match &e.kind {
        ElementKind::Image(i) => Some(i.clone()),
        _ => None,
    }) else {
        return false;
    };
    super::edit::leave(state, session, None);
    state.selection.elements = vec![id];
    state.crop = Some(CropEdit { id, full: img.full_rect(), rect: notecraft_doc::Rect::from_min_size(img.pos, img.size), drag: None });
    true
}

/// Keeps the crop (Enter, a click elsewhere).
pub fn commit(state: &mut CanvasState, session: &mut Session) {
    let Some(c) = state.crop.take() else { return };
    let r = c.rect;
    run(state, session, ids::PICTURE_CROP, json!({"element": c.id, "rect": {"x": r.x, "y": r.y, "w": r.w, "h": r.h}, "page": state.page}));
    state.selection.elements = vec![c.id];
}

/// Leaves the picture as it was (Esc).
pub fn cancel(state: &mut CanvasState) {
    state.crop = None;
}

/// The handles' centres on screen, in handle order.
fn handles(r: Rect) -> [Pos2; 8] {
    [r.left_top(), r.center_top(), r.right_top(), r.right_center(), r.right_bottom(), r.center_bottom(), r.left_bottom(), r.left_center()]
}

/// A press while cropping: grabs a handle or the crop; elsewhere keeps the crop. Returns
/// whether the press was used (a press off the picture also falls through to the page after
/// the crop is kept).
pub fn press(state: &mut CanvasState, session: &mut Session, v: &View, at: Pos2) -> bool {
    let Some(c) = state.crop.as_mut() else { return false };
    let sr = v.rect(c.rect);
    let p = v.to_page(at);
    if let Some(h) = handles(sr).iter().position(|h| Rect::from_center_size(*h, vec2(GRAB, GRAB)).contains(at)) {
        c.drag = Some((h as u8, c.rect, p));
        return true;
    }
    if sr.contains(at) {
        c.drag = Some((8, c.rect, p));
        return true;
    }
    if v.rect(c.full).contains(at) {
        // On the dimmed part: nothing to grab there.
        return true;
    }
    commit(state, session);
    false
}

/// A drag while cropping.
pub fn drag(state: &mut CanvasState, v: &View, at: Pos2) {
    let Some(c) = state.crop.as_mut() else { return };
    let Some((h, start, from)) = c.drag else { return };
    let p = v.to_page(at);
    c.rect = dragged(c.full, start, h, p.x - from.x, p.y - from.y);
}

/// A release while cropping ends the drag (the crop is kept with Enter or a click elsewhere).
pub fn release(state: &mut CanvasState) {
    if let Some(c) = state.crop.as_mut() {
        c.drag = None;
    }
}

/// `start` with handle `h` moved by (dx, dy), kept inside `full` and at least [`MIN_SIDE`].
pub fn dragged(full: notecraft_doc::Rect, start: notecraft_doc::Rect, h: u8, dx: f32, dy: f32) -> notecraft_doc::Rect {
    let (mut l, mut t, mut r, mut b) = (start.x, start.y, start.right(), start.bottom());
    if h == 8 {
        let dx = dx.clamp(full.x - l, full.right() - r);
        let dy = dy.clamp(full.y - t, full.bottom() - b);
        return start.translate(dx, dy);
    }
    let left = matches!(h, 0 | 6 | 7);
    let right = matches!(h, 2 | 3 | 4);
    let top = matches!(h, 0..=2);
    let bottom = matches!(h, 4..=6);
    let min_w = MIN_SIDE.min(full.w);
    let min_h = MIN_SIDE.min(full.h);
    if left {
        l = (l + dx).clamp(full.x, r - min_w);
    }
    if right {
        r = (r + dx).clamp(l + min_w, full.right());
    }
    if top {
        t = (t + dy).clamp(full.y, b - min_h);
    }
    if bottom {
        b = (b + dy).clamp(t + min_h, full.bottom());
    }
    notecraft_doc::Rect::new(l, t, r - l, b - t)
}

/// The cursor over a crop handle.
pub fn cursor(state: &CanvasState, v: &View, at: Pos2) -> Option<CursorIcon> {
    let c = state.crop.as_ref()?;
    let sr = v.rect(c.rect);
    if let Some((h, ..)) = c.drag {
        return Some(handle_cursor(h));
    }
    if let Some(h) = handles(sr).iter().position(|h| Rect::from_center_size(*h, vec2(GRAB, GRAB)).contains(at)) {
        return Some(handle_cursor(h as u8));
    }
    sr.contains(at).then_some(CursorIcon::Move)
}

fn handle_cursor(h: u8) -> CursorIcon {
    match h {
        0 | 4 => CursorIcon::ResizeNwSe,
        2 | 6 => CursorIcon::ResizeNeSw,
        1 | 5 => CursorIcon::ResizeVertical,
        3 | 7 => CursorIcon::ResizeHorizontal,
        _ => CursorIcon::Move,
    }
}

/// The dimmed part outside the crop, the crop's frame and its handles.
pub fn paint(painter: &Painter, state: &CanvasState, tokens: &Tokens, v: &View) {
    let Some(c) = &state.crop else { return };
    let full = v.rect(c.full);
    let keep = v.rect(c.rect);
    let dim = Color32::from_rgba_unmultiplied(0x10, 0x10, 0x14, 0x90);
    // Four bands round the kept part.
    for r in [
        Rect::from_min_max(full.min, pos2(full.max.x, keep.min.y)),
        Rect::from_min_max(pos2(full.min.x, keep.max.y), full.max),
        Rect::from_min_max(pos2(full.min.x, keep.min.y), pos2(keep.min.x, keep.max.y)),
        Rect::from_min_max(pos2(keep.max.x, keep.min.y), pos2(full.max.x, keep.max.y)),
    ] {
        if r.width() > 0.0 && r.height() > 0.0 {
            painter.rect_filled(r, 0.0, dim);
        }
    }
    painter.rect_stroke(full, 0.0, Stroke::new(1.0, tokens.border_strong), StrokeKind::Outside);
    painter.rect_stroke(keep, 0.0, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
    painter.rect_stroke(keep, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Outside);
    // Crop handles: thick black bars, L-shaped at the corners, with a white edge.
    let len = 12.0f32.min(keep.width() / 3.0).min(keep.height() / 3.0).max(4.0);
    let w = 4.0;
    let bar = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], Stroke::new(w + 2.0, Color32::WHITE));
        painter.line_segment([a, b], Stroke::new(w, Color32::BLACK));
    };
    let (l, t, r, b) = (keep.left(), keep.top(), keep.right(), keep.bottom());
    for (corner, dx, dy) in
        [(keep.left_top(), 1.0, 1.0), (keep.right_top(), -1.0, 1.0), (keep.right_bottom(), -1.0, -1.0), (keep.left_bottom(), 1.0, -1.0)]
    {
        bar(corner, corner + vec2(dx * len, 0.0));
        bar(corner, corner + vec2(0.0, dy * len));
    }
    let (cx, cy) = (keep.center().x, keep.center().y);
    bar(pos2(cx - len / 2.0, t), pos2(cx + len / 2.0, t));
    bar(pos2(cx - len / 2.0, b), pos2(cx + len / 2.0, b));
    bar(pos2(l, cy - len / 2.0), pos2(l, cy + len / 2.0));
    bar(pos2(r, cy - len / 2.0), pos2(r, cy + len / 2.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_stay_inside_the_picture() {
        let full = notecraft_doc::Rect::new(0.0, 0.0, 200.0, 100.0);
        let r = dragged(full, full, 0, 50.0, 20.0);
        assert_eq!((r.x, r.y, r.w, r.h), (50.0, 20.0, 150.0, 80.0));
        let r = dragged(full, r, 0, -500.0, -500.0);
        assert_eq!((r.x, r.y), (0.0, 0.0));
        let small = dragged(full, full, 3, -1000.0, 0.0);
        assert!(small.w >= MIN_SIDE - 1e-3);
        let moved = dragged(full, notecraft_doc::Rect::new(10.0, 10.0, 50.0, 50.0), 8, 500.0, 500.0);
        assert_eq!((moved.right(), moved.bottom()), (200.0, 100.0));
    }
}
