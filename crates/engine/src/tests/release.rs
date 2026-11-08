//! 0.2.0 end to end: a notebook with everything 0.2.0 adds (a table with shading and a set column
//! width, alignment, list styles, every tag, a font, a link, an attachment, a turned and cropped
//! picture, page versions, deleted pages and sections in the recycle bin) through a `.one` folder,
//! a `.onepkg` and a PDF; then every reader fed truncated and bit-flipped copies of those files
//! and of the corpus (`scripts/fetch-corpus.sh`), which must give errors, never panics.

use std::panic::{AssertUnwindSafe, catch_unwind};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::DateTime;
use notecraft_doc::{Align, Block, ElementKind, Notebook, Section, Tag};
use serde_json::{Value, json};

use super::commands::{png_b64, tiny_pdf};
use super::roundtrip::normalize;
use crate::io::safe_save::tests::temp_dir;
use crate::io::{FolderStorage, Origin};
use crate::text::CellRef;
use crate::{Session, ids};

fn gid(v: &Value) -> String {
    v.get("id").and_then(Value::as_str).unwrap().to_owned()
}

fn at(secs: i64) -> chrono::DateTime<chrono::Utc> {
    DateTime::from_timestamp(secs, 0).unwrap()
}

const T0: i64 = 1_800_000_000;
const LINES: &str = "Centred heading\nOn the right\nfirst\nsecond\nidea\nSee the site\nIn a real font\nimportant\nquestion";

/// Everything 0.2.0 adds, built through commands only.
fn rich() -> Session {
    let mut s = Session::new_default();
    s.clock = Some(at(T0));
    s.run(ids::SECTION_NEW, &json!({"name": "Release"})).unwrap();
    s.run(ids::PAGE_RENAME, &json!({"title": "Everything 0.2"})).unwrap();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": LINES})).unwrap().value);
    let para = |p: usize, a: usize, b: usize| json!({"outline": o, "start": {"para": p, "char": a}, "end": {"para": p, "char": b}});
    let with = |mut v: Value, extra: Value| {
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    s.run(ids::FORMAT_ALIGN, &with(para(0, 0, 0), json!({"align": "center"}))).unwrap();
    s.run(ids::FORMAT_ALIGN, &with(para(1, 0, 0), json!({"align": "right"}))).unwrap();
    s.run(ids::FORMAT_NUMBERING, &json!({"outline": o, "start": {"para": 2, "char": 0}, "end": {"para": 3, "char": 0}, "style": "upperRoman"}))
        .unwrap();
    s.run(ids::FORMAT_TAG, &with(para(4, 0, 0), json!({"tag": "idea"}))).unwrap();
    s.run(ids::FORMAT_TAG, &with(para(7, 0, 0), json!({"tag": "important"}))).unwrap();
    s.run(ids::FORMAT_TAG, &with(para(8, 0, 0), json!({"tag": "question"}))).unwrap();
    s.run(ids::LINK_SET, &with(para(5, 8, 12), json!({"url": "https://example.org/notes"}))).unwrap();
    s.run(ids::FORMAT_FONT, &with(para(6, 0, 14), json!({"font": "Georgia"}))).unwrap();
    // A 3 × 3 table in its own container, with text, a shaded cell and a set column width.
    let t = gid(&s.run(ids::INSERT_TABLE, &json!({"rows": 3, "cols": 3, "x": 36, "y": 400})).unwrap().value);
    for (r, c, text) in [(0, 0, "Item"), (0, 1, "Qty"), (0, 2, "Note"), (1, 0, "eggs"), (1, 1, "12"), (2, 0, "milk")] {
        s.run(ids::TEXT_INSERT, &json!({"outline": t, "cell": CellRef::new(0, r, c).to_json(), "text": text})).unwrap();
    }
    s.run(ids::TABLE_SHADING, &json!({"outline": t, "cell": CellRef::new(0, 0, 0).to_json(), "color": "#ffe8a0"})).unwrap();
    s.run(ids::TABLE_SET_COLUMN_WIDTH, &json!({"outline": t, "cell": CellRef::new(0, 0, 2).to_json(), "width": 144})).unwrap();
    // A turned, cropped picture; an attachment; pen ink.
    let pic = gid(&s.run(ids::INSERT_IMAGE, &json!({"bytesBase64": png_b64(64, 48), "name": "pic.png", "x": 400, "y": 120})).unwrap().value);
    s.run(ids::PICTURE_ROTATE, &json!({"element": pic, "degrees": 90})).unwrap();
    s.run(ids::PICTURE_CROP, &json!({"element": pic, "crop": [0.1, 0.0, 0.9, 1.0]})).unwrap();
    s.run(ids::INSERT_FILE, &json!({"bytesBase64": STANDARD.encode(b"a,b\n1,2\n"), "name": "data.csv", "x": 400, "y": 300})).unwrap();
    let pen: Vec<Value> = (0..24).map(|i| json!([60.0 + i as f32 * 5.0, 600.0 + (i as f32 * 0.5).sin() * 8.0, 0.3 + i as f32 / 50.0])).collect();
    s.run(ids::DRAW_STROKE, &json!({"points": pen, "tool": "pen", "color": "#1f5fbf", "width": 2})).unwrap();
    // A later edit keeps a version of the page as it was.
    s.clock = Some(at(T0 + 600));
    s.run(ids::PAGE_RENAME, &json!({"title": "Everything in 0.2"})).unwrap();
    // A deleted page and a deleted section in the recycle bin.
    let doomed = gid(&s.run(ids::PAGE_NEW, &json!({"title": "Doomed page"})).unwrap().value);
    s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "goodbye"})).unwrap();
    s.run(ids::PAGE_DELETE, &json!({"id": doomed})).unwrap();
    let gone = gid(&s.run(ids::SECTION_NEW, &json!({"name": "Gone"})).unwrap().value);
    s.run(ids::PAGE_NEW, &json!({"title": "In a deleted section"})).unwrap();
    s.run(ids::SECTION_DELETE, &json!({"id": gone})).unwrap();
    s
}

