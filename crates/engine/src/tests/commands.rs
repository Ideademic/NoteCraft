//! Every phase-2 command with valid parameters, undo/redo round trips, hostile parameters aimed
//! at real ids.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::{Color, ElementKind, Guid, InkTool, ListKind, ParaStyle, Tag};
use serde_json::{Value, json};

use crate::text::TextPos;
use crate::{CmdError, Session, ids, registry};

fn gid(v: &Value, k: &str) -> Guid {
    Guid::parse(v[k].as_str().unwrap()).unwrap()
}

/// A session on an empty page with one outline holding `text`.
fn session_with(text: &str) -> (Session, Guid) {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({"title": "Test"})).unwrap();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": text})).unwrap().value, "id");
    (s, o)
}

fn outline(s: &Session, o: Guid) -> &notecraft_doc::Outline {
    s.current_page().unwrap().outline(o).unwrap()
}

fn sel(o: Guid, a: (usize, usize), b: (usize, usize)) -> Value {
    json!({"outline": o.to_string(), "start": {"para": a.0, "char": a.1}, "end": {"para": b.0, "char": b.1}})
}

fn with(mut v: Value, extra: Value) -> Value {
    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            m.insert(k.clone(), x.clone());
        }
    }
    v
}

pub(super) fn png_b64(w: u32, h: u32) -> String {
    STANDARD.encode(notecraft_formats::encode_png(w, h, vec![120; (w * h * 4) as usize]).unwrap())
}

pub(super) fn tiny_pdf() -> Vec<u8> {
    let content = "1 0 0 rg 0 0 50 50 re f";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 50] /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let start = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", offsets.len() + 1).as_bytes());
    bytes
}

#[test]
fn typing_commands() {
    let (mut s, o) = session_with("hello");
    let id = o.to_string();
    let depth = s.undo_depth();
    for (i, ch) in [" ", "w", "o"].iter().enumerate() {
        let out = s.run(ids::TEXT_INSERT, &json!({"outline": id, "at": {"para": 0, "char": 5 + i}, "text": ch, "mergeKey": id})).unwrap();
        assert_eq!(out.value["caret"], json!({"para": 0, "char": 6 + i}));
    }
    assert_eq!(s.undo_depth(), depth + 1, "typing merges");
    assert_eq!(outline(&s, o).text(), "hello wo");
    // Enter, then type with a style.
    let out = s.run(ids::TEXT_SPLIT_PARAGRAPH, &json!({"outline": id, "at": [0, 5]})).unwrap();
    assert_eq!(out.value["caret"], json!({"para": 1, "char": 0}));
    assert_eq!(outline(&s, o).text(), "hello\n wo");
    s.run(ids::TEXT_INSERT, &json!({"outline": id, "at": [1, 0], "text": "B", "style": {"bold": true, "color": "#ff0000"}})).unwrap();
    let run = &outline(&s, o).para(1).unwrap().runs[0];
    assert!(run.bold && run.color == Color::parse_hex("#ff0000"));
    // Backspace at the start of a paragraph joins it to the one above.
    let out = s.run(ids::TEXT_DELETE, &json!({"outline": id, "at": [1, 0]})).unwrap();
    assert_eq!(out.value["caret"], json!({"para": 0, "char": 5}));
    assert_eq!(outline(&s, o).text(), "helloB wo");
    s.run(ids::TEXT_DELETE, &json!({"outline": id, "at": [0, 9], "unit": "word"})).unwrap();
    assert_eq!(outline(&s, o).text(), "helloB ");
    s.run(ids::TEXT_DELETE, &json!({"outline": id, "at": [0, 0], "backward": false, "count": 2})).unwrap();
    assert_eq!(outline(&s, o).text(), "lloB ");
    s.run(ids::TEXT_DELETE, &sel(o, (0, 0), (0, 3))).unwrap();
    assert_eq!(outline(&s, o).text(), "B ");
    // Typing over a selection replaces it; a multi-line paste makes paragraphs.
    s.run(ids::TEXT_INSERT, &with(sel(o, (0, 0), (0, 1)), json!({"text": "one\ntwo"}))).unwrap();
    assert_eq!(outline(&s, o).text(), "one\ntwo ");
    let out = s.run(ids::TEXT_MERGE_PARAGRAPHS, &json!({"outline": id, "para": 0})).unwrap();
    assert_eq!(out.value["caret"], json!({"para": 0, "char": 3}));
    assert_eq!(outline(&s, o).text(), "onetwo ");
    assert!(!s.run(ids::TEXT_MERGE_PARAGRAPHS, &json!({"outline": id, "para": 0})).unwrap().changed);
    // An empty insert at a caret changes nothing.
    assert!(!s.run(ids::TEXT_INSERT, &json!({"outline": id, "text": ""})).unwrap().changed);
}

