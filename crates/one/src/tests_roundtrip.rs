//! Round trips: model → `.one` / `.onetoc2` / notebook files → model, plus hostile input and the
//! real-file corpus (when `corpus/` has been fetched with `scripts/fetch-corpus.sh`).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use notecraft_doc::page::CONTENT_ORIGIN;
use notecraft_doc::{
    Align, Color, ElementKind, Guid, ImageElement, ImageFormat, InkPoint, InkStroke, InkTool, ListKind, ListStyle, Notebook, Outline, Page,
    PageBackground, PageElement, ParaStyle, Paragraph, Point, Printout, RuleLines, Run, Section, SectionEntry, SectionGroup, Size, Tag,
};

use super::*;

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(secs, 0).unwrap()
}

fn png() -> Arc<Vec<u8>> {
    Arc::new(crate::icon::file_icon_png())
}

fn rich_outline() -> Outline {
    let runs = vec![
        Run::plain("Plain, "),
        Run { text: "bold".into(), bold: true, ..Run::default() },
        Run { text: " italic".into(), italic: true, underline: true, ..Run::default() },
        Run { text: " struck".into(), strike: true, color: Some(Color::rgb(0xC0, 0x10, 0x20)), ..Run::default() },
        Run { text: " marked".into(), highlight: Some(Color::HIGHLIGHT_YELLOW), font: Some("Georgia".into()), size: Some(14.0), ..Run::default() },
        Run { text: " link".into(), link: Some("https://example.com/?q=\"x\"".replace('"', "%22")), underline: true, ..Run::default() },
        Run { text: " half".into(), size: Some(10.25), color: Some(Color::rgba(1, 2, 3, 0x80)), ..Run::default() },
    ];
    let mut paragraphs = vec![
        Paragraph { runs, ..Paragraph::default() },
        Paragraph { style: ParaStyle::Heading1, ..Paragraph::plain("A heading") },
        Paragraph { list: Some(ListKind::Bullet), ..Paragraph::plain("bullet one") },
        Paragraph { list: Some(ListKind::Bullet), indent: 1, ..Paragraph::plain("bullet nested") },
        Paragraph { list: Some(ListKind::Numbered), indent: 2, ..Paragraph::plain("numbered deeper") },
        Paragraph { list: Some(ListKind::Numbered), ..Paragraph::plain("numbered top") },
        Paragraph { tag: Some(Tag::ToDo { checked: false }), ..Paragraph::plain("to do") },
        Paragraph { tag: Some(Tag::ToDo { checked: true }), ..Paragraph::plain("done") },
        Paragraph { tag: Some(Tag::Important), style: ParaStyle::Quote, ..Paragraph::plain("important") },
        Paragraph { tag: Some(Tag::Question), style: ParaStyle::Code, ..Paragraph::plain("question?") },
        Paragraph::plain(""),
        Paragraph::plain("line one\nsoft break, ünïcödé ✓ 𝄞"),
        // Proofing languages: one with an LCID, one without (don't check), US English kept as chosen.
        Paragraph { lang: Some("en-GB".into()), ..Paragraph::plain("colour") },
        Paragraph { lang: Some("zxx".into()), ..Paragraph::plain("qzxv") },
        Paragraph { lang: Some("en-US".into()), ..Paragraph::plain("color") },
        Paragraph {
            runs: vec![
                Run::plain("See "),
                Run {
                    text: "Plans".into(),
                    link: Some("notecraft:page?id=0A1B2C3D-0000-4000-8000-000000000001&para=2&title=Plans%20%26%20ideas".into()),
                    ..Run::default()
                },
            ],
            lang: Some("pl-PL".into()),
            ..Paragraph::default()
        },
    ];
    for s in [ParaStyle::Heading2, ParaStyle::Heading3, ParaStyle::Heading4, ParaStyle::Heading5, ParaStyle::Heading6, ParaStyle::Title] {
        paragraphs.push(Paragraph { style: s, ..Paragraph::plain(s.name()) });
    }
    for a in Align::ALL {
        paragraphs.push(Paragraph { align: a, ..Paragraph::plain(a.name()) });
    }
    for st in ListStyle::BULLETS.into_iter().chain(ListStyle::NUMBERS) {
        // A level's own default style reads back as no style, so each sits where it isn't one.
        let indent = if st == ListStyle::by_level(st.kind(), 1) { 0 } else { 1 };
        paragraphs.push(Paragraph { list: Some(st.kind()), list_style: Some(st), indent, ..Paragraph::plain(st.name()) });
    }
    for t in Tag::ALL.into_iter().skip(1) {
        paragraphs.push(Paragraph { tag: Some(t), align: Align::Center, ..Paragraph::plain(t.label()) });
    }
    Outline { pos: CONTENT_ORIGIN, width: 432.0, paragraphs }
}

