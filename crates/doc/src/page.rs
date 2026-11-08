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
    /// Earlier versions of the page, oldest first (History ▸ Page Versions). See
    /// [`crate::history`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<crate::PageVersion>,
    /// View ▸ Hide Page Title: the title block isn't shown (kept per page).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_title: bool,
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
            versions: Vec::new(),
            hide_title: false,
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

    /// Moves the element's top-left corner to `to` (no-op for opaque content). A printout's page
    /// pictures move with it, keeping their places relative to its `pos`.
    pub fn set_pos(&mut self, to: Point) {
        let to = to.sanitized();
        match &mut self.kind {
            ElementKind::Outline(o) => o.pos = to,
            ElementKind::Image(i) => i.pos = to,
            ElementKind::Printout(p) => p.move_to(to),
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

/// How a list item's marker looks: a bullet shape or a numbering format. A paragraph without
/// one ([`Paragraph::list_style`] `None`) takes its marker from its indent level (bullets: disc,
/// circle, square; numbers: `1.`, `a.`, `i.`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListStyle {
    /// `•`
    Disc,
    /// `◦`
    Circle,
    /// `▪`
    Square,
    /// `–`
    Dash,
    /// `➢`
    Arrow,
    /// `1.` `2.` `3.`
    Decimal,
    /// `a.` `b.` `c.`
    LowerAlpha,
    /// `A.` `B.` `C.`
    UpperAlpha,
    /// `i.` `ii.` `iii.`
    LowerRoman,
    /// `I.` `II.` `III.`
    UpperRoman,
}

impl ListStyle {
    pub const BULLETS: [ListStyle; 5] = [ListStyle::Disc, ListStyle::Circle, ListStyle::Square, ListStyle::Dash, ListStyle::Arrow];
    pub const NUMBERS: [ListStyle; 5] =
        [ListStyle::Decimal, ListStyle::LowerAlpha, ListStyle::UpperAlpha, ListStyle::LowerRoman, ListStyle::UpperRoman];

    /// Whether this is a bullet or a numbering format.
    pub fn kind(self) -> ListKind {
        if ListStyle::BULLETS.contains(&self) { ListKind::Bullet } else { ListKind::Numbered }
    }

    pub fn name(self) -> &'static str {
        match self {
            ListStyle::Disc => "disc",
            ListStyle::Circle => "circle",
            ListStyle::Square => "square",
            ListStyle::Dash => "dash",
            ListStyle::Arrow => "arrow",
            ListStyle::Decimal => "decimal",
            ListStyle::LowerAlpha => "lowerAlpha",
            ListStyle::UpperAlpha => "upperAlpha",
            ListStyle::LowerRoman => "lowerRoman",
            ListStyle::UpperRoman => "upperRoman",
        }
    }

    /// By [`ListStyle::name`] (any case), or by a sample of the marker (`"1."`, `"a."`, `"I."`,
    /// `"•"`).
    pub fn parse(s: &str) -> Option<ListStyle> {
        let t = s.trim();
        if let Some(st) = ListStyle::BULLETS.into_iter().chain(ListStyle::NUMBERS).find(|x| x.name().eq_ignore_ascii_case(t)) {
            return Some(st);
        }
        let sample = t.trim_end_matches(['.', ')']);
        let mut chars = sample.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && !c.is_ascii_alphanumeric()
        {
            return ListStyle::from_bullet_char(c);
        }
        Some(match sample {
            "1" => ListStyle::Decimal,
            "a" => ListStyle::LowerAlpha,
            "A" => ListStyle::UpperAlpha,
            "i" => ListStyle::LowerRoman,
            "I" => ListStyle::UpperRoman,
            "o" => ListStyle::Circle,
            _ => return None,
        })
    }

    /// The bullet character (bullets only).
    pub fn bullet_char(self) -> Option<char> {
        match self {
            ListStyle::Disc => Some('\u{2022}'),
            ListStyle::Circle => Some('\u{25E6}'),
            ListStyle::Square => Some('\u{25AA}'),
            ListStyle::Dash => Some('\u{2013}'),
            ListStyle::Arrow => Some('\u{27A2}'),
            _ => None,
        }
    }

    /// The bullet for a character a file uses: the shapes NoteCraft draws, look-alikes, and the
    /// private-use codes symbol fonts put bullets at. `None` for characters it doesn't recognise.
    pub fn from_bullet_char(c: char) -> Option<ListStyle> {
        Some(match c {
            '\u{2022}' | '\u{25CF}' | '\u{F0B7}' | '\u{F06C}' | '\u{00B7}' | '\u{2219}' => ListStyle::Disc,
            '\u{25E6}' | '\u{25CB}' | 'o' | '\u{F06F}' => ListStyle::Circle,
            '\u{25AA}' | '\u{25A0}' | '\u{25FE}' | '\u{F0A7}' | '\u{F06E}' => ListStyle::Square,
            '\u{2013}' | '\u{2014}' | '-' | '\u{2012}' => ListStyle::Dash,
            '\u{27A2}' | '\u{2192}' | '\u{25B6}' | '\u{25BA}' | '\u{27A4}' | '>' | '\u{F0D8}' | '\u{F0E0}' => ListStyle::Arrow,
            _ => return None,
        })
    }

    /// The marker text of item `n` (from 1) in this numbering format, with its full stop:
    /// `"3."`, `"c."`, `"III."`. Bullets give their character.
    pub fn label(self, n: u32) -> String {
        match self {
            ListStyle::Decimal => format!("{n}."),
            ListStyle::LowerAlpha => format!("{}.", letters(n)),
            ListStyle::UpperAlpha => format!("{}.", letters(n).to_uppercase()),
            ListStyle::LowerRoman => format!("{}.", roman(n)),
            ListStyle::UpperRoman => format!("{}.", roman(n).to_uppercase()),
            b => b.bullet_char().map(String::from).unwrap_or_default(),
        }
    }

    /// The default style at indent level `indent`: bullets go disc, circle, square; numbers
    /// `1.`, `a.`, `i.`; then again.
    pub fn by_level(kind: ListKind, indent: u8) -> ListStyle {
        match (kind, indent % 3) {
            (ListKind::Bullet, 0) => ListStyle::Disc,
            (ListKind::Bullet, 1) => ListStyle::Circle,
            (ListKind::Bullet, _) => ListStyle::Square,
            (ListKind::Numbered, 0) => ListStyle::Decimal,
            (ListKind::Numbered, 1) => ListStyle::LowerAlpha,
            (ListKind::Numbered, _) => ListStyle::LowerRoman,
        }
    }
}

