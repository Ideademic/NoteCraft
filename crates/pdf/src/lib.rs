//! NoteCraft's PDF output, for File › Export › PDF and for printing.
//!
//! Pages are drawn as vectors with craft-pdf's writer (Krilla): the paper colour and rule lines,
//! the title block, pictures and printout pages (embedded images), attached files, ink with its
//! pressure tapers and see-through highlighters, and the text of note containers in real fonts,
//! embedded as subsets, so it can be searched and copied. Text is laid out with epaint, the
//! library the canvas lays it out with, from the same faces, so lines break where they do on
//! screen (see [`fonts`] and [`style`]). Links become link areas (web and mail links only, as
//! [`craft_pdf::safe_url`] allows). A section or notebook becomes one PDF with a bookmark per
//! page, nested by section group, section and subpage.
//!
//! A page is either one sheet the size of its content ([`Paper::Fit`]), or laid across sheets
//! of a paper size, scaled down to the paper's width when wider (as OneNote prints) and broken
//! between lines of text, not through them.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod draw;
pub mod fonts;
pub mod ink;
mod page;
pub mod style;
pub mod text;
pub mod thumbnails;

use std::collections::HashSet;

use craft_pdf::write::{self, Metadata, Writer, backend as k};
use k::annotation::{Annotation, LinkAnnotation};
use k::destination::XyzDestination;
use k::geom::{Point, Size, Transform};
use k::outline::{Outline, OutlineNode};
use k::page::PageSettings;
use notecraft_doc::{ElementKind, Notebook, Page, Section, SectionEntry};
use notecraft_geom::Rect;

pub use fonts::{BundledFonts, FaceData, FontSource, SystemFonts};
pub use thumbnails::{Thumbnail, thumbnails};

/// Largest side of a PDF page, points (the format's 200-inch limit).
pub const MAX_SIDE: f32 = 14_400.0;
/// Most sheets one export writes.
pub const MAX_SHEETS: usize = 10_000;
/// Most sheets one page is spread over.
pub const MAX_SHEETS_PER_PAGE: usize = 500;
/// Most pages one export lays out.
pub const MAX_PAGES: usize = 5_000;
/// Smallest scale a wide page is shrunk to before it is tiled across sheets instead.
const MIN_FIT_SCALE: f32 = 0.3;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("nothing to print: no pages")]
    NoPages,
    #[error("PDF writer: {0}")]
    Write(String),
}

impl From<write::WriteError> for PdfError {
    fn from(e: write::WriteError) -> Self {
        PdfError::Write(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PdfError>;

/// The sheet size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Paper {
    /// One sheet per page, the size of what is on it.
    #[default]
    Fit,
    /// US Letter, 8.5 × 11 in.
    Letter,
    /// A4, 210 × 297 mm.
    A4,
    /// US Legal, 8.5 × 14 in.
    Legal,
    /// A5, 148 × 210 mm.
    A5,
}

impl Paper {
    pub const ALL: [Paper; 5] = [Paper::Fit, Paper::Letter, Paper::A4, Paper::Legal, Paper::A5];

    /// Portrait width and height, points; `None` for [`Paper::Fit`].
    pub fn size(self) -> Option<(f32, f32)> {
        match self {
            Paper::Fit => None,
            Paper::Letter => Some((612.0, 792.0)),
            Paper::A4 => Some((595.276, 841.89)),
            Paper::Legal => Some((612.0, 1008.0)),
            Paper::A5 => Some((419.528, 595.276)),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Paper::Fit => "fit",
            Paper::Letter => "letter",
            Paper::A4 => "a4",
            Paper::Legal => "legal",
            Paper::A5 => "a5",
        }
    }

    pub fn parse(s: &str) -> Option<Paper> {
        Paper::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s.trim()))
    }
}

