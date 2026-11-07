//! Ink (Draw tab): draw a stroke; the stroke eraser; lasso select, move, delete and recolour
//! the selection ([`Session::ink_selection`]); Ink to Shape; insert a shape.
//!
//! The lasso selection belongs to the current page and is not undoable (like a text
//! selection); commands that change selected ink are.
//!
//! **Ink to Shape** is a mode, as in OneNote: `draw.inkToShape` turns it on or off
//! ([`Session::ink_to_shape`]); while it is on, `draw.stroke` redraws a stroke that looks like a
//! shape ([`super::ink_shape`]) in the same undo step, and joins a line and an arrowhead drawn
//! one after the other into one arrow. Highlighter strokes are never converted. Given `ids`, or
//! with ink lasso-selected, `draw.inkToShape` converts those strokes instead; it never converts
//! the whole page by itself.
//!
//! **Shapes** (`draw.shape`) are ink too: the polylines of [`super::shapes::shape_strokes`],
//! selected once inserted.

use notecraft_doc::ink::{MAX_INK_PER_PAGE, MAX_WIDTH, MIN_WIDTH};
use notecraft_doc::{Guid, InkPoint, InkStroke, InkTool, Point};
use serde_json::{Value, json};

use super::ink_shape::{self, InkShape};
use super::shapes::{self, ShapeKind};
use super::{has_page, target_page};
use crate::ids::{
    DRAW_DELETE_SELECTION, DRAW_ERASE_AT, DRAW_ERASE_STROKE, DRAW_INK_TO_SHAPE, DRAW_LASSO_SELECT, DRAW_MOVE_SELECTION, DRAW_RECOLOR, DRAW_SHAPE,
    DRAW_STROKE,
};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// The stroke eraser's default radius, points.
pub const ERASER_RADIUS: f32 = 6.0;
/// Largest eraser radius, points.
pub const MAX_ERASER_RADIUS: f32 = 200.0;