fn sample_section() -> Section {
    let mut s = Section::new("Plans & Notes", Color::rgb(0x8A, 0xA8, 0xE4));
    let mut p = Page::new_at("Welcome page", at(1_700_000_000));
    p.modified = at(1_700_000_123);
    p.background = PageBackground { color: Some(Color::rgb(0xFF, 0xF8, 0xE0)), rule: RuleLines::College };
    p.elements.push(PageElement::new(ElementKind::Outline(rich_outline())));
    p.elements.push(PageElement::new(ElementKind::Image(ImageElement {
        pos: Point::new(360.0, 72.0),
        size: Size::new(144.0, 108.0),
        data: png(),
        format: ImageFormat::Png,
        alt: "A small icon".into(),
    })));
    let page_img =
        |y: f32| ImageElement { pos: Point::new(36.0, y), size: Size::new(306.0, 396.0), data: png(), format: ImageFormat::Png, alt: String::new() };
    p.elements.push(PageElement::new(ElementKind::Printout(Printout {
        file_name: "report.pdf".into(),
        file_bytes: Arc::new(b"%PDF-1.7 fake".to_vec()),
        pos: Point::new(36.0, 540.0),
        pages: vec![page_img(540.0), page_img(950.0)],
    })));
    p.elements.push(PageElement::new(ElementKind::EmbeddedFile {
        name: "data.csv".into(),
        bytes: Arc::new(b"a,b\n1,2\n".to_vec()),
        pos: Point::new(504.0, 216.0),
    }));
    let mut pen = InkStroke::new(
        InkTool::Pen,
        (0..20).map(|i| InkPoint::new(50.0 + i as f32 * 3.5, 300.0 + (i as f32).sin() * 4.0, 0.3 + i as f32 / 40.0)).collect(),
    );
    pen.color = Color::NOTECRAFT;
    pen.width = 2.5;
    p.ink.push(pen);
    p.ink.push(InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(40.0, 116.0, 1.0), InkPoint::new(260.0, 116.0, 1.0)]));
    p.ink.push(InkStroke::new(InkTool::Pencil, vec![InkPoint::new(10.0, 10.0, 0.5)]));
    s.pages.push(p);
    for (i, level) in [1u8, 2, 0].into_iter().enumerate() {
        let mut sub = Page::new_at(format!("Page {i}"), at(1_700_100_000 + i as i64));
        sub.level = level;
        sub.elements.push(PageElement::new(ElementKind::Outline(Outline::with_text(Point::new(72.0, 144.0), "eggs\nmilk"))));
        s.pages.push(sub);
    }
    s.pages.push(Page::new_at("", at(1_700_200_000)));
    s
}