/// How pages are put on sheets.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub paper: Paper,
    pub landscape: bool,
    /// Margin round the content on paper sheets, points.
    pub margin: f32,
    /// Shrink a page wider than the paper to the paper's width (else it is tiled across).
    pub fit_width: bool,
    /// Print each page's title, the rule under it and its date.
    pub header: bool,
    /// Number the sheets at the bottom (paper sizes only).
    pub page_numbers: bool,
    /// The reader's time zone for the dates under titles, minutes east of UTC.
    pub utc_offset_minutes: i32,
    /// Only these sheets, 1-based inclusive ranges (a print range); `None` for all.
    pub sheets: Option<Vec<(usize, usize)>>,
    /// The document title (metadata); the first page's title when `None`.
    pub title: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            paper: Paper::Fit,
            landscape: false,
            margin: 36.0,
            fit_width: true,
            header: true,
            page_numbers: true,
            utc_offset_minutes: 0,
            sheets: None,
            title: None,
        }
    }
}

/// What to print, as bookmarks nest it.
pub enum Item<'a> {
    Page(&'a Page),
    /// A section or section group: a bookmark holding its pages.
    Group {
        name: String,
        items: Vec<Item<'a>>,
    },
}

/// A page, a section or a notebook to print.
pub struct Doc<'a> {
    pub title: String,
    pub items: Vec<Item<'a>>,
}

impl<'a> Doc<'a> {
    pub fn page(page: &'a Page) -> Doc<'a> {
        Doc { title: page.display_title().to_owned(), items: vec![Item::Page(page)] }
    }

    /// The section's pages (subpages nest under their page's bookmark).
    pub fn section(sec: &'a Section) -> Doc<'a> {
        Doc { title: sec.name.clone(), items: sec.pages.iter().map(Item::Page).collect() }
    }

    pub fn notebook(nb: &'a Notebook) -> Doc<'a> {
        Doc { title: nb.name.clone(), items: entries(&nb.sections, 0) }
    }
}

fn entries(list: &[SectionEntry], depth: usize) -> Vec<Item<'_>> {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return Vec::new();
    }
    list.iter()
        .map(|e| match e {
            SectionEntry::Section(s) => Item::Group { name: s.name.clone(), items: s.pages.iter().map(Item::Page).collect() },
            SectionEntry::Group(g) => Item::Group { name: g.name.clone(), items: entries(&g.entries, depth + 1) },
        })
        .collect()
}

/// A bookmark tree as built: name, the first sheet of the page (when printed), children.
struct Mark {
    name: String,
    sheet: Option<usize>,
    level: u8,
    children: Vec<Mark>,
}

fn pages_of<'a>(items: &[Item<'a>], out: &mut Vec<&'a Page>) {
    for it in items {
        if out.len() >= MAX_PAGES {
            return;
        }
        match it {
            Item::Page(p) => out.push(p),
            Item::Group { items, .. } => pages_of(items, out),
        }
    }
}

/// Every style and character the pages draw (to resolve fonts once per export).
fn wanted(pages: &[&Page]) -> (Vec<fonts::StyleKey>, HashSet<char>) {
    let mut keys = vec![fonts::StyleKey::new(fonts::DEFAULT_FONT, false, false)];
    let mut chars: HashSet<char> = "0123456789.,:APM?abcdefghijklmnopqrstuvwxyz".chars().collect();
    let ink = style::Ink::new(None);
    for p in pages {
        chars.extend(p.title.chars().take(2000));
        for el in &p.elements {
            match &el.kind {
                ElementKind::Outline(o) => {
                    for para in o.paragraphs() {
                        for r in &para.runs {
                            let look = style::run_look(r, para.style, &ink);
                            keys.push(text::key_of(&look));
                            if chars.len() < 100_000 {
                                chars.extend(r.text.chars().take(text::MAX_PARA_CHARS));
                            }
                        }
                        keys.push(text::key_of(&style::run_look(&notecraft_doc::Run::default(), para.style, &ink)));
                    }
                }
                ElementKind::Printout(pr) => chars.extend(pr.file_name.chars().take(2000)),
                ElementKind::EmbeddedFile { name, .. } => chars.extend(name.chars().take(2000)),
                _ => {}
            }
        }
    }
    (fonts::dedup_keys(keys), chars)
}

/// Where one sheet of a page comes from: the page area it shows and the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheet {
    /// The page area, points.
    pub area: Rect,
    pub scale: f32,
    /// The sheet's size, points.
    pub size: (f32, f32),
    /// Where the area's top-left corner goes on the sheet.
    pub offset: (f32, f32),
}