#[test]
fn character_formatting_toggles_like_word() {
    let (mut s, o) = session_with("alpha beta\ngamma");
    let all = sel(o, (0, 0), (1, 5));
    s.run(ids::FORMAT_BOLD, &sel(o, (0, 0), (0, 5))).unwrap();
    // Part bold: applies to all.
    s.run(ids::FORMAT_BOLD, &all).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.runs.iter().all(|r| r.bold)));
    // All bold: removes.
    s.run(ids::FORMAT_BOLD, &all).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.runs.iter().all(|r| !r.bold)));
    // A caret in a word formats the word.
    s.run(ids::FORMAT_ITALIC, &sel(o, (0, 7), (0, 7))).unwrap();
    let runs = &outline(&s, o).para(0).unwrap().runs;
    assert_eq!(runs.iter().map(|r| (r.text.as_str(), r.italic)).collect::<Vec<_>>(), vec![("alpha ", false), ("beta", true)]);
    // A caret on a space: nothing to format, the canvas keeps it for typing.
    let out = s.run(ids::FORMAT_UNDERLINE, &sel(o, (0, 5), (0, 5))).unwrap();
    assert!(!out.changed && out.value["collapsed"] == json!(true));
    s.run(ids::FORMAT_STRIKE, &with(all.clone(), json!({"value": true}))).unwrap();
    s.run(ids::FORMAT_STRIKE, &with(all.clone(), json!({"value": true}))).unwrap();
    assert!(outline(&s, o).para(1).unwrap().runs[0].strike, "value sets instead of toggling");
    s.run(ids::FORMAT_HIGHLIGHT, &all).unwrap();
    assert_eq!(outline(&s, o).para(1).unwrap().runs[0].highlight, Some(Color::HIGHLIGHT_YELLOW));
    s.run(ids::FORMAT_HIGHLIGHT, &all).unwrap();
    assert_eq!(outline(&s, o).para(1).unwrap().runs[0].highlight, None, "the highlight button toggles");
    s.run(ids::FORMAT_HIGHLIGHT, &with(all.clone(), json!({"color": "#00ff00"}))).unwrap();
    s.run(ids::FORMAT_COLOR, &with(sel(o, (1, 0), (1, 2)), json!({"color": "#123456"}))).unwrap();
    s.run(ids::FORMAT_SIZE, &with(all.clone(), json!({"size": 1000}))).unwrap();
    s.run(ids::FORMAT_FONT, &with(all.clone(), json!({"font": "Calibri"}))).unwrap();
    let p1 = &outline(&s, o).para(1).unwrap();
    assert_eq!(p1.runs[0].color, Color::parse_hex("#123456"));
    assert_eq!(p1.runs[0].size, Some(crate::cmd::text::MAX_FONT_SIZE));
    assert_eq!(p1.runs[1].font.as_deref(), Some("Calibri"));
    assert_eq!(p1.runs[1].highlight, Color::parse_hex("#00ff00"));
    // The ribbon's state for a mixed selection.
    let f = s.run(ids::TEXT_FORMAT_AT, &sel(o, (1, 0), (1, 5))).unwrap().value;
    assert_eq!(f["color"], json!("mixed"));
    assert_eq!(f["strike"], json!(true));
    assert_eq!(f["font"], json!("Calibri"));
    let f = s.run(ids::TEXT_FORMAT_AT, &sel(o, (0, 8), (0, 8))).unwrap().value;
    assert_eq!(f["italic"], json!(true));
    s.run(ids::FORMAT_CLEAR, &all).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.runs.len() == 1 && !p.runs[0].strike && p.runs[0].size.is_none()));
    // Unchanged when nothing would change.
    let depth = s.undo_depth();
    s.run(ids::FORMAT_COLOR, &with(all, json!({"color": null}))).unwrap();
    assert_eq!(s.undo_depth(), depth);
}

#[test]
fn paragraph_formatting() {
    let (mut s, o) = session_with("one\ntwo\nthree");
    let first_two = json!({"outline": o.to_string(), "paras": [0, 1]});
    s.run(ids::FORMAT_BULLETS, &first_two).unwrap();
    assert_eq!(outline(&s, o).paragraphs().map(|p| p.list).collect::<Vec<_>>(), vec![Some(ListKind::Bullet), Some(ListKind::Bullet), None]);
    s.run(ids::FORMAT_BULLETS, &first_two).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.list.is_none()), "toggles off");
    s.run(ids::FORMAT_NUMBERING, &sel(o, (1, 1), (2, 0))).unwrap();
    assert_eq!(outline(&s, o).para(2).unwrap().list, Some(ListKind::Numbered));
    s.run(ids::FORMAT_INDENT, &first_two).unwrap();
    s.run(ids::FORMAT_INDENT, &first_two).unwrap();
    s.run(ids::FORMAT_OUTDENT, &json!({"outline": o.to_string(), "para": 0})).unwrap();
    assert_eq!(outline(&s, o).paragraphs().map(|p| p.indent).collect::<Vec<_>>(), vec![1, 2, 0]);
    s.run(ids::FORMAT_STYLE, &json!({"outline": o.to_string(), "para": 2, "style": "heading2"})).unwrap();
    assert_eq!(outline(&s, o).para(2).unwrap().style, ParaStyle::Heading2);
    assert!(s.run(ids::FORMAT_STYLE, &json!({"outline": o.to_string(), "style": "fancy"})).is_err());
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string(), "para": 1})).unwrap();
    s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o.to_string(), "para": 1})).unwrap();
    assert_eq!(outline(&s, o).para(1).unwrap().tag, Some(Tag::ToDo { checked: true }));
    // Tagging all three keeps the checked one checked; tagging again removes all.
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string()})).unwrap();
    let tags: Vec<_> = outline(&s, o).paragraphs().map(|p| p.tag).collect();
    assert_eq!(tags, vec![Some(Tag::ToDo { checked: false }), Some(Tag::ToDo { checked: true }), Some(Tag::ToDo { checked: false })]);
    s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o.to_string(), "checked": true})).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.tag == Some(Tag::ToDo { checked: true })));
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string()})).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.tag.is_none()));
}