/// Positions and sizes pass through half-inch floats: compare them to 1/100 pt.
fn round(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

fn normalize_point(p: &mut Point) {
    p.x = round(p.x);
    p.y = round(p.y);
}

fn normalize(mut s: Section) -> Section {
    for page in &mut s.pages {
        for e in &mut page.elements {
            match &mut e.kind {
                ElementKind::Outline(o) => {
                    normalize_point(&mut o.pos);
                    o.width = round(o.width);
                }
                ElementKind::Image(i) => {
                    normalize_point(&mut i.pos);
                    i.size = Size::new(round(i.size.w), round(i.size.h));
                }
                ElementKind::Printout(p) => {
                    normalize_point(&mut p.pos);
                    for i in &mut p.pages {
                        normalize_point(&mut i.pos);
                        i.size = Size::new(round(i.size.w), round(i.size.h));
                    }
                }
                ElementKind::EmbeddedFile { pos, .. } => normalize_point(pos),
                ElementKind::Opaque { .. } => {}
            }
        }
    }
    s
}

#[test]
fn section_round_trip() {
    let s = sample_section();
    let bytes = write_section(&s).unwrap();
    let back = read_section(&bytes).unwrap();
    assert!(back.source.authored_by_notecraft);
    assert!(!back.source.unsupported_content);
    assert_eq!(normalize(back), normalize(s));
}

#[test]
fn written_files_have_spec_header_values() {
    let s = sample_section();
    let toc_id = Guid::new();
    let bytes = write_section_with(&s, "Plans & Notes.one", toc_id).unwrap();
    let h = notecraft_onestore::Header::parse(&bytes).unwrap();
    assert_eq!(h.expected_length, bytes.len() as u64);
    assert_eq!(h.ancestor, toc_id.0);
    assert_eq!(h.crc_name, notecraft_onestore::crc::name_crc("Plans & Notes.one"));
    assert_eq!(h.file_guid, s.id.0);
    assert_eq!(h.transactions, 1);
}

#[test]
fn empty_section_round_trip() {
    let s = Section::new("Empty", Color::rgb(1, 2, 3));
    assert_eq!(read_section(&write_section(&s).unwrap()).unwrap(), s);
}

#[test]
fn toc_round_trip() {
    let t = NotebookToc {
        id: Guid::new(),
        name: "My Notebook".into(),
        color: Color::rgb(0x9C, 0xD8, 0x7E),
        entries: vec![
            TocEntry::Section { file_name: "Quick Notes.one".into(), color: Some(Color::rgb(0xF6, 0xD3, 0x6B)) },
            TocEntry::Group { folder_name: "Work".into() },
            TocEntry::Section { file_name: "Later.one".into(), color: None },
        ],
        history_disabled: true,
    };
    assert_eq!(read_toc(&write_toc(&t).unwrap()).unwrap(), t);
    assert!(read_section(&write_toc(&t).unwrap()).is_err());
    assert!(read_toc(&write_section(&sample_section()).unwrap()).is_err());
}

fn sample_notebook() -> Notebook {
    let mut nb = Notebook::new("Field Notes");
    nb.color = Color::rgb(0xC4, 0x9B, 0xEA);
    let mut a = sample_section();
    a.name = "Quick Notes".into();
    nb.sections.push(SectionEntry::Section(a));
    let mut g = SectionGroup::new("Work: 2026");
    let mut inner = SectionGroup::new("Deep");
    inner.entries.push(SectionEntry::Section(Section::new("Deepest", Color::rgb(0xF2, 0x8B, 0x8B))));
    g.entries.push(SectionEntry::Section(Section::new("Plans", Color::rgb(0x7F, 0xD3, 0xD8))));
    g.entries.push(SectionEntry::Group(inner));
    nb.sections.push(SectionEntry::Group(g));
    nb.sections.push(SectionEntry::Section(Section::new("Zebra", Color::rgb(0xF7, 0xB2, 0x6E))));
    nb.sections.push(SectionEntry::Section(Section::new("Alpha", Color::rgb(0xEB, 0x94, 0xC8))));
    nb
}

/// What a notebook looks like after a save and load: file names recorded, group folder names
/// made safe.
fn as_loaded(mut nb: Notebook) -> Notebook {
    fn fix(entries: &mut [SectionEntry]) {
        for e in entries {
            match e {
                SectionEntry::Section(s) => {
                    s.source.file_name = Some(section_file_name(s));
                    *s = normalize(s.clone());
                }
                SectionEntry::Group(g) => {
                    g.name = safe_file_stem(&g.name);
                    fix(&mut g.entries);
                }
            }
        }
    }
    fix(&mut nb.sections);
    nb
}

fn normalize_nb(mut nb: Notebook) -> Notebook {
    for s in nb.all_sections_mut() {
        *s = normalize(s.clone());
    }
    nb
}

#[test]
fn notebook_files_round_trip() {
    let nb = sample_notebook();
    let files = notebook_files(&nb).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"Open Notebook.onetoc2"));
    assert!(paths.contains(&"Work_ 2026/Open Notebook.onetoc2"));
    assert!(paths.contains(&"Work_ 2026/Deep/Deepest.one"));
    let (back, skipped) = notebook_from_files(&files, "ignored").unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(normalize_nb(back), as_loaded(nb));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn notebook_dir_round_trip() {
    let dir = std::env::temp_dir().join(format!("notecraft-one-test-{}", Guid::new()));
    let nb = sample_notebook();
    write_notebook_dir(&nb, &dir).unwrap();
    // Things a notebook folder may hold that aren't sections.
    std::fs::create_dir_all(dir.join("OneNote_RecycleBin")).unwrap();
    std::fs::write(dir.join("OneNote_RecycleBin/Old.one"), write_section(&Section::new("Old", Color::WHITE)).unwrap()).unwrap();
    std::fs::write(dir.join("Zebra.one.bak"), b"backup").unwrap();
    std::fs::write(dir.join(".notecraft-save-Zebra.one.tmp"), b"partial").unwrap();
    std::fs::write(dir.join("Broken.one"), b"not a section").unwrap();
    let mut back = read_notebook_dir(&dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    // A section in the recycle bin is read as deleted, not as a section of the notebook.
    let bin = std::mem::take(&mut back.recycle_bin);
    assert_eq!(bin.sections.len(), 1);
    assert_eq!(bin.sections[0].section.name, "Old");
    assert_eq!(normalize_nb(back), as_loaded(nb));
}

#[test]
fn page_versions_round_trip() {
    let mut s = sample_section();
    let page = s.pages.first_mut().unwrap();
    let mut old = page.clone();
    old.title = "Welcome (draft)".into();
    old.elements.truncate(1);
    old.modified = at(1_700_000_050);
    let mut v1 = notecraft_doc::PageVersion::of(&old);
    v1.saved = DateTime::from_timestamp(1_700_000_050, 123_400).unwrap();
    old.title = "Welcome (second draft)".into();
    old.modified = at(1_700_000_060);
    let v2 = notecraft_doc::PageVersion::of(&old);
    page.versions = vec![v1.clone(), v2.clone()];
    let bytes = write_section(&s).unwrap();
    // On disk: the version history context and one context per version.
    let store = notecraft_onestore::read(&bytes).unwrap();
    let space = store.spaces.iter().find(|sp| sp.id.guid == s.pages[0].id.0).unwrap();
    assert_eq!(space.contexts.len(), 3);
    assert!(space.context(crate::history_context()).is_some());
    let back = read_section(&bytes).unwrap();
    let vs = &back.pages[0].versions;
    assert_eq!(vs.len(), 2);
    assert_eq!((vs[0].id, vs[0].saved), (v1.id, v1.saved));
    assert_eq!(vs[0].page.title, "Welcome (draft)");
    assert_eq!(vs[0].page.elements.len(), 1);
    assert_eq!(vs[1].page.title, "Welcome (second draft)");
    assert!(vs[1].page.versions.is_empty());
    // The page itself reads as before.
    let mut plain = back.clone();
    plain.pages[0].versions.clear();
    let mut expected = s.clone();
    expected.pages[0].versions.clear();
    assert_eq!(read_section(&write_section(&plain).unwrap()).unwrap(), plain);
    assert_eq!(plain.pages[0].title, expected.pages[0].title);
    // Rewriting what was read keeps the versions.
    assert_eq!(read_section(&write_section(&back).unwrap()).unwrap().pages[0].versions, back.pages[0].versions);
}

#[test]
fn notebook_files_carry_the_recycle_bin_and_history_setting() {
    let mut nb = sample_notebook();
    nb.history_disabled = true;
    let mut gone = Section::new("Gone", Color::WHITE);
    gone.pages.push(Page::new_at("Lost page", at(1_700_000_000)));
    nb.recycle_bin.sections.push(notecraft_doc::DeletedSection { section: gone, deleted: at(1_750_000_000), group_path: vec![], index: 1 });
    nb.recycle_bin.pages.push(notecraft_doc::DeletedPage {
        page: Page::new_at("Deleted page", at(1_700_000_000)),
        deleted: at(1_760_000_000),
        section: Guid::new(),
        section_name: "Quick Notes".into(),
        index: 0,
    });
    let files = notebook_files(&nb).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"OneNote_RecycleBin/Gone.one"), "{paths:?}");
    assert!(paths.contains(&"OneNote_RecycleBin/OneNote_DeletedPages.one"));
    let (back, _) = notebook_from_files(&files, "x").unwrap();
    assert!(back.history_disabled);
    assert_eq!(back.recycle_bin.sections.len(), 1);
    assert_eq!(back.recycle_bin.sections[0].section.pages[0].title, "Lost page");
    assert_eq!(back.recycle_bin.pages[0].page.title, "Deleted page");
    assert_eq!(back.all_sections().len(), nb.all_sections().len(), "bin sections stay out of the notebook");
}