/// [`normalize`], plus table column widths (half-inch floats in the file).
fn norm(s: Section) -> Section {
    let mut s = normalize(s);
    for page in &mut s.pages {
        for e in &mut page.elements {
            if let ElementKind::Outline(o) = &mut e.kind {
                for b in &mut o.blocks {
                    if let Block::Table { table } = b {
                        for w in &mut table.col_widths {
                            *w = (*w * 10.0).round() / 10.0;
                        }
                    }
                }
            }
        }
    }
    s
}

fn sections(nb: &Notebook) -> Vec<Section> {
    nb.all_sections().into_iter().cloned().map(norm).collect()
}

fn bin(nb: &Notebook) -> (Vec<Section>, Vec<String>) {
    let secs = nb.recycle_bin.sections.iter().map(|d| norm(d.section.clone())).collect();
    let pages = nb.recycle_bin.pages.iter().map(|d| d.page.title.clone()).collect();
    (secs, pages)
}

/// What the notebook must still have after a trip through a file.
fn check(nb: &Notebook) {
    let s = nb.all_sections().into_iter().find(|s| s.name == "Release").expect("the section");
    let p = s.pages.iter().find(|p| p.title == "Everything in 0.2").expect("the page");
    assert!(p.versions.iter().any(|v| v.page.title == "Everything 0.2"), "the version");
    let outlines: Vec<_> = p
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Outline(o) => Some(o),
            _ => None,
        })
        .collect();
    let o = outlines.iter().find(|o| o.text().starts_with("Centred")).expect("the text");
    assert_eq!(o.para(0).unwrap().align, Align::Center);
    assert_eq!(o.para(1).unwrap().align, Align::Right);
    assert_eq!(o.para(2).unwrap().list_style, Some(notecraft_doc::ListStyle::UpperRoman));
    assert_eq!(o.para(4).unwrap().tag, Some(Tag::Idea));
    assert_eq!(o.para(7).unwrap().tag, Some(Tag::Important));
    assert_eq!(o.para(8).unwrap().tag, Some(Tag::Question));
    assert!(o.para(5).unwrap().runs.iter().any(|r| r.text == "site" && r.link.as_deref() == Some("https://example.org/notes")));
    assert!(o.para(6).unwrap().runs.iter().any(|r| r.font.as_deref() == Some("Georgia")));
    let t = outlines.iter().find_map(|o| o.table(0)).expect("the table");
    assert_eq!((t.row_count(), t.col_count()), (3, 3));
    assert_eq!(t.cell(1, 0).unwrap().text(), "eggs");
    assert_eq!(t.cell(0, 0).unwrap().shading, Some(notecraft_doc::Color::rgb(0xFF, 0xE8, 0xA0)));
    assert!((t.col_widths[2] - 144.0).abs() < 0.5, "{:?}", t.col_widths);
    let pic = p
        .elements
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Image(i) => Some(i),
            _ => None,
        })
        .expect("the picture");
    assert_eq!(pic.transform.rotation, 1);
    assert!(pic.transform.crop.is_some());
    assert!(p.elements.iter().any(|e| matches!(&e.kind, ElementKind::EmbeddedFile { name, .. } if name == "data.csv")));
    assert!(!p.ink.is_empty());
    assert_eq!(nb.recycle_bin.pages.iter().map(|d| d.page.title.as_str()).collect::<Vec<_>>(), ["Doomed page"]);
    assert!(nb.recycle_bin.sections.iter().any(|d| d.section.pages.iter().any(|p| p.title == "In a deleted section")));
}

