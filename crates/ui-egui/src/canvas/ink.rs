//! The ink layer. Strokes are drawn as one anti-aliased mesh each, its width following the pen
//! pressure sample by sample (smooth tapers); a pen has round ends, a pencil is a little lighter,
//! a highlighter is wide, see-through, flat-ended and drawn under the text.
//!
//! Tools: the pens draw a live stroke and commit it on release with `draw.stroke` (pressure from
//! a pen or touch screen's `Event::Touch` force, else 0.5, which draws at full width); with Ink to
//! Shape on, the engine redraws it as a shape in that same command. The eraser removes strokes it
//! passes over (`draw.eraseAt`, one undo step per drag) and shows its size as a circle. The lasso
//! draws a loop and selects the ink inside (`draw.lassoSelect`); the selection shows a dashed box,
//! a drag inside it moves the ink (`draw.moveSelection`) and Delete deletes it. The Shape tool is
//! in [`super::shape`].

use egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, Ui, pos2, vec2};
use notecraft_doc::ink::MAX_POINTS;
use notecraft_doc::{Guid, InkPoint, InkStroke, InkTool, Page, Point};
use notecraft_engine::{Session, ids};
use serde_json::json;

use super::pointer::{self, Gesture};
use super::style::Ink;
use super::view::View;
use super::{CanvasState, Tool, edit, run};
use crate::theme::{Tokens, color32, mix};

/// Smallest pointer move that adds a point to a live stroke, pixels.
const MIN_STEP_PX: f32 = 1.5;
/// Pressure recorded without a pen (drawn at full width).
pub const NO_PRESSURE: f32 = 0.5;

/// Pressure reported by a pen or touch screen this frame (`0..=1`); a mouse reports none.
pub fn reported_pressure(ui: &Ui) -> Option<f32> {
    ui.input(|i| {
        i.events.iter().rev().find_map(|e| match e {
            egui::Event::Touch { force: Some(f), phase, .. }
                if matches!(phase, egui::TouchPhase::Start | egui::TouchPhase::Move) && f.is_finite() =>
            {
                Some(f.clamp(0.0, 1.0))
            }
            _ => None,
        })
    })
}