#[test]
fn list_styles_are_kept_and_toggle_per_style() {
    use notecraft_doc::ListStyle;
    let (mut s, o) = session_with("one\ntwo\nthree");
    let all = json!({"outline": o.to_string()});
    s.run(ids::FORMAT_BULLETS, &with(all.clone(), json!({"style": "square"}))).unwrap();
    let styles: Vec<_> = outline(&s, o).paragraphs().map(|p| p.marker_style()).collect();
    assert_eq!(styles, vec![Some(ListStyle::Square); 3]);
    // Another style switches style instead of toggling the list off.
    s.run(ids::FORMAT_BULLETS, &with(all.clone(), json!({"style": "arrow"}))).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.list == Some(ListKind::Bullet) && p.marker_style() == Some(ListStyle::Arrow)));
    // The same style again toggles it off.
    s.run(ids::FORMAT_BULLETS, &with(all.clone(), json!({"style": "arrow"}))).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.list.is_none() && p.list_style.is_none()));
    // Numbering by name or by sample; a bullet style isn't a numbering style.
    s.run(ids::FORMAT_NUMBERING, &with(all.clone(), json!({"style": "I."}))).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.marker_style() == Some(ListStyle::UpperRoman)));
    assert!(matches!(s.run(ids::FORMAT_NUMBERING, &with(all.clone(), json!({"style": "disc"}))), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::FORMAT_BULLETS, &with(all.clone(), json!({"style": 5}))), Err(CmdError::Params(_))));
    // Enter carries the style to the new paragraph.
    s.run(ids::TEXT_SPLIT_PARAGRAPH, &json!({"outline": o.to_string(), "at": {"para": 2, "char": 5}})).unwrap();
    assert_eq!(outline(&s, o).para(3).unwrap().marker_style(), Some(ListStyle::UpperRoman));
    // The button without a style toggles off whatever numbering style is there.
    s.run(ids::FORMAT_NUMBERING, &all).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.list.is_none()));
}

#[test]
fn alignment_and_tags() {
    use notecraft_doc::Align;
    let (mut s, o) = session_with("one\ntwo\nthree");
    s.run(ids::FORMAT_ALIGN, &json!({"outline": o.to_string(), "paras": [0, 1], "align": "center"})).unwrap();
    let aligns: Vec<_> = outline(&s, o).paragraphs().map(|p| p.align).collect();
    assert_eq!(aligns, vec![Align::Center, Align::Center, Align::Left]);
    s.run(ids::FORMAT_ALIGN, &json!({"outline": o.to_string(), "para": 2, "align": "justify"})).unwrap();
    assert_eq!(outline(&s, o).para(2).unwrap().align, Align::Justify);
    let same = s.run(ids::FORMAT_ALIGN, &json!({"outline": o.to_string(), "para": 2, "align": "justify"})).unwrap();
    assert!(!same.changed, "nothing to change");
    assert!(matches!(s.run(ids::FORMAT_ALIGN, &json!({"outline": o.to_string(), "align": "middle"})), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::FORMAT_ALIGN, &json!({"outline": o.to_string()})), Err(CmdError::Params(_))));
    s.run(ids::TEXT_SPLIT_PARAGRAPH, &json!({"outline": o.to_string(), "at": {"para": 0, "char": 3}})).unwrap();
    assert_eq!(outline(&s, o).para(1).unwrap().align, Align::Center, "Enter keeps the alignment");

    for t in Tag::ALL {
        s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "para": 0, "tag": t.name()})).unwrap();
        assert_eq!(outline(&s, o).para(0).unwrap().tag, Some(t), "{t:?}");
    }
    // Toggling the same tag removes it; a label works as a name.
    s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "para": 0, "tag": "Critical"})).unwrap();
    assert_eq!(outline(&s, o).para(0).unwrap().tag, None);
    s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "tag": "Remember for later"})).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.tag == Some(Tag::RememberForLater)));
    // A checked To Do stays checked when tagged To Do again with `value: true`.
    s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "para": 1, "tag": "toDo"})).unwrap();
    s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o.to_string(), "para": 1})).unwrap();
    s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "para": 1, "tag": "toDo", "value": true})).unwrap();
    assert_eq!(outline(&s, o).para(1).unwrap().tag, Some(Tag::ToDo { checked: true }));
    // null removes every tag.
    s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "tag": null})).unwrap();
    assert!(outline(&s, o).paragraphs().all(|p| p.tag.is_none()));
    assert!(matches!(s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string(), "tag": "sparkly"})), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::FORMAT_TAG, &json!({"outline": o.to_string()})), Err(CmdError::Params(_))));
    // Every Mod+N shortcut names a tag, To Do first.
    assert_eq!(crate::cmd::format::tag_key(Tag::Phone), Some("Mod+9"));
    assert_eq!(crate::cmd::format::tag_key(Tag::Idea), None);
}

