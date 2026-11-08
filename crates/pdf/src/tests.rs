use std::sync::Arc;

use notecraft_doc::{
    Color, ElementKind, ImageElement, ImageFormat, InkPoint, InkStroke, InkTool, ListKind, Outline, Page, PageElement, Paragraph, Point, RuleLines,
    Run, Section, Size, Tag,
};

use super::*;

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| image::Rgba([(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 0x80, 0xFF]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn para(text: &str) -> Paragraph {
    Paragraph::plain(text)
}

/// A page with most of what a page can hold.
pub(crate) fn rich_page() -> Page {
    let mut page = Page::new("Project kickoff notes");
    page.background.rule = RuleLines::College;
    let mut o = Outline::with_text(notecraft_doc::page::CONTENT_ORIGIN, "");
    o.width = 380.0;
    let mut h1 = para("Agenda for the week");
    h1.style = notecraft_doc::ParaStyle::Heading1;
    let mut b1 = para("Review the quarterly roadmap and decide which features ship first");
    b1.list = Some(ListKind::Bullet);
    let mut b2 = para("Nested point under it");
    b2.list = Some(ListKind::Bullet);
    b2.indent = 1;
    let mut n1 = para("Numbered step one");
    n1.list = Some(ListKind::Numbered);
    let mut n2 = para("Numbered step two");
    n2.list = Some(ListKind::Numbered);
    let mut todo = para("Book the meeting room");
    todo.tag = Some(Tag::ToDo { checked: true });
    let mut todo2 = para("Send the invitations");
    todo2.tag = Some(Tag::ToDo { checked: false });
    let mut star = para("Budget is due Friday");
    star.tag = Some(Tag::Important);
    let mut q = para("Who owns the launch plan?");
    q.tag = Some(Tag::Question);
    let rich = Paragraph {
        runs: vec![
            Run::plain("Mixed "),
            Run { text: "bold".into(), bold: true, ..Run::default() },
            Run::plain(", "),
            Run { text: "italic".into(), italic: true, ..Run::default() },
            Run::plain(", "),
            Run { text: "underlined".into(), underline: true, ..Run::default() },
            Run::plain(", "),
            Run { text: "struck".into(), strike: true, ..Run::default() },
            Run::plain(", "),
            Run { text: "highlighted".into(), highlight: Some(Color::HIGHLIGHT_YELLOW), ..Run::default() },
            Run::plain(", "),
            Run { text: "red".into(), color: Some(Color::rgb(0xC0, 0x10, 0x10)), ..Run::default() },
            Run::plain(" and a "),
            Run { text: "link".into(), link: Some("https://example.com/notes".into()), ..Run::default() },
            Run::plain("."),
        ],
        ..Paragraph::default()
    };
    let mut code = para("let answer = 42;");
    code.style = notecraft_doc::ParaStyle::Code;
    o.blocks = [h1, b1, b2, n1, n2, todo, todo2, star, q, rich, code].into_iter().map(Into::into).collect();
    page.elements.push(PageElement::new(ElementKind::Outline(o)));
    page.elements.push(PageElement::new(ElementKind::Image(ImageElement {
        pos: Point::new(460.0, 120.0),
        size: Size::new(120.0, 90.0),
        data: Arc::new(png(64, 48)),
        format: ImageFormat::Png,
        alt: "gradient".into(),
    })));
    page.elements.push(PageElement::new(ElementKind::EmbeddedFile {
        name: "budget.xlsx".into(),
        bytes: Arc::new(vec![1, 2, 3]),
        pos: Point::new(460.0, 240.0),
    }));
    let wave: Vec<InkPoint> =
        (0..60).map(|i| InkPoint::new(460.0 + i as f32 * 2.0, 320.0 + (i as f32 / 6.0).sin() * 12.0, 0.2 + (i as f32 / 60.0) * 0.8)).collect();
    let mut pen = InkStroke::new(InkTool::Pen, wave);
    pen.width = 3.0;
    pen.color = Color::rgb(0x1F, 0x5F, 0xBF);
    page.ink.push(pen);
    let hl = InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(40.0, 136.0, 0.5), InkPoint::new(200.0, 136.0, 0.5)]);
    page.ink.push(hl);
    page
}

/// Each sheet's text, through craft-pdf's interpreter (glyph to Unicode via the ToUnicode maps).
pub(crate) fn extract_text(bytes: &[u8]) -> Vec<String> {
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
            // Synthetic bold fills and strokes each glyph: one character, two draws.
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
    pdf.pages()
        .iter()
        .map(|page| {
            let mut ctx =
                Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &cache, pdf.xref(), InterpreterSettings::default());
            let mut ex = Ex(String::new());
            interpret_page(page, &mut ctx, &mut ex);
            ex.0
        })
        .collect()
}