/// The text of every sheet of a PDF, through craft-pdf's interpreter.
fn pdf_text(bytes: &[u8]) -> String {
    use craft_pdf::interpret::font::Glyph;
    use craft_pdf::interpret::hayro_cmap::BfString;
    use craft_pdf::interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask,
        interpret_page,
    };
    struct Ex(String);
    impl Device<'_> for Ex {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {}
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, g: &Glyph<'_>, _: kurbo::Affine, _: kurbo::Affine, _: &Paint<'_>, mode: &GlyphDrawMode) {
            if matches!(mode, GlyphDrawMode::Stroke(_)) {
                return;
            }
            match g.as_unicode() {
                Some(BfString::Char(c)) => self.0.push(c),
                Some(BfString::String(s)) => self.0.push_str(&s),
                None => self.0.push('\u{FFFD}'),
            }
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }
    let pdf = craft_pdf::open_slice(bytes, None).unwrap();
    let cache = InterpreterCache::new();
    let mut out = String::new();
    for page in pdf.pages().iter() {
        let mut ctx = Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &cache, pdf.xref(), InterpreterSettings::default());
        let mut ex = Ex(String::new());
        interpret_page(page, &mut ctx, &mut ex);
        out.push_str(&ex.0);
    }
    out
}

fn decode(v: &Value) -> Vec<u8> {
    STANDARD.decode(v.get("bytesBase64").and_then(Value::as_str).unwrap()).unwrap()
}