/// The sheets a page with this content goes on.
pub fn plan_sheets(bounds: Rect, rows: &[(f32, f32)], boxes: &[Rect], opts: &Options) -> Vec<Sheet> {
    let x0 = bounds.x.min(0.0);
    let y0 = bounds.y.min(0.0);
    let x1 = (bounds.x + bounds.w).max(x0 + 1.0);
    let y1 = (bounds.y + bounds.h).max(y0 + 1.0);
    let Some((pw, ph)) = opts.paper.size() else {
        // One sheet the size of the content, with the title's 36 pt margin on the right and below.
        let (w, h) = (x1 - x0 + 36.0, y1 - y0 + 36.0);
        let (w, h) = (w.max(300.0), h.max(150.0));
        let scale = (MAX_SIDE / w).min(MAX_SIDE / h).min(1.0);
        return vec![Sheet { area: Rect::new(x0, y0, w, h), scale, size: (w * scale, h * scale), offset: (0.0, 0.0) }];
    };
    let (pw, ph) = if opts.landscape { (ph, pw) } else { (pw, ph) };
    let margin = if opts.margin.is_finite() { opts.margin.clamp(0.0, pw.min(ph) / 4.0) } else { 36.0 };
    let footer = if opts.page_numbers { 14.0 } else { 0.0 };
    let (aw, ah) = (pw - 2.0 * margin, ph - 2.0 * margin - footer);
    let width = x1 - x0;
    let scale = if opts.fit_width && width > aw { (aw / width).max(MIN_FIT_SCALE) } else { 1.0 };
    let (tw, th) = (aw / scale, ah / scale);
    // Rows: break before a text row the break would cut, unless that leaves the sheet too empty.
    let mut breaks = vec![y0];
    let mut y = y0;
    while y + th < y1 && breaks.len() < MAX_SHEETS_PER_PAGE {
        let mut b = y + th;
        for _ in 0..64 {
            let cut = rows.iter().filter(|(a, z)| *a < b - 0.5 && *z > b + 0.5).map(|(a, _)| *a).fold(f32::INFINITY, f32::min);
            if cut.is_finite() && cut > y + th * 0.5 {
                b = cut;
            } else {
                break;
            }
        }
        y = b;
        breaks.push(y);
    }
    let cols = ((width / tw).ceil().max(1.0) as usize).min(MAX_SHEETS_PER_PAGE);
    let mut out = Vec::new();
    for (i, top) in breaks.iter().enumerate() {
        // The last sheet shows a whole sheet's height (its rule lines reach the bottom).
        let bottom = breaks.get(i + 1).copied().unwrap_or(top + th);
        for c in 0..cols {
            if out.len() >= MAX_SHEETS_PER_PAGE {
                break;
            }
            let area = Rect::new(x0 + c as f32 * tw, *top, tw, (bottom - top).max(1.0));
            let used = boxes.iter().any(|b| b.x < area.x + area.w && b.x + b.w > area.x && b.y < area.y + area.h && b.y + b.h > area.y);
            if used || (i == 0 && c == 0) {
                out.push(Sheet { area, scale, size: (pw, ph), offset: (margin, margin) });
            }
        }
    }
    out
}

fn in_ranges(n: usize, ranges: &Option<Vec<(usize, usize)>>) -> bool {
    ranges.as_ref().is_none_or(|r| r.iter().any(|(a, b)| n >= *a && n <= *b))
}

/// Parses a print range like `1-3, 5, 8-`: 1-based sheets; `None` when it names none.
pub fn parse_ranges(s: &str) -> Option<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    for part in s.split([',', ';']).take(256) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => {
                let a = if a.trim().is_empty() { 1 } else { a.trim().parse().ok()? };
                let b = if b.trim().is_empty() { usize::MAX } else { b.trim().parse().ok()? };
                (a, b)
            }
            None => {
                let n = part.parse().ok()?;
                (n, n)
            }
        };
        if a == 0 || b < a {
            return None;
        }
        out.push((a, b));
    }
    (!out.is_empty()).then_some(out)
}