fn has_selection(s: &Session) -> bool {
    has_page(s) && !s.ink_selection.is_empty()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(DRAW_STROKE, "Draw", stroke)
            .when(has_page)
            .params(r##"{"points": [[x, y, pressure?]] | [{"x","y","pressure"?}], "tool"?: "pen"|"pencil"|"highlighter", "color"?: "#rrggbb", "width"?: number, "page"?: guid} -> {"id": guid, "shape": null|"line"|"arrow"|"rectangle"|"square"|"ellipse"|"circle"|"triangle"|"polygon"}"##),
        CommandSpec::new(DRAW_ERASE_STROKE, "Erase Stroke", erase_stroke).when(has_page).params(r##"{"id"?: guid, "ids"?: [guid], "page"?: guid}"##),
        CommandSpec::new(DRAW_ERASE_AT, "Eraser", erase_at).when(has_page).params(
            r##"{"x": number, "y": number} | {"points": [[x, y]]} (an eraser drag), "radius"?: number (default 6), "mergeKey"?: string, "page"?: guid} -> {"erased": [guid]}"##,
        ),
        CommandSpec::new(DRAW_LASSO_SELECT, "Lasso Select", lasso_select)
            .pure()
            .when(has_page)
            .params(r##"{"points": [[x, y]] (the lasso polygon; fewer than 3 points clears) | "ids": [guid] | "all": true, "add"?: bool} -> {"ids": [guid]}"##),
        CommandSpec::new(DRAW_MOVE_SELECTION, "Move Ink", move_selection)
            .when(has_selection)
            .params(r##"{"dx": number, "dy": number, "mergeKey"?: string}"##),
        CommandSpec::new(DRAW_DELETE_SELECTION, "Delete Ink", delete_selection).when(has_selection).params("{}"),
        CommandSpec::new(DRAW_RECOLOR, "Ink Color", recolor)
            .when(has_page)
            .params(r##"{"color"?: "#rrggbb", "width"?: number, "ids"?: [guid] (default: the selection)}"##),
        CommandSpec::new(DRAW_INK_TO_SHAPE, "Ink to Shape", ink_to_shape).params(
            r##"{"value"?: bool, "ids"?: [guid], "page"?: guid} -> {"value": bool} | {"converted": number, "kept": number, "shapes": [string]} (value: set the mode; ids or a lasso selection: convert those; else toggle the mode)"##,
        ),
        CommandSpec::new(DRAW_SHAPE, "Shapes", shape).when(has_page).params(
            r##"{"kind": "line"|"arrow"|"doubleArrow"|"rectangle"|"roundedRectangle"|"ellipse"|"triangle"|"rightTriangle"|"parallelogram"|"trapezoid"|"diamond"|"pentagon"|"hexagon"|"axes2"|"axes2Arrows"|"axes3", "x": number, "y": number, "w": number, "h": number, "constrain"?: bool, "color"?: "#rrggbb", "width"?: number, "page"?: guid} -> {"ids": [guid]}"##,
        ),
    ]
}

/// The `points` parameter: `[[x, y], [x, y, p]…]` or `[{"x", "y", "pressure"?}…]`.
pub(crate) fn parse_points(v: &Value) -> Result<Vec<InkPoint>, CmdError> {
    let arr = v.get("points").and_then(Value::as_array).ok_or_else(|| CmdError::Params("`points` (array) is required".into()))?;
    if arr.len() > notecraft_doc::ink::MAX_POINTS {
        return Err(CmdError::Params(format!("at most {} points", notecraft_doc::ink::MAX_POINTS)));
    }
    let num = |x: Option<&Value>| x.and_then(Value::as_f64).map(|f| f as f32);
    arr.iter()
        .map(|pt| match pt {
            Value::Array(a) => match (num(a.first()), num(a.get(1))) {
                (Some(x), Some(y)) => Ok(InkPoint::new(x, y, num(a.get(2)).unwrap_or(1.0))),
                _ => Err(CmdError::Params("a point is [x, y] or [x, y, pressure]".into())),
            },
            Value::Object(_) => match (num(pt.get("x")), num(pt.get("y"))) {
                (Some(x), Some(y)) => Ok(InkPoint::new(x, y, num(pt.get("pressure")).unwrap_or(1.0))),
                _ => Err(CmdError::Params("a point needs x and y".into())),
            },
            _ => Err(CmdError::Params("a point is [x, y] or {\"x\", \"y\"}".into())),
        })
        .collect()
}

fn stroke(s: &mut Session, params: &Value) -> CmdResult {
    let points = parse_points(params)?;
    let tool = match p::str(params, "tool") {
        Some(t) => InkTool::parse(t).ok_or_else(|| CmdError::Params(format!("unknown tool `{}`", t.chars().take(20).collect::<String>())))?,
        None => InkTool::Pen,
    };
    let color = p::color(params, "color")?;
    let width = p::f32(params, "width");
    let mut st = InkStroke::new(tool, points);
    if let Some(c) = color {
        st.color = c;
    }
    if let Some(w) = width {
        st.width = w;
    }
    let st = st.sanitized();
    if st.points.is_empty() {
        return Err(CmdError::Params("a stroke needs at least one point".into()));
    }
    let convert = s.ink_to_shape && st.tool != InkTool::Highlighter;
    let page = target_page(s, params)?;
    if page.ink.len() >= MAX_INK_PER_PAGE {
        return Err(CmdError::Failed("this page holds as much ink as it can".into()));
    }
    let id = st.id;
    page.ink.push(st);
    let (id, shape) = if convert { convert_new_stroke(page, id) } else { (id, None) };
    page.touch();
    Ok(CmdOutput::value(json!({"id": id, "shape": shape})))
}

/// Ink to Shape on the stroke `id` just added (the last on `page`): with the stroke before it, a
/// line and an arrowhead become one arrow (the earlier stroke keeps its id, the new one goes);
/// else the stroke is redrawn as the shape it looks like. Returns the stroke's id and what it
/// became.
fn convert_new_stroke(page: &mut notecraft_doc::Page, id: Guid) -> (Guid, Option<&'static str>) {
    let n = page.ink.len();
    let new_pts = page.ink.last().map(|st| ink_shape::xy(&st.points)).unwrap_or_default();
    let prev = n.checked_sub(2).and_then(|i| page.ink.get(i)).filter(|p| p.tool != InkTool::Highlighter).map(|p| (p.id, ink_shape::xy(&p.points)));
    if let Some((prev_id, prev_pts)) = prev {
        let (new_parts, prev_parts) = (ink_shape::arrow_parts(&new_pts), ink_shape::arrow_parts(&prev_pts));
        // The line drawn first and the head now, or the head first and the line now.
        let paired = ink_shape::pair_parts(&prev_parts, &new_parts)
            .map(|a| (a, prev_id, id))
            .or_else(|| ink_shape::pair_parts(&new_parts, &prev_parts).map(|a| (a, id, prev_id)));
        if let Some((arrow, keep, gone)) = paired {
            if let Some(st) = page.ink.iter_mut().find(|st| st.id == keep) {
                st.points = ink_shape::redraw(&st.points, &arrow);
            }
            page.ink.retain(|st| st.id != gone);
            return (keep, Some(arrow.name()));
        }
    }
    match ink_shape::recognise(&new_pts) {
        Some(shape) => {
            if let Some(st) = page.ink.last_mut() {
                st.points = ink_shape::redraw(&st.points, &shape);
            }
            (id, Some(shape.name()))
        }
        None => (id, None),
    }
}

fn erase_stroke(s: &mut Session, params: &Value) -> CmdResult {
    let mut ids: Vec<Guid> = p::guid(params, "id")?.into_iter().collect();
    if let Some(arr) = params.get("ids").and_then(Value::as_array) {
        for v in arr.iter().take(MAX_INK_PER_PAGE) {
            ids.push(v.as_str().and_then(Guid::parse).ok_or_else(|| CmdError::Params("`ids` holds GUID strings".into()))?);
        }
    }
    if ids.is_empty() {
        return Err(CmdError::Params("`id` or `ids` is required".into()));
    }
    let page = target_page(s, params)?;
    let before = page.ink.len();
    page.ink.retain(|st| !ids.contains(&st.id));
    if page.ink.len() == before {
        return Ok(CmdOutput::unchanged());
    }
    page.touch();
    s.ink_selection.retain(|id| !ids.contains(id));
    Ok(CmdOutput::ok())
}

/// `params[key]` as GUIDs (bounded).
fn guid_list(params: &Value, key: &str) -> Result<Option<Vec<Guid>>, CmdError> {
    let Some(arr) = params.get(key) else { return Ok(None) };
    let arr = arr.as_array().ok_or_else(|| CmdError::Params(format!("`{key}` is a list of GUIDs")))?;
    let ids = arr
        .iter()
        .take(MAX_INK_PER_PAGE)
        .map(|v| v.as_str().and_then(Guid::parse).ok_or_else(|| CmdError::Params(format!("`{key}` holds GUID strings"))))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ids))
}

/// Points of a lasso or eraser path: `[[x, y]…]` or `[{"x","y"}…]`, sanitised.
fn path_points(params: &Value) -> Result<Vec<Point>, CmdError> {
    Ok(parse_points(params)?.into_iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| p.pos().sanitized()).collect())
}

/// Stroke eraser: removes every stroke the eraser touches at a point or along a drag.
fn erase_at(s: &mut Session, params: &Value) -> CmdResult {
    let radius = p::f32(params, "radius").unwrap_or(ERASER_RADIUS).clamp(0.0, MAX_ERASER_RADIUS);
    let pts = if params.get("points").is_some() { path_points(params)? } else { vec![p::req_point(params)?] };
    if pts.is_empty() {
        return Err(CmdError::Params("the eraser needs a point".into()));
    }
    let pts = densify(&pts, (radius / 2.0).max(1.0));
    let page = target_page(s, params)?;
    let hit: Vec<Guid> = page.ink.iter().filter(|st| pts.iter().any(|pt| st.hit(*pt, radius))).map(|st| st.id).collect();
    if hit.is_empty() {
        return Ok(CmdOutput::query(json!({"erased": []})));
    }
    page.ink.retain(|st| !hit.contains(&st.id));
    page.touch();
    s.ink_selection.retain(|id| !hit.contains(id));
    Ok(CmdOutput::value(json!({"erased": hit})))
}

/// Most points an eraser path is tested at.
const MAX_ERASER_SAMPLES: usize = 20_000;

/// The path with extra points so no gap between samples is longer than `step` (bounded): a
/// fast eraser drag still erases what it crossed.
fn densify(pts: &[Point], step: f32) -> Vec<Point> {
    let mut out = Vec::with_capacity(pts.len());
    if let Some(first) = pts.first() {
        out.push(*first);
    }
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let n = (a.distance(*b) / step).ceil().clamp(1.0, MAX_ERASER_SAMPLES as f32) as usize;
        for i in 1..=n {
            if out.len() >= MAX_ERASER_SAMPLES {
                return out;
            }
            let t = i as f32 / n as f32;
            out.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

/// Lasso: selects the strokes on the current page mostly inside the polygon (or by `ids`, or
/// `all`). Not undoable.
fn lasso_select(s: &mut Session, params: &Value) -> CmdResult {
    let add = p::bool(params, "add").unwrap_or(false);
    let ids = guid_list(params, "ids")?;
    let poly = if params.get("points").is_some() { Some(path_points(params)?) } else { None };
    let all = p::bool(params, "all").unwrap_or(false);
    let page = s.current_page().ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
    let picked: Vec<Guid> = match (ids, poly) {
        (Some(ids), _) => page.ink.iter().filter(|st| ids.contains(&st.id)).map(|st| st.id).collect(),
        (None, Some(poly)) if poly.len() >= 3 => page.ink.iter().filter(|st| st.in_lasso(&poly)).map(|st| st.id).collect(),
        (None, Some(_)) => Vec::new(),
        (None, None) if all => page.ink.iter().map(|st| st.id).collect(),
        (None, None) => return Err(CmdError::Params("`points`, `ids` or `all` is required".into())),
    };
    if !add {
        s.ink_selection.clear();
    }
    for id in picked {
        if !s.ink_selection.contains(&id) {
            s.ink_selection.push(id);
        }
    }
    Ok(CmdOutput::query(json!({"ids": s.ink_selection})))
}

fn move_selection(s: &mut Session, params: &Value) -> CmdResult {
    let (dx, dy) = (p::req_f32(params, "dx")?, p::req_f32(params, "dy")?);
    if dx == 0.0 && dy == 0.0 {
        return Ok(CmdOutput::unchanged());
    }
    let sel = s.ink_selection.clone();
    let page = s.current_page_mut().ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
    let mut any = false;
    for st in page.ink.iter_mut().filter(|st| sel.contains(&st.id)) {
        st.translate(dx, dy);
        any = true;
    }
    if !any {
        return Ok(CmdOutput::unchanged());
    }
    page.touch();
    Ok(CmdOutput::ok())
}

fn delete_selection(s: &mut Session, _: &Value) -> CmdResult {
    let sel = std::mem::take(&mut s.ink_selection);
    let page = s.current_page_mut().ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
    let before = page.ink.len();
    page.ink.retain(|st| !sel.contains(&st.id));
    if page.ink.len() == before {
        return Ok(CmdOutput::unchanged());
    }
    page.touch();
    Ok(CmdOutput::ok())
}

/// Recolours (and optionally re-widths) the selected strokes, or `ids`.
fn recolor(s: &mut Session, params: &Value) -> CmdResult {
    let color = p::color(params, "color")?;
    let width = p::f32(params, "width").map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH));
    if color.is_none() && width.is_none() {
        return Err(CmdError::Params("`color` or `width` is required".into()));
    }
    let ids = guid_list(params, "ids")?.unwrap_or_else(|| s.ink_selection.clone());
    let page = target_page(s, params)?;
    let mut changed = false;
    for st in page.ink.iter_mut().filter(|st| ids.contains(&st.id)) {
        if let Some(c) = color
            && st.color != c
        {
            st.color = c;
            changed = true;
        }
        if let Some(w) = width
            && st.width != w
        {
            st.width = w;
            changed = true;
        }
    }
    if !changed {
        return Ok(CmdOutput::unchanged());
    }
    page.touch();
    Ok(CmdOutput::ok())
}

/// Ink to Shape: `value` turns the mode on or off. Otherwise, with `ids` or a lasso selection,
/// redraws those strokes that look like shapes (same id, colour, width and tool; highlighters
/// stay as drawn) and says so when none does; with neither, toggles the mode. A mode change is
/// not an undo step.
fn ink_to_shape(s: &mut Session, params: &Value) -> CmdResult {
    if let Some(on) = p::bool(params, "value") {
        s.ink_to_shape = on;
        return Ok(CmdOutput::query(json!({"value": on})));
    }
    let ids = match guid_list(params, "ids")? {
        Some(ids) => ids,
        None if !s.ink_selection.is_empty() && s.current_page().is_some() => s.ink_selection.clone(),
        None => {
            s.ink_to_shape = !s.ink_to_shape;
            return Ok(CmdOutput::query(json!({"value": s.ink_to_shape})));
        }
    };
    let page = target_page(s, params)?;
    let (mut converted, mut kept, mut names) = (0usize, 0usize, Vec::new());
    for st in page.ink.iter_mut().filter(|st| ids.contains(&st.id)) {
        let found: Option<InkShape> = if st.tool == InkTool::Highlighter { None } else { ink_shape::recognise(&ink_shape::xy(&st.points)) };
        match found {
            Some(shape) => {
                st.points = ink_shape::redraw(&st.points, &shape);
                names.push(shape.name());
                converted += 1;
            }
            None => kept += 1,
        }
    }
    let out = json!({"converted": converted, "kept": kept, "shapes": names});
    if converted == 0 {
        s.notices.push(if kept == 0 {
            "There is no ink to turn into shapes.".to_owned()
        } else {
            "No shapes were recognised in the selected ink.".to_owned()
        });
        return Ok(CmdOutput::query(out));
    }
    page.touch();
    Ok(CmdOutput::value(out))
}

/// Inserts a shape as ink strokes (pen, the given colour and width) in one undo step and selects
/// them (on the current page).
fn shape(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::req_str(params, "kind")?;
    let kind = ShapeKind::parse(name).ok_or_else(|| CmdError::Params(format!("unknown shape `{}`", name.chars().take(24).collect::<String>())))?;
    let rect = [p::req_f32(params, "x")?, p::req_f32(params, "y")?, p::req_f32(params, "w")?, p::req_f32(params, "h")?];
    let strokes = shapes::shape_strokes(kind, rect, p::bool(params, "constrain").unwrap_or(false));
    if strokes.is_empty() {
        return Err(CmdError::Params("the shape is too small".into()));
    }
    let color = p::color(params, "color")?;
    let width = p::f32(params, "width");
    let on_current = match p::guid(params, "page")? {
        Some(id) => s.current.page == Some(id),
        None => true,
    };
    let page = target_page(s, params)?;
    if page.ink.len().saturating_add(strokes.len()) > MAX_INK_PER_PAGE {
        return Err(CmdError::Failed("this page holds as much ink as it can".into()));
    }
    let mut ids = Vec::with_capacity(strokes.len());
    for pts in strokes {
        let mut st = InkStroke::new(InkTool::Pen, pts.into_iter().map(|[x, y]| InkPoint::new(x, y, 1.0)).collect());
        if let Some(c) = color {
            st.color = c;
        }
        if let Some(w) = width {
            st.width = w;
        }
        let st = st.sanitized();
        ids.push(st.id);
        page.ink.push(st);
    }
    page.touch();
    if on_current {
        s.ink_selection = ids.clone();
    }
    Ok(CmdOutput::value(json!({"ids": ids, "kind": kind.name()})))
}
