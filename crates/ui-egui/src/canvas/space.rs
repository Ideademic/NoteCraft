//! Insert › Insert Space: with the tool on, a horizontal line follows the pointer across the
//! page; drag it down to push everything below it (note containers, pictures, printouts,
//! attached files and ink) down by that much, or up to take space away. One `page.insertSpace`
//! on release, one undo step. The band being added (or removed) is shaded while dragging.

use egui::{Color32, CursorIcon, Painter, Pos2, Rect, Shape, Stroke, Ui, pos2, vec2};
use notecraft_engine::{Session, ids};
use serde_json::json;

use super::pointer::Gesture;
use super::view::View;
use super::{CanvasState, Tool, run};
use crate::theme::Tokens;

/// Smallest drag that changes anything, points.
const MIN_DRAG: f32 = 1.0;

/// The drag in progress: `(from, to)` page y.
pub fn drag_span(state: &CanvasState) -> Option<(f32, f32)> {
    match state.gesture {
        Gesture::Space { from, to } => Some((from, to)),
        _ => None,
    }
}

/// The pointer with the Insert Space tool: press starts the line, release commits.
pub fn pointer(ui: &Ui, state: &mut CanvasState, session: &mut Session, v: &View, (starts, down, released, at): (bool, bool, bool, Pos2)) {
    let y = v.to_page(at).y.max(0.0);
    if starts {
        super::edit::leave(state, session, None);
        state.selection.clear();
        state.gesture = Gesture::Space { from: y, to: y };
    } else if down && let Gesture::Space { to, .. } = &mut state.gesture {
        *to = y;
    }
    if released && let Gesture::Space { from, to } = std::mem::take(&mut state.gesture) {
        let dy = to - from;
        if dy.abs() >= MIN_DRAG {
            run(state, session, ids::PAGE_INSERT_SPACE, json!({"y": from, "dy": dy, "page": state.page}));
        }
    }
    if state.tool == Tool::InsertSpace {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }
}

/// The line under the pointer (or the band being dragged), across the visible page.
pub fn paint(ui: &Ui, painter: &Painter, state: &CanvasState, tokens: &Tokens, v: &View, sheet: Rect, resp: &egui::Response) {
    if state.tool != Tool::InsertSpace {
        return;
    }
    let line = tokens.accent;
    let (from, to) = match drag_span(state) {
        Some(span) => span,
        None => {
            let Some(p) = resp.hover_pos().or_else(|| ui.input(|i| i.pointer.latest_pos())).filter(|p| sheet.contains(*p)) else { return };
            let y = v.to_page(p).y;
            (y, y)
        }
    };
    let (sy0, sy1) = (v.pt(0.0, from).y, v.pt(0.0, to).y);
    if (sy1 - sy0).abs() > 0.5 {
        let band = Rect::from_x_y_ranges(sheet.x_range(), sy0.min(sy1)..=sy0.max(sy1));
        let fill =
            if to > from { Color32::from_rgba_unmultiplied(0x90, 0x60, 0xD0, 0x22) } else { Color32::from_rgba_unmultiplied(0xD0, 0x50, 0x50, 0x28) };
        painter.rect_filled(band, 0.0, fill);
        dashed(painter, sheet, sy0, line);
    }
    painter.line_segment([pos2(sheet.left(), sy1), pos2(sheet.right(), sy1)], Stroke::new(1.5, line));
    // An arrow at the pointer: down while adding space, up while removing it.
    let x = resp.hover_pos().map_or(sheet.center().x, |p| p.x.clamp(sheet.left() + 12.0, sheet.right() - 12.0));
    let dir = if to < from { -1.0 } else { 1.0 };
    let tip = pos2(x, sy1 + dir * 14.0);
    painter.line_segment([pos2(x, sy1), tip], Stroke::new(1.5, line));
    painter.add(Shape::convex_polygon(
        vec![tip + vec2(0.0, dir * 4.0), tip + vec2(-4.5, -dir * 2.0), tip + vec2(4.5, -dir * 2.0)],
        line,
        Stroke::NONE,
    ));
}

fn dashed(painter: &Painter, sheet: Rect, y: f32, color: Color32) {
    let mut x = sheet.left();
    while x < sheet.right() {
        let e = (x + 6.0).min(sheet.right());
        painter.line_segment([pos2(x, y), pos2(e, y)], Stroke::new(1.0, color));
        x += 10.0;
    }
}
