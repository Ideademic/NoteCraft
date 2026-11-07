//! Pages and what is on them: outlines (note containers) of rich text, pictures, file
//! printouts, embedded files, content kept for round-trip, and ink.
//!
//! Coordinates are points from the page's top-left corner (see [`notecraft_geom`]). A page is an
//! open canvas: it has no fixed size, it grows to fit what is on it.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use notecraft_geom::{Point, Rect, Size};
use serde::{Deserialize, Serialize};

use crate::{Color, Guid, InkStroke};

/// Deepest subpage level: 0 is a page, 1 and 2 are subpages.
pub const MAX_PAGE_LEVEL: u8 = 2;
/// Most list indent levels a paragraph has.
pub const MAX_INDENT: u8 = 8;
/// Most elements on one page (hostile files): more are dropped.
pub const MAX_ELEMENTS_PER_PAGE: usize = 10_000;
/// Most paragraphs in one outline (hostile files and commands).
pub const MAX_PARAGRAPHS: usize = 20_000;
/// Default width of a new outline, points (about 4 in).
pub const DEFAULT_OUTLINE_WIDTH: f32 = 288.0;
/// Where the title sits and where content starts below it, points.
pub const TITLE_ORIGIN: Point = Point { x: 36.0, y: 36.0 };
/// Where the first outline of a new page goes.
pub const CONTENT_ORIGIN: Point = Point { x: 36.0, y: 108.0 };

/// Rule lines and grids (View ▸ Rule Lines). Spacing is NoteCraft's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleLines {
    #[default]
    None,
    Narrow,
    College,
    Standard,
    Wide,
    GridSmall,
    GridMedium,
    GridLarge,
}

impl RuleLines {
    pub const ALL: [RuleLines; 8] = [
        RuleLines::None,
        RuleLines::Narrow,
        RuleLines::College,
        RuleLines::Standard,
        RuleLines::Wide,
        RuleLines::GridSmall,
        RuleLines::GridMedium,
        RuleLines::GridLarge,
    ];

    /// Distance between lines, points; `None` for no lines.
    pub fn spacing(self) -> Option<f32> {
        match self {
            RuleLines::None => None,
            RuleLines::Narrow => Some(14.4),
            RuleLines::College => Some(19.8),
            RuleLines::Standard => Some(23.4),
            RuleLines::Wide => Some(28.8),
            RuleLines::GridSmall => Some(9.0),
            RuleLines::GridMedium => Some(18.0),
            RuleLines::GridLarge => Some(36.0),
        }
    }

    /// Vertical lines too.
    pub fn is_grid(self) -> bool {
        matches!(self, RuleLines::GridSmall | RuleLines::GridMedium | RuleLines::GridLarge)
    }

    pub fn name(self) -> &'static str {
        match self {
            RuleLines::None => "none",
            RuleLines::Narrow => "narrow",
            RuleLines::College => "college",
            RuleLines::Standard => "standard",
            RuleLines::Wide => "wide",
            RuleLines::GridSmall => "gridSmall",
            RuleLines::GridMedium => "gridMedium",
            RuleLines::GridLarge => "gridLarge",
        }
    }

    pub fn parse(s: &str) -> Option<RuleLines> {
        RuleLines::ALL.into_iter().find(|r| r.name().eq_ignore_ascii_case(s))
    }
}

/// Page colour and rule lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PageBackground {
    /// `None`: the default (white).
    pub color: Option<Color>,
    pub rule: RuleLines,
}

/// One page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: Guid,
    /// The title line's text (also the page list's label).
    pub title: String,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    /// 0: page; 1..=[`MAX_PAGE_LEVEL`]: subpage of the nearest page above with a lower level.
    #[serde(default)]
    pub level: u8,
    #[serde(default)]
    pub background: PageBackground,
    /// Back to front.
    #[serde(default)]
    pub elements: Vec<PageElement>,
    /// Page-level ink in page points, drawn above the elements, oldest first. See [`crate::ink`].
    #[serde(default)]
    pub ink: Vec<InkStroke>,
}

impl Page {
    /// An empty page created now.
    pub fn new(title: impl Into<String>) -> Page {
        let now = crate::now();
        Page::new_at(title, now)
    }

    /// An empty page created at `when`.
    pub fn new_at(title: impl Into<String>, when: DateTime<Utc>) -> Page {
        Page {
            id: Guid::new(),
            title: title.into(),
            created: when,
            modified: when,
            level: 0,
            background: PageBackground::default(),
            elements: Vec::new(),
            ink: Vec::new(),
        }
    }

    /// The title, or "Untitled page" when blank (page list, window title).
    pub fn display_title(&self) -> &str {
        let t = self.title.trim();
        if t.is_empty() { "Untitled page" } else { t }
    }