/// The colour a stroke is drawn in.
pub(crate) fn stroke_color(tool: InkTool, c: notecraft_doc::Color, ink: &Ink) -> Color32 {
    let base = ink.adapt(color32(c.with_alpha(255)));
    let base = if tool == InkTool::Pencil { mix(base, ink.paper, 0.18) } else { base };
    let a = (f32::from(c.a) * tool.alpha()).round().clamp(0.0, 255.0) as u8;
    let [r, g, b, _] = base.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Paints one stroke: page-point samples `(point, width in points)`.
pub fn paint_stroke(painter: &Painter, v: &View, samples: &[(Point, f32)], tool: InkTool, color: Color32) {
    let feather = 1.0 / painter.pixels_per_point().max(0.5);
    let mut pts: Vec<(Pos2, f32)> = Vec::with_capacity(samples.len());
    for (p, w) in samples {
        let s = v.to_screen(*p);
        let hw = (w * v.scale / 2.0).max(0.35);
        if pts.last().is_some_and(|(l, _)| l.distance(s) < 0.4) {
            continue;
        }
        pts.push((s, hw));
    }
    let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) else { return };
    let round = tool != InkTool::Highlighter;
    if pts.len() == 1 {
        if round {
            painter.circle_filled(first.0, first.1, color);
        } else {
            painter.rect_filled(Rect::from_center_size(first.0, vec2(first.1 * 0.6, first.1 * 2.0)), 0.0, color);
        }
        return;
    }
    // Each sample becomes a cross-section of four vertices (feathered edges); a bend is joined
    // with a mitre so the line keeps its width round a corner, and a sharp corner (a shape's)
    // ends one strip and starts the next, with a round dot over the joint.
    let perp = |d: egui::Vec2| {
        let l = d.length();
        (l > 1.0e-4).then(|| vec2(-d.y / l, d.x / l))
    };
    let mut mesh = Mesh::default();
    let mut corners: Vec<(Pos2, f32)> = Vec::new();
    let n = pts.len();
    let mut ring = 0u32;
    for i in 0..n {
        let (p, hw) = pts.get(i).copied().unwrap_or(first);
        let n_in = i.checked_sub(1).and_then(|j| pts.get(j)).and_then(|q| perp(p - q.0));
        let n_out = pts.get(i + 1).and_then(|q| perp(q.0 - p));
        // Very thin lines fade instead of thinning below a pixel.
        let c = if hw * 2.0 < feather { color.gamma_multiply((hw * 2.0 / feather).clamp(0.2, 1.0)) } else { color };
        let mut section = |off: egui::Vec2, join: bool| {
            let inner = (hw - feather / 2.0).max(0.0);
            let outer = hw + feather / 2.0;
            mesh.colored_vertex(p + off * outer, Color32::TRANSPARENT);
            mesh.colored_vertex(p + off * inner, c);
            mesh.colored_vertex(p - off * inner, c);
            mesh.colored_vertex(p - off * outer, Color32::TRANSPARENT);
            if join && ring > 0 {
                let a = (ring - 1) * 4;
                let b = a + 4;
                for k in 0..3 {
                    mesh.add_triangle(a + k, a + k + 1, b + k);
                    mesh.add_triangle(a + k + 1, b + k + 1, b + k);
                }
            }
            ring += 1;
        };
        match (n_in, n_out) {
            (Some(a), Some(b)) => {
                let m = a + b;
                let ml = m.length();
                let k = if ml > 1.0e-3 { 1.0 / (m / ml).dot(a).max(1.0e-3) } else { f32::INFINITY };
                if k <= MITER_LIMIT {
                    section(m / ml * k, true);
                } else {
                    section(a, true);
                    section(b, false);
                    corners.push((p, hw));
                }
            }
            (Some(a), None) | (None, Some(a)) => section(a, true),
            (None, None) => section(vec2(0.0, 1.0), true),
        }
    }
    painter.add(Shape::mesh(mesh));
    if round && color.a() == 255 {
        painter.circle_filled(first.0, first.1, color);
        painter.circle_filled(last.0, last.1, color);
        for (p, hw) in corners {
            painter.circle_filled(p, hw, color);
        }
    }
}

/// Longest mitre at a bend, in half-widths; sharper corners are joined round instead.
const MITER_LIMIT: f32 = 2.0;

fn samples_of(s: &InkStroke, dx: f32, dy: f32) -> Vec<(Point, f32)> {
    s.points.iter().map(|p| (Point::new(p.x + dx, p.y + dy), s.sample_width(p.pressure))).collect()
}

/// The highlighter strokes (`under_text`) or the others, plus the live stroke of that kind.
pub fn paint_layer(painter: &Painter, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View, under_text: bool) {
    let ink = Ink::new(tokens, page.background.color);
    let (dx, dy) = pointer::ink_drag_offset(state);
    for s in &page.ink {
        if (s.tool == InkTool::Highlighter) != under_text {
            continue;
        }
        let moving = (dx, dy) != (0.0, 0.0) && state.selection.strokes.contains(&s.id);
        let (ox, oy) = if moving { (dx, dy) } else { (0.0, 0.0) };
        if let Some(b) = s.bounds()
            && !v.rect(b.translate(ox, oy)).intersects(v.clip)
        {
            continue;
        }
        paint_stroke(painter, v, &samples_of(s, ox, oy), s.tool, stroke_color(s.tool, s.color, &ink));
    }
    if let Gesture::Ink { tool, points } = &state.gesture
        && (*tool == InkTool::Highlighter) == under_text
    {
        let pen = state.pens.style(*tool);
        let live = InkStroke { id: Guid::NIL, points: points.clone(), color: pen.color, width: pen.width, tool: *tool }.sanitized();
        paint_stroke(painter, v, &samples_of(&live, 0.0, 0.0), *tool, stroke_color(*tool, pen.color, &ink));
    }
}

