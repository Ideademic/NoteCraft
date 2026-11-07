//! End-to-end round trips through the real storage paths: a notebook built with commands (text,
//! lists, tags, pen ink with pressure, a highlighter, a picture, a PDF printout) saved with
//! `FolderStorage` and opened again with `io::open_path`; the same through `.onepkg` export and
//! import; and every corpus file (`scripts/fetch-corpus.sh`) read without panicking.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::{ElementKind, InkTool, ListKind, Notebook, Section, Tag};
use serde_json::{Value, json};

use super::commands::{png_b64, tiny_pdf};
use crate::io::safe_save::tests::temp_dir;
use crate::io::{FolderStorage, Origin};
use crate::{Session, ids};

fn gid(v: &Value) -> String {
    v.get("id").and_then(Value::as_str).unwrap().to_owned()
}

/// A notebook with one of everything, built through commands only.
fn built() -> Session {
    let mut s = Session::new_default();
    s.run(ids::SECTION_NEW, &json!({"name": "Round Trip"})).unwrap();
    s.run(ids::PAGE_RENAME, &json!({"title": "Everything"})).unwrap();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "Heading\nbold words\none\ntwo\ntask"})).unwrap().value);
    let para = |p: usize, a: usize, b: usize| json!({"outline": o, "start": {"para": p, "char": a}, "end": {"para": p, "char": b}});
    s.run(ids::FORMAT_STYLE, &json!({"outline": o, "start": {"para": 0, "char": 0}, "end": {"para": 0, "char": 0}, "style": "heading1"})).unwrap();
    s.run(ids::FORMAT_BOLD, &para(1, 0, 4)).unwrap();
    s.run(ids::FORMAT_ITALIC, &para(1, 5, 10)).unwrap();
    s.run(ids::FORMAT_HIGHLIGHT, &para(1, 5, 10)).unwrap();
    s.run(ids::FORMAT_BULLETS, &json!({"outline": o, "start": {"para": 2, "char": 0}, "end": {"para": 3, "char": 0}})).unwrap();
    s.run(ids::FORMAT_TAG_TODO, &para(4, 0, 0)).unwrap();
    s.run(ids::FORMAT_TOGGLE_TODO, &para(4, 0, 0)).unwrap();
    let n = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 300, "y": 108, "text": "first\nsecond"})).unwrap().value);
    s.run(ids::FORMAT_NUMBERING, &json!({"outline": n})).unwrap();
    let pen: Vec<Value> = (0..30).map(|i| json!([50.0 + i as f32 * 4.0, 320.0 + (i as f32 * 0.4).sin() * 6.0, 0.2 + i as f32 / 40.0])).collect();
    s.run(ids::DRAW_STROKE, &json!({"points": pen, "tool": "pen", "color": "#c44df0", "width": 2.5})).unwrap();
    s.run(ids::DRAW_STROKE, &json!({"points": [[36, 140], [200, 139], [380, 138]], "tool": "highlighter"})).unwrap();
    s.run(ids::INSERT_IMAGE, &json!({"bytesBase64": png_b64(48, 32), "name": "pic.png", "x": 400, "y": 300, "alt": "a picture"})).unwrap();
    let pr = gid(&s
        .run(ids::INSERT_PRINTOUT, &json!({"bytesBase64": STANDARD.encode(tiny_pdf()), "name": "doc.pdf", "dpi": 72, "x": 36, "y": 400}))
        .unwrap()
        .value);
    // A moved printout keeps its pages with it through the files too.
    s.run(ids::OUTLINE_MOVE, &json!({"id": pr, "x": 60, "y": 420})).unwrap();
    s
}

/// Positions pass through half-inch floats and ink units: round to 1/10 pt. Where the file came
/// from (`source`) and modification times aren't content.
fn normalize(mut s: Section) -> Section {
    let r = |v: &mut f32| *v = (*v * 10.0).round() / 10.0;
    s.source = Default::default();
    for page in &mut s.pages {
        page.modified = page.created;
        for e in &mut page.elements {
            match &mut e.kind {
                ElementKind::Outline(o) => {
                    r(&mut o.pos.x);
                    r(&mut o.pos.y);
                    r(&mut o.width);
                }
                ElementKind::Image(i) => {
                    r(&mut i.pos.x);
                    r(&mut i.pos.y);
                    r(&mut i.size.w);
                    r(&mut i.size.h);
                }
                ElementKind::Printout(p) => {
                    r(&mut p.pos.x);
                    r(&mut p.pos.y);
                    for i in &mut p.pages {
                        r(&mut i.pos.x);
                        r(&mut i.pos.y);
                        r(&mut i.size.w);
                        r(&mut i.size.h);
                    }
                }
                ElementKind::EmbeddedFile { pos, .. } => {
                    r(&mut pos.x);
                    r(&mut pos.y);
                }
                ElementKind::Opaque { .. } => {}
            }
        }
        for k in &mut page.ink {
            r(&mut k.width);
            for p in &mut k.points {
                r(&mut p.x);
                r(&mut p.y);
                p.pressure = (p.pressure * 100.0).round() / 100.0;
            }
        }
    }
    s
}