#[test]
fn inserting_pictures_printouts_files_and_dates() {
    let mut s = Session::new_default();
    let out = s.run(ids::INSERT_IMAGE, &json!({"bytesBase64": png_b64(40, 20), "name": "a.png", "x": 10, "y": 20, "alt": "logo"})).unwrap();
    let img = gid(&out.value, "id");
    match &s.current_page().unwrap().element(img).unwrap().kind {
        ElementKind::Image(i) => assert_eq!((i.pos.x, i.pos.y, i.size.w, i.alt.as_str()), (10.0, 20.0, 30.0, "logo")),
        k => panic!("{k:?}"),
    }
    assert!(s.run(ids::INSERT_IMAGE, &json!({"bytesBase64": STANDARD.encode(b"not a picture")})).is_err());
    let out = s.run(ids::INSERT_PRINTOUT, &json!({"bytesBase64": STANDARD.encode(tiny_pdf()), "name": "doc.pdf", "dpi": 72})).unwrap();
    assert_eq!(out.value["pages"], json!(1));
    let pr = gid(&out.value, "id");
    match &s.current_page().unwrap().element(pr).unwrap().kind {
        ElementKind::Printout(p) => {
            assert_eq!(p.file_name, "doc.pdf");
            assert_eq!((p.pages[0].size.w, p.pages[0].size.h), (100.0, 50.0));
            // Placed below what was on the page.
            assert!(p.pos.y > 108.0, "{:?}", p.pos);
        }
        k => panic!("{k:?}"),
    }
    let out = s.run(ids::INSERT_PRINTOUT, &json!({"bytesBase64": png_b64(4, 4), "name": "scan.png"})).unwrap();
    assert_eq!(out.value["pages"], json!(1));
    let out = s.run(ids::INSERT_FILE, &json!({"bytesBase64": STANDARD.encode(b"hello"), "name": "dir/notes.txt"})).unwrap();
    match &s.current_page().unwrap().element(gid(&out.value, "id")).unwrap().kind {
        ElementKind::EmbeddedFile { name, bytes, .. } => assert_eq!((name.as_str(), bytes.as_slice()), ("notes.txt", &b"hello"[..])),
        k => panic!("{k:?}"),
    }
    // Date and time: a new note container, or at the caret.
    let out = s.run(ids::INSERT_DATE_TIME, &json!({"what": "date", "format": "%Y", "x": 0, "y": 0})).unwrap();
    let year = out.value["text"].as_str().unwrap().to_owned();
    assert_eq!(year.len(), 4);
    let o = gid(&out.value, "id");
    let out = s.run(ids::INSERT_DATE_TIME, &json!({"outline": o.to_string(), "at": [0, 0], "format": "%Y-"})).unwrap();
    assert_eq!(out.value["caret"], json!({"para": 0, "char": 5}));
    assert!(s.current_page().unwrap().outline(o).unwrap().text().starts_with(&format!("{year}-{year}")));
    assert!(s.run(ids::INSERT_DATE_TIME, &json!({"format": "%Q"})).is_err());
    assert!(s.run(ids::INSERT_DATE_TIME, &json!({"utcOffsetMinutes": 1e9})).is_ok());
}

#[test]
fn moving_a_printout_moves_its_pages_and_undo_takes_them_back() {
    let mut s = Session::new_default();
    let out =
        s.run(ids::INSERT_PRINTOUT, &json!({"bytesBase64": STANDARD.encode(tiny_pdf()), "name": "doc.pdf", "dpi": 72, "x": 40, "y": 300})).unwrap();
    let pr = gid(&out.value, "id");
    let pages = |s: &Session| match &s.current_page().unwrap().element(pr).unwrap().kind {
        ElementKind::Printout(p) => (p.pos, p.pages.iter().map(|pg| pg.pos).collect::<Vec<_>>()),
        k => panic!("{k:?}"),
    };
    let (pos, before) = pages(&s);
    s.run(ids::OUTLINE_MOVE, &json!({"id": pr.to_string(), "x": pos.x + 100.0, "y": pos.y + 50.0})).unwrap();
    let (moved, after) = pages(&s);
    assert_eq!((moved.x, moved.y), (pos.x + 100.0, pos.y + 50.0));
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert_eq!((b.x - a.x, b.y - a.y), (100.0, 50.0), "each page moves with the printout");
    }
    s.run(ids::EDIT_UNDO, &json!({})).unwrap();
    assert_eq!(pages(&s), (pos, before));
}

fn stroke(s: &mut Session, pts: Value) -> Guid {
    gid(&s.run(ids::DRAW_STROKE, &json!({"points": pts})).unwrap().value, "id")
}