    /// Sets `modified` to now.
    pub fn touch(&mut self) {
        self.modified = crate::now();
    }

    /// `level` clamped to `0..=MAX_PAGE_LEVEL`.
    pub fn set_level(&mut self, level: u8) {
        self.level = level.min(MAX_PAGE_LEVEL);
    }

    pub fn element(&self, id: Guid) -> Option<&PageElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: Guid) -> Option<&mut PageElement> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    /// Removes and returns the element.
    pub fn remove_element(&mut self, id: Guid) -> Option<PageElement> {
        let i = self.elements.iter().position(|e| e.id == id)?;
        Some(self.elements.remove(i))
    }

    pub fn outline(&self, id: Guid) -> Option<&Outline> {
        match &self.element(id)?.kind {
            ElementKind::Outline(o) => Some(o),
            _ => None,
        }
    }

    pub fn outline_mut(&mut self, id: Guid) -> Option<&mut Outline> {
        match &mut self.element_mut(id)?.kind {
            ElementKind::Outline(o) => Some(o),
            _ => None,
        }
    }

    pub fn stroke(&self, id: Guid) -> Option<&InkStroke> {
        self.ink.iter().find(|s| s.id == id)
    }

    pub fn stroke_mut(&mut self, id: Guid) -> Option<&mut InkStroke> {
        self.ink.iter_mut().find(|s| s.id == id)
    }

    /// The page's text: title, then each outline's paragraphs (one per line), in element order.
    /// For search and plain-text export.
    pub fn plain_text(&self) -> String {
        let mut out = self.title.clone();
        for e in &self.elements {
            if let ElementKind::Outline(o) = &e.kind {
                for p in &o.paragraphs {
                    out.push('\n');
                    out.push_str(&p.text());
                }
            }
        }
        out
    }

    /// The area covered by elements and ink (for scrolling); `None` for an empty page.
    pub fn content_bounds(&self) -> Option<Rect> {
        let elems = self.elements.iter().filter_map(PageElement::bounds);
        let ink = self.ink.iter().filter_map(InkStroke::bounds);
        elems.chain(ink).reduce(Rect::union)
    }
}

/// Something placed on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageElement {
    pub id: Guid,
    pub kind: ElementKind,
}

impl PageElement {
    pub fn new(kind: ElementKind) -> PageElement {
        PageElement { id: Guid::new(), kind }
    }

    /// Top-left corner, points; `None` for opaque content.
    pub fn pos(&self) -> Option<Point> {
        match &self.kind {
            ElementKind::Outline(o) => Some(o.pos),
            ElementKind::Image(i) => Some(i.pos),
            ElementKind::Printout(p) => Some(p.pos),
            ElementKind::EmbeddedFile { pos, .. } => Some(*pos),
            ElementKind::Opaque { .. } => None,
        }
    }

    /// Moves the element's top-left corner to `to` (no-op for opaque content).
    pub fn set_pos(&mut self, to: Point) {
        let to = to.sanitized();
        match &mut self.kind {
            ElementKind::Outline(o) => o.pos = to,
            ElementKind::Image(i) => i.pos = to,
            ElementKind::Printout(p) => p.pos = to,
            ElementKind::EmbeddedFile { pos, .. } => *pos = to,
            ElementKind::Opaque { .. } => {}
        }
    }

    /// Approximate bounds, points. An outline's height is estimated from its paragraph count
    /// (the canvas measures real text); an embedded file is an icon-sized box.
    pub fn bounds(&self) -> Option<Rect> {
        match &self.kind {
            ElementKind::Outline(o) => Some(Rect::new(o.pos.x, o.pos.y, o.width, o.paragraphs.len().max(1) as f32 * 18.0)),
            ElementKind::Image(i) => Some(Rect::from_min_size(i.pos, i.size)),
            ElementKind::Printout(p) => Some(p.bounds()),
            ElementKind::EmbeddedFile { pos, .. } => Some(Rect::from_min_size(*pos, Size::new(160.0, 40.0))),
            ElementKind::Opaque { .. } => None,
        }
    }
}

/// What an element is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ElementKind {
    /// A note container of rich text.
    Outline(Outline),
    /// A picture.
    Image(ImageElement),
    /// A file printout: the file (a PDF) and its pages as pictures, stacked down the page, that
    /// can be inked over.
    Printout(Printout),
    /// An attached file, shown as an icon with its name.
    EmbeddedFile {
        name: String,
        #[serde(with = "crate::b64")]
        bytes: Arc<Vec<u8>>,
        pos: Point,
    },
    /// Content NoteCraft doesn't edit (tables, equations, audio…), kept as read so it can be
    /// written back: the MS-ONE JCID of its root object and the format crate's encoding of it.
    Opaque {
        jcid: u32,
        #[serde(with = "crate::b64")]
        data: Arc<Vec<u8>>,
    },
}