fn sections(nb: &Notebook) -> Vec<Section> {
    nb.all_sections().into_iter().cloned().map(normalize).collect()
}

/// What the built notebook must still have after a trip through a file.
fn check_content(nb: &Notebook) {
    let s = nb.all_sections().into_iter().find(|s| s.name == "Round Trip").expect("the section");
    let p = s.pages.iter().find(|p| p.title == "Everything").expect("the page");
    let outlines: Vec<_> = p
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Outline(o) => Some(o),
            _ => None,
        })
        .collect();
    let o = outlines.iter().find(|o| o.text().starts_with("Heading")).expect("the outline");
    assert_eq!(o.paragraphs[0].style, notecraft_doc::ParaStyle::Heading1);
    assert!(o.paragraphs[1].runs.iter().any(|r| r.bold && r.text == "bold"));
    assert!(o.paragraphs[1].runs.iter().any(|r| r.italic && r.highlight.is_some()));
    assert_eq!(o.paragraphs[2].list, Some(ListKind::Bullet));
    assert_eq!(o.paragraphs[4].tag, Some(Tag::ToDo { checked: true }));
    assert!(outlines.iter().any(|o| o.paragraphs.iter().all(|p| p.list == Some(ListKind::Numbered))));
    let pen = p.ink.iter().find(|k| k.tool == InkTool::Pen).expect("pen ink");
    assert_eq!(pen.points.len(), 30);
    let (lo, hi) = pen.points.iter().fold((1.0f32, 0.0f32), |(a, b), q| (a.min(q.pressure), b.max(q.pressure)));
    assert!(hi - lo > 0.5, "pressure survives: {lo}..{hi}");
    assert!(p.ink.iter().any(|k| k.tool == InkTool::Highlighter));
    assert!(p.elements.iter().any(|e| matches!(&e.kind, ElementKind::Image(i) if i.alt == "a picture")));
    let pr = p
        .elements
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Printout(p) => Some(p),
            _ => None,
        })
        .expect("the printout");
    assert_eq!(pr.file_bytes.as_slice(), tiny_pdf().as_slice());
    assert_eq!(pr.pages.len(), 1);
    let first = &pr.pages[0];
    assert!((first.pos.x - pr.pos.x).abs() < 0.1 && (first.pos.y - pr.pos.y).abs() < 0.1, "{:?} vs {:?}", first.pos, pr.pos);
}

