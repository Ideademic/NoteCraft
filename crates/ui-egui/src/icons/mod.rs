//! NoteCraft's icons, drawn in code (no image assets and nothing from Microsoft).
//!
//! The drawings are DeckCraft's icon set ([`deck`], vendored from storytold/deckcraft,
//! `MIT OR Apache-2.0`) on a 24×24 grid, plus NoteCraft's own in the same style
//! ([`deck::extra`]). [`Icon`] names what the UI needs; [`paint_styled`] maps it to a drawing.
//! Line work is in the caller's ink colour with a few fixed accents (a gold clip, a green badge);
//! strokes get thinner relative to the icon as it grows. A disabled icon is all ink at 35%.

pub mod deck;

use deck::Mode;
use deck::extra::Extra;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
/// Every icon the UI uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    // Clipboard
    Paste,
    Cut,
    Copy,
    FormatPainter,
    // Basic Text
    Bold,
    Italic,
    Underline,
    Strike,
    Highlight,
    FontColor,
    Bullets,
    Numbering,
    Indent,
    Outdent,
    ClearFormat,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Justify,
    // Tags
    ToDo,
    Important,
    Question,
    /// Remember for later: text on a yellow highlight.
    RememberForLater,
    /// Definition: text on a green highlight.
    Definition,
    /// The Highlight tag: text on a yellow highlight with a marker.
    HighlightTag,
    Contact,
    Address,
    Phone,
    Idea,
    Critical,
    // Insert
    InsertSpace,
    Table,
    Printout,
    Attach,
    Picture,
    Date,
    Time,
    DateTime,
    // Draw
    Type,
    Select,
    Lasso,
    Eraser,
    Pen,
    Pencil,
    Highlighter,
    ColorThickness,
    InkToShape,
    InkToText,
    /// The Shapes gallery (a square, a circle and a triangle).
    Shapes,
    Delete,
    Arrange,
    Rotate,
    // Pictures, arrange, find (editing)
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
    RotateRight,
    RotateLeft,
    FlipHorizontal,
    FlipVertical,
    Crop,
    AltText,
    ResetPicture,
    FindTags,
    Templates,
    // View
    NormalView,
    FullPageView,
    DockDesktop,
    PageColor,
    RuleLines,
    HideTitle,
    ZoomIn,
    ZoomOut,
    Zoom100,
    PageWidth,
    NewWindow,
    DarkMode,
    // Quick access, panes, backstage
    Back,
    Undo,
    Redo,
    More,
    Dropdown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    Plus,
    Pin,
    Search,
    /// A page (no badge).
    Page,
    NewPage,
    NewSection,
    /// A notebook in the app colour (no badge).
    Notebook,
    NewNotebook,
    SectionGroup,
    Info,
    Open,
    Export,
    Print,
    Import,
    Options,
    About,
    Close,
    Rename,
    // History
    History,
    RecycleBin,
    PageVersions,
    Unread,
    MarkRead,
    RecentEdits,
    // Review
    Spelling,
    WordCount,
    Language,
    Link,
    // Window buttons
    Minimize,
    Maximize,
    Restore,
    WindowClose,
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: &'static [Icon] = &[
        Icon::Paste,
        Icon::Cut,
        Icon::Copy,
        Icon::FormatPainter,
        Icon::Bold,
        Icon::Italic,
        Icon::Underline,
        Icon::Strike,
        Icon::Highlight,
        Icon::FontColor,
        Icon::Bullets,
        Icon::Numbering,
        Icon::Indent,
        Icon::Outdent,
        Icon::ClearFormat,
        Icon::AlignLeft,
        Icon::AlignCenter,
        Icon::AlignRight,
        Icon::Justify,
        Icon::ToDo,
        Icon::Important,
        Icon::Question,
        Icon::RememberForLater,
        Icon::Definition,
        Icon::HighlightTag,
        Icon::Contact,
        Icon::Address,
        Icon::Phone,
        Icon::Idea,
        Icon::Critical,
        Icon::InsertSpace,
        Icon::Table,
        Icon::Printout,
        Icon::Attach,
        Icon::Picture,
        Icon::Date,
        Icon::Time,
        Icon::DateTime,
        Icon::Type,
        Icon::Select,
        Icon::Lasso,
        Icon::Eraser,
        Icon::Pen,
        Icon::Pencil,
        Icon::Highlighter,
        Icon::ColorThickness,
        Icon::InkToShape,
        Icon::InkToText,
        Icon::Shapes,
        Icon::Delete,
        Icon::Arrange,
        Icon::Rotate,
        Icon::BringToFront,
        Icon::BringForward,
        Icon::SendBackward,
        Icon::SendToBack,
        Icon::RotateRight,
        Icon::RotateLeft,
        Icon::FlipHorizontal,
        Icon::FlipVertical,
        Icon::Crop,
        Icon::AltText,
        Icon::ResetPicture,
        Icon::FindTags,
        Icon::Templates,
        Icon::NormalView,
        Icon::FullPageView,
        Icon::DockDesktop,
        Icon::PageColor,
        Icon::RuleLines,
        Icon::HideTitle,
        Icon::ZoomIn,
        Icon::ZoomOut,
        Icon::Zoom100,
        Icon::PageWidth,
        Icon::NewWindow,
        Icon::DarkMode,
        Icon::Back,
        Icon::Undo,
        Icon::Redo,
        Icon::More,
        Icon::Dropdown,
        Icon::ChevronLeft,
        Icon::ChevronRight,
        Icon::ChevronUp,
        Icon::ChevronDown,
        Icon::Plus,
        Icon::Pin,
        Icon::Search,
        Icon::Page,
        Icon::NewPage,
        Icon::NewSection,
        Icon::Notebook,
        Icon::NewNotebook,
        Icon::SectionGroup,
        Icon::Info,
        Icon::Open,
        Icon::Export,
        Icon::Print,
        Icon::Import,
        Icon::Options,
        Icon::About,
        Icon::Close,
        Icon::Rename,
        Icon::History,
        Icon::RecycleBin,
        Icon::PageVersions,
        Icon::Unread,
        Icon::MarkRead,
        Icon::RecentEdits,
        Icon::Spelling,
        Icon::WordCount,
        Icon::Language,
        Icon::Link,
        Icon::Minimize,
        Icon::Maximize,
        Icon::Restore,
        Icon::WindowClose,
    ];
}