/// A paragraph's list marker, as the canvas and the PDF draw it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Marker {
    /// Drawn as a shape: disc, circle, square, dash or arrow.
    Bullet(ListStyle),
    /// "1.", "a.", "IV." …
    Number(String),
}

/// Each paragraph's marker. Numbers count per level and numbering style (as MS-ONE counts list
/// items with the same NumberListFormat at the same level); a paragraph at a lower level, or
/// another kind at the same level, starts the count again.
pub fn markers(paras: &[Paragraph]) -> Vec<Option<Marker>> {
    const LEVELS: usize = MAX_INDENT as usize + 1;
    let mut counters = [0u32; LEVELS];
    let mut styles: [Option<ListStyle>; LEVELS] = [None; LEVELS];
    let mut out = Vec::with_capacity(paras.len());
    for p in paras {
        let level = usize::from(p.indent.min(MAX_INDENT));
        // Deeper levels start again under this paragraph.
        for c in counters.iter_mut().skip(level + 1) {
            *c = 0;
        }
        match (p.list, p.marker_style()) {
            (Some(ListKind::Numbered), Some(st)) => {
                let same = styles.get(level).copied().flatten() == Some(st);
                let n = if same { counters.get(level).copied().unwrap_or(0).saturating_add(1) } else { 1 };
                if let (Some(c), Some(s)) = (counters.get_mut(level), styles.get_mut(level)) {
                    *c = n;
                    *s = Some(st);
                }
                out.push(Some(Marker::Number(st.label(n))));
            }
            (Some(ListKind::Bullet), Some(st)) => {
                if let (Some(c), Some(s)) = (counters.get_mut(level), styles.get_mut(level)) {
                    *c = 0;
                    *s = None;
                }
                out.push(Some(Marker::Bullet(st)));
            }
            _ => {
                if let (Some(c), Some(s)) = (counters.get_mut(level), styles.get_mut(level)) {
                    *c = 0;
                    *s = None;
                }
                out.push(None);
            }
        }
    }
    out
}

