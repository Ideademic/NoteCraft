//! Canvas tests for the editing features, driven through egui_kittest: the Format Painter, rich
//! copy and paste (pictures with Ctrl+V, HTML from another app, whole containers), Insert Space,
//! cropping and turning pictures, and how typing is grouped into undo steps.

use std::sync::Arc;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use notecraft_doc::{ElementKind, ImageElement};
use notecraft_engine::{Session, ids};
use serde_json::{Value, json};

use super::*;
use crate::services::Services;
use crate::theme::Tokens;

struct T {
    canvas: CanvasState,
    session: Session,
    tokens: Tokens,
    services: Services,
}

fn blank() -> T {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Editing", "blank": true})).unwrap();
    T { canvas: CanvasState::default(), session, tokens: Tokens::light(), services: Services::default() }
}

fn harness(t: T) -> Harness<'static, T> {
    Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, t: &mut T| {
            t.tokens.apply(ui.ctx());
            show(ui, &mut t.canvas, &mut t.session, &t.tokens, &t.services);
        },
        t,
    )
}

fn at(h: &Harness<'_, T>, x: f32, y: f32) -> Pos2 {
    let rect = h.state().canvas.last_rect.unwrap();
    let sheet = page::sheet_rect(rect);
    let k = h.state().canvas.scale();
    pos2(sheet.min.x + x * k - h.state().canvas.scroll.x, sheet.min.y + y * k - h.state().canvas.scroll.y)
}

fn press(h: &mut Harness<'_, T>, p: Pos2, down: bool) {
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: Modifiers::NONE });
    h.step();
}

fn click(h: &mut Harness<'_, T>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
    press(h, p, true);
    press(h, p, false);
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, T>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.step();
    press(h, from, true);
    for k in 1..=4 {
        let p = from + (to - from) * (k as f32 / 4.0);
        h.event(Event::PointerMoved(p));
        h.step();
    }
    press(h, to, false);
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, T>, s: &str) {
    h.event(Event::Text(s.into()));
    h.run_steps(1);
}

fn outlines(h: &Harness<'_, T>) -> Vec<(notecraft_doc::Guid, notecraft_doc::Outline)> {
    h.state()
        .session
        .current_page()
        .unwrap()
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Outline(o) => Some((e.id, o.clone())),
            _ => None,
        })
        .collect()
}

fn pictures(h: &Harness<'_, T>) -> Vec<(notecraft_doc::Guid, ImageElement)> {
    h.state()
        .session
        .current_page()
        .unwrap()
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Image(i) => Some((e.id, i.clone())),
            _ => None,
        })
        .collect()
}

/// A note container at (x, y) holding `paras` (JSON paragraphs).
fn note(t: &mut T, x: f32, y: f32, paras: Value) -> notecraft_doc::Guid {
    let id = t.session.run(ids::OUTLINE_NEW, &json!({"x": x, "y": y})).unwrap().value["id"].as_str().and_then(notecraft_doc::Guid::parse).unwrap();
    t.session.run(ids::TEXT_SET_PARAGRAPHS, &json!({"id": id, "paragraphs": paras})).unwrap();
    id
}

/// A 40×20 picture's PNG.
fn png() -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..20u8 {
        for x in 0..40u8 {
            rgba.extend_from_slice(&[x * 6, y * 12, 200, 255]);
        }
    }
    crate::app::files::png_from_rgba(40, 20, &rgba).unwrap()
}

#[test]
fn format_painter_copies_formatting_onto_a_clicked_word() {
    let mut t = blank();
    let id = note(
        &mut t,
        60.0,
        150.0,
        json!([{"runs": [{"text": "Loud", "bold": true, "color": "#c00000", "size": 16.0}, {"text": " and quiet words"}]}]),
    );
    let mut h = harness(t);
    h.run_steps(2);
    // The caret in the bold red word, then the painter picks it up.
    let p = at(&h, 70.0, 158.0);
    click(&mut h, p);
    assert!(h.state().canvas.caret.is_some());
    let picked = {
        let s = h.state_mut();
        format_painter::pick(&mut s.canvas, &mut s.session, false)
    };
    assert!(picked, "the painter picks up the caret's formatting");
    // A click on "quiet" paints it.
    let quiet_x = {
        let lay_x = 60.0 + 70.0;
        lay_x
    };
    let p = at(&h, quiet_x, 158.0);
    click(&mut h, p);
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    let runs = &o.paragraphs[0].runs;
    let painted = runs.iter().find(|r| r.text.contains("quiet") || r.text.contains("words") || r.text.contains("and")).unwrap();
    let bold_words: Vec<&str> = runs.iter().filter(|r| r.bold && r.color.is_some()).map(|r| r.text.as_str()).collect();
    assert!(bold_words.len() >= 2, "a second word took the formatting: {runs:?}");
    assert!(painted.bold || bold_words.iter().any(|w| w.trim() != "Loud"), "{runs:?}");
    assert!(h.state().canvas.painter.is_none(), "a one-use painter turns off after painting");
}