/// Fixed colours the UI uses next to icons (the pen swatches in the Draw tab).
pub mod ink {
    use egui::Color32;
    pub const YELLOW: Color32 = Color32::from_rgb(0xF6, 0xC3, 0x2C);
    pub const FOLDER: Color32 = Color32::from_rgb(0xF2, 0xB2, 0x3A);
    pub const RED: Color32 = Color32::from_rgb(0xE0, 0x3A, 0x3A);
    pub const GREEN: Color32 = Color32::from_rgb(0x2E, 0xA0, 0x4F);
    pub const BLUE: Color32 = Color32::from_rgb(0x2F, 0x7B, 0xE0);
    pub const SKY: Color32 = Color32::from_rgb(0x7F, 0xC4, 0xF5);
    pub const PINK: Color32 = Color32::from_rgb(0xF0, 0x6E, 0xB4);
    pub const PAPER: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    pub const CREAM: Color32 = Color32::from_rgb(0xEF, 0xE9, 0xDC);
    pub const GREY: Color32 = Color32::from_rgb(0x9A, 0x9A, 0x9E);
}

/// How to colour an icon.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    /// The ink: line work, and everything when `flat` or disabled.
    pub line: Color32,
    /// The app colour, for the parts that are NoteCraft's own (a new notebook's cover).
    pub accent: Color32,
    /// False: everything in `line` at 35% alpha, badges as outlines.
    pub enabled: bool,
    /// Everything in `line` at full strength (no accent colours), for glyphs on a coloured bar.
    pub flat: bool,
}