#[test]
fn a_page_becomes_a_pdf_with_its_text_ink_and_pictures() {
    let page = rich_page();
    let pdf = export(&Doc::page(&page), &Options::default(), &BundledFonts).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let doc = craft_pdf::open_slice(&pdf, None).unwrap();
    assert_eq!(doc.pages().len(), 1, "fit to content: one sheet");
    let text = extract_text(&pdf).join("");
    for want in [
        "Project kickoff notes",
        "Agenda for the week",
        "Review the quarterly roadmap",
        "Budget is due Friday",
        "let answer = 42;",
        "a link.",
        "1.",
        "2.",
    ] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    let raw = String::from_utf8_lossy(&pdf).into_owned();
    assert!(raw.contains("/Image"), "a picture");
    assert!(raw.contains("/URI") && raw.contains("example.com/notes"), "a link annotation");
    assert!(raw.contains("/FontFile"), "embedded font subsets");
    // Renders without trouble.
    let t = thumbnails(&pdf, 400, 4);
    assert_eq!(t.len(), 1);
    assert!(t[0].rgba.chunks(4).any(|p| p[0] < 100), "something dark was drawn");
}

#[test]
fn paper_sizes_tile_long_pages() {
    let mut page = Page::new("Long");
    let text: Vec<String> = (0..200).map(|i| format!("Line number {i} of a long page")).collect();
    let o = Outline::with_text(notecraft_doc::page::CONTENT_ORIGIN, &text.join("\n"));
    page.elements.push(PageElement::new(ElementKind::Outline(o)));
    let letter = Options { paper: Paper::Letter, ..Options::default() };
    let pdf = export(&Doc::page(&page), &letter, &BundledFonts).unwrap();
    let n = thumbnails::sheet_count(&pdf);
    // 200 lines of about 15 pt, 108 pt down: some 3100 pt over 706 pt of sheet.
    assert!((4..=6).contains(&n), "{n} sheets");
    assert_eq!(count_sheets(&Doc::page(&page), &letter, &BundledFonts), n);
    let landscape = Options { landscape: true, ..letter.clone() };
    assert!(count_sheets(&Doc::page(&page), &landscape, &BundledFonts) > n, "shorter sheets, more of them");
    // A print range keeps only those sheets.
    let some = Options { sheets: parse_ranges("2-3"), ..letter };
    assert_eq!(thumbnails::sheet_count(&export(&Doc::page(&page), &some, &BundledFonts).unwrap()), 2);
}

#[test]
fn sheet_breaks_fall_between_lines() {
    let rows: Vec<(f32, f32)> = (0..100).map(|i| (i as f32 * 15.0, i as f32 * 15.0 + 14.0)).collect();
    let opts = Options { paper: Paper::Letter, page_numbers: false, ..Options::default() };
    let sheets = plan_sheets(Rect::new(0.0, 0.0, 300.0, 1500.0), &rows, &[Rect::new(0.0, 0.0, 300.0, 1500.0)], &opts);
    assert!(sheets.len() >= 2);
    for s in &sheets[1..] {
        let y = s.area.y;
        assert!(!rows.iter().any(|(a, b)| *a < y - 0.5 && *b > y + 0.5), "break at {y} cuts a row");
    }
    // Wide content is shrunk to the paper's width.
    let wide = plan_sheets(Rect::new(0.0, 0.0, 1080.0, 100.0), &[], &[Rect::new(0.0, 0.0, 1080.0, 100.0)], &opts);
    assert_eq!(wide.len(), 1);
    assert!((wide[0].scale - 0.5).abs() < 0.01);
    // Without fitting, it is tiled across.
    let tiled = plan_sheets(Rect::new(0.0, 0.0, 1080.0, 100.0), &[], &[Rect::new(0.0, 0.0, 1080.0, 100.0)], &Options { fit_width: false, ..opts });
    assert_eq!(tiled.len(), 2);
}