/// The lasso loop, the selection's dashed box and the eraser circle.
pub fn paint_overlay(painter: &Painter, state: &CanvasState, session: &Session, tokens: &Tokens, v: &View, resp: &egui::Response) {
    let dash = |pts: &[Pos2]| Shape::dashed_line(pts, Stroke::new(1.0, tokens.handle), 4.0, 3.0);
    if let Gesture::Lasso { points } = &state.gesture {
        let mut loop_pts: Vec<Pos2> = points.iter().map(|p| v.to_screen(*p)).collect();
        if let Some(f) = loop_pts.first().copied() {
            loop_pts.push(f);
        }
        painter.extend(dash(&loop_pts));
    }
    if let Some(r) = selection_bounds(session) {
        let (dx, dy) = pointer::ink_drag_offset(state);
        let sr = v.rect(r.translate(dx, dy)).expand(4.0);
        painter.extend(dash(&[sr.left_top(), sr.right_top(), sr.right_bottom(), sr.left_bottom(), sr.left_top()]));
    }
    if state.tool == Tool::Eraser
        && let Some(p) = resp.hover_pos()
    {
        let r = v.len(state.eraser_radius).max(4.0);
        painter.circle(p, r, Color32::from_white_alpha(40), Stroke::new(1.0, tokens.border_strong));
    }
}

/// Bounds of the selected strokes (page points).
pub fn selection_bounds(session: &Session) -> Option<notecraft_doc::Rect> {
    let page = session.current_page()?;
    session.ink_selection.iter().filter_map(|id| page.stroke(*id)).filter_map(InkStroke::bounds).reduce(notecraft_doc::Rect::union)
}

/// The top-most stroke under `at`, within a few pixels.
pub fn stroke_at(session: &Session, v: &View, at: Pos2) -> Option<Guid> {
    let page = session.current_page()?;
    let p = v.to_page(at);
    let tol = 4.0 / v.scale.max(0.01);
    page.ink.iter().rev().find(|s| s.hit(p, tol)).map(|s| s.id)
}