/// A note container: positioned, fixed width, paragraphs flowing down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outline {
    /// Top-left corner, points.
    pub pos: Point,
    /// Width, points.
    pub width: f32,
    pub paragraphs: Vec<Paragraph>,
}

impl Outline {
    /// An outline at `pos` holding `text` (one paragraph per line).
    pub fn with_text(pos: Point, text: &str) -> Outline {
        Outline { pos: pos.sanitized(), width: DEFAULT_OUTLINE_WIDTH, paragraphs: text.split('\n').map(Paragraph::plain).collect() }
    }

    /// Paragraph texts joined with newlines.
    pub fn text(&self) -> String {
        self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.text().is_empty())
    }
}

/// List formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    Bullet,
    Numbered,
}

/// A note tag on a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Tag {
    ToDo { checked: bool },
    Important,
    Question,
}

/// Paragraph styles (Home ▸ Styles).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParaStyle {
    #[default]
    Normal,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Title,
    Quote,
    Code,
}

impl ParaStyle {
    pub const ALL: [ParaStyle; 10] = [
        ParaStyle::Normal,
        ParaStyle::Heading1,
        ParaStyle::Heading2,
        ParaStyle::Heading3,
        ParaStyle::Heading4,
        ParaStyle::Heading5,
        ParaStyle::Heading6,
        ParaStyle::Title,
        ParaStyle::Quote,
        ParaStyle::Code,
    ];

    /// 1–6 for headings.
    pub fn heading_level(self) -> Option<u8> {
        match self {
            ParaStyle::Heading1 => Some(1),
            ParaStyle::Heading2 => Some(2),
            ParaStyle::Heading3 => Some(3),
            ParaStyle::Heading4 => Some(4),
            ParaStyle::Heading5 => Some(5),
            ParaStyle::Heading6 => Some(6),
            _ => None,
        }
    }

    /// Default font size, points.
    pub fn font_size(self) -> f32 {
        match self {
            ParaStyle::Heading1 => 16.0,
            ParaStyle::Heading2 => 14.0,
            ParaStyle::Heading3 | ParaStyle::Heading4 => 12.0,
            ParaStyle::Heading5 | ParaStyle::Heading6 => 11.0,
            ParaStyle::Title => 20.0,
            ParaStyle::Normal | ParaStyle::Quote | ParaStyle::Code => 11.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ParaStyle::Normal => "normal",
            ParaStyle::Heading1 => "heading1",
            ParaStyle::Heading2 => "heading2",
            ParaStyle::Heading3 => "heading3",
            ParaStyle::Heading4 => "heading4",
            ParaStyle::Heading5 => "heading5",
            ParaStyle::Heading6 => "heading6",
            ParaStyle::Title => "title",
            ParaStyle::Quote => "quote",
            ParaStyle::Code => "code",
        }
    }

    pub fn parse(s: &str) -> Option<ParaStyle> {
        ParaStyle::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(s))
    }
}

/// One paragraph of an outline.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Paragraph {
    pub runs: Vec<Run>,
    /// Indent level, 0..=[`MAX_INDENT`].
    pub indent: u8,
    pub list: Option<ListKind>,
    pub tag: Option<Tag>,
    pub style: ParaStyle,
}

impl Paragraph {
    /// One unformatted run.
    pub fn plain(text: &str) -> Paragraph {
        Paragraph { runs: vec![Run::plain(text)], ..Paragraph::default() }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Hostile-input bounds: indent clamped.
    pub fn sanitized(mut self) -> Paragraph {
        self.indent = self.indent.min(MAX_INDENT);
        self
    }
}

/// A run of text with one formatting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub highlight: Option<Color>,
    pub color: Option<Color>,
    /// Font family; `None` uses the default font.
    pub font: Option<String>,
    /// Size, points; `None` uses the paragraph style's size.
    pub size: Option<f32>,
    /// Hyperlink target.
    pub link: Option<String>,
}

impl Run {
    pub fn plain(text: &str) -> Run {
        Run { text: text.to_owned(), ..Run::default() }
    }

    /// Same formatting (everything but the text): adjacent runs that agree can merge.
    pub fn same_format(&self, other: &Run) -> bool {
        Run { text: String::new(), ..self.clone() } == Run { text: String::new(), ..other.clone() }
    }
}

/// Picture formats NoteCraft recognises.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    Gif,
    Bmp,
    /// Anything else (kept as bytes; drawn as a placeholder).
    Other,
}