/// The PDF of `doc`. Panics in the PDF backend come back as errors.
pub fn export(doc: &Doc<'_>, opts: &Options, source: &dyn FontSource) -> Result<Vec<u8>> {
    write::guard(|| export_inner(doc, opts, source))
}

/// How many sheets `doc` prints on, without writing it.
pub fn count_sheets(doc: &Doc<'_>, opts: &Options, source: &dyn FontSource) -> usize {
    let mut pages = Vec::new();
    pages_of(&doc.items, &mut pages);
    let (keys, chars) = wanted(&pages);
    let mut fs = fonts::FontSet::new(source, &keys, &chars);
    pages
        .iter()
        .map(|p| {
            let lay = page::lay_out(&mut fs, p, opts.header, opts.utc_offset_minutes);
            plan_sheets(lay.bounds, &lay.rows, &lay.boxes, opts).len()
        })
        .fold(0usize, usize::saturating_add)
        .min(MAX_SHEETS)
}

fn export_inner(doc: &Doc<'_>, opts: &Options, source: &dyn FontSource) -> Result<Vec<u8>> {
    let mut pages = Vec::new();
    pages_of(&doc.items, &mut pages);
    if pages.is_empty() {
        return Err(PdfError::NoPages);
    }
    let (keys, chars) = wanted(&pages);
    let mut fs = fonts::FontSet::new(source, &keys, &chars);
    let title = opts.title.clone().unwrap_or_else(|| doc.title.clone());
    let metadata = Metadata {
        creator: "NoteCraft".into(),
        producer: "NoteCraft".into(),
        title: (!title.trim().is_empty()).then_some(title),
        ..Metadata::default()
    };
    let mut writer = Writer::new(k::SerializeSettings::default(), metadata);
    let pdf = writer.document_mut();
    let mut images = page::Images::default();
    // The first sheet written for each page (bookmarks), by the page's address.
    let mut first_sheet: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut sheet_no = 0usize;
    let mut written = 0usize;
    let mut planned: Vec<(usize, Vec<Sheet>)> = Vec::new();
    // Plan first so sheet numbers ("3 of 7") are known.
    for (pi, p) in pages.iter().enumerate() {
        let lay = page::lay_out(&mut fs, p, opts.header, opts.utc_offset_minutes);
        planned.push((pi, plan_sheets(lay.bounds, &lay.rows, &lay.boxes, opts)));
    }
    let total: usize = planned.iter().map(|(_, s)| s.len()).fold(0, usize::saturating_add).min(MAX_SHEETS);
    for (pi, sheets) in planned {
        let Some(p) = pages.get(pi) else { continue };
        if sheet_no >= MAX_SHEETS {
            break;
        }
        // Lay out again (cheap: epaint caches the galleys) so one page's layout is alive at a time.
        let lay = page::lay_out(&mut fs, p, opts.header, opts.utc_offset_minutes);
        for sh in sheets {
            sheet_no += 1;
            if sheet_no > MAX_SHEETS || !in_ranges(sheet_no, &opts.sheets) {
                continue;
            }
            let Some(size) = Size::from_wh(sh.size.0.max(1.0), sh.size.1.max(1.0)) else { continue };
            first_sheet.entry(std::ptr::from_ref::<Page>(p) as usize).or_insert(written);
            let mut links = Vec::new();
            let mut page_out = pdf.start_page_with(PageSettings::new(size));
            {
                let mut s = page_out.surface();
                if let Some(c) = p.background.color {
                    draw::fill_rect(&mut s, 0.0, 0.0, sh.size.0, sh.size.1, c.with_alpha(255));
                }
                let (ox, oy) = sh.offset;
                let t = Transform::from_row(sh.scale, 0.0, 0.0, sh.scale, ox - sh.area.x * sh.scale, oy - sh.area.y * sh.scale);
                s.push_transform(&t);
                let clipped = opts.paper != Paper::Fit;
                if clipped && let Some(clip) = draw::rect_path(sh.area.x, sh.area.y, sh.area.w, sh.area.h) {
                    s.push_clip_path(&clip, &k::paint::FillRule::NonZero);
                }
                page::draw(&mut s, &mut fs, &mut images, &lay, sh.area, &mut links);
                if clipped && draw::rect_path(sh.area.x, sh.area.y, sh.area.w, sh.area.h).is_some() {
                    s.pop();
                }
                s.pop();
                if opts.page_numbers && clipped {
                    let label = format!("{sheet_no} / {total}");
                    let b = text::simple(&mut fs, &label, text::plain_look(9.0, style::mix(lay.ink.paper, lay.ink.text, 0.55)));
                    let y = sh.size.1 - sh.offset.1 - b.height() + 6.0;
                    let x = (sh.size.0 - b.width()) / 2.0;
                    text::draw_block(&mut s, &mut fs, &b, x, y, Rect::new(0.0, 0.0, sh.size.0, sh.size.1), &mut Vec::new());
                }
                s.finish();
            }
            for l in links {
                if let Some(a) = link_annotation(&l, &sh) {
                    page_out.add_annotation(a);
                }
            }
            page_out.finish();
            written += 1;
        }
        fs.next_page();
    }
    if written == 0 {
        return Err(PdfError::NoPages);
    }
    let marks = marks(&doc.items, &first_sheet, 0);
    pdf.set_outline(outline(&marks));
    writer.finish().map_err(Into::into)
}