#[test]
fn a_rich_notebook_survives_one_folders_packages_and_pdf() {
    let mut s = rich();
    check(&s.notebooks[0]);
    // A folder of `.one` files, reopened.
    let root = temp_dir("release-folder");
    s.storage = Some(Box::new(FolderStorage::new(root.clone())));
    s.save_dirty().unwrap();
    let Some(Origin::Folder(dir)) = s.origins.get(&s.notebooks[0].id).cloned() else { panic!("saved to a folder") };
    let back = crate::io::open_path(&dir).unwrap();
    check(&back);
    assert_eq!(sections(&back), sections(&s.notebooks[0]));
    assert_eq!(bin(&back), bin(&s.notebooks[0]));
    // A `.onepkg`, opened again.
    let pkg = decode(&s.run(ids::FILE_EXPORT_ONEPKG, &json!({})).unwrap().value);
    let from_pkg = crate::io::open_bytes("Release.onepkg", &pkg).unwrap();
    check(&from_pkg);
    assert_eq!(sections(&from_pkg), sections(&s.notebooks[0]));
    assert_eq!(bin(&from_pkg), bin(&s.notebooks[0]));
    // A PDF of the page whose text reads back, table cells included.
    let release = s.notebooks[0].all_sections().into_iter().find(|x| x.name == "Release").unwrap().pages[0].id;
    s.run(ids::NAV_PAGE, &json!({"id": release.to_string()})).unwrap();
    let pdf = decode(&s.run(ids::FILE_EXPORT_PDF, &json!({"fonts": "bundled"})).unwrap().value);
    let text = pdf_text(&pdf);
    for want in ["Everything in 0.2", "Centred heading", "eggs", "milk", "Qty", "important"] {
        assert!(text.contains(want), "{want:?} in the PDF text {text:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// A small xorshift generator: the same mutations on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// `count` truncations of `base` (evenly spread) and `count` copies with a few flipped bits or
/// stomped bytes.
fn mutations(base: &[u8], count: usize, rng: &mut Rng) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(count * 2);
    if base.is_empty() {
        return out;
    }
    let step = (base.len() / count).max(1);
    for len in (0..base.len()).step_by(step) {
        out.push(base[..len].to_vec());
    }
    for _ in 0..count {
        let mut b = base.to_vec();
        for _ in 0..1 + rng.next() % 6 {
            let i = (rng.next() as usize) % b.len();
            if rng.next().is_multiple_of(3) {
                b[i] = rng.next() as u8;
            } else {
                b[i] ^= 1 << (rng.next() % 8);
            }
        }
        out.push(b);
    }
    out
}

/// Runs `f` and fails the test, naming the input, if it panicked.
fn no_panic(what: &str, i: usize, f: impl FnOnce()) {
    assert!(catch_unwind(AssertUnwindSafe(f)).is_ok(), "{what}: mutation {i} panicked");
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut out = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "one" || x == "onetoc2" || x == "onepkg") {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_reader_survives_truncated_and_flipped_files() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut s = rich();
    let nb = s.notebooks[0].clone();
    let files = notecraft_one::notebook_files(&nb).unwrap();
    assert!(files.iter().any(|f| f.path.ends_with(".onetoc2")) && files.iter().any(|f| f.path.contains("RecycleBin")));
    let pkg = notecraft_onepkg::write(&nb).unwrap();
    // `.one` sections (tables, versions, tags, alignment, links, pictures, ink, the bin) and the
    // `.onetoc2`, NoteCraft-written and from the corpus: the section and TOC readers and the
    // notebook opener (by file name).
    let mut inputs: Vec<(String, Vec<u8>)> = files.iter().map(|f| (f.path.clone(), f.bytes.clone())).collect();
    inputs.extend(corpus());
    let mut total = 0;
    for (name, bytes) in &inputs {
        let count = if bytes.len() > 1 << 20 { 12 } else { 60 };
        for (i, m) in mutations(bytes, count, &mut rng).iter().enumerate() {
            no_panic(name, i, || {
                let _ = notecraft_one::read_section(m);
                let _ = notecraft_one::read_toc(m);
                let _ = crate::io::open_bytes(name, m);
            });
            total += 1;
        }
    }
    // A notebook folder's files with one of them mutated.
    for (k, f) in files.iter().enumerate() {
        for (i, m) in mutations(&f.bytes, 25, &mut rng).into_iter().enumerate() {
            let mut set = files.clone();
            set[k].bytes = m;
            no_panic(&format!("folder with {}", f.path), i, || {
                let _ = notecraft_one::notebook_from_files(&set, "x");
            });
            total += 1;
        }
    }
    // A notebook folder on disk with a mutated section, through the directory reader.
    let root = temp_dir("release-fuzz-dir");
    for (i, m) in mutations(&files[0].bytes, 8, &mut rng).into_iter().enumerate() {
        let dir = root.join(format!("nb{i}"));
        for f in &files {
            let p = dir.join(&f.path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, if f.path == files[0].path { &m } else { &f.bytes }).unwrap();
        }
        no_panic("notebook folder", i, || {
            let _ = crate::io::open_path(&dir);
        });
    }
    let _ = std::fs::remove_dir_all(&root);
    // `.onepkg` packages.
    for (i, m) in mutations(&pkg, 150, &mut rng).iter().enumerate() {
        no_panic("onepkg", i, || {
            let _ = notecraft_onepkg::read(m);
            let _ = notecraft_onepkg::read_files(m);
        });
        total += 1;
    }
    // Clipboard HTML import of the exported page (cut at bytes, so often mid-character).
    let page = nb.all_sections().into_iter().find(|x| x.name == "Release").unwrap().pages[0].clone();
    let html = notecraft_formats::page_to_html(&page).unwrap();
    for (i, m) in mutations(html.as_bytes(), 80, &mut rng).iter().enumerate() {
        no_panic("html", i, || {
            let _ = notecraft_formats::clip::from_html_blocks(&String::from_utf8_lossy(m));
        });
    }
    // PDF import (file printouts) of an exported PDF, and of the tiny one.
    s.run(ids::NAV_PAGE, &json!({"id": page.id.to_string()})).unwrap();
    let pdf = decode(&s.run(ids::FILE_EXPORT_PDF, &json!({"fonts": "bundled"})).unwrap().value);
    for base in [pdf, tiny_pdf()] {
        for (i, m) in mutations(&base, 20, &mut rng).iter().enumerate() {
            no_panic("pdf", i, || {
                let _ = notecraft_printout::render_pdf(m, 24.0);
            });
        }
    }
    eprintln!("{total} mutated .one/.onetoc2/.onepkg inputs read without a panic");
}
