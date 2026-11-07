//! Ink (Draw tab): add a stroke, erase strokes; lasso select / move / delete / recolour (to do).

use notecraft_doc::ink::MAX_INK_PER_PAGE;
use notecraft_doc::{Guid, InkPoint, InkStroke, InkTool};
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::{DRAW_DELETE_SELECTION, DRAW_ERASE_AT, DRAW_ERASE_STROKE, DRAW_LASSO_SELECT, DRAW_MOVE_SELECTION, DRAW_RECOLOR, DRAW_STROKE};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(DRAW_STROKE, "Draw", stroke)
            .when(has_page)
            .params(r##"{"points": [[x, y, pressure?]] | [{"x","y","pressure"?}], "tool"?: "pen"|"pencil"|"highlighter", "color"?: "#rrggbb", "width"?: number, "page"?: guid}"##),
        CommandSpec::new(DRAW_ERASE_STROKE, "Erase Stroke", erase_stroke).when(has_page).params(r##"{"id"?: guid, "ids"?: [guid], "page"?: guid}"##),
        CommandSpec::stub(DRAW_ERASE_AT, "Eraser").when(has_page).params(r##"{"x": number, "y": number, "radius"?: number}"##),
        CommandSpec::stub(DRAW_LASSO_SELECT, "Lasso Select").pure().when(has_page).params(r##"{"points": [[x, y]]}"##),
        CommandSpec::stub(DRAW_MOVE_SELECTION, "Move Ink").when(has_page).params(r##"{"dx": number, "dy": number}"##),
        CommandSpec::stub(DRAW_DELETE_SELECTION, "Delete Ink").when(has_page),
        CommandSpec::stub(DRAW_RECOLOR, "Ink Color").when(has_page).params(r##"{"color": "#rrggbb"}"##),
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
    let page = target_page(s, params)?;
    if page.ink.len() >= MAX_INK_PER_PAGE {
        return Err(CmdError::Failed("this page holds as much ink as it can".into()));
    }
    let id = st.id;
    page.ink.push(st);
    page.touch();
    Ok(CmdOutput::value(json!({"id": id})))
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