/// `1` → `a`, `26` → `z`, `27` → `aa` (at most 8 letters).
fn letters(mut n: u32) -> String {
    let mut s = Vec::new();
    while n > 0 && s.len() < 8 {
        let d = ((n - 1) % 26) as u8;
        s.push(char::from(b'a' + d));
        n = (n - 1) / 26;
    }
    s.iter().rev().collect()
}

/// Lower-case roman numerals for 1–3999, digits otherwise.
fn roman(n: u32) -> String {
    if n == 0 || n >= 4000 {
        return n.to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    let mut n = n;
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// Paragraph alignment (Home ▸ Paragraph Alignment).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    /// Word-wrapped lines fill the container's width (the last line stays on the left).
    Justify,
}

impl Align {
    pub const ALL: [Align; 4] = [Align::Left, Align::Center, Align::Right, Align::Justify];

    pub fn name(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
            Align::Justify => "justify",
        }
    }

    pub fn parse(s: &str) -> Option<Align> {
        let t = s.trim();
        if t.eq_ignore_ascii_case("centre") {
            return Some(Align::Center);
        }
        Align::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(t))
    }
}

/// A note tag on a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Tag {
    ToDo { checked: bool },
    Important,
    Question,
    RememberForLater,
    Definition,
    Highlight,
    Contact,
    Address,
    Phone,
    Idea,
    Critical,
}

impl Tag {
    /// Every tag in gallery order (To Do unchecked): the Mod+1…Mod+9 ones first.
    pub const ALL: [Tag; 11] = [
        Tag::ToDo { checked: false },
        Tag::Important,
        Tag::Question,
        Tag::RememberForLater,
        Tag::Definition,
        Tag::Highlight,
        Tag::Contact,
        Tag::Address,
        Tag::Phone,
        Tag::Idea,
        Tag::Critical,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tag::ToDo { .. } => "toDo",
            Tag::Important => "important",
            Tag::Question => "question",
            Tag::RememberForLater => "rememberForLater",
            Tag::Definition => "definition",
            Tag::Highlight => "highlight",
            Tag::Contact => "contact",
            Tag::Address => "address",
            Tag::Phone => "phone",
            Tag::Idea => "idea",
            Tag::Critical => "critical",
        }
    }

    /// The English label (menus, exports, the `.one` tag definition).
    pub fn label(self) -> &'static str {
        match self {
            Tag::ToDo { .. } => "To Do",
            Tag::Important => "Important",
            Tag::Question => "Question",
            Tag::RememberForLater => "Remember for later",
            Tag::Definition => "Definition",
            Tag::Highlight => "Highlight",
            Tag::Contact => "Contact",
            Tag::Address => "Address",
            Tag::Phone => "Phone number",
            Tag::Idea => "Idea",
            Tag::Critical => "Critical",
        }
    }

    /// By [`Tag::name`] or [`Tag::label`] (any case; spaces, `-` and `_` ignored); To Do comes
    /// unchecked.
    pub fn parse(s: &str) -> Option<Tag> {
        let key: String = s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '_').collect::<String>().to_ascii_lowercase();
        let key = match key.as_str() {
            "phonenumber" => "phone",
            k => k,
        };
        Tag::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(key))
    }

    /// The colour a highlight tag paints behind the text; `None` for the tags with an icon.
    pub fn highlight(self) -> Option<Color> {
        match self {
            Tag::RememberForLater | Tag::Highlight => Some(Color::rgb(0xFF, 0xF2, 0x00)),
            Tag::Definition => Some(Color::rgb(0x92, 0xD0, 0x50)),
            _ => None,
        }
    }

    /// Whether the tag draws an icon before the text (the highlight tags don't).
    pub fn has_icon(self) -> bool {
        self.highlight().is_none()
    }
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
    /// The marker's look; `None` (or a style of the other list kind) takes the level's default
    /// ([`Paragraph::marker_style`]).
    pub list_style: Option<ListStyle>,
    pub tag: Option<Tag>,
    pub style: ParaStyle,
    pub align: Align,
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

    /// The list marker style in effect: the paragraph's own when it matches its list kind, else
    /// the level's default. `None` outside a list.
    pub fn marker_style(&self) -> Option<ListStyle> {
        let kind = self.list?;
        Some(self.list_style.filter(|s| s.kind() == kind).unwrap_or_else(|| ListStyle::by_level(kind, self.indent)))
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
    /// Rotation, flips and crop, applied when the picture is drawn (the file is kept as it is).
    #[serde(default, skip_serializing_if = "PictureTransform::is_identity")]
    pub transform: PictureTransform,
}