#[test]
fn ink_editing() {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({})).unwrap();
    let a = stroke(&mut s, json!([[0, 0], [100, 0]]));
    let b = stroke(&mut s, json!([[0, 50], [100, 50]]));
    let c = stroke(&mut s, json!([[300, 300], [310, 310]]));
    // Lasso around the first two.
    let out = s.run(ids::DRAW_LASSO_SELECT, &json!({"points": [[-10, -10], [120, -10], [120, 60], [-10, 60]]})).unwrap();
    assert!(!out.changed);
    assert_eq!(s.ink_selection, vec![a, b]);
    s.run(ids::DRAW_MOVE_SELECTION, &json!({"dx": 5, "dy": 7, "mergeKey": "drag"})).unwrap();
    s.run(ids::DRAW_MOVE_SELECTION, &json!({"dx": 5, "dy": 3, "mergeKey": "drag"})).unwrap();
    assert_eq!(s.current_page().unwrap().stroke(a).unwrap().points[0].pos(), notecraft_doc::Point::new(10.0, 10.0));
    s.run(ids::DRAW_RECOLOR, &json!({"color": "#ff0000", "width": 3})).unwrap();
    assert_eq!(s.current_page().unwrap().stroke(b).unwrap().color, Color::rgb(255, 0, 0));
    assert_eq!(s.current_page().unwrap().stroke(c).unwrap().color, InkTool::Pen.default_color());
    s.run(ids::DRAW_DELETE_SELECTION, &json!({})).unwrap();
    assert_eq!(s.current_page().unwrap().ink.len(), 1);
    assert!(s.ink_selection.is_empty());
    assert!(matches!(s.run(ids::DRAW_DELETE_SELECTION, &json!({})), Err(CmdError::Disabled(_))));
    assert!(s.undo());
    assert_eq!(s.current_page().unwrap().ink.len(), 3);
    // The eraser: a miss changes nothing, a drag over a stroke erases it.
    assert!(!s.run(ids::DRAW_ERASE_AT, &json!({"x": 1000, "y": 1000})).unwrap().changed);
    let out = s.run(ids::DRAW_ERASE_AT, &json!({"points": [[305, 290], [305, 320]], "radius": 2})).unwrap();
    assert_eq!(out.value["erased"], json!([c]));
    s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [a.to_string()]})).unwrap();
    s.run(ids::DRAW_LASSO_SELECT, &json!({"all": true, "add": true})).unwrap();
    assert_eq!(s.ink_selection.len(), 2);
    s.run(ids::DRAW_LASSO_SELECT, &json!({"points": []})).unwrap();
    assert!(s.ink_selection.is_empty());
    // Ink to Shape on explicit ids: a wobbly rectangle becomes five clean corners.
    let rect = crate::cmd::ink_shape::tests::hand(&crate::cmd::ink_shape::tests::rect_corners(200.0, 150.0, 200.0, 100.0, 4.0, 0), 0.8, 1);
    let pts: Vec<Value> = rect.iter().map(|p| json!([p[0], p[1], 0.7])).collect();
    let r = stroke(&mut s, Value::Array(pts));
    let out = s.run(ids::DRAW_INK_TO_SHAPE, &json!({"ids": [r.to_string()]})).unwrap();
    assert_eq!(out.value, json!({"converted": 1, "kept": 0, "shapes": ["rectangle"]}));
    assert_eq!(s.current_page().unwrap().stroke(r).unwrap().points.len(), 5);
}

/// Points as `draw.stroke` takes them, with pressure.
fn pts_json(pts: &[[f32; 2]], pressure: impl Fn(usize) -> f32) -> Value {
    Value::Array(pts.iter().enumerate().map(|(i, p)| json!([p[0], p[1], pressure(i)])).collect())
}

