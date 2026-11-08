//! Pictures: rotate by quarter turns, flip, crop, alt text, reset. None of these change the
//! picture's file: they set its [`PictureTransform`] (and its displayed size and place), which
//! the canvas, the PDF and the HTML export honour and `.one` keeps.

use notecraft_doc::{Crop, ElementKind, Guid, ImageElement, Point, Rect, Size};
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::{PICTURE_ALT_TEXT, PICTURE_CROP, PICTURE_FLIP, PICTURE_RESET, PICTURE_ROTATE};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Longest alt text, characters.
pub const MAX_ALT_TEXT: usize = 4096;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(PICTURE_ROTATE, "Rotate Picture", rotate)
            .when(has_page)
            .params(r##"{"element": guid (or "id"), "degrees": 90|-90|180|270 (clockwise), "page"?: guid} -> {"rotation": 0..3 (quarter turns)}"##),
        CommandSpec::new(PICTURE_FLIP, "Flip Picture", flip)
            .when(has_page)
            .params(r##"{"element": guid, "axis": "horizontal"|"vertical" (as it looks on the page), "page"?: guid}"##),
        CommandSpec::new(PICTURE_CROP, "Crop", crop).when(has_page).params(
            r##"{"element": guid, "rect": {"x","y","w","h"} (page points: the part to keep, within the whole picture)} | {"element", "crop": [left, top, right, bottom] (fractions of the original)} | {"element", "reset": true}, "page"?: guid"##,
        ),
        CommandSpec::new(PICTURE_ALT_TEXT, "Alt Text", alt_text)
            .when(has_page)
            .params(r##"{"element": guid, "text": string (at most 4096 characters), "page"?: guid}"##),
        CommandSpec::new(PICTURE_RESET, "Reset Picture", reset)
            .when(has_page)
            .params(r##"{"element": guid, "page"?: guid} (no rotation, flip or crop; the whole picture at its current scale)"##),
    ]
}

/// `params.element`, else `params.id`.
pub(crate) fn element_id(params: &Value) -> Result<Guid, CmdError> {
    match p::guid(params, "element")? {
        Some(id) => Ok(id),
        None => p::guid(params, "id")?.ok_or_else(|| CmdError::Params("`element` (GUID) is required".into())),
    }
}

/// Runs `f` on the named picture; touches the page when it changed.
fn with_picture(s: &mut Session, params: &Value, f: impl FnOnce(&mut ImageElement) -> CmdResult) -> CmdResult {
    let id = element_id(params)?;
    let page = target_page(s, params)?;
    let el = page.element_mut(id).ok_or_else(|| CmdError::Params(format!("no element {id}")))?;
    let ElementKind::Image(img) = &mut el.kind else { return Err(CmdError::Params("that item isn't a picture".into())) };
    img.transform = img.transform.sanitized();
    let before = img.clone();
    let out = f(img)?;
    let changed = *img != before;
    if changed {
        page.touch();
    }
    Ok(CmdOutput { changed, ..out })
}

fn rotate(s: &mut Session, params: &Value) -> CmdResult {
    let deg = params.get("degrees").and_then(Value::as_i64).ok_or_else(|| CmdError::Params("`degrees` (90, -90, 180 or 270) is required".into()))?;
    let turns: u8 = match deg.rem_euclid(360) {
        90 => 1,
        180 => 2,
        270 => 3,
        0 => 0,
        _ => return Err(CmdError::Params("`degrees` must be a multiple of 90".into())),
    };
    with_picture(s, params, |img| {
        img.transform.rotation = (img.transform.rotation + turns) % 4;
        if turns % 2 == 1 {
            // Turn about the centre: width and height swap.
            let c = Rect::from_min_size(img.pos, img.size).center();
            img.size = Size::new(img.size.h, img.size.w).sanitized();
            img.pos = Point::new(c.x - img.size.w / 2.0, c.y - img.size.h / 2.0).sanitized();
        }
        Ok(CmdOutput::value(json!({"rotation": img.transform.rotation})))
    })
}

fn flip(s: &mut Session, params: &Value) -> CmdResult {
    let horizontal = match p::req_str(params, "axis")? {
        "horizontal" | "h" | "x" => true,
        "vertical" | "v" | "y" => false,
        _ => return Err(CmdError::Params("`axis` is \"horizontal\" or \"vertical\"".into())),
    };
    with_picture(s, params, |img| {
        // Flips are in the source image: a quarter-turned picture's horizontal is its vertical.
        let source_h = horizontal != (img.transform.rotation % 2 == 1);
        if source_h {
            img.transform.flip_h = !img.transform.flip_h;
        } else {
            img.transform.flip_v = !img.transform.flip_v;
        }
        Ok(CmdOutput::ok())
    })
}

fn crop(s: &mut Session, params: &Value) -> CmdResult {
    if p::bool(params, "reset") == Some(true) {
        return with_picture(s, params, |img| {
            let full = img.full_rect();
            img.transform.crop = None;
            img.pos = full.min().sanitized();
            img.size = full.size().sanitized();
            Ok(CmdOutput::ok())
        });
    }
    if let Some(arr) = params.get("crop") {
        let a = arr.as_array().filter(|a| a.len() == 4).ok_or_else(|| CmdError::Params("`crop` is [left, top, right, bottom]".into()))?;
        let n = |i: usize| a.get(i).and_then(Value::as_f64).map(|f| f as f32).ok_or_else(|| CmdError::Params("`crop` holds numbers".into()));
        let c = Crop { left: n(0)?, top: n(1)?, right: n(2)?, bottom: n(3)? };
        return with_picture(s, params, |img| {
            let full = img.full_rect();
            img.transform.crop = c.sanitized();
            place_in(img, full);
            Ok(CmdOutput::ok())
        });
    }
    let r = params.get("rect").ok_or_else(|| CmdError::Params("`rect`, `crop` or `reset` is required".into()))?;
    let rect = Rect::new(p::req_f32(r, "x")?, p::req_f32(r, "y")?, p::req_f32(r, "w")?, p::req_f32(r, "h")?);
    with_picture(s, params, |img| {
        let full = img.full_rect();
        if full.w <= 0.0 || full.h <= 0.0 {
            return Err(CmdError::Failed("the picture has no size".into()));
        }
        // The kept part in fractions of the whole (displayed) picture, inside it.
        let x0 = ((rect.x.min(rect.right()) - full.x) / full.w).clamp(0.0, 1.0);
        let x1 = ((rect.x.max(rect.right()) - full.x) / full.w).clamp(0.0, 1.0);
        let y0 = ((rect.y.min(rect.bottom()) - full.y) / full.h).clamp(0.0, 1.0);
        let y1 = ((rect.y.max(rect.bottom()) - full.y) / full.h).clamp(0.0, 1.0);
        img.transform.crop = img.transform.crop_from_display([x0, y0, x1, y1]);
        place_in(img, full);
        Ok(CmdOutput::ok())
    })
}

/// Puts the visible part where it sits inside `full` (the whole picture's displayed rect).
fn place_in(img: &mut ImageElement, full: Rect) {
    let [l, t, r, b] = img.transform.display_crop();
    img.pos = Point::new(full.x + l * full.w, full.y + t * full.h).sanitized();
    img.size = Size::new((r - l) * full.w, (b - t) * full.h).sanitized();
}

fn alt_text(s: &mut Session, params: &Value) -> CmdResult {
    let text: String = p::req_str(params, "text")?.chars().filter(|c| *c == '\n' || !c.is_control()).take(MAX_ALT_TEXT).collect();
    with_picture(s, params, |img| {
        img.alt = text;
        Ok(CmdOutput::ok())
    })
}

fn reset(s: &mut Session, params: &Value) -> CmdResult {
    with_picture(s, params, |img| {
        let full = img.full_rect();
        // Undo the turn: an odd rotation shows the picture with its sides swapped.
        let (w, h) = if img.transform.rotation % 2 == 1 { (full.h, full.w) } else { (full.w, full.h) };
        img.transform = notecraft_doc::PictureTransform::default();
        img.pos = full.min().sanitized();
        img.size = Size::new(w, h).sanitized();
        Ok(CmdOutput::ok())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use notecraft_doc::{ImageFormat, PageElement};

    use super::*;
    use crate::ids;

    fn with_picture_page() -> (Session, Guid) {
        let mut s = Session::new_default();
        let img = ImageElement {
            pos: Point::new(100.0, 100.0),
            size: Size::new(200.0, 100.0),
            data: Arc::new(vec![1, 2, 3]),
            format: ImageFormat::Png,
            alt: String::new(),
            transform: Default::default(),
        };
        let el = PageElement::new(ElementKind::Image(img));
        let id = el.id;
        if let Some(p) = s.current_page_mut() {
            p.elements.push(el);
        }
        (s, id)
    }

    fn pic(s: &Session, id: Guid) -> ImageElement {
        match &s.current_page().unwrap().element(id).unwrap().kind {
            ElementKind::Image(i) => i.clone(),
            _ => panic!("a picture"),
        }
    }

    #[test]
    fn rotate_turns_about_the_centre() {
        let (mut s, id) = with_picture_page();
        let out = s.run(ids::PICTURE_ROTATE, &json!({"element": id, "degrees": 90})).unwrap();
        assert_eq!(out.value["rotation"], 1);
        let i = pic(&s, id);
        assert_eq!((i.pos.x, i.pos.y, i.size.w, i.size.h), (150.0, 50.0, 100.0, 200.0));
        s.run(ids::PICTURE_ROTATE, &json!({"id": id, "degrees": -90})).unwrap();
        let i = pic(&s, id);
        assert_eq!((i.pos.x, i.pos.y, i.size.w, i.size.h, i.transform.rotation), (100.0, 100.0, 200.0, 100.0, 0));
        assert!(s.run(ids::PICTURE_ROTATE, &json!({"element": id, "degrees": 45})).is_err());
        // Flip on a turned picture flips the source's other axis.
        s.run(ids::PICTURE_ROTATE, &json!({"element": id, "degrees": 270})).unwrap();
        s.run(ids::PICTURE_FLIP, &json!({"element": id, "axis": "horizontal"})).unwrap();
        let t = pic(&s, id).transform;
        assert!(t.flip_v && !t.flip_h);
        s.run(ids::PICTURE_RESET, &json!({"element": id})).unwrap();
        let i = pic(&s, id);
        assert!(i.transform.is_identity());
        assert_eq!((i.size.w, i.size.h), (200.0, 100.0));
    }

    #[test]
    fn crop_keeps_the_part_and_reset_brings_it_back() {
        let (mut s, id) = with_picture_page();
        s.run(ids::PICTURE_CROP, &json!({"element": id, "rect": {"x": 150, "y": 100, "w": 100, "h": 50}})).unwrap();
        let i = pic(&s, id);
        assert_eq!((i.pos.x, i.pos.y, i.size.w, i.size.h), (150.0, 100.0, 100.0, 50.0));
        let c = i.transform.crop.unwrap();
        assert_eq!((c.left, c.top, c.right, c.bottom), (0.25, 0.0, 0.75, 0.5));
        assert_eq!(i.full_rect(), Rect::new(100.0, 100.0, 200.0, 100.0));
        // A rect past the picture is clamped to it.
        s.run(ids::PICTURE_CROP, &json!({"element": id, "rect": {"x": 0, "y": 0, "w": 1000, "h": 1000}})).unwrap();
        assert!(pic(&s, id).transform.crop.is_none());
        s.run(ids::PICTURE_CROP, &json!({"element": id, "crop": [0.5, 0.5, 1.0, 1.0]})).unwrap();
        assert_eq!(pic(&s, id).size, Size::new(100.0, 50.0));
        s.run(ids::PICTURE_CROP, &json!({"element": id, "reset": true})).unwrap();
        let i = pic(&s, id);
        assert_eq!((i.pos.x, i.pos.y, i.size.w, i.size.h), (100.0, 100.0, 200.0, 100.0));
        s.run(ids::PICTURE_ALT_TEXT, &json!({"element": id, "text": "A chart\u{0}"})).unwrap();
        assert_eq!(pic(&s, id).alt, "A chart");
        let depth = s.undo_depth();
        s.run(ids::PICTURE_ALT_TEXT, &json!({"element": id, "text": "A chart"})).unwrap();
        assert_eq!(s.undo_depth(), depth, "the same text changes nothing");
        // Not a picture.
        let o = s.run(ids::OUTLINE_NEW, &json!({"x": 1, "y": 1})).unwrap();
        assert!(s.run(ids::PICTURE_ROTATE, &json!({"element": o.value["id"], "degrees": 90})).is_err());
    }
}