#[test]
fn sections_get_bookmarks_and_hostile_pages_dont_break_it() {
    let mut sec = Section::new("Work", Color::NOTECRAFT);
    sec.pages.push(rich_page());
    let mut sub = Page::new("A subpage");
    sub.level = 1;
    sec.pages.push(sub);
    let mut junk = Page::new("\u{0}\u{1}junk\u{202E}");
    junk.background.color = Some(Color::rgb(0x10, 0x10, 0x20));
    junk.background.rule = RuleLines::GridSmall;
    let mut o = Outline::with_text(Point::new(f32::NAN, -1e30), &"x".repeat(5000));
    o.width = f32::INFINITY;
    o.blocks.push(notecraft_doc::Block::Paragraph(Paragraph {
        runs: vec![Run { text: "big".into(), size: Some(1e9), font: Some("\u{0}".repeat(500)), ..Run::default() }],
        indent: 255,
        ..Paragraph::default()
    }));
    junk.elements.push(PageElement::new(ElementKind::Outline(o)));
    junk.elements.push(PageElement::new(ElementKind::Image(ImageElement {
        pos: Point::new(1e6, 1e6),
        size: Size::new(1e6, 1e6),
        data: Arc::new(b"\x89PNG\r\n\x1a\nnot really".to_vec()),
        format: ImageFormat::Png,
        alt: String::new(),
    })));
    junk.ink.push(InkStroke::new(InkTool::Pen, vec![InkPoint::new(1e6, 1e6, f32::NAN), InkPoint::new(-1e6, 0.0, 2.0)]));
    sec.pages.push(junk);
    for paper in Paper::ALL {
        let pdf = export(&Doc::section(&sec), &Options { paper, ..Options::default() }, &BundledFonts).unwrap();
        assert!(thumbnails::sheet_count(&pdf) >= 3, "{paper:?}");
        let s = String::from_utf8_lossy(&pdf).into_owned();
        assert!(s.contains("/Outlines"), "bookmarks");
    }
    assert!(matches!(export(&Doc { title: String::new(), items: vec![] }, &Options::default(), &BundledFonts), Err(PdfError::NoPages)));
    let none = Options { sheets: Some(vec![(50, 60)]), ..Options::default() };
    assert!(matches!(export(&Doc::page(&rich_page()), &none, &BundledFonts), Err(PdfError::NoPages)));
}

#[test]
fn ranges_parse() {
    assert_eq!(parse_ranges("1-3, 5"), Some(vec![(1, 3), (5, 5)]));
    assert_eq!(parse_ranges("4-"), Some(vec![(4, usize::MAX)]));
    assert_eq!(parse_ranges("0"), None);
    assert_eq!(parse_ranges("3-1"), None);
    assert_eq!(parse_ranges("x"), None);
    assert_eq!(parse_ranges(" "), None);
    assert_eq!(Paper::parse("A4"), Some(Paper::A4));
    assert_eq!(Paper::parse("tabloid"), None);
}