impl Style {
    /// Line work in `line`, the fixed accents, and `accent` for the app's own parts.
    pub fn new(line: Color32, accent: Color32) -> Style {
        Style { line, accent, enabled: true, flat: false }
    }
    /// Line work in `c` with the accents kept (the File menu, the quick access toolbar, panes).
    pub fn mono(c: Color32) -> Style {
        Style::new(c, crate::theme::APP_COLOR)
    }
    /// Everything in `c` (window buttons and glyphs on the title bar).
    pub fn flat(c: Color32) -> Style {
        Style { flat: true, ..Style::mono(c) }
    }
    /// A disabled icon: `ink` (the normal icon colour) at 35%, badges as outlines.
    pub fn disabled(ink: Color32) -> Style {
        Style { enabled: false, ..Style::mono(ink) }
    }
    fn mode(self) -> Mode {
        if !self.enabled {
            Mode::Disabled
        } else if self.flat {
            Mode::Flat
        } else {
            Mode::Colour
        }
    }
}

/// What draws an [`Icon`].
enum Drawing {
    Deck(deck::Icon),
    Extra(Extra),
    Window,
}

/// Paints `icon` filling `rect` in `color` (line work), with the default accent.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    paint_styled(painter, rect, icon, Style::new(color, crate::theme::APP_COLOR));
}

/// Paints `icon` filling `rect`.
pub fn paint_styled(painter: &Painter, rect: Rect, icon: Icon, style: Style) {
    let mode = style.mode();
    match drawing(icon, style) {
        Drawing::Deck(d) => deck::paint_mode(painter, rect, d, style.line, mode),
        Drawing::Extra(x) => deck::extra::paint_mode(painter, rect, x, style.line, mode),
        Drawing::Window => window_button(painter, rect, icon, if style.enabled { style.line } else { style.line.gamma_multiply(0.35) }),
    }
}

fn drawing(icon: Icon, style: Style) -> Drawing {
    use deck::Icon as D;
    let d = match icon {
        Icon::Paste => D::Paste,
        Icon::Cut => D::Cut,
        Icon::Copy => D::Copy,
        Icon::FormatPainter => D::FormatPainter,
        Icon::Bold => D::Bold,
        Icon::Italic => D::Italic,
        Icon::Underline => D::Underline,
        Icon::Strike => D::Strikethrough,
        Icon::Highlight => D::Highlight,
        Icon::FontColor => D::FontColor,
        Icon::Bullets => D::Bullets,
        Icon::Numbering => D::Numbering,
        Icon::Indent => D::IncreaseIndent,
        Icon::Outdent => D::DecreaseIndent,
        Icon::ClearFormat => D::ClearFormatting,
        Icon::AlignLeft => D::AlignLeft,
        Icon::AlignCenter => D::AlignCenter,
        Icon::AlignRight => D::AlignRight,
        Icon::Justify => D::Justify,
        Icon::Table => D::Table,
        Icon::Printout => D::Object,
        Icon::Picture => D::Picture,
        Icon::Date => D::DateTime,
        Icon::Time => D::Recent,
        Icon::Type => D::TextBox,
        Icon::Lasso => D::LassoSelect,
        Icon::Eraser => D::Eraser,
        Icon::Pen => D::Pen,
        Icon::Pencil => D::Pencil,
        Icon::Highlighter => D::HighlighterPen,
        Icon::InkToShape => D::InkToShape,
        Icon::InkToText => D::InkToText,
        Icon::Shapes => D::Shapes,
        Icon::Rotate => D::Rotate,
        Icon::BringToFront => D::BringToFront,
        Icon::BringForward => D::BringForward,
        Icon::SendBackward => D::SendBackward,
        Icon::SendToBack => D::SendToBack,
        Icon::RotateRight => D::RotateRight,
        Icon::RotateLeft => D::RotateLeft,
        Icon::FlipHorizontal => D::FlipH,
        Icon::FlipVertical => D::FlipV,
        Icon::Crop => D::Crop,
        Icon::AltText => D::AltText,
        Icon::ResetPicture => D::ResetPicture,
        Icon::FindTags => D::SelectionPane,
        Icon::Templates => D::Layout,
        Icon::NormalView => D::ViewNormal,
        Icon::FullPageView => D::FitToWindow,
        Icon::PageColor => D::FormatBackground,
        Icon::HideTitle => D::HideSlide,
        Icon::ZoomIn => D::ZoomIn,
        Icon::ZoomOut => D::ZoomOut,
        Icon::NewWindow => D::NewWindow,
        Icon::Back => D::BackCircle,
        Icon::Undo => D::Undo,
        Icon::Redo => D::Redo,
        Icon::More => D::More,
        Icon::ChevronLeft => D::ChevronLeft,
        Icon::ChevronRight => D::ChevronRight,
        Icon::ChevronUp => D::ChevronUp,
        Icon::ChevronDown => D::ChevronDown,
        Icon::Plus => D::Plus,
        Icon::Pin => D::Pin,
        Icon::Search => D::Search,
        Icon::Page => D::File,
        Icon::NewSection => D::Section,
        Icon::Info => D::Info,
        Icon::Open => D::OpenFolder,
        Icon::Export => D::Export,
        Icon::Print => D::Print,
        Icon::Options => D::Settings,
        Icon::About => D::Help,
        Icon::Close => D::Close,
        Icon::Spelling => D::Spelling,
        Icon::WordCount => D::SlideNumber,
        Icon::Language => D::Language,
        Icon::Link => D::Link,
        _ => return extra_drawing(icon, style),
    };
    Drawing::Deck(d)
}