#[test]
fn ink_to_shape_is_a_mode_for_new_strokes() {
    use crate::cmd::ink_shape::tests::{hand, rect_corners};
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({})).unwrap();
    let rough = hand(&rect_corners(200.0, 200.0, 180.0, 100.0, 7.0, 0), 0.9, 3);
    // Off: strokes stay as drawn.
    assert!(!s.ink_to_shape());
    let raw = stroke(&mut s, pts_json(&rough, |_| 0.5));
    assert_eq!(s.current_page().unwrap().stroke(raw).unwrap().points.len(), rough.len());
    // Turning it on is not an undo step and touches no notebook.
    let depth = s.undo_depth();
    let out = s.run(ids::DRAW_INK_TO_SHAPE, &json!({})).unwrap();
    assert_eq!(out.value, json!({"value": true}));
    assert!(!out.changed && s.ink_to_shape() && s.undo_depth() == depth);
    // On: a rough tilted rectangle with varying pressure becomes a clean one, in one undo step.
    let out = s.run(ids::DRAW_STROKE, &json!({"points": pts_json(&rough, |i| 0.3 + (i % 10) as f32 * 0.05), "color": "#1f5fbf"})).unwrap();
    assert_eq!(out.value["shape"], json!("rectangle"));
    let id = gid(&out.value, "id");
    let st = s.current_page().unwrap().stroke(id).unwrap().clone();
    assert_eq!(st.points.len(), 5);
    assert_eq!(st.points.first().map(|p| p.pos()), st.points.last().map(|p| p.pos()), "closed");
    assert_eq!(st.color, Color::rgb(0x1f, 0x5f, 0xbf));
    assert_eq!(s.undo_depth(), depth + 1);
    assert!(s.undo());
    assert!(s.current_page().unwrap().stroke(id).is_none(), "one undo step takes the whole stroke");
    assert!(s.ink_to_shape(), "undo doesn't change the mode");
    // Highlighters are never converted; handwriting isn't either.
    let hl = gid(&s.run(ids::DRAW_STROKE, &json!({"points": pts_json(&rough, |_| 0.5), "tool": "highlighter"})).unwrap().value, "id");
    assert_eq!(s.current_page().unwrap().stroke(hl).unwrap().points.len(), rough.len());
    let o: Vec<[f32; 2]> =
        (0..=40).map(|i| i as f32 / 40.0 * std::f32::consts::TAU).map(|a| [300.0 + 5.0 * a.cos(), 400.0 + 6.0 * a.sin()]).collect();
    let out = s.run(ids::DRAW_STROKE, &json!({"points": pts_json(&o, |_| 0.5)})).unwrap();
    assert_eq!(out.value["shape"], Value::Null);
    // A line, then an arrowhead at its end: one arrow (the line's stroke), the head stroke gone.
    let n = s.current_page().unwrap().ink.len();
    let line = gid(&s.run(ids::DRAW_STROKE, &json!({"points": [[100, 500], [180, 501], [260, 500]]})).unwrap().value, "id");
    let out = s.run(ids::DRAW_STROKE, &json!({"points": [[240, 490], [260, 500], [240, 510]]})).unwrap();
    assert_eq!(out.value["shape"], json!("arrow"));
    assert_eq!(gid(&out.value, "id"), line);
    assert_eq!(s.current_page().unwrap().ink.len(), n + 1);
    // Off again.
    assert_eq!(s.run(ids::DRAW_INK_TO_SHAPE, &json!({"value": false})).unwrap().value, json!({"value": false}));
    assert!(!s.ink_to_shape());
    // With a lasso selection it converts the selection (and leaves the mode alone).
    s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [raw.to_string(), hl.to_string()]})).unwrap();
    let out = s.run(ids::DRAW_INK_TO_SHAPE, &json!({})).unwrap();
    assert_eq!(out.value["converted"], json!(1));
    assert_eq!(out.value["kept"], json!(1), "the highlighter stays");
    assert!(!s.ink_to_shape());
    assert_eq!(s.current_page().unwrap().stroke(raw).unwrap().points.len(), 5);
    // Nothing recognised: a notice, no change, no undo step.
    let squiggle = stroke(&mut s, json!([[10, 10], [20, 30], [30, 10], [40, 30], [50, 10]]));
    s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [squiggle.to_string()]})).unwrap();
    let (depth, notices) = (s.undo_depth(), s.notices.len());
    let out = s.run(ids::DRAW_INK_TO_SHAPE, &json!({})).unwrap();
    assert!(!out.changed && out.value["converted"] == json!(0));
    assert_eq!(s.undo_depth(), depth);
    assert_eq!(s.notices.len(), notices + 1);
    // The mode can be set without a page.
    let mut empty = Session::empty();
    assert_eq!(empty.run(ids::DRAW_INK_TO_SHAPE, &json!({})).unwrap().value, json!({"value": true}));
}

#[test]
fn shapes_insert_as_selected_ink_in_one_undo_step() {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({})).unwrap();
    let depth = s.undo_depth();
    let out = s.run(ids::DRAW_SHAPE, &json!({"kind": "ellipse", "x": 100, "y": 100, "w": 120, "h": 60, "color": "#c42b1c", "width": 2})).unwrap();
    let ids_out: Vec<Guid> = out.value["ids"].as_array().unwrap().iter().map(|v| Guid::parse(v.as_str().unwrap()).unwrap()).collect();
    assert_eq!(ids_out.len(), 1);
    assert_eq!(s.ink_selection, ids_out, "the new shape is selected");
    let st = s.current_page().unwrap().stroke(ids_out[0]).unwrap().clone();
    assert_eq!((st.color, st.width, st.tool), (Color::rgb(0xc4, 0x2b, 0x1c), 2.0, InkTool::Pen));
    let b = st.bbox().unwrap();
    assert!((b.x - 100.0).abs() < 0.01 && (b.w - 120.0).abs() < 0.01 && (b.h - 60.0).abs() < 0.01, "{b:?}");
    // Shift: a circle; an arrow is two strokes; the 3-D axes six.
    let out = s.run(ids::DRAW_SHAPE, &json!({"kind": "ellipse", "x": 300, "y": 100, "w": 120, "h": 60, "constrain": true})).unwrap();
    let c = gid(&json!({"id": out.value["ids"][0]}), "id");
    let b = s.current_page().unwrap().stroke(c).unwrap().bbox().unwrap();
    assert!((b.w - b.h).abs() < 0.01);
    assert_eq!(
        s.run(ids::DRAW_SHAPE, &json!({"kind": "arrow", "x": 0, "y": 300, "w": 200, "h": 0})).unwrap().value["ids"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        s.run(ids::DRAW_SHAPE, &json!({"kind": "axes3", "x": 0, "y": 400, "w": 200, "h": 150})).unwrap().value["ids"].as_array().unwrap().len(),
        6
    );
    assert_eq!(s.undo_depth(), depth + 4, "one step per shape");
    assert!(s.undo());
    assert_eq!(s.current_page().unwrap().ink.len(), 4, "the axes go in one undo");
    // Bad input.
    assert!(matches!(s.run(ids::DRAW_SHAPE, &json!({"kind": "blob", "x": 0, "y": 0, "w": 10, "h": 10})), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::DRAW_SHAPE, &json!({"kind": "line", "x": 0, "y": 0, "w": 0.1, "h": 0})), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::DRAW_SHAPE, &json!({"kind": "line", "x": 0, "y": 0})), Err(CmdError::Params(_))));
    // Huge boxes are clamped.
    let out = s.run(ids::DRAW_SHAPE, &json!({"kind": "rectangle", "x": 0, "y": 0, "w": 1e20, "h": -1e20})).unwrap();
    let r = gid(&json!({"id": out.value["ids"][0]}), "id");
    let b = s.current_page().unwrap().stroke(r).unwrap().bbox().unwrap();
    assert!(b.w <= crate::cmd::shapes::MAX_SHAPE_SIZE + 0.01 && b.h <= crate::cmd::shapes::MAX_SHAPE_SIZE + 0.01);
}