/// A link area as an annotation on its sheet, clipped to the sheet's area.
fn link_annotation(l: &text::LinkArea, sh: &Sheet) -> Option<Annotation> {
    let target = write::uri_target(&l.url)?;
    let a = &sh.area;
    let (x0, y0) = (l.rect.x.max(a.x), l.rect.y.max(a.y));
    let (x1, y1) = ((l.rect.x + l.rect.w).min(a.x + a.w), (l.rect.y + l.rect.h).min(a.y + a.h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let map = |x: f32, y: f32| (sh.offset.0 + (x - a.x) * sh.scale, sh.offset.1 + (y - a.y) * sh.scale);
    let (l0, t0) = map(x0, y0);
    let (r1, b1) = map(x1, y1);
    let rect = k::geom::Rect::from_ltrb(l0, t0, r1, b1)?;
    Some(Annotation::new_link(LinkAnnotation::new(rect, target), Some(l.url.chars().take(500).collect())))
}

fn marks(items: &[Item<'_>], first: &std::collections::HashMap<usize, usize>, depth: usize) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH + 1 {
        return out;
    }
    for it in items {
        match it {
            Item::Page(p) => {
                let m = Mark {
                    name: p.display_title().chars().take(300).collect(),
                    sheet: first.get(&(std::ptr::from_ref::<Page>(p) as usize)).copied(),
                    level: p.level,
                    children: Vec::new(),
                };
                // A subpage goes under the nearest page above with a lower level.
                push_nested(&mut out, m);
            }
            Item::Group { name, items } => {
                let children = marks(items, first, depth + 1);
                let sheet = first_of(&children);
                out.push(Mark { name: name.chars().take(300).collect(), sheet, level: 0, children });
            }
        }
    }
    out
}

fn push_nested(list: &mut Vec<Mark>, m: Mark) {
    match list.last_mut() {
        Some(last) if m.level > last.level => push_nested(&mut last.children, m),
        _ => list.push(m),
    }
}

fn first_of(marks: &[Mark]) -> Option<usize> {
    marks.iter().find_map(|m| m.sheet.or_else(|| first_of(&m.children)))
}

fn outline(marks: &[Mark]) -> Outline {
    let mut o = Outline::new();
    for m in marks {
        if let Some(n) = node(m) {
            o.push_child(n);
        }
    }
    o
}

fn node(m: &Mark) -> Option<OutlineNode> {
    let sheet = m.sheet.or_else(|| first_of(&m.children))?;
    let mut n = OutlineNode::new(m.name.clone(), XyzDestination::new(sheet, Point::from_xy(0.0, 0.0)));
    for c in &m.children {
        if let Some(cn) = node(c) {
            n.push_child(cn);
        }
    }
    Some(n)
}

#[cfg(test)]
mod tests;