impl ImageElement {
    /// The displayed rect (page points) the whole, uncropped picture would have at its current
    /// scale: `pos`/`size` are the visible (cropped, rotated) part.
    pub fn full_rect(&self) -> Rect {
        let [l, t, r, b] = self.transform.sanitized().display_crop();
        let (cw, ch) = ((r - l).max(0.01), (b - t).max(0.01));
        let size = self.size.sanitized();
        let (fw, fh) = (size.w / cw, size.h / ch);
        let pos = self.pos.sanitized();
        Rect::new(pos.x - l * fw, pos.y - t * fh, fw, fh)
    }
}

/// A crop: the part of the picture kept, as fractions of the source image (0 = left/top edge,
/// 1 = right/bottom edge).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// Smallest crop, as a fraction of the picture each way.
pub const MIN_CROP: f32 = 0.01;

impl Crop {
    /// Finite, inside 0..=1, ordered and at least [`MIN_CROP`] each way; `None` when it keeps the
    /// whole picture.
    pub fn sanitized(self) -> Option<Crop> {
        let f = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        let (mut l, mut r) = (f(self.left, 0.0), f(self.right, 1.0));
        let (mut t, mut b) = (f(self.top, 0.0), f(self.bottom, 1.0));
        if r < l {
            std::mem::swap(&mut l, &mut r);
        }
        if b < t {
            std::mem::swap(&mut t, &mut b);
        }
        let widen = |a: f32, z: f32| -> (f32, f32) {
            if z - a >= MIN_CROP {
                (a, z)
            } else {
                let a = a.min(1.0 - MIN_CROP);
                (a, a + MIN_CROP)
            }
        };
        let (l, r) = widen(l, r);
        let (t, b) = widen(t, b);
        let c = Crop { left: l, top: t, right: r, bottom: b };
        let whole = l <= 1e-4 && t <= 1e-4 && r >= 1.0 - 1e-4 && b >= 1.0 - 1e-4;
        (!whole).then_some(c)
    }
}

/// How a picture is drawn: rotation by quarter turns, flips and a crop. The file isn't changed.
/// Flips apply to the source image before the rotation; the crop is in source fractions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PictureTransform {
    /// Quarter turns clockwise, 0..=3.
    pub rotation: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    pub crop: Option<Crop>,
}

impl PictureTransform {
    pub fn is_identity(&self) -> bool {
        self.rotation.is_multiple_of(4) && !self.flip_h && !self.flip_v && self.crop.and_then(Crop::sanitized).is_none()
    }

    /// Rotation in 0..=3 and a sane crop.
    pub fn sanitized(self) -> PictureTransform {
        PictureTransform { rotation: self.rotation % 4, crop: self.crop.and_then(Crop::sanitized), ..self }
    }

    /// A source fraction (u, v) as a fraction of the displayed, uncropped box: flipped, then
    /// turned clockwise.
    pub fn to_display(&self, u: f32, v: f32) -> (f32, f32) {
        let mut p = (if self.flip_h { 1.0 - u } else { u }, if self.flip_v { 1.0 - v } else { v });
        for _ in 0..self.rotation % 4 {
            p = (1.0 - p.1, p.0);
        }
        p
    }

    /// The inverse of [`PictureTransform::to_display`].
    pub fn to_source(&self, x: f32, y: f32) -> (f32, f32) {
        let mut p = (x, y);
        for _ in 0..self.rotation % 4 {
            p = (p.1, 1.0 - p.0);
        }
        (if self.flip_h { 1.0 - p.0 } else { p.0 }, if self.flip_v { 1.0 - p.1 } else { p.1 })
    }