#[test]
fn exports_and_search() {
    let (mut s, o) = session_with("find the needle here");
    let _ = o;
    let out = s.run(ids::FILE_EXPORT_HTML, &json!({})).unwrap();
    assert!(!out.changed);
    assert_eq!(out.value["fileName"], json!("Test.html"));
    let html = String::from_utf8(STANDARD.decode(out.value["bytesBase64"].as_str().unwrap()).unwrap()).unwrap();
    assert!(html.contains("find the needle here"));
    let out = s.run(ids::FILE_EXPORT_HTML, &json!({"scope": "section", "positioned": false})).unwrap();
    assert!(out.value["fileName"].as_str().unwrap().ends_with(".html"));
    let out = s.run(ids::FILE_EXPORT_MARKDOWN, &json!({"scope": "section"})).unwrap();
    let md = String::from_utf8(STANDARD.decode(out.value["bytesBase64"].as_str().unwrap()).unwrap()).unwrap();
    assert!(md.contains("# Welcome to NoteCraft") && md.contains("---") && md.contains("needle"), "{md}");
    let out = s.run(ids::FILE_EXPORT_TEXT, &json!({})).unwrap();
    assert_eq!(out.value["mime"], json!("text/plain"));
    let dir = crate::io::safe_save::tests::temp_dir("export");
    let path = dir.join("page.md");
    let out = s.run(ids::FILE_EXPORT_MARKDOWN, &json!({"path": path.to_string_lossy()})).unwrap();
    assert!(path.is_file() && out.value["size"].as_u64().unwrap() > 0);
    let _ = std::fs::remove_dir_all(&dir);
    // .onepkg export works once the format crate can write packages; until then it reports why.
    match s.run(ids::FILE_EXPORT_ONEPKG, &json!({})) {
        Ok(out) => assert!(out.value["fileName"].as_str().unwrap().ends_with(".onepkg")),
        Err(e) => assert!(matches!(e, CmdError::Io(_)), "{e:?}"),
    }
    let out = s.run(ids::SEARCH, &json!({"query": "NEEDLE"})).unwrap();
    let hits = out.value["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["pageTitle"], json!("Test"));
    assert_eq!(hits[0]["sectionName"], json!("Quick Notes"));
    assert!(hits[0]["snippet"].as_str().unwrap().contains("needle"));
    assert_eq!(s.run(ids::SEARCH, &json!({"query": "e", "limit": 1})).unwrap().value["truncated"], json!(true));
    assert!(s.run(ids::SEARCH, &json!({"query": "needle", "scope": "section"})).unwrap().value["hits"].as_array().unwrap().len() == 1);
    assert!(s.run(ids::SEARCH, &json!({"query": "x", "scope": "galaxy"})).is_err());
}

#[test]
fn imports_report_format_errors() {
    let mut s = Session::new_default();
    let n = s.notebooks.len();
    assert!(s.run(ids::FILE_IMPORT_ONEPKG, &json!({"bytesBase64": STANDARD.encode(b"PK not a cab"), "name": "x.onepkg"})).is_err());
    assert!(s.run(ids::FILE_IMPORT_ONE, &json!({"bytesBase64": STANDARD.encode(b"junk"), "name": "x.one"})).is_err());
    assert_eq!(s.notebooks.len(), n);
    assert!(!s.is_dirty());
}

#[test]
fn reid_gives_fresh_ids_to_duplicates() {
    let s = Session::new_default();
    let mut copy = s.notebooks[0].clone();
    crate::cmd::file::reid_notebook(&mut copy, &s);
    assert_ne!(copy.id, s.notebooks[0].id);
    assert_ne!(copy.first_section().unwrap().id, s.notebooks[0].first_section().unwrap().id);
    assert_ne!(copy.first_section().unwrap().pages[0].id, s.notebooks[0].first_section().unwrap().pages[0].id);
}

/// Each mutating command, run with valid parameters on a fresh session, undoes to exactly the
/// state before and redoes to exactly the state after.
#[test]
fn undo_redo_round_trips() {
    let cases: Vec<(&str, fn(Guid) -> Value)> = vec![
        (ids::TEXT_INSERT, |o| json!({"outline": o.to_string(), "at": [0, 1], "text": "x\ny"})),
        (ids::TEXT_DELETE, |o| json!({"outline": o.to_string(), "at": [0, 3]})),
        (ids::TEXT_SPLIT_PARAGRAPH, |o| json!({"outline": o.to_string(), "at": [0, 2]})),
        (ids::TEXT_MERGE_PARAGRAPHS, |o| json!({"outline": o.to_string(), "para": 0})),
        (ids::FORMAT_BOLD, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_ITALIC, |o| json!({"outline": o.to_string(), "start": [0, 1], "end": [1, 1]})),
        (ids::FORMAT_UNDERLINE, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_STRIKE, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_HIGHLIGHT, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_COLOR, |o| json!({"outline": o.to_string(), "color": "#c44df0"})),
        (ids::FORMAT_SIZE, |o| json!({"outline": o.to_string(), "size": 20})),
        (ids::FORMAT_FONT, |o| json!({"outline": o.to_string(), "font": "Arial"})),
        (ids::FORMAT_CLEAR, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_STYLE, |o| json!({"outline": o.to_string(), "style": "heading1"})),
        (ids::FORMAT_BULLETS, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_NUMBERING, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_INDENT, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_OUTDENT, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_TAG_TODO, |o| json!({"outline": o.to_string()})),
        (ids::FORMAT_TOGGLE_TODO, |o| json!({"outline": o.to_string()})),
        (ids::INSERT_FILE, |_| json!({"bytesBase64": "aGk=", "name": "a.txt"})),
        (ids::INSERT_DATE_TIME, |_| json!({})),
        (ids::DRAW_ERASE_AT, |_| json!({"x": 240, "y": 116})),
        (ids::DRAW_RECOLOR, |_| json!({"color": "#00ff00", "ids": []})),
        (ids::DRAW_INK_TO_SHAPE, |_| json!({})),
        (ids::DRAW_SHAPE, |_| json!({"kind": "hexagon", "x": 40, "y": 300, "w": 120, "h": 80})),
    ];
    for (id, params) in cases {
        let (mut s, o) = session_with("abc\ndef");
        // Something for the toggles to undo: bold first paragraph, a checked To Do, indent.
        s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string(), "para": 0})).unwrap();
        s.run(ids::FORMAT_INDENT, &json!({"outline": o.to_string()})).unwrap();
        s.run(ids::DRAW_STROKE, &json!({"points": [[40, 116], [260, 116]], "tool": "highlighter"})).unwrap();
        let before = s.snapshot();
        let out = s.run(id, &params(o)).unwrap_or_else(|e| panic!("{id}: {e}"));
        let after = s.snapshot();
        if !out.changed {
            assert_eq!(before.notebooks, after.notebooks, "{id} said unchanged");
            continue;
        }
        assert_ne!(before.notebooks, after.notebooks, "{id} changed nothing");
        assert!(s.undo(), "{id}");
        assert_eq!(s.notebooks, before.notebooks, "{id} undo");
        assert!(s.redo(), "{id}");
        assert_eq!(s.notebooks, after.notebooks, "{id} redo");
    }
}