fn extra_drawing(icon: Icon, style: Style) -> Drawing {
    let x = match icon {
        Icon::ToDo => Extra::Checkbox,
        Icon::Important => Extra::Important,
        Icon::Question => Extra::Question,
        Icon::RememberForLater => Extra::Marked(TAG_YELLOW, false),
        Icon::Definition => Extra::Marked(TAG_GREEN, false),
        Icon::HighlightTag => Extra::Marked(TAG_YELLOW, true),
        Icon::Contact => Extra::Contact,
        Icon::Address => Extra::Address,
        Icon::Phone => Extra::Phone,
        Icon::Idea => Extra::Idea,
        Icon::Critical => Extra::Critical,
        Icon::InsertSpace => Extra::InsertSpace,
        Icon::Attach => Extra::Attach,
        Icon::DateTime => Extra::DateTime,
        Icon::Select => Extra::Select,
        Icon::ColorThickness => Extra::Thickness,
        Icon::Delete => Extra::Delete,
        Icon::Arrange => Extra::Arrange,
        Icon::DockDesktop => Extra::DockDesktop,
        Icon::RuleLines => Extra::RuleLines,
        Icon::Zoom100 => Extra::Zoom100,
        Icon::PageWidth => Extra::PageWidth,
        Icon::DarkMode => Extra::DarkMode,
        Icon::Dropdown => Extra::Dropdown,
        Icon::NewPage => Extra::NewPage,
        Icon::Notebook => Extra::Notebook(crate::theme::opaque(style.accent)),
        Icon::NewNotebook => Extra::NewNotebook(crate::theme::opaque(style.accent)),
        Icon::SectionGroup => Extra::SectionGroup,
        Icon::Import => Extra::Import,
        Icon::Rename => Extra::Rename,
        Icon::History => Extra::History,
        Icon::RecycleBin => Extra::RecycleBin,
        Icon::PageVersions => Extra::PageVersions,
        Icon::Unread => Extra::Unread,
        Icon::MarkRead => Extra::MarkRead,
        Icon::RecentEdits => Extra::RecentEdits,
        _ => return Drawing::Window,
    };
    Drawing::Extra(x)
}

/// The highlight tags' colours (Remember for later and Highlight; Definition).
const TAG_YELLOW: Color32 = Color32::from_rgb(0xFF, 0xF2, 0x00);
const TAG_GREEN: Color32 = Color32::from_rgb(0x92, 0xD0, 0x50);