#[test]
fn folder_save_and_reopen_keeps_everything() {
    let mut s = built();
    check_content(&s.notebooks[0]);
    let root = temp_dir("roundtrip-folder");
    s.storage = Some(Box::new(FolderStorage::new(root.clone())));
    assert_eq!(s.save_dirty().unwrap(), 1);
    assert!(!s.is_dirty());
    let Some(Origin::Folder(dir)) = s.origins.get(&s.notebooks[0].id).cloned() else { panic!("saved to a folder") };
    assert!(dir.join("Open Notebook.onetoc2").is_file());
    assert!(dir.join("Round Trip.one").is_file());
    let back = crate::io::open_path(&dir).unwrap();
    check_content(&back);
    assert_eq!(back.name, s.notebooks[0].name);
    assert_eq!(sections(&back), sections(&s.notebooks[0]));
    // The `.onetoc2` alone opens the same notebook; a single section file opens on its own.
    assert_eq!(sections(&crate::io::open_path(&dir.join("Open Notebook.onetoc2")).unwrap()), sections(&back));
    let one = crate::io::open_path(&dir.join("Round Trip.one")).unwrap();
    check_content(&one);
    // Save again after an edit: only what changed, and it still reads back.
    let mut s2 = Session::empty();
    s2.add_notebook(back, Origin::Folder(dir.clone()));
    s2.storage = Some(Box::new(FolderStorage::new(root.clone())));
    s2.run(ids::PAGE_RENAME, &json!({"title": "Renamed"})).unwrap();
    assert_eq!(s2.save_dirty().unwrap(), 1);
    let again = crate::io::open_path(&dir).unwrap();
    assert!(again.all_sections().iter().any(|sec| sec.pages.iter().any(|p| p.title == "Renamed")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn onepkg_export_and_import_keep_everything() {
    let mut s = built();
    let out = s.run(ids::FILE_EXPORT_ONEPKG, &json!({})).unwrap();
    let b64 = out.value.get("bytesBase64").and_then(Value::as_str).unwrap().to_owned();
    let bytes = STANDARD.decode(&b64).unwrap();
    assert!(bytes.starts_with(b"MSCF"), "a Cabinet file");
    // Opening the file as a notebook.
    let nb = crate::io::open_bytes("Exported.onepkg", &bytes).unwrap();
    check_content(&nb);
    assert_eq!(sections(&nb), sections(&s.notebooks[0]));
    // Importing it into the session (it's already open, so it gets fresh ids).
    let id = gid(&s.run(ids::FILE_IMPORT_ONEPKG, &json!({"bytesBase64": b64, "name": "Exported.onepkg"})).unwrap().value);
    assert_eq!(s.notebooks.len(), 2);
    let imported = s.notebooks.iter().find(|n| n.id.to_string() == id).unwrap();
    check_content(imported);
    // Written to a path (desktop) and opened from there.
    let root = temp_dir("roundtrip-onepkg");
    let path = root.join("Copy.onepkg");
    s.run(ids::FILE_EXPORT_ONEPKG, &json!({"id": s.notebooks[0].id, "path": path.to_string_lossy()})).unwrap();
    check_content(&crate::io::open_path(&path).unwrap());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn shapes_and_ink_to_shape_round_trip_through_one_files() {
    let mut s = Session::new_default();
    s.run(ids::SECTION_NEW, &json!({"name": "Shapes"})).unwrap();
    s.run(ids::PAGE_RENAME, &json!({"title": "Shapes"})).unwrap();
    for (i, kind) in crate::cmd::shapes::ShapeKind::ALL.iter().enumerate() {
        let (x, y) = (40.0 + (i % 4) as f32 * 130.0, 120.0 + (i / 4) as f32 * 110.0);
        s.run(ids::DRAW_SHAPE, &json!({"kind": kind.name(), "x": x, "y": y, "w": 100, "h": 80, "color": "#1f5fbf", "width": 1.5})).unwrap();
    }
    s.run(ids::DRAW_INK_TO_SHAPE, &json!({"value": true})).unwrap();
    let rough = crate::cmd::ink_shape::tests::hand(&crate::cmd::ink_shape::tests::rect_corners(300.0, 640.0, 160.0, 80.0, 6.0, 0), 0.8, 9);
    let pts: Vec<Value> = rough.iter().enumerate().map(|(i, p)| json!([p[0], p[1], 0.4 + (i % 7) as f32 * 0.05])).collect();
    assert_eq!(s.run(ids::DRAW_STROKE, &json!({"points": pts})).unwrap().value["shape"], json!("rectangle"));
    let section = s.current_section().unwrap().clone();
    let strokes = section.pages[0].ink.len();
    assert!(strokes > 16);
    let bytes = notecraft_one::write_section(&section).unwrap();
    let back = notecraft_one::read_section(&bytes).unwrap();
    assert_eq!(back.pages[0].ink.len(), strokes);
    assert_eq!(normalize(back), normalize(section));
}

/// Every corpus file reads (or fails cleanly) without panicking, and what reads also survives
/// being saved again as a NoteCraft notebook folder and a `.onepkg`, and exported as text.
#[test]
fn corpus_files_read_and_resave_without_panics() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut files = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "one" || x == "onepkg" || x == "onetoc2") {
                files.push(p);
            }
        }
    }
    files.sort();
    let root = temp_dir("roundtrip-corpus");
    let (mut read, mut failed) = (0, 0);
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let nb = match std::panic::catch_unwind(|| crate::io::open_path(f)) {
            Ok(Ok(nb)) => nb,
            Ok(Err(e)) => {
                eprintln!("{name}: {e}");
                failed += 1;
                continue;
            }
            Err(_) => panic!("{name}: reading panicked"),
        };
        read += 1;
        let mut s = Session::empty();
        s.add_notebook(nb.clone(), Origin::Scratch);
        for id in [ids::FILE_EXPORT_TEXT, ids::FILE_EXPORT_MARKDOWN, ids::FILE_EXPORT_HTML] {
            let _ = s.run(id, &json!({"scope": "section"}));
        }
        let pkg = s.run(ids::FILE_EXPORT_ONEPKG, &json!({})).unwrap_or_else(|e| panic!("{name}: .onepkg export: {e}"));
        let bytes = STANDARD.decode(pkg.value.get("bytesBase64").and_then(Value::as_str).unwrap()).unwrap();
        let again = crate::io::open_bytes("again.onepkg", &bytes).unwrap_or_else(|e| panic!("{name}: re-reading the .onepkg: {e}"));
        let count = |n: &Notebook| n.all_sections().iter().map(|s| s.pages.len()).sum::<usize>();
        assert_eq!(count(&again), count(&nb), "{name}: pages");
        // Saved as a folder (a page edit makes it dirty) and opened again.
        s.storage = Some(Box::new(FolderStorage::new(root.clone())));
        s.run(ids::PAGE_NEW, &json!({"title": "added"})).unwrap();
        s.save_dirty().unwrap_or_else(|e| panic!("{name}: saving: {e}"));
        let Some(Origin::Folder(out)) = s.origins.get(&s.notebooks[0].id).cloned() else { panic!("{name}: saved to a folder") };
        let back = crate::io::open_path(&out).unwrap_or_else(|e| panic!("{name}: reopening: {e}"));
        assert_eq!(count(&back), count(&nb) + 1, "{name}: pages after saving");
    }
    eprintln!("corpus: {} files, {read} read, {failed} refused cleanly", files.len());
    let _ = std::fs::remove_dir_all(&root);
}