/// Writes a sample PDF of the rich page and PNG renders of it to `NOTECRAFT_PDF_SHOTS`, to look
/// at by eye (`NOTECRAFT_PDF_SHOTS=dir cargo test -p notecraft-pdf samples -- --nocapture`).
#[test]
fn samples() {
    let Some(dir) = std::env::var_os("NOTECRAFT_PDF_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let page = rich_page();
    let mut dark = rich_page();
    dark.background.color = Some(Color::rgb(0x2B, 0x2B, 0x3A));
    dark.background.rule = RuleLines::GridMedium;
    for (name, page, opts, fonts) in [
        ("rich-fit-system", &page, Options::default(), &SystemFonts as &dyn FontSource),
        ("rich-fit-bundled", &page, Options::default(), &BundledFonts as &dyn FontSource),
        ("rich-letter", &page, Options { paper: Paper::Letter, ..Options::default() }, &SystemFonts as &dyn FontSource),
        ("dark-a4", &dark, Options { paper: Paper::A4, ..Options::default() }, &SystemFonts as &dyn FontSource),
    ] {
        let pdf = export(&Doc::page(page), &opts, fonts).unwrap();
        std::fs::write(dir.join(format!("{name}.pdf")), &pdf).unwrap();
        for (i, t) in thumbnails(&pdf, 1400, 3).into_iter().enumerate() {
            let img = image::RgbaImage::from_raw(t.width, t.height, t.rgba).unwrap();
            img.save(dir.join(format!("{name}-{}.png", i + 1))).unwrap();
        }
    }
}

#[test]
fn the_recycle_bin_and_page_versions_stay_out_of_the_pdf() {
    let mut nb = notecraft_doc::Notebook::new("Book");
    let mut sec = Section::new("Live", Color::NOTECRAFT);
    let mut page = Page::new("Keepme");
    let mut old = Page::new("Oldversion");
    old.elements.push(PageElement::new(ElementKind::Outline(Outline::with_text(Point::new(36.0, 108.0), "Stalecontent"))));
    page.versions.push(notecraft_doc::PageVersion::of(&old));
    sec.pages.push(page);
    nb.sections.push(notecraft_doc::SectionEntry::Section(sec));
    let mut gone = Section::new("Gonesection", Color::WHITE);
    gone.pages.push(Page::new("Binnedsectionpage"));
    nb.recycle_bin.sections.push(notecraft_doc::DeletedSection { section: gone, deleted: notecraft_doc::now(), group_path: vec![], index: 0 });
    nb.recycle_bin.pages.push(notecraft_doc::DeletedPage {
        page: Page::new("Binnedpage"),
        deleted: notecraft_doc::now(),
        section: notecraft_doc::Guid::NIL,
        section_name: String::new(),
        index: 0,
    });
    let pdf = export(&Doc::notebook(&nb), &Options::default(), &BundledFonts).unwrap();
    let text = extract_text(&pdf).concat();
    assert!(text.contains("Keepme"), "{text}");
    for hidden in ["Oldversion", "Stalecontent", "Gonesection", "Binnedsectionpage", "Binnedpage"] {
        assert!(!text.contains(hidden), "{hidden} leaked into {text}");
    }
}

#[test]
fn alignment_list_styles_and_tags_print() {
    use notecraft_doc::{Align, ListStyle};
    let mut page = Page::new("Formats");
    let mut o = Outline::with_text(notecraft_doc::page::CONTENT_ORIGIN, "");
    o.width = 300.0;
    let mut right = para("Rightside");
    right.align = Align::Right;
    let mut mid = para("Inthemiddle");
    mid.align = Align::Center;
    o.blocks = vec![right.into(), mid.into()];
    for t in ["Firstroman", "Secondroman"] {
        let mut p = para(t);
        p.list = Some(ListKind::Numbered);
        p.list_style = Some(ListStyle::UpperRoman);
        o.blocks.push(p.into());
    }
    let mut arrow = para("Arrowbullet");
    arrow.list = Some(ListKind::Bullet);
    arrow.list_style = Some(ListStyle::Arrow);
    o.blocks.push(arrow.into());
    for t in Tag::ALL {
        let mut p = para(&format!("Tagged{}", t.name()));
        p.tag = Some(t);
        o.blocks.push(p.into());
    }
    // Laid out as the canvas lays it out: the right-aligned row ends at the text area's right,
    // the centred one sits in the middle.
    let ink = style::Ink::new(None);
    let chars: std::collections::HashSet<char> = o.paragraphs().flat_map(|p| p.text().chars().collect::<Vec<_>>()).collect();
    let mut fs = fonts::FontSet::new(&BundledFonts, &[fonts::StyleKey::new(fonts::DEFAULT_FONT, false, false)], &chars);
    let lay = text::layout_outline(&mut fs, &o, &ink);
    let edge = |i: usize, f: f32| {
        let p = &lay.paras[i];
        let row = p.block.galley.rows.first().unwrap();
        p.x + p.align_dx + row.pos.x + row.rect().width() * f
    };
    assert!((edge(0, 1.0) - (o.pos.x + o.width)).abs() < 2.0, "right edge {}", edge(0, 1.0));
    assert!((edge(1, 0.5) - (o.pos.x + o.width / 2.0)).abs() < 2.0, "centre {}", edge(1, 0.5));
    page.elements.push(PageElement::new(ElementKind::Outline(o)));
    let pdf = export(&Doc::page(&page), &Options::default(), &BundledFonts).unwrap();
    let text = extract_text(&pdf).concat();
    for want in ["Rightside", "Inthemiddle", "I.", "II.", "Firstroman", "Arrowbullet", "Taggedphone", "Taggedhighlight"] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
}

#[test]
fn notecraft_links_go_to_pages_of_the_same_pdf() {
    let mut sec = Section::new("Work", Color::NOTECRAFT);
    let target = Page::new("Target page");
    let url = notecraft_doc::link::InternalLink::to_page(notecraft_doc::Guid::new(), sec.id, target.id, "Target page").to_url();
    let mut from = Page::new("From");
    let mut o = Outline::with_text(notecraft_doc::page::CONTENT_ORIGIN, "");
    o.para_mut(0).unwrap().runs = vec![Run::plain("Go to "), Run { text: "the target".into(), link: Some(url), ..Run::default() }];
    from.elements.push(PageElement::new(ElementKind::Outline(o)));
    sec.pages.push(from.clone());
    sec.pages.push(target);
    let pdf = export(&Doc::section(&sec), &Options::default(), &BundledFonts).unwrap();
    let raw = String::from_utf8_lossy(&pdf).into_owned();
    assert!(raw.contains("/Link") && raw.contains("/XYZ"), "a link to a page of the PDF");
    assert!(!raw.contains("/URI"), "not a web link");
    // On its own the page has nowhere to send the link: no annotation.
    let alone = String::from_utf8_lossy(&export(&Doc::page(&from), &Options::default(), &BundledFonts).unwrap()).into_owned();
    assert!(!alone.contains("/URI") && !alone.contains("/Link"));
}