#[test]
fn hostile_sections_never_panic() {
    let good = [
        write_section(&sample_section()).unwrap(),
        write_toc(&NotebookToc { id: Guid::new(), name: "x".into(), color: Color::WHITE, entries: vec![], history_disabled: false }).unwrap(),
    ];
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for base in &good {
        for _ in 0..1500 {
            let mut b = base.clone();
            for _ in 0..1 + next() % 6 {
                let i = (next() as usize) % b.len();
                b[i] ^= 1 << (next() % 8);
            }
            let _ = read_section(&b);
            let _ = read_toc(&b);
        }
        for len in (0..base.len()).step_by(97) {
            let _ = read_section(&base[..len]);
        }
    }
}

/// The corpus fetched by `scripts/fetch-corpus.sh` (skipped when absent).
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
            } else if p.extension().is_some_and(|x| x == "one" || x == "onetoc2") {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn corpus_files_read() {
    let files = corpus();
    if files.is_empty() {
        eprintln!("corpus/ not fetched (scripts/fetch-corpus.sh): skipped");
        return;
    }
    let mut read = 0;
    for (name, bytes) in &files {
        let fuzzed = name.contains("fuzz");
        match read_section(bytes) {
            Ok(s) => {
                read += 1;
                let text: String = s.pages.iter().map(|p| p.plain_text()).collect::<Vec<_>>().join("\n");
                eprintln!(
                    "{name}: {} page(s), titles {:?}, {} chars, unsupported {}",
                    s.pages.len(),
                    s.pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(),
                    text.len(),
                    s.source.unsupported_content
                );
                assert!(!s.source.authored_by_notecraft, "{name}");
                if !fuzzed {
                    assert!(!s.pages.is_empty(), "{name}: no pages");
                    assert!(s.pages.iter().any(|p| !p.title.trim().is_empty() || p.plain_text().trim().len() > 3), "{name}: no text");
                }
                // NoteCraft can save what it read, and read that back.
                let again = read_section(&write_section(&s).unwrap()).unwrap();
                assert_eq!(again.pages.len(), s.pages.len(), "{name}");
                for (a, b) in again.pages.iter().zip(&s.pages) {
                    assert_eq!(a.plain_text(), b.plain_text(), "{name}");
                    assert_eq!(a.ink.len(), b.ink.len(), "{name}");
                }
            }
            Err(Error::Store(notecraft_onestore::Error::Unsupported(m))) => eprintln!("{name}: unsupported ({m})"),
            Err(e) => {
                eprintln!("{name}: error {e}");
                assert!(fuzzed, "{name}: {e}");
            }
        }
    }
    assert!(read >= 8, "only {read} corpus files read");
}

#[test]
fn corpus_known_content() {
    let files = corpus();
    let Some((_, bytes)) = files.iter().find(|(n, _)| n == "testOneNote2016.one") else { return };
    let s = read_section(bytes).unwrap();
    let text: String = s.pages.iter().map(|p| p.plain_text()).collect::<Vec<_>>().join("\n");
    assert!(text.contains("This is one note 2016"), "{text}");
    if let Some((_, bytes)) = files.iter().find(|(n, _)| n == "testOneNote-fuzz3.one") {
        let s = read_section(bytes).unwrap();
        assert!(s.pages.iter().map(|p| p.ink.len()).sum::<usize>() > 10, "ink strokes from the sample");
    }
}
