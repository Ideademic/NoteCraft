//! Insert › Insert Space: everything below a line moves down (or up, removing space).

use notecraft_doc::ElementKind;
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::PAGE_INSERT_SPACE;
use crate::{CmdOutput, CmdResult, CommandSpec, Session, p};

/// Most space added or removed at once, points.
pub const MAX_SPACE: f32 = 100_000.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(PAGE_INSERT_SPACE, "Insert Space", insert_space).when(has_page).params(
        r##"{"y": number (the line, page points), "dy": number (down; negative removes space), "page"?: guid} -> {"moved": number} (note containers, pictures, printouts, files and ink whose top is at or below y)"##,
    )]
}

fn insert_space(s: &mut Session, params: &Value) -> CmdResult {
    let y = p::req_f32(params, "y")?;
    let dy = p::req_f32(params, "dy")?.clamp(-MAX_SPACE, MAX_SPACE);
    let page = target_page(s, params)?;
    let el_tops: Vec<f32> = page.elements.iter().filter_map(|e| e.pos().map(|p| p.y)).filter(|t| *t >= y).collect();
    let ink_tops: Vec<f32> = page.ink.iter().filter_map(|st| st.bbox().map(|b| b.y)).filter(|t| *t >= y).collect();
    let moved = el_tops.len() + ink_tops.len();
    // Nothing goes above the top of the page.
    let min_top = el_tops.iter().chain(&ink_tops).copied().fold(f32::INFINITY, f32::min);
    let dy = if min_top.is_finite() { dy.max(-min_top.max(0.0)) } else { dy };
    if moved == 0 || dy.abs() < 1e-3 {
        return Ok(CmdOutput { value: json!({"moved": 0}), changed: false });
    }
    for el in &mut page.elements {
        if matches!(el.kind, ElementKind::Opaque { .. }) {
            continue;
        }
        if let Some(pos) = el.pos().filter(|p| p.y >= y) {
            el.set_pos(pos.offset(0.0, dy));
        }
    }
    for st in &mut page.ink {
        if st.bbox().is_some_and(|b| b.y >= y) {
            for pt in &mut st.points {
                pt.y = notecraft_doc::geom::sane(pt.y + dy);
            }
        }
    }
    page.touch();
    Ok(CmdOutput::value(json!({"moved": moved})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn space_pushes_what_is_below_the_line() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let above = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "above"})).unwrap().value["id"].clone();
        let below = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 300, "text": "below"})).unwrap().value["id"].clone();
        s.run(ids::DRAW_STROKE, &json!({"points": [[50, 400], [80, 420]]})).unwrap();
        let out = s.run(ids::PAGE_INSERT_SPACE, &json!({"y": 200, "dy": 50})).unwrap();
        assert_eq!(out.value["moved"], 2);
        let page = s.current_page().unwrap();
        let y = |id: &Value| page.element(notecraft_doc::Guid::parse(id.as_str().unwrap()).unwrap()).unwrap().pos().unwrap().y;
        assert_eq!((y(&above), y(&below)), (100.0, 350.0));
        assert_eq!(page.ink[0].points[0].y, 450.0);
        // Removing more space than there is stops at the top of the page.
        s.run(ids::PAGE_INSERT_SPACE, &json!({"y": 0, "dy": -10_000})).unwrap();
        let page = s.current_page().unwrap();
        assert!(page.elements.iter().all(|e| e.pos().unwrap().y >= 0.0));
        assert_eq!(page.elements.iter().map(|e| e.pos().unwrap().y).fold(f32::INFINITY, f32::min), 0.0);
        let r = s.run(ids::PAGE_INSERT_SPACE, &json!({"y": 1e6, "dy": 10})).unwrap();
        assert!(!r.changed);
    }
}