/// The pointer with a pen, the eraser or the lasso.
pub fn pointer(
    ui: &Ui,
    state: &mut CanvasState,
    session: &mut Session,
    v: &View,
    (starts, down, released, at): (bool, bool, bool, Pos2),
    resp: &egui::Response,
) {
    let _ = resp;
    let p = v.to_page(at);
    match state.tool {
        Tool::Pen | Tool::Pencil | Tool::Highlighter => {
            let Some(tool) = state.tool.ink_tool() else { return };
            let pressure = reported_pressure(ui).unwrap_or(NO_PRESSURE);
            if starts {
                edit::leave(state, session, None);
                state.pending = None;
                state.gesture = Gesture::Ink { tool, points: vec![InkPoint::new(p.x, p.y, pressure)] };
            } else if down && let Gesture::Ink { points, .. } = &mut state.gesture {
                let far = points.last().is_none_or(|l| Point::new(l.x, l.y).distance(p) * v.scale >= MIN_STEP_PX);
                if far && points.len() < MAX_POINTS {
                    points.push(InkPoint::new(p.x, p.y, pressure));
                }
            }
            if released && let Gesture::Ink { tool, points } = std::mem::take(&mut state.gesture) {
                commit_stroke(state, session, tool, points);
            }
        }
        Tool::Eraser => {
            if starts {
                state.gesture = Gesture::Erase { key: format!("erase:{}", Guid::new()), last: p };
                erase(state, session, p, p);
            } else if down && let Gesture::Erase { last, .. } = &state.gesture {
                let from = *last;
                if from.distance(p) * v.scale >= 1.0 {
                    erase(state, session, from, p);
                    if let Gesture::Erase { last, .. } = &mut state.gesture {
                        *last = p;
                    }
                }
            }
            if released && matches!(state.gesture, Gesture::Erase { .. }) {
                state.gesture = Gesture::None;
            }
        }
        Tool::Lasso => {
            if starts {
                let grab = selection_bounds(session).is_some_and(|r| v.rect(r).expand(4.0).contains(at));
                state.gesture = if grab { Gesture::InkMove { from: p, to: p } } else { Gesture::Lasso { points: vec![p] } };
            } else if down {
                match &mut state.gesture {
                    Gesture::Lasso { points } => {
                        let far = points.last().is_none_or(|l| l.distance(p) * v.scale >= MIN_STEP_PX * 2.0);
                        if far && points.len() < MAX_POINTS {
                            points.push(p);
                        }
                    }
                    Gesture::InkMove { to, .. } => *to = p,
                    _ => {}
                }
            }
            if released {
                match std::mem::take(&mut state.gesture) {
                    Gesture::Lasso { points } if points.len() >= 3 => {
                        let pts: Vec<[f32; 2]> = points.iter().map(|q| [q.x, q.y]).collect();
                        if let Some(out) = run(state, session, ids::DRAW_LASSO_SELECT, json!({"points": pts})) {
                            state.selection.strokes = pointer::ids_of(&out.value);
                        }
                    }
                    Gesture::Lasso { .. } => pointer::clear_ink_selection(state, session),
                    Gesture::InkMove { from, to } => {
                        let (dx, dy) = (to.x - from.x, to.y - from.y);
                        if dx.abs() + dy.abs() > 0.25 {
                            run(state, session, ids::DRAW_MOVE_SELECTION, json!({"dx": dx, "dy": dy}));
                        }
                    }
                    other => state.gesture = other,
                }
            }
        }
        Tool::Shape(kind) => super::shape::pointer(ui, state, session, v, kind, (starts, down, released, at)),
        Tool::Type | Tool::Select | Tool::InsertSpace => {}
    }
}

fn commit_stroke(state: &mut CanvasState, session: &mut Session, tool: InkTool, points: Vec<InkPoint>) {
    if points.is_empty() {
        return;
    }
    let pen = state.pens.style(tool);
    let pts: Vec<[f32; 3]> = points.iter().map(|q| [q.x, q.y, q.pressure]).collect();
    let params = json!({"points": pts, "tool": tool.name(), "color": pen.color.to_hex(), "width": pen.width, "page": state.page});
    run(state, session, ids::DRAW_STROKE, params);
}

/// Erases along `a`→`b` when a stroke is in reach (no command, and no undo snapshot, otherwise).
fn erase(state: &mut CanvasState, session: &mut Session, a: Point, b: Point) {
    let radius = state.eraser_radius.clamp(0.5, notecraft_engine::cmd::draw::MAX_ERASER_RADIUS);
    let Some(page) = session.current_page() else { return };
    let steps = ((a.distance(b) / (radius / 2.0).max(0.5)).ceil() as usize).clamp(1, 2000);
    let hit = page.ink.iter().any(|s| {
        (0..=steps).any(|k| {
            let t = k as f32 / steps as f32;
            s.hit(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), radius)
        })
    });
    if !hit {
        return;
    }
    let key = match &state.gesture {
        Gesture::Erase { key, .. } => key.clone(),
        _ => format!("erase:{}", Guid::new()),
    };
    run(state, session, ids::DRAW_ERASE_AT, json!({"points": [[a.x, a.y], [b.x, b.y]], "radius": radius, "mergeKey": key, "page": state.page}));
    let page_sel = session.ink_selection.clone();
    state.selection.strokes = page_sel;
    let _ = pos2(0.0, 0.0);
}