    /// The crop as `[left, top, right, bottom]` fractions of the displayed box; the whole box
    /// when uncropped.
    pub fn display_crop(&self) -> [f32; 4] {
        let Some(c) = self.crop.and_then(Crop::sanitized) else { return [0.0, 0.0, 1.0, 1.0] };
        let a = self.to_display(c.left, c.top);
        let b = self.to_display(c.right, c.bottom);
        [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
    }

    /// Converts a crop in display fractions back to source fractions.
    pub fn crop_from_display(&self, [l, t, r, b]: [f32; 4]) -> Option<Crop> {
        let a = self.to_source(l, t);
        let z = self.to_source(r, b);
        Crop { left: a.0.min(z.0), top: a.1.min(z.1), right: a.0.max(z.0), bottom: a.1.max(z.1) }.sanitized()
    }

    /// Source texture coordinates of the displayed (visible) rect's corners: top-left,
    /// top-right, bottom-right, bottom-left.
    pub fn uv_corners(&self) -> [(f32, f32); 4] {
        let [l, t, r, b] = self.display_crop();
        [self.to_source(l, t), self.to_source(r, t), self.to_source(r, b), self.to_source(l, b)]
    }
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
    /// Moves the printout so `pos` is `to`, its page pictures by the same amount (sanitized: a
    /// junk position never spreads to the pages).
    pub fn move_to(&mut self, to: Point) {
        let to = to.sanitized();
        let from = self.pos.sanitized();
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        for pg in &mut self.pages {
            pg.pos = pg.pos.sanitized().offset(dx, dy).sanitized();
        }
        self.pos = to;
    }

    /// Union of the page pictures, or a point at `pos` when there are none.
    pub fn bounds(&self) -> Rect {
        self.pages.iter().map(|p| Rect::from_min_size(p.pos, p.size)).reduce(Rect::union).unwrap_or(Rect::from_min_size(self.pos, Size::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_a_printout_moves_its_pages() {
        let page = |y: f32| ImageElement {
            pos: Point::new(40.0, y),
            size: Size::new(100.0, 140.0),
            data: Arc::default(),
            format: ImageFormat::Png,
            alt: String::new(),
            transform: Default::default(),
        };
        let pr =
            Printout { file_name: "a.pdf".into(), file_bytes: Arc::default(), pos: Point::new(40.0, 100.0), pages: vec![page(100.0), page(250.0)] };
        let mut el = PageElement::new(ElementKind::Printout(pr));
        el.set_pos(Point::new(140.0, 120.0));
        let ElementKind::Printout(p) = &el.kind else { panic!("a printout") };
        assert_eq!(p.pos, Point::new(140.0, 120.0));
        assert_eq!(p.pages[0].pos, Point::new(140.0, 120.0));
        assert_eq!(p.pages[1].pos, Point::new(140.0, 270.0));
        assert_eq!(el.bounds().map(|b| (b.x, b.y)), Some((140.0, 120.0)));
        // A junk position doesn't reach the pages.
        el.set_pos(Point::new(f32::NAN, f32::INFINITY));
        let ElementKind::Printout(p) = &el.kind else { panic!("a printout") };
        assert!(p.pages.iter().all(|pg| pg.pos.x.is_finite() && pg.pos.y.is_finite()));
    }

    #[test]
    fn picture_transforms_map_both_ways() {
        let id = PictureTransform::default();
        assert!(id.is_identity());
        assert_eq!(id.uv_corners(), [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        // A quarter turn clockwise: the source's top-left corner is shown top-right.
        let cw = PictureTransform { rotation: 1, ..Default::default() };
        assert_eq!(cw.to_display(0.0, 0.0), (1.0, 0.0));
        assert_eq!(cw.uv_corners()[0], (0.0, 1.0), "the shown top-left is the source's bottom-left");
        for rotation in 0..4 {
            for (flip_h, flip_v) in [(false, false), (true, false), (false, true), (true, true)] {
                let t = PictureTransform { rotation, flip_h, flip_v, crop: Some(Crop { left: 0.1, top: 0.2, right: 0.6, bottom: 0.9 }) };
                let (x, y) = t.to_display(0.3, 0.7);
                let (u, v) = t.to_source(x, y);
                assert!((u - 0.3).abs() < 1e-6 && (v - 0.7).abs() < 1e-6, "{t:?}");
                let back = t.crop_from_display(t.display_crop()).unwrap();
                assert!((back.left - 0.1).abs() < 1e-6 && (back.bottom - 0.9).abs() < 1e-6, "{t:?}");
            }
        }
        // Junk crops are tamed; a whole-picture crop is none.
        let c = Crop { left: f32::NAN, top: 0.5, right: -3.0, bottom: 0.5 }.sanitized().unwrap();
        assert!(c.left <= c.right && c.bottom - c.top >= MIN_CROP - 1e-6);
        assert!(Crop { left: 0.0, top: 0.0, right: 1.0, bottom: 1.0 }.sanitized().is_none());
        assert_eq!(PictureTransform { rotation: 9, ..Default::default() }.sanitized().rotation, 1);
        // The full rect of a picture cropped to its right half extends to the left.
        let img = ImageElement {
            pos: Point::new(100.0, 10.0),
            size: Size::new(50.0, 40.0),
            data: Arc::default(),
            format: ImageFormat::Png,
            alt: String::new(),
            transform: PictureTransform { crop: Some(Crop { left: 0.5, top: 0.0, right: 1.0, bottom: 1.0 }), ..Default::default() },
        };
        assert_eq!(img.full_rect(), Rect::new(50.0, 10.0, 100.0, 40.0));
        let j = serde_json::to_string(&img).unwrap();
        assert!(j.contains("\"transform\""));
        assert_eq!(serde_json::from_str::<ImageElement>(&j).unwrap(), img);
    }

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
            transform: Default::default(),
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
    fn list_styles_tags_and_alignment_parse() {
        for s in ListStyle::BULLETS.into_iter().chain(ListStyle::NUMBERS) {
            assert_eq!(ListStyle::parse(s.name()), Some(s));
            if let Some(c) = s.bullet_char() {
                assert_eq!(ListStyle::from_bullet_char(c), Some(s));
                assert_eq!(s.kind(), ListKind::Bullet);
            } else {
                assert_eq!(s.kind(), ListKind::Numbered);
            }
        }
        assert_eq!(ListStyle::parse("I."), Some(ListStyle::UpperRoman));
        assert_eq!(ListStyle::parse("a)"), Some(ListStyle::LowerAlpha));
        assert_eq!(ListStyle::parse("\u{2022}"), Some(ListStyle::Disc));
        assert_eq!(ListStyle::parse("nope"), None);
        assert_eq!(ListStyle::UpperRoman.label(14), "XIV.");
        assert_eq!(ListStyle::UpperAlpha.label(28), "AB.");
        assert_eq!(ListStyle::Decimal.label(u32::MAX), format!("{}.", u32::MAX));
        assert_eq!(ListStyle::LowerRoman.label(0), "0.");
        let mut p = Paragraph { list: Some(ListKind::Numbered), indent: 1, ..Paragraph::plain("x") };
        assert_eq!(p.marker_style(), Some(ListStyle::LowerAlpha));
        p.list_style = Some(ListStyle::Square);
        assert_eq!(p.marker_style(), Some(ListStyle::LowerAlpha), "a bullet style doesn't apply to a numbered item");
        p.list_style = Some(ListStyle::UpperRoman);
        assert_eq!(p.marker_style(), Some(ListStyle::UpperRoman));
        for t in Tag::ALL {
            assert_eq!(Tag::parse(t.name()), Some(t));
            assert_eq!(Tag::parse(t.label()), Some(t));
        }
        assert_eq!(Tag::parse("to-do"), Some(Tag::ToDo { checked: false }));
        assert!(Tag::Definition.highlight().is_some() && !Tag::Definition.has_icon() && Tag::Idea.has_icon());
        for a in Align::ALL {
            assert_eq!(Align::parse(a.name()), Some(a));
        }
        assert_eq!(Align::parse("Centre"), Some(Align::Center));
        // Older JSON (no align, no list style) still reads.
        let old: Paragraph = serde_json::from_str(r#"{"runs":[{"text":"x"}],"list":"bullet","tag":{"type":"idea"}}"#).unwrap();
        assert_eq!((old.align, old.list_style, old.tag), (Align::Left, None, Some(Tag::Idea)));
        let j = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Paragraph>(&j).unwrap(), p);
    }

    #[test]
    fn run_format_compare() {
        let a = Run { text: "a".into(), bold: true, ..Run::default() };
        let b = Run { text: "b".into(), bold: true, ..Run::default() };
        assert!(a.same_format(&b));
        assert!(!a.same_format(&Run::plain("a")));
    }
}