/// The icon a tag shows in menus and before tagged text; `None` for To Do, which draws its own
/// check box with its tick.
pub fn tag_icon(tag: notecraft_doc::Tag) -> Option<Icon> {
    use notecraft_doc::Tag;
    Some(match tag {
        Tag::ToDo { .. } => return None,
        Tag::Important => Icon::Important,
        Tag::Question => Icon::Question,
        Tag::RememberForLater => Icon::RememberForLater,
        Tag::Definition => Icon::Definition,
        Tag::Highlight => Icon::HighlightTag,
        Tag::Contact => Icon::Contact,
        Tag::Address => Icon::Address,
        Tag::Phone => Icon::Phone,
        Tag::Idea => Icon::Idea,
        Tag::Critical => Icon::Critical,
    })
}

/// A list bullet of `style` centred at `c`, `r` its radius: the canvas's markers and the
/// Bullets menu draw the same shapes.
pub fn paint_bullet(painter: &Painter, c: Pos2, r: f32, style: notecraft_doc::ListStyle, color: Color32) {
    use notecraft_doc::ListStyle;
    let w = (r * 0.4).max(1.0);
    match style {
        ListStyle::Circle => {
            painter.circle_stroke(c, r - 0.5, Stroke::new((r * 0.35).max(1.0), color));
        }
        ListStyle::Square => {
            painter.rect_filled(Rect::from_center_size(c, vec2(r * 1.7, r * 1.7)), 0.0, color);
        }
        ListStyle::Dash => {
            painter.line_segment([pos2(c.x - r * 1.4, c.y), pos2(c.x + r * 1.4, c.y)], Stroke::new(w, color));
        }
        ListStyle::Arrow => {
            // A solid arrowhead pointing at the text, in two convex halves.
            let s = r * 1.5;
            let (top, tip, bottom, notch) =
                (pos2(c.x - s * 0.7, c.y - s), pos2(c.x + s * 0.9, c.y), pos2(c.x - s * 0.7, c.y + s), pos2(c.x - s * 0.2, c.y));
            painter.add(Shape::convex_polygon(vec![top, tip, notch], color, Stroke::NONE));
            painter.add(Shape::convex_polygon(vec![notch, tip, bottom], color, Stroke::NONE));
        }
        _ => {
            painter.circle_filled(c, r, color);
        }
    }
}

/// A notebook icon in the notebook's own colour (the notebook pane, the File menu lists).
pub fn paint_notebook(painter: &Painter, rect: Rect, color: Color32) {
    deck::extra::paint_mode(painter, rect, Extra::Notebook(crate::theme::opaque(color)), Color32::from_gray(0x80), Mode::Colour);
}

/// The window buttons and the app mark: NoteCraft's own thin glyphs on a 20×20 grid.
struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    c: Color32,
    w: f32,
}

impl Pen<'_> {
    fn s(&self) -> f32 {
        self.r.width().min(self.r.height()) / 20.0
    }
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        let s = self.s();
        let o = self.r.center() - vec2(10.0 * s, 10.0 * s);
        pos2(o.x + x * s, o.y + y * s)
    }
    fn stroke(&self, c: Color32) -> Stroke {
        Stroke::new((self.w * self.s()).max(1.0), c)
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(v, self.stroke(self.c)));
    }
    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::convex_polygon(v, c, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_stroke(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 1.0, self.stroke(c), StrokeKind::Middle);
    }
    fn frect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 1.0, c);
    }
    fn lines(&self, x0: f32, x1: f32, ys: &[f32]) {
        for y in ys {
            self.line(&[(x0, *y), (x1, *y)]);
        }
    }
}

/// Minimise, maximise, restore and close (anything else unrouted: a framed square).
fn window_button(painter: &Painter, rect: Rect, icon: Icon, c: Color32) {
    let size = rect.width().min(rect.height());
    if !size.is_finite() || size <= 0.5 {
        return;
    }
    let pen = Pen { p: painter, r: rect, c, w: 1.25 };
    match icon {
        Icon::Minimize => pen.line(&[(5.0, 10.5), (15.0, 10.5)]),
        Icon::Maximize => pen.rect(5.0, 5.0, 15.0, 15.0, c),
        Icon::Restore => {
            pen.rect(5.0, 7.5, 12.5, 15.0, c);
            pen.line(&[(7.5, 7.5), (7.5, 5.0), (15.0, 5.0), (15.0, 12.5), (12.5, 12.5)]);
        }
        Icon::WindowClose => {
            pen.line(&[(5.5, 5.5), (14.5, 14.5)]);
            pen.line(&[(14.5, 5.5), (5.5, 14.5)]);
        }
        _ => pen.rect(4.0, 4.0, 16.0, 16.0, c),
    }
}

