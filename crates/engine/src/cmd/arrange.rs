//! Draw › Arrange (what is in front of what) and Draw › Rotate for ink.
//!
//! **Arrange** reorders the page's elements (pictures, note containers, printouts, files; drawn
//! back to front) or its ink strokes. Bring Forward moves the items in front of the next item
//! above them that they overlap (one step when none does); Send Backward is the reverse; Bring
//! to Front and Send to Back go to the end. Ink is always drawn over the elements, so the two
//! are ordered separately.
//!
//! **Rotate** turns or flips ink strokes about the centre of their bounding box.

use notecraft_doc::ink::MAX_INK_PER_PAGE;
use notecraft_doc::{Guid, Point, Rect};
use serde_json::{Value, json};

use super::draw::guid_list;
use super::{has_page, target_page};
use crate::ids::{ARRANGE_BRING_FORWARD, ARRANGE_BRING_TO_FRONT, ARRANGE_SEND_BACKWARD, ARRANGE_SEND_TO_BACK, INK_FLIP, INK_ROTATE};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

const ARRANGE: &str = r##"{"element"?: guid, "elements"?: [guid], "strokes"?: [guid], "ink"?: true (the lasso selection), "page"?: guid}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(ARRANGE_BRING_TO_FRONT, "Bring to Front", |s, p| arrange(s, p, Order::Front)).when(has_page).params(ARRANGE),
        CommandSpec::new(ARRANGE_BRING_FORWARD, "Bring Forward", |s, p| arrange(s, p, Order::Forward)).when(has_page).params(ARRANGE),
        CommandSpec::new(ARRANGE_SEND_BACKWARD, "Send Backward", |s, p| arrange(s, p, Order::Backward)).when(has_page).params(ARRANGE),
        CommandSpec::new(ARRANGE_SEND_TO_BACK, "Send to Back", |s, p| arrange(s, p, Order::Back)).when(has_page).params(ARRANGE),
        CommandSpec::new(INK_ROTATE, "Rotate Ink", rotate_ink)
            .when(has_page)
            .params(r##"{"degrees": 90|-90|180|270 (clockwise), "ids"?: [guid] (default: the lasso selection), "page"?: guid}"##),
        CommandSpec::new(INK_FLIP, "Flip Ink", flip_ink)
            .when(has_page)
            .params(r##"{"axis": "horizontal"|"vertical", "ids"?: [guid] (default: the lasso selection), "page"?: guid}"##),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    Front,
    Forward,
    Backward,
    Back,
}

/// Reorders `items` (back to front) so the `picked` ones move as `order` says. Returns whether
/// anything moved.
fn reorder<T>(items: &mut Vec<T>, picked: &dyn Fn(&T) -> bool, bounds: &dyn Fn(&T) -> Option<Rect>, order: Order) -> bool {
    let before: Vec<bool> = items.iter().map(picked).collect();
    if !before.contains(&true) {
        return false;
    }
    let (sel, rest): (Vec<(usize, T)>, Vec<(usize, T)>) = std::mem::take(items).into_iter().enumerate().partition(|(_, t)| picked(t));
    let sel_bounds: Vec<Rect> = sel.iter().filter_map(|(_, t)| bounds(t)).collect();
    let overlaps = |t: &T| bounds(t).is_some_and(|b| sel_bounds.iter().any(|s| s.intersects(b)));
    let top = sel.iter().map(|(i, _)| *i).max().unwrap_or(0);
    let bottom = sel.iter().map(|(i, _)| *i).min().unwrap_or(0);
    // Where the moved items go in `rest`: before index `at`.
    let at = match order {
        Order::Front => rest.len(),
        Order::Back => 0,
        Order::Forward => {
            let above: Vec<usize> = rest.iter().enumerate().filter(|(_, (i, _))| *i > top).map(|(k, _)| k).collect();
            let hit = above.iter().copied().find(|k| rest.get(*k).is_some_and(|(_, t)| overlaps(t)));
            match hit.or_else(|| above.first().copied()) {
                Some(k) => k + 1,
                None => rest.len(),
            }
        }
        Order::Backward => {
            let below: Vec<usize> = rest.iter().enumerate().filter(|(_, (i, _))| *i < bottom).map(|(k, _)| k).collect();
            let hit = below.iter().rev().copied().find(|k| rest.get(*k).is_some_and(|(_, t)| overlaps(t)));
            hit.or_else(|| below.last().copied()).unwrap_or(0)
        }
    };
    let mut rest: Vec<T> = rest.into_iter().map(|(_, t)| t).collect();
    let at = at.min(rest.len());
    let tail = rest.split_off(at);
    rest.extend(sel.into_iter().map(|(_, t)| t));
    rest.extend(tail);
    *items = rest;
    let after: Vec<bool> = items.iter().map(picked).collect();
    after != before
}

fn arrange(s: &mut Session, params: &Value, order: Order) -> CmdResult {
    let mut elements = guid_list(params, "elements")?.unwrap_or_default();
    if let Some(id) = p::guid(params, "element")?.or(p::guid(params, "id")?) {
        elements.push(id);
    }
    let mut strokes = guid_list(params, "strokes")?.unwrap_or_default();
    if p::bool(params, "ink") == Some(true) {
        strokes.extend(s.ink_selection.iter().copied());
    }
    if elements.is_empty() && strokes.is_empty() {
        return Err(CmdError::Params("`element`, `elements`, `strokes` or `ink` is required".into()));
    }
    let page = target_page(s, params)?;
    let mut changed = false;
    if !elements.is_empty() {
        changed |= reorder(&mut page.elements, &|e| elements.contains(&e.id), &|e| e.bounds(), order);
    }
    if !strokes.is_empty() {
        changed |= reorder(&mut page.ink, &|st| strokes.contains(&st.id), &|st| st.bounds(), order);
    }
    if !changed {
        return Ok(CmdOutput::unchanged());
    }
    page.touch();
    Ok(CmdOutput::ok())
}

/// The strokes to turn: `ids`, else the lasso selection.
fn ink_targets(s: &Session, params: &Value) -> Result<Vec<Guid>, CmdError> {
    let ids = guid_list(params, "ids")?.unwrap_or_else(|| s.ink_selection.clone());
    if ids.is_empty() {
        return Err(CmdError::Disabled("select some ink first".into()));
    }
    Ok(ids.into_iter().take(MAX_INK_PER_PAGE).collect())
}

/// Maps every point of the strokes `ids` through `f` (given the union bounding box's centre).
fn transform_ink(s: &mut Session, params: &Value, f: &dyn Fn(Point, Point) -> Point) -> CmdResult {
    let ids = ink_targets(s, params)?;
    let page = target_page(s, params)?;
    let boxes = page.ink.iter().filter(|st| ids.contains(&st.id)).filter_map(|st| st.bbox());
    let Some(bb) = boxes.reduce(Rect::union) else { return Err(CmdError::Params("no such strokes on the page".into())) };
    let c = bb.center();
    for st in page.ink.iter_mut().filter(|st| ids.contains(&st.id)) {
        for pt in &mut st.points {
            let q = f(Point::new(pt.x, pt.y), c).sanitized();
            pt.x = q.x;
            pt.y = q.y;
        }
    }
    page.touch();
    Ok(CmdOutput::value(json!({"ids": ids})))
}

fn rotate_ink(s: &mut Session, params: &Value) -> CmdResult {
    let deg = params.get("degrees").and_then(Value::as_i64).ok_or_else(|| CmdError::Params("`degrees` (90, -90, 180 or 270) is required".into()))?;
    let turns = match deg.rem_euclid(360) {
        0 => return Ok(CmdOutput::unchanged()),
        90 => 1,
        180 => 2,
        270 => 3,
        _ => return Err(CmdError::Params("`degrees` must be a multiple of 90".into())),
    };
    // Clockwise on the page (y down): (dx, dy) → (-dy, dx).
    transform_ink(s, params, &|p, c| {
        let (mut dx, mut dy) = (p.x - c.x, p.y - c.y);
        for _ in 0..turns {
            (dx, dy) = (-dy, dx);
        }
        Point::new(c.x + dx, c.y + dy)
    })
}

fn flip_ink(s: &mut Session, params: &Value) -> CmdResult {
    let horizontal = match p::req_str(params, "axis")? {
        "horizontal" | "h" | "x" => true,
        "vertical" | "v" | "y" => false,
        _ => return Err(CmdError::Params("`axis` is \"horizontal\" or \"vertical\"".into())),
    };
    transform_ink(s, params, &|p, c| if horizontal { Point::new(2.0 * c.x - p.x, p.y) } else { Point::new(p.x, 2.0 * c.y - p.y) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    fn ids_on(s: &Session) -> Vec<Guid> {
        s.current_page().unwrap().elements.iter().map(|e| e.id).collect()
    }

    #[test]
    fn arrange_moves_elements_and_ink() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let mk = |s: &mut Session, x: f32| -> Guid {
            let v = s.run(ids::OUTLINE_NEW, &json!({"x": x, "y": 100, "text": "x"})).unwrap().value;
            Guid::parse(v["id"].as_str().unwrap()).unwrap()
        };
        let a = mk(&mut s, 100.0);
        let b = mk(&mut s, 2000.0); // far away: doesn't overlap
        let c = mk(&mut s, 110.0); // overlaps a
        s.run(ids::ARRANGE_BRING_FORWARD, &json!({"element": a})).unwrap();
        assert_eq!(ids_on(&s), vec![b, c, a], "past the next overlapping item");
        s.run(ids::ARRANGE_SEND_TO_BACK, &json!({"element": a})).unwrap();
        assert_eq!(ids_on(&s), vec![a, b, c]);
        s.run(ids::ARRANGE_BRING_TO_FRONT, &json!({"elements": [a, b]})).unwrap();
        assert_eq!(ids_on(&s), vec![c, a, b]);
        s.run(ids::ARRANGE_SEND_BACKWARD, &json!({"element": b})).unwrap();
        assert_eq!(ids_on(&s), vec![c, b, a], "one step when nothing overlaps");
        let depth = s.undo_depth();
        s.run(ids::ARRANGE_SEND_TO_BACK, &json!({"element": c})).unwrap();
        assert_eq!(s.undo_depth(), depth, "already at the back");
        assert!(s.run(ids::ARRANGE_BRING_TO_FRONT, &json!({})).is_err());
        // Ink.
        let st = |s: &mut Session| -> Guid {
            let v = s.run(ids::DRAW_STROKE, &json!({"points": [[0, 0], [10, 10]]})).unwrap().value;
            Guid::parse(v["id"].as_str().unwrap()).unwrap()
        };
        let (i1, i2) = (st(&mut s), st(&mut s));
        s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [i1]})).unwrap();
        s.run(ids::ARRANGE_BRING_TO_FRONT, &json!({"ink": true})).unwrap();
        let order: Vec<Guid> = s.current_page().unwrap().ink.iter().map(|x| x.id).collect();
        assert_eq!(order, vec![i2, i1]);
    }

    #[test]
    fn ink_turns_and_flips_about_its_centre() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        assert!(s.run(ids::INK_ROTATE, &json!({"degrees": 90})).is_err(), "nothing selected");
        let v = s.run(ids::DRAW_STROKE, &json!({"points": [[0, 0], [20, 0]]})).unwrap().value;
        let id = Guid::parse(v["id"].as_str().unwrap()).unwrap();
        s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [id]})).unwrap();
        s.run(ids::INK_ROTATE, &json!({"degrees": 90})).unwrap();
        let pts: Vec<(f32, f32)> = s.current_page().unwrap().stroke(id).unwrap().points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(pts, vec![(10.0, -10.0), (10.0, 10.0)]);
        assert_eq!(s.ink_selection, vec![id], "still selected");
        s.run(ids::INK_FLIP, &json!({"axis": "vertical", "ids": [id]})).unwrap();
        let pts: Vec<(f32, f32)> = s.current_page().unwrap().stroke(id).unwrap().points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(pts, vec![(10.0, 10.0), (10.0, -10.0)]);
        assert!(s.run(ids::INK_FLIP, &json!({"axis": "sideways"})).is_err());
    }
}