#[test]
fn a_sticky_painter_stays_on_until_esc() {
    let mut t = blank();
    note(&mut t, 60.0, 150.0, json!([{"runs": [{"text": "Mark", "italic": true}, {"text": " one two three"}]}]));
    let mut h = harness(t);
    h.run_steps(2);
    let p = at(&h, 66.0, 158.0);
    click(&mut h, p);
    {
        let s = h.state_mut();
        assert!(format_painter::pick(&mut s.canvas, &mut s.session, true));
    }
    let p = at(&h, 150.0, 158.0);
    click(&mut h, p);
    assert!(h.state().canvas.painter.is_some(), "a double-clicked painter keeps painting");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.state().canvas.painter.is_none(), "Esc turns it off");
}

#[test]
fn ctrl_v_with_a_picture_on_the_clipboard_pastes_it() {
    let mut h = harness(blank());
    h.run_steps(2);
    let img = egui::ColorImage::new([8, 6], vec![egui::Color32::from_rgb(10, 120, 200); 48]);
    let p = at(&h, 200.0, 220.0);
    h.event(Event::PointerMoved(p));
    h.event(Event::PasteImage(Arc::new(img)));
    h.run_steps(2);
    let pics = pictures(&h);
    assert_eq!(pics.len(), 1, "the picture is on the page");
    assert!((pics[0].1.pos.x - 200.0).abs() < 2.0 && (pics[0].1.pos.y - 220.0).abs() < 2.0, "at the pointer: {:?}", pics[0].1.pos);
    assert_eq!(h.state().canvas.selection.elements, vec![pics[0].0], "and selected");
    // One undo step takes it away.
    h.state_mut().session.undo();
    assert!(pictures(&h).is_empty());
}

#[test]
fn pasting_html_from_another_app_keeps_basic_formatting() {
    let mut t = blank();
    t.services.clipboard_html = Some(Box::new(|| Some("<p><b>Bold</b> and <i>slanted</i></p><ul><li>one</li><li>two</li></ul>".to_owned())));
    let mut h = harness(t);
    h.run_steps(2);
    let p = at(&h, 120.0, 260.0);
    click(&mut h, p);
    h.event(Event::Paste("Bold and slanted\none\ntwo".into()));
    h.run_steps(2);
    let os = outlines(&h);
    assert_eq!(os.len(), 1, "a new container where the caret waited");
    let o = &os[0].1;
    assert_eq!(o.paragraphs.len(), 3, "{:?}", o.text());
    assert!(o.paragraphs[0].runs.iter().any(|r| r.bold && r.text == "Bold"));
    assert!(o.paragraphs[0].runs.iter().any(|r| r.italic && r.text == "slanted"));
    assert_eq!(o.paragraphs[1].list, Some(notecraft_doc::ListKind::Bullet));
    assert!((o.pos.y - 260.0).abs() < 2.0);
}

#[test]
fn paste_as_plain_text_drops_the_formatting() {
    let mut t = blank();
    t.services.clipboard_html = Some(Box::new(|| Some("<b>Strong</b>".to_owned())));
    t.services.clipboard_text = Some(Box::new(|| Some("Strong".to_owned())));
    let id = note(&mut t, 60.0, 150.0, json!([{"runs": [{"text": "x"}]}]));
    let mut h = harness(t);
    h.run_steps(2);
    let p = at(&h, 80.0, 158.0);
    click(&mut h, p);
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::V);
    h.run_steps(2);
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    assert!(o.text().contains("Strong"), "{}", o.text());
    assert!(o.paragraphs[0].runs.iter().all(|r| !r.bold), "{:?}", o.paragraphs[0].runs);
}

#[test]
fn copying_a_container_and_pasting_makes_a_copy() {
    let mut t = blank();
    let id = note(&mut t, 60.0, 150.0, json!([{"runs": [{"text": "Shopping", "bold": true}]}, {"runs": [{"text": "eggs"}], "list": "bullet"}]));
    let mut h = harness(t);
    h.run_steps(2);
    h.state_mut().canvas.selection.elements = vec![id];
    h.event(Event::Copy);
    h.run_steps(1);
    let internal = clipboard::internal(&h.ctx).expect("the copy is on the internal clipboard");
    assert_eq!(internal.clip.elements.len(), 1);
    let p = at(&h, 400.0, 300.0);
    h.event(Event::PointerMoved(p));
    h.event(Event::Paste(internal.text.clone()));
    h.run_steps(2);
    let os = outlines(&h);
    assert_eq!(os.len(), 2, "pasted next to the pointer");
    let copy = os.iter().find(|(i, _)| *i != id).unwrap();
    assert!(copy.1.paragraphs[0].runs[0].bold && copy.1.paragraphs[1].list.is_some(), "with its formatting");
    assert!((copy.1.pos.x - 400.0).abs() < 2.0, "{:?}", copy.1.pos);
}

