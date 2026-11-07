//! The Shape tool (Draw › Shapes): drag on the page to draw the chosen shape. The drag shows a
//! live preview in the pen's colour and width; Shift keeps a square box (a square, a circle) or
//! snaps a line to 45°; Esc cancels. On release `draw.shape` inserts it (one undo step) and
//! selects it; a drag that starts inside the selection moves it instead. The tool stays chosen
//! until another one is, as in OneNote.

use egui::{Key, Painter, Pos2, Ui};
use notecraft_doc::{InkTool, Page, Point};
use notecraft_engine::cmd::shapes::{ShapeKind, shape_strokes};
use notecraft_engine::{Session, ids};
use serde_json::json;

use super::pointer::{self, Gesture};
use super::style::Ink;
use super::view::View;
use super::{CanvasState, edit, ink, run};
use crate::theme::Tokens;

/// Shortest drag that inserts a shape, screen pixels (a click inserts nothing).
const MIN_DRAG_PX: f32 = 4.0;

/// The pointer with the Shape tool.
pub fn pointer(
    ui: &Ui,
    state: &mut CanvasState,
    session: &mut Session,
    v: &View,
    kind: ShapeKind,
    (starts, down, released, at): (bool, bool, bool, Pos2),
) {
    let (shift, esc) = ui.input(|i| (i.modifiers.shift, i.key_pressed(Key::Escape)));
    let p = v.to_page(at);
    if esc && matches!(state.gesture, Gesture::Shape { .. }) {
        state.gesture = Gesture::None;
        return;
    }
    if starts {
        edit::leave(state, session, None);
        state.pending = None;
        let grab = ink::selection_bounds(session).is_some_and(|r| v.rect(r).expand(4.0).contains(at));
        if grab {
            state.gesture = Gesture::InkMove { from: p, to: p };
        } else {
            pointer::clear_ink_selection(state, session);
            state.gesture = Gesture::Shape { kind, from: p, to: p, constrain: shift };
        }
    } else if down {
        match &mut state.gesture {
            Gesture::Shape { to, constrain, .. } => {
                *to = p;
                *constrain = shift;
            }
            Gesture::InkMove { to, .. } => *to = p,
            _ => {}
        }
    }
    if !released {
        return;
    }
    match std::mem::take(&mut state.gesture) {
        Gesture::Shape { kind, from, to, constrain } => {
            let (w, h) = (to.x - from.x, to.y - from.y);
            if w.hypot(h) * v.scale < MIN_DRAG_PX {
                return;
            }
            let pen = state.pens.style(InkTool::Pen);
            let params = json!({
                "kind": kind.name(), "x": from.x, "y": from.y, "w": w, "h": h, "constrain": constrain,
                "color": pen.color.to_hex(), "width": pen.width, "page": state.page,
            });
            if let Some(out) = run(state, session, ids::DRAW_SHAPE, params) {
                state.selection.strokes = pointer::ids_of(&out.value);
            }
        }
        Gesture::InkMove { from, to } => {
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            if dx.abs() + dy.abs() > 0.25 {
                run(state, session, ids::DRAW_MOVE_SELECTION, json!({"dx": dx, "dy": dy}));
            }
        }
        other => state.gesture = other,
    }
}

/// The strokes of the shape being dragged (page points), if one is.
pub fn preview_strokes(state: &CanvasState) -> Vec<Vec<[f32; 2]>> {
    match &state.gesture {
        Gesture::Shape { kind, from, to, constrain } => shape_strokes(*kind, [from.x, from.y, to.x - from.x, to.y - from.y], *constrain),
        _ => Vec::new(),
    }
}

/// Paints the shape being dragged, as it will be inserted.
pub fn paint_preview(painter: &Painter, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View) {
    let strokes = preview_strokes(state);
    if strokes.is_empty() {
        return;
    }
    let pen = state.pens.style(InkTool::Pen);
    let color = ink::stroke_color(InkTool::Pen, pen.color, &Ink::new(tokens, page.background.color));
    for st in strokes {
        let samples: Vec<(Point, f32)> = st.into_iter().map(|[x, y]| (Point::new(x, y), pen.width)).collect();
        ink::paint_stroke(painter, v, &samples, InkTool::Pen, color);
    }
}