/// NoteCraft's own mark, drawn in code for the title bar: a cream page with section tabs on its
/// edge and a pen nib, in the house style of the app icon.
pub fn paint_app_mark(painter: &Painter, rect: Rect) {
    let pen = Pen { p: painter, r: rect, c: Color32::from_rgb(0x0B, 0x0B, 0x0C), w: 1.1 };
    painter.rect_filled(rect, 3.0, Color32::WHITE);
    pen.frect(4.0, 3.0, 14.0, 17.0, ink::CREAM);
    pen.rect(4.0, 3.0, 14.0, 17.0, pen.c);
    for (i, col) in [ink::BLUE, ink::YELLOW, ink::GREEN].into_iter().enumerate() {
        let y = 4.5 + i as f32 * 4.0;
        pen.frect(14.0, y, 16.5, y + 3.0, col);
    }
    pen.lines(6.0, 12.0, &[7.0, 10.0, 13.0]);
    pen.fill(&[(9.5, 15.5), (12.5, 12.5), (13.0, 16.0)], crate::theme::APP_COLOR);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const WINDOW: [Icon; 4] = [Icon::Minimize, Icon::Maximize, Icon::Restore, Icon::WindowClose];

    /// Every icon draws at every size, light and dark, in colour, mono, flat and disabled, and
    /// into degenerate rects, without panicking.
    #[test]
    fn every_icon_draws() {
        let ctx = egui::Context::default();
        let mut drawn = 0;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            for &icon in Icon::ALL {
                for size in [12.0, 16.0, 20.0, 32.0] {
                    let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(size, size));
                    for ink in [Color32::from_gray(0x3A), Color32::from_gray(0xE8)] {
                        for style in [Style::new(ink, crate::theme::APP_COLOR), Style::mono(ink), Style::flat(ink), Style::disabled(ink)] {
                            paint_styled(painter, r, icon, style);
                        }
                    }
                }
                for r in [Rect::NOTHING, Rect::ZERO, Rect::from_min_size(pos2(0.0, 0.0), vec2(f32::NAN, 16.0)), Rect::EVERYTHING] {
                    paint(painter, r, icon, Color32::BLACK);
                }
                drawn += 1;
            }
            for size in [12.0, 16.0, 18.0, 28.0, 40.0] {
                paint_notebook(painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(size, size)), Color32::RED);
            }
            paint_notebook(painter, Rect::NOTHING, Color32::TRANSPARENT);
            paint_app_mark(painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(18.0, 18.0)));
        });
        out.textures_delta.clear();
        assert_eq!(drawn, Icon::ALL.len());
    }

    /// `Icon::ALL` lists each icon once, and only the window buttons use NoteCraft's own glyphs:
    /// everything else has a real drawing (no fallback square).
    #[test]
    fn every_icon_has_a_drawing() {
        let unique: HashSet<Icon> = Icon::ALL.iter().copied().collect();
        assert_eq!(unique.len(), Icon::ALL.len());
        for &icon in Icon::ALL {
            let own = matches!(drawing(icon, Style::mono(Color32::BLACK)), Drawing::Window);
            assert_eq!(own, WINDOW.contains(&icon), "{icon:?}");
        }
    }

    /// Mono keeps the accents and stays enabled (the old mono drew every fill in the line colour,
    /// so outlined icons became solid blobs); disabled and flat are their own modes.
    #[test]
    fn styles_pick_their_mode() {
        let ink = Color32::WHITE;
        assert_eq!(Style::mono(ink).mode(), Mode::Colour);
        assert_eq!(Style::new(ink, ink).mode(), Mode::Colour);
        assert_eq!(Style::flat(ink).mode(), Mode::Flat);
        assert_eq!(Style::disabled(ink).mode(), Mode::Disabled);
    }
}