#[test]
fn copied_text_pastes_into_another_container_with_its_formatting() {
    let mut t = blank();
    let a = note(&mut t, 60.0, 150.0, json!([{"runs": [{"text": "red", "color": "#ff0000"}, {"text": " text"}]}]));
    let b = note(&mut t, 60.0, 300.0, json!([{"runs": [{"text": "into: "}]}]));
    let mut h = harness(t);
    h.run_steps(2);
    {
        let s = h.state_mut();
        s.canvas.caret = Some(TextCaret { anchor: TextPos::new(0, 0), head: TextPos::new(0, 3), ..TextCaret::at(a, TextPos::new(0, 0)) });
        s.canvas.editing = Some(a);
    }
    h.run_steps(1);
    h.event(Event::Copy);
    h.run_steps(1);
    {
        let s = h.state_mut();
        s.canvas.caret = Some(TextCaret::at(b, TextPos::new(0, 6)));
        s.canvas.editing = Some(b);
    }
    h.run_steps(1);
    h.event(Event::Paste("red".into()));
    h.run_steps(2);
    let o = h.state().session.current_page().unwrap().outline(b).unwrap().clone();
    assert_eq!(o.text(), "into: red");
    assert!(o.paragraphs[0].runs.iter().any(|r| r.text == "red" && r.color.is_some()), "{:?}", o.paragraphs[0].runs);
}

#[test]
fn insert_space_pushes_what_is_below_down() {
    let mut t = blank();
    let above = note(&mut t, 60.0, 120.0, json!([{"runs": [{"text": "above"}]}]));
    let below = note(&mut t, 60.0, 320.0, json!([{"runs": [{"text": "below"}]}]));
    let mut h = harness(t);
    h.run_steps(2);
    h.state_mut().canvas.set_tool(Tool::InsertSpace);
    let (a, b) = (at(&h, 300.0, 250.0), at(&h, 300.0, 310.0));
    drag(&mut h, a, b);
    let page = h.state().session.current_page().unwrap().clone();
    let y = |id| page.element(id).and_then(|e| e.pos()).unwrap().y;
    assert!((y(above) - 120.0).abs() < 0.5, "what is above stays");
    assert!((y(below) - 380.0).abs() < 1.5, "what is below moves down by the drag: {}", y(below));
    // One undo step.
    h.state_mut().session.undo();
    let y2 = h.state().session.current_page().unwrap().element(below).and_then(|e| e.pos()).unwrap().y;
    assert!((y2 - 320.0).abs() < 0.5);
}

#[test]
fn pictures_turn_and_crop_with_handles() {
    let mut t = blank();
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png());
    let id = t.session.run(ids::EDIT_PASTE_PICTURE, &json!({"bytesBase64": b64, "x": 100.0, "y": 150.0})).unwrap().value["id"]
        .as_str()
        .and_then(notecraft_doc::Guid::parse)
        .unwrap();
    let mut h = harness(t);
    h.run_steps(2);
    let before = pictures(&h)[0].1.clone();
    h.state_mut().session.run(ids::PICTURE_ROTATE, &json!({"element": id, "degrees": 90})).unwrap();
    let turned = pictures(&h)[0].1.clone();
    assert_eq!(turned.transform.rotation, 1);
    assert!((turned.size.w - before.size.h).abs() < 0.01 && (turned.size.h - before.size.w).abs() < 0.01, "a quarter turn swaps the sides");
    h.run_steps(2);
    // Crop: drag the right edge's handle in, then Enter keeps it.
    {
        let s = h.state_mut();
        assert!(crop::begin(&mut s.canvas, &mut s.session, id));
    }
    h.run_steps(1);
    let r = turned.pos;
    let (w, hh) = (turned.size.w, turned.size.h);
    let edge = at(&h, r.x + w, r.y + hh / 2.0);
    let to = at(&h, r.x + w * 0.5, r.y + hh / 2.0);
    drag(&mut h, edge, to);
    assert!(h.state().canvas.crop.as_ref().is_some_and(|c| (c.rect.w - w * 0.5).abs() < 1.5), "the crop follows the handle");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert!(h.state().canvas.crop.is_none());
    let cropped = pictures(&h)[0].1.clone();
    assert!(cropped.transform.crop.is_some(), "the crop is kept");
    assert!((cropped.size.w - w * 0.5).abs() < 1.5, "{:?}", cropped.size);
    assert_eq!(cropped.data, before.data, "the file itself is unchanged");
}

#[test]
fn typing_breaks_into_undo_steps_at_pauses_and_punctuation() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 100.0, 200.0);
    click(&mut h, p);
    type_text(&mut h, "Hello");
    type_text(&mut h, " there");
    // A pause of more than a second, then more words.
    h.run_steps(6);
    type_text(&mut h, " friend.");
    type_text(&mut h, " Bye");
    let text = |h: &Harness<'_, T>| outlines(h).first().map(|(_, o)| o.text()).unwrap_or_default();
    assert_eq!(text(&h), "Hello there friend. Bye");
    h.state_mut().session.undo();
    assert_eq!(text(&h), "Hello there friend.", "the sentence after the full stop is its own step");
    h.state_mut().session.undo();
    assert_eq!(text(&h), "Hello there", "the words after the pause are another");
    h.state_mut().session.undo();
    assert!(outlines(&h).is_empty(), "the first burst and the new container go together");
}
