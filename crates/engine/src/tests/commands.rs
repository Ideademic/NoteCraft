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

fn png_b64(w: u32, h: u32) -> String {
    STANDARD.encode(notecraft_formats::encode_png(w, h, vec![120; (w * h * 4) as usize]).unwrap())
}

fn tiny_pdf() -> Vec<u8> {
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
    let run = &outline(&s, o).paragraphs[1].runs[0];
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
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.runs.iter().all(|r| r.bold)));
    // All bold: removes.
    s.run(ids::FORMAT_BOLD, &all).unwrap();
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.runs.iter().all(|r| !r.bold)));
    // A caret in a word formats the word.
    s.run(ids::FORMAT_ITALIC, &sel(o, (0, 7), (0, 7))).unwrap();
    let runs = &outline(&s, o).paragraphs[0].runs;
    assert_eq!(runs.iter().map(|r| (r.text.as_str(), r.italic)).collect::<Vec<_>>(), vec![("alpha ", false), ("beta", true)]);
    // A caret on a space: nothing to format, the canvas keeps it for typing.
    let out = s.run(ids::FORMAT_UNDERLINE, &sel(o, (0, 5), (0, 5))).unwrap();
    assert!(!out.changed && out.value["collapsed"] == json!(true));
    s.run(ids::FORMAT_STRIKE, &with(all.clone(), json!({"value": true}))).unwrap();
    s.run(ids::FORMAT_STRIKE, &with(all.clone(), json!({"value": true}))).unwrap();
    assert!(outline(&s, o).paragraphs[1].runs[0].strike, "value sets instead of toggling");
    s.run(ids::FORMAT_HIGHLIGHT, &all).unwrap();
    assert_eq!(outline(&s, o).paragraphs[1].runs[0].highlight, Some(Color::HIGHLIGHT_YELLOW));
    s.run(ids::FORMAT_HIGHLIGHT, &all).unwrap();
    assert_eq!(outline(&s, o).paragraphs[1].runs[0].highlight, None, "the highlight button toggles");
    s.run(ids::FORMAT_HIGHLIGHT, &with(all.clone(), json!({"color": "#00ff00"}))).unwrap();
    s.run(ids::FORMAT_COLOR, &with(sel(o, (1, 0), (1, 2)), json!({"color": "#123456"}))).unwrap();
    s.run(ids::FORMAT_SIZE, &with(all.clone(), json!({"size": 1000}))).unwrap();
    s.run(ids::FORMAT_FONT, &with(all.clone(), json!({"font": "Calibri"}))).unwrap();
    let p1 = &outline(&s, o).paragraphs[1];
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
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.runs.len() == 1 && !p.runs[0].strike && p.runs[0].size.is_none()));
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
    assert_eq!(outline(&s, o).paragraphs.iter().map(|p| p.list).collect::<Vec<_>>(), vec![Some(ListKind::Bullet), Some(ListKind::Bullet), None]);
    s.run(ids::FORMAT_BULLETS, &first_two).unwrap();
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.list.is_none()), "toggles off");
    s.run(ids::FORMAT_NUMBERING, &sel(o, (1, 1), (2, 0))).unwrap();
    assert_eq!(outline(&s, o).paragraphs[2].list, Some(ListKind::Numbered));
    s.run(ids::FORMAT_INDENT, &first_two).unwrap();
    s.run(ids::FORMAT_INDENT, &first_two).unwrap();
    s.run(ids::FORMAT_OUTDENT, &json!({"outline": o.to_string(), "para": 0})).unwrap();
    assert_eq!(outline(&s, o).paragraphs.iter().map(|p| p.indent).collect::<Vec<_>>(), vec![1, 2, 0]);
    s.run(ids::FORMAT_STYLE, &json!({"outline": o.to_string(), "para": 2, "style": "heading2"})).unwrap();
    assert_eq!(outline(&s, o).paragraphs[2].style, ParaStyle::Heading2);
    assert!(s.run(ids::FORMAT_STYLE, &json!({"outline": o.to_string(), "style": "fancy"})).is_err());
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string(), "para": 1})).unwrap();
    s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o.to_string(), "para": 1})).unwrap();
    assert_eq!(outline(&s, o).paragraphs[1].tag, Some(Tag::ToDo { checked: true }));
    // Tagging all three keeps the checked one checked; tagging again removes all.
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string()})).unwrap();
    let tags: Vec<_> = outline(&s, o).paragraphs.iter().map(|p| p.tag).collect();
    assert_eq!(tags, vec![Some(Tag::ToDo { checked: false }), Some(Tag::ToDo { checked: true }), Some(Tag::ToDo { checked: false })]);
    s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o.to_string(), "checked": true})).unwrap();
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.tag == Some(Tag::ToDo { checked: true })));
    s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o.to_string()})).unwrap();
    assert!(outline(&s, o).paragraphs.iter().all(|p| p.tag.is_none()));
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
    // Ink to Shape: a wobbly rectangle becomes five clean corners; a straight line stays two points.
    let rect = crate::cmd::ink_shape::tests::poly(&[(100.0, 100.0), (300.0, 100.0), (300.0, 200.0), (100.0, 200.0)], 30);
    let pts: Vec<Value> = rect.iter().map(|p| json!([p.x, p.y, p.pressure])).collect();
    let r = stroke(&mut s, Value::Array(pts));
    let out = s.run(ids::DRAW_INK_TO_SHAPE, &json!({"ids": [r.to_string()]})).unwrap();
    assert_eq!(out.value, json!({"converted": 1, "kept": 0}));
    assert_eq!(s.current_page().unwrap().stroke(r).unwrap().points.len(), 5);
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