impl ImageFormat {
    /// Sniffs the format from the file signature.
    pub fn sniff(bytes: &[u8]) -> ImageFormat {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            ImageFormat::Png
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            ImageFormat::Jpeg
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            ImageFormat::Gif
        } else if bytes.starts_with(b"BM") {
            ImageFormat::Bmp
        } else {
            ImageFormat::Other
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Other => "bin",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Other => "application/octet-stream",
        }
    }
}

/// A picture on the page (or one page of a printout).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageElement {
    /// Top-left corner, points.
    pub pos: Point,
    /// Displayed size, points.
    pub size: Size,
    /// The encoded file (PNG, JPEG…), shared between undo snapshots.
    #[serde(with = "crate::b64")]
    pub data: Arc<Vec<u8>>,
    pub format: ImageFormat,
    /// Alt text.
    #[serde(default)]
    pub alt: String,
}

/// A file printout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Printout {
    pub file_name: String,
    /// The original file (kept so it can be saved back out or re-rendered).
    #[serde(with = "crate::b64")]
    pub file_bytes: Arc<Vec<u8>>,
    /// Top-left corner of the first page, points.
    pub pos: Point,
    /// The rendered pages; each has its own position (page points), normally stacked down.
    pub pages: Vec<ImageElement>,
}

impl Printout {
    /// Union of the page pictures, or a point at `pos` when there are none.
    pub fn bounds(&self) -> Rect {
        self.pages.iter().map(|p| Rect::from_min_size(p.pos, p.size)).reduce(Rect::union).unwrap_or(Rect::from_min_size(self.pos, Size::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_and_lookup() {
        let mut p = Page::new("Shopping");
        let o = PageElement::new(ElementKind::Outline(Outline::with_text(CONTENT_ORIGIN, "eggs\nmilk")));
        let id = o.id;
        p.elements.push(o);
        assert_eq!(p.plain_text(), "Shopping\neggs\nmilk");
        assert_eq!(p.outline(id).map(Outline::text), Some("eggs\nmilk".into()));
        p.element_mut(id).unwrap().set_pos(Point::new(f32::NAN, 5.0));
        assert_eq!(p.element(id).unwrap().pos(), Some(Point::new(0.0, 5.0)));
        assert!(p.content_bounds().is_some());
        assert!(p.remove_element(id).is_some());
        assert!(p.content_bounds().is_none());
        assert_eq!(Page::new("  ").display_title(), "Untitled page");
    }

    #[test]
    fn levels_clamp() {
        let mut p = Page::new("x");
        p.set_level(9);
        assert_eq!(p.level, MAX_PAGE_LEVEL);
    }

    #[test]
    fn enums_parse() {
        for r in RuleLines::ALL {
            assert_eq!(RuleLines::parse(r.name()), Some(r));
        }
        assert!(RuleLines::GridLarge.is_grid() && !RuleLines::Wide.is_grid());
        assert_eq!(RuleLines::None.spacing(), None);
        for s in ParaStyle::ALL {
            assert_eq!(ParaStyle::parse(s.name()), Some(s));
        }
        assert_eq!(ParaStyle::Heading3.heading_level(), Some(3));
        assert_eq!(ImageFormat::sniff(b"\x89PNG\r\n\x1a\nxxxx"), ImageFormat::Png);
        assert_eq!(ImageFormat::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::sniff(b""), ImageFormat::Other);
    }

    #[test]
    fn element_json_round_trip() {
        let img = PageElement::new(ElementKind::Image(ImageElement {
            pos: Point::new(1.0, 2.0),
            size: Size::new(3.0, 4.0),
            data: Arc::new(vec![1, 2, 3]),
            format: ImageFormat::Png,
            alt: "a".into(),
        }));
        let opaque = PageElement::new(ElementKind::Opaque { jcid: 0x0006_0024, data: Arc::new(vec![9; 5]) });
        for e in [img, opaque] {
            let j = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<PageElement>(&j).unwrap(), e, "{j}");
        }
        let para: Paragraph = serde_json::from_str(r#"{"runs":[{"text":"hi","bold":true}],"tag":{"type":"toDo","checked":true}}"#).unwrap();
        assert_eq!(para.tag, Some(Tag::ToDo { checked: true }));
        assert!(para.runs[0].bold);
    }

    #[test]
    fn run_format_compare() {
        let a = Run { text: "a".into(), bold: true, ..Run::default() };
        let b = Run { text: "b".into(), bold: true, ..Run::default() };
        assert!(a.same_format(&b));
        assert!(!a.same_format(&Run::plain("a")));
    }
}