/// The hostile test again, with real outline, page and stroke ids mixed into the junk, so
/// commands get past their lookups.
#[test]
fn hostile_params_with_real_ids_never_panic() {
    let junk = [
        json!({}),
        json!({"start": {"para": 1e300, "char": -1}, "end": "x", "text": "\u{0}\r\n\n", "style": {"size": f64::MAX, "font": "", "link": "javascript:"}}),
        json!({"at": [u64::MAX, u64::MAX], "backward": false, "count": u64::MAX, "unit": "word", "para": u64::MAX}),
        json!({"paras": [u64::MAX, 0], "value": true, "checked": false, "color": null, "size": null, "font": null, "style": "title"}),
        json!({"points": [[1e30, -1e30], [f64::MIN, 0], [0, 0]], "dx": 1e30, "dy": -1e30, "radius": -5, "add": true, "all": true}),
        json!({"bytesBase64": STANDARD.encode(tiny_pdf().get(..60).unwrap()), "name": "cut.pdf", "dpi": 1e30, "x": 1e30, "y": f64::MIN}),
        json!({"bytesBase64": STANDARD.encode(b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj"), "name": "x.pdf"}),
        json!({"scope": "section", "query": "\u{feff}", "limit": u64::MAX, "format": "%", "what": "time", "utcOffsetMinutes": f64::NAN}),
        json!({"kind": "doubleArrow", "x": 1e30, "y": f64::MIN, "w": 1e30, "h": -1e30, "constrain": true, "value": false}),
    ];
    for spec in registry().all() {
        for j in &junk {
            let (mut s, o) = session_with("héllo wörld\n\nthird");
            let st = gid(&s.run(ids::DRAW_STROKE, &json!({"points": [[0, 0], [5, 5]]})).unwrap().value, "id");
            s.ink_selection = vec![st];
            let page = s.current.page.unwrap();
            let params = with(j.clone(), json!({"outline": o.to_string(), "page": page.to_string(), "ids": [st.to_string()]}));
            let before = s.snapshot();
            let r = s.run(spec.id, &params);
            assert!(!matches!(&r, Err(CmdError::Failed(m)) if m.starts_with("internal error")), "{} panicked with {params}", spec.id);
            if let Err(e) = r {
                // A failed command leaves the notebooks as they were.
                assert_eq!(s.notebooks, before.notebooks, "{} failed with {e} but changed things", spec.id);
            }
            // Text positions stay valid.
            if let Some(ol) = s.current_page().and_then(|p| p.outline(o)) {
                let end = crate::text::end_of(ol);
                assert!(crate::text::clamp_ref(ol, TextPos::new(usize::MAX, usize::MAX)) == end);
            }
        }
    }
}
