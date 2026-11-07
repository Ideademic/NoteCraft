//! NoteCraft's icons, drawn in code on a 20×20 grid (original artwork; no image assets and
//! nothing from Microsoft). Each icon has line work in the caller's colour and, where it helps,
//! bold fills of its own (a yellow folder, a red eraser, a green check) so the ribbon reads at a
//! glance. A disabled icon is drawn all in one grey.

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
    // Tags
    ToDo,
    Important,
    Question,
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
    Delete,
    Arrange,
    Rotate,
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
    NewPage,
    NewSection,
    NewNotebook,
    SectionGroup,
    Info,
    Open,
    Export,
    Import,
    Options,
    About,
    Close,
    Rename,
    // Window buttons
    Minimize,
    Maximize,
    Restore,
    WindowClose,
}

/// Bold fills icons use besides the line colour (the same in both themes; they sit on light
/// button faces or dark ones and stay readable on either).
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

/// How to colour an icon: its line work, the accent, and whether it is enabled (a disabled icon
/// is drawn in `line` alone).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub line: Color32,
    pub accent: Color32,
    pub enabled: bool,
}

impl Style {
    pub fn new(line: Color32, accent: Color32) -> Style {
        Style { line, accent, enabled: true }
    }
    /// All one colour (title bar glyphs, disabled icons).
    pub fn mono(c: Color32) -> Style {
        Style { line: c, accent: c, enabled: false }
    }
}

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    c: Color32,
    a: Color32,
    /// Fills are drawn (false: everything in `c`).
    colour: bool,
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
    /// A bold fill colour, or the line colour when drawing in one colour.
    fn k(&self, c: Color32) -> Color32 {
        if self.colour { c } else { self.c }
    }
    fn stroke(&self, c: Color32) -> Stroke {
        Stroke::new((self.w * self.s()).max(1.0), c)
    }
    fn line(&self, pts: &[(f32, f32)]) {
        self.line_c(pts, self.c);
    }
    fn line_c(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(v, self.stroke(c)));
    }
    fn thick(&self, pts: &[(f32, f32)], c: Color32, w: f32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::line(v, Stroke::new((w * self.s()).max(1.0), c)));
    }
    fn closed(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::closed_line(v, self.stroke(c)));
    }
    fn fill(&self, pts: &[(f32, f32)], c: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::convex_polygon(v, c, Stroke::NONE));
    }
    fn poly(&self, pts: &[(f32, f32)], fill: Color32, line: Color32) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.pt(x, y)).collect();
        self.p.add(Shape::convex_polygon(v, fill, self.stroke(line)));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_stroke(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 1.0, self.stroke(c), StrokeKind::Middle);
    }
    fn frect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.pt(x0, y0), self.pt(x1, y1)), 1.0, c);
    }
    /// A filled rect with a line border.
    fn box_(&self, x0: f32, y0: f32, x1: f32, y1: f32, fill: Color32) {
        self.frect(x0, y0, x1, y1, self.k(fill));
        self.rect(x0, y0, x1, y1, self.c);
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.pt(x, y), r * self.s(), self.stroke(c));
    }
    fn fcircle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.pt(x, y), r * self.s(), c);
    }
    fn text(&self, x: f32, y: f32, size: f32, t: &str, c: Color32, bold: bool) {
        let font = egui::FontId::proportional(size * self.s());
        let at = self.pt(x, y);
        self.p.text(at, egui::Align2::CENTER_CENTER, t, font.clone(), c);
        if bold {
            self.p.text(at + vec2(0.06 * size * self.s(), 0.0), egui::Align2::CENTER_CENTER, t, font, c);
        }
    }
    /// Horizontal text lines.
    fn lines(&self, x0: f32, x1: f32, ys: &[f32]) {
        for y in ys {
            self.line(&[(x0, *y), (x1, *y)]);
        }
    }
    /// A page with a folded corner, filled white.
    fn page(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.fill(&[(x0, y0), (x1 - 4.0, y0), (x1, y0 + 4.0), (x1, y1), (x0, y1)], self.k(ink::PAPER));
        self.closed(&[(x0, y0), (x1 - 4.0, y0), (x1, y0 + 4.0), (x1, y1), (x0, y1)], self.c);
        self.line(&[(x1 - 4.0, y0), (x1 - 4.0, y0 + 4.0), (x1, y0 + 4.0)]);
    }
    /// An arc of a circle from angle `a0` to `a1` (radians, clockwise from +x since y is down).
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, c: Color32) {
        let n = 16;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.pt(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.p.add(Shape::line(pts, self.stroke(c)));
    }
    fn arrow_head(&self, tip: (f32, f32), dir: (f32, f32), size: f32, c: Color32) {
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(0.001);
        let (dx, dy) = (dir.0 / len, dir.1 / len);
        let (px, py) = (-dy, dx);
        let b = (tip.0 - dx * size, tip.1 - dy * size);
        self.fill(&[tip, (b.0 + px * size * 0.6, b.1 + py * size * 0.6), (b.0 - px * size * 0.6, b.1 - py * size * 0.6)], c);
    }
    fn folder(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let fill = self.k(ink::FOLDER);
        self.poly(&[(x0, y0), (x0 + 5.0, y0), (x0 + 6.5, y0 + 2.0), (x1, y0 + 2.0), (x1, y1), (x0, y1)], fill, self.c);
        self.line(&[(x0, y0 + 4.0), (x1, y0 + 4.0)]);
    }
    /// A pen-like tool drawn along a diagonal from bottom-left (tip) to top-right.
    fn pen_body(&self, body: Color32, tip: Color32) {
        self.poly(&[(14.5, 2.5), (17.5, 5.5), (7.0, 16.0), (4.0, 13.0)], self.k(body), self.c);
        self.poly(&[(4.0, 13.0), (7.0, 16.0), (2.5, 17.5)], self.k(tip), self.c);
    }
}

/// Paints `icon` filling `rect` in `color` (line work), with the default accent.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    paint_styled(painter, rect, icon, Style::new(color, crate::theme::APP_COLOR));
}

/// Paints `icon` filling `rect`.
pub fn paint_styled(painter: &Painter, rect: Rect, icon: Icon, style: Style) {
    let pen = Pen { p: painter, r: rect, c: style.line, a: if style.enabled { style.accent } else { style.line }, colour: style.enabled, w: 1.25 };
    if !clipboard_text(&pen, icon) && !insert_draw(&pen, icon) && !view_nav(&pen, icon) {
        chrome(&pen, icon);
    }
}

fn clipboard_text(pen: &Pen, icon: Icon) -> bool {
    let (c, a) = (pen.c, pen.a);
    match icon {
        Icon::Paste => {
            pen.box_(3.0, 3.5, 14.0, 17.5, ink::FOLDER);
            pen.box_(6.0, 2.0, 11.0, 5.0, ink::GREY);
            pen.page(9.0, 8.0, 17.5, 18.5);
            pen.lines(10.5, 15.0, &[12.0, 14.0, 16.0]);
        }
        Icon::Cut => {
            pen.circle(6.0, 15.0, 2.6, c);
            pen.circle(14.0, 15.0, 2.6, c);
            pen.line_c(&[(7.6, 13.0), (14.0, 2.5)], c);
            pen.line_c(&[(12.4, 13.0), (6.0, 2.5)], c);
            pen.fcircle(10.0, 9.4, 0.9, pen.k(ink::RED));
        }
        Icon::Copy => {
            pen.page(3.0, 2.5, 12.5, 14.0);
            pen.page(7.5, 6.0, 17.0, 17.5);
            pen.lines(9.5, 14.5, &[10.0, 12.5, 15.0]);
        }
        Icon::FormatPainter => {
            pen.box_(3.0, 2.5, 15.0, 7.5, ink::YELLOW);
            pen.line(&[(15.0, 5.0), (17.0, 5.0), (17.0, 10.0), (10.0, 10.0), (10.0, 12.0)]);
            pen.box_(8.5, 12.0, 11.5, 18.0, ink::GREY);
        }
        Icon::Bold => pen.text(10.0, 10.5, 16.0, "B", c, true),
        Icon::Italic => {
            pen.line(&[(8.5, 3.5), (15.0, 3.5)]);
            pen.line(&[(5.0, 16.5), (11.5, 16.5)]);
            pen.line(&[(12.0, 3.5), (8.0, 16.5)]);
        }
        Icon::Underline => {
            pen.line(&[(6.0, 3.0), (6.0, 10.0)]);
            pen.arc(10.0, 10.0, 4.0, 0.0, std::f32::consts::PI, c);
            pen.line(&[(14.0, 3.0), (14.0, 10.0)]);
            pen.line(&[(4.5, 17.0), (15.5, 17.0)]);
        }
        Icon::Strike => {
            pen.text(10.0, 10.0, 13.0, "abc", c, false);
            pen.line_c(&[(2.5, 10.5), (17.5, 10.5)], pen.k(ink::RED));
        }
        Icon::Highlight => {
            pen.poly(&[(12.5, 2.5), (16.5, 6.5), (9.0, 14.0), (5.0, 10.0)], pen.k(ink::PAPER), c);
            pen.poly(&[(5.0, 10.0), (9.0, 14.0), (6.0, 15.5), (3.5, 13.0)], pen.k(ink::GREY), c);
            pen.frect(2.0, 16.5, 18.0, 19.0, pen.k(ink::YELLOW));
        }
        Icon::FontColor => {
            pen.line(&[(4.5, 14.0), (10.0, 2.5), (15.5, 14.0)]);
            pen.line(&[(6.5, 10.0), (13.5, 10.0)]);
            pen.frect(2.0, 16.5, 18.0, 19.0, pen.k(ink::RED));
        }
        Icon::Bullets => {
            for y in [4.5, 10.0, 15.5] {
                pen.fcircle(4.0, y, 1.5, a);
                pen.line(&[(8.0, y), (17.5, y)]);
            }
        }
        Icon::Numbering => {
            for (i, y) in [4.5f32, 10.0, 15.5].into_iter().enumerate() {
                pen.text(4.0, y, 6.5, ["1", "2", "3"].get(i).copied().unwrap_or(""), a, true);
                pen.line(&[(8.0, y), (17.5, y)]);
            }
        }
        Icon::Indent | Icon::Outdent => {
            pen.lines(9.0, 17.5, &[7.5, 12.5]);
            pen.lines(2.5, 17.5, &[3.0, 17.0]);
            if icon == Icon::Indent {
                pen.fill(&[(2.5, 7.0), (6.5, 10.0), (2.5, 13.0)], a);
            } else {
                pen.fill(&[(6.5, 7.0), (2.5, 10.0), (6.5, 13.0)], a);
            }
        }
        Icon::ClearFormat => {
            pen.line(&[(4.0, 14.0), (9.0, 2.5), (14.0, 14.0)]);
            pen.line(&[(6.0, 9.5), (12.0, 9.5)]);
            pen.poly(&[(12.0, 13.0), (16.0, 13.0), (18.5, 16.5), (14.5, 16.5)], pen.k(ink::PINK), c);
            pen.line(&[(12.0, 13.0), (9.5, 16.5), (14.5, 16.5)]);
        }
        Icon::AlignLeft => {
            pen.lines(3.0, 17.0, &[4.0, 12.0]);
            pen.lines(3.0, 12.0, &[8.0, 16.0]);
        }
        Icon::AlignCenter => {
            pen.lines(3.0, 17.0, &[4.0, 12.0]);
            pen.lines(5.5, 14.5, &[8.0, 16.0]);
        }
        Icon::AlignRight => {
            pen.lines(3.0, 17.0, &[4.0, 12.0]);
            pen.lines(8.0, 17.0, &[8.0, 16.0]);
        }
        Icon::ToDo => {
            pen.box_(3.0, 3.0, 17.0, 17.0, ink::PAPER);
            pen.thick(&[(6.0, 10.5), (9.0, 13.5), (14.5, 6.5)], pen.k(ink::GREEN), 2.0);
        }
        Icon::Important => {
            let star: Vec<(f32, f32)> = (0..10)
                .map(|i| {
                    let r = if i % 2 == 0 { 8.0 } else { 3.6 };
                    let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                    (10.0 + r * ang.cos(), 10.5 + r * ang.sin())
                })
                .collect();
            for i in 0..5 {
                let p = |k: usize| star.get(k % 10).copied().unwrap_or((10.0, 10.0));
                pen.fill(&[(10.0, 10.5), p(2 * i + 9), p(2 * i), p(2 * i + 1)], pen.k(ink::YELLOW));
            }
            pen.closed(&star, c);
        }
        Icon::Question => {
            if pen.colour {
                pen.fcircle(10.0, 10.0, 8.0, ink::BLUE);
                pen.text(10.0, 10.5, 12.0, "?", ink::PAPER, true);
            } else {
                pen.circle(10.0, 10.0, 7.5, c);
                pen.text(10.0, 10.5, 12.0, "?", c, true);
            }
        }
        _ => return false,
    }
    true
}

fn insert_draw(pen: &Pen, icon: Icon) -> bool {
    let (c, a) = (pen.c, pen.a);
    match icon {
        Icon::InsertSpace => {
            pen.lines(3.0, 17.0, &[3.0, 17.0]);
            pen.line(&[(10.0, 5.5), (10.0, 14.5)]);
            pen.arrow_head((10.0, 5.0), (0.0, -1.0), 3.0, a);
            pen.arrow_head((10.0, 15.0), (0.0, 1.0), 3.0, a);
        }
        Icon::Table => {
            pen.box_(2.5, 3.5, 17.5, 16.5, ink::PAPER);
            pen.frect(2.5, 3.5, 17.5, 7.0, pen.k(ink::BLUE));
            pen.rect(2.5, 3.5, 17.5, 16.5, c);
            pen.line(&[(2.5, 7.0), (17.5, 7.0)]);
            pen.line(&[(2.5, 11.75), (17.5, 11.75)]);
            pen.line(&[(7.5, 3.5), (7.5, 16.5)]);
            pen.line(&[(12.5, 3.5), (12.5, 16.5)]);
        }
        Icon::Printout => {
            pen.page(4.0, 2.0, 16.0, 18.0);
            pen.frect(6.0, 5.0, 12.0, 8.0, pen.k(ink::RED));
            pen.lines(6.0, 14.0, &[11.0, 13.5, 16.0]);
        }
        Icon::Attach => {
            pen.line(&[(13.0, 6.0), (13.0, 14.5)]);
            pen.arc(10.0, 14.5, 3.0, 0.0, std::f32::consts::PI, c);
            pen.line(&[(7.0, 14.5), (7.0, 4.5)]);
            pen.arc(9.0, 4.5, 2.0, std::f32::consts::PI, 2.0 * std::f32::consts::PI, c);
            pen.line(&[(11.0, 4.5), (11.0, 13.0)]);
        }
        Icon::Picture => {
            pen.box_(2.0, 3.5, 18.0, 16.5, ink::SKY);
            pen.fill(&[(3.0, 15.5), (8.0, 8.5), (12.0, 15.5)], pen.k(ink::GREEN));
            pen.fill(&[(9.0, 15.5), (13.0, 10.5), (17.0, 15.5)], pen.k(Color32::from_rgb(0x1E, 0x7A, 0x3A)));
            pen.fcircle(13.5, 7.0, 1.8, pen.k(ink::YELLOW));
            pen.rect(2.0, 3.5, 18.0, 16.5, c);
        }
        Icon::Date => {
            pen.box_(2.5, 4.0, 17.5, 17.0, ink::PAPER);
            pen.frect(2.5, 4.0, 17.5, 7.5, pen.k(ink::RED));
            pen.rect(2.5, 4.0, 17.5, 17.0, c);
            pen.line(&[(6.0, 2.5), (6.0, 5.5)]);
            pen.line(&[(14.0, 2.5), (14.0, 5.5)]);
            pen.text(10.0, 12.5, 7.5, "12", c, true);
        }
        Icon::Time => {
            pen.fcircle(10.0, 10.0, 7.5, pen.k(ink::PAPER));
            pen.circle(10.0, 10.0, 7.5, c);
            pen.line_c(&[(10.0, 5.0), (10.0, 10.0), (13.5, 12.0)], a);
        }
        Icon::DateTime => {
            pen.box_(2.0, 4.0, 13.5, 15.0, ink::PAPER);
            pen.frect(2.0, 4.0, 13.5, 7.0, pen.k(ink::RED));
            pen.rect(2.0, 4.0, 13.5, 15.0, c);
            pen.fcircle(13.5, 13.5, 5.0, pen.k(ink::PAPER));
            pen.circle(13.5, 13.5, 5.0, c);
            pen.line_c(&[(13.5, 10.5), (13.5, 13.5), (15.5, 14.5)], a);
        }
        Icon::Type => {
            pen.text(7.5, 9.5, 14.0, "T", c, true);
            pen.line_c(&[(15.0, 4.0), (15.0, 17.0)], a);
            pen.line_c(&[(13.5, 4.0), (16.5, 4.0)], a);
            pen.line_c(&[(13.5, 17.0), (16.5, 17.0)], a);
        }
        Icon::Select => {
            pen.poly(&[(5.0, 2.5), (5.0, 15.5), (8.5, 12.0), (12.0, 12.0)], pen.k(ink::PAPER), c);
            pen.line(&[(8.5, 12.0), (11.0, 17.0)]);
        }
        Icon::Lasso => {
            let n = 20;
            let pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    (10.0 + 7.0 * t.cos(), 7.5 + 4.5 * t.sin())
                })
                .collect();
            let dashed: Vec<Pos2> = pts.iter().map(|&(x, y)| pen.pt(x, y)).collect();
            pen.p.extend(Shape::dashed_line(&dashed, pen.stroke(a), 2.0 * pen.s(), 1.5 * pen.s()));
            pen.line(&[(6.0, 11.5), (5.0, 15.0), (7.5, 17.5)]);
            pen.fcircle(7.5, 17.5, 1.2, c);
        }
        Icon::Eraser => {
            pen.poly(&[(11.0, 3.0), (17.5, 9.5), (12.0, 15.0), (5.5, 8.5)], pen.k(ink::PINK), c);
            pen.poly(&[(5.5, 8.5), (12.0, 15.0), (9.5, 17.5), (6.5, 17.5), (3.0, 14.0), (3.0, 11.0)], pen.k(ink::PAPER), c);
            pen.line(&[(9.5, 17.5), (17.5, 17.5)]);
        }
        Icon::Pen => {
            pen.pen_body(a, Color32::from_rgb(0x22, 0x22, 0x26));
        }
        Icon::Pencil => {
            pen.poly(&[(14.5, 2.5), (17.5, 5.5), (7.0, 16.0), (4.0, 13.0)], pen.k(ink::YELLOW), c);
            pen.poly(&[(4.0, 13.0), (7.0, 16.0), (2.5, 17.5)], pen.k(ink::CREAM), c);
            pen.fill(&[(3.2, 15.8), (4.2, 16.8), (2.5, 17.5)], c);
            pen.line(&[(13.0, 4.0), (16.0, 7.0)]);
        }
        Icon::Highlighter => {
            pen.poly(&[(13.5, 2.0), (18.0, 6.5), (10.5, 14.0), (6.0, 9.5)], pen.k(ink::GREY), c);
            pen.poly(&[(6.0, 9.5), (10.5, 14.0), (7.0, 16.0), (4.0, 13.0)], pen.k(ink::YELLOW), c);
            pen.frect(1.5, 17.0, 12.0, 19.0, pen.k(ink::YELLOW));
        }
        Icon::ColorThickness => {
            pen.fcircle(6.0, 6.0, 3.5, pen.k(ink::RED));
            pen.fcircle(14.0, 6.0, 3.5, pen.k(ink::BLUE));
            pen.fcircle(10.0, 12.0, 3.5, pen.k(ink::GREEN));
            pen.thick(&[(2.5, 17.0), (17.5, 17.0)], c, 2.2);
            pen.thick(&[(2.5, 19.0), (17.5, 19.0)], c, 0.8);
        }
        Icon::InkToShape => {
            pen.line_c(&[(2.5, 14.0), (4.0, 9.0), (6.0, 13.0), (8.0, 7.0)], a);
            pen.box_(10.5, 8.0, 17.5, 15.0, ink::SKY);
            pen.arrow_head((10.0, 4.0), (1.0, 0.0), 2.2, c);
            pen.line(&[(4.0, 4.0), (9.5, 4.0)]);
        }
        Icon::InkToText => {
            pen.line_c(&[(2.5, 9.0), (4.0, 5.0), (5.5, 9.0), (7.0, 5.0)], a);
            pen.text(13.5, 13.0, 10.0, "Aa", c, true);
        }
        Icon::Delete => {
            pen.thick(&[(4.5, 4.5), (15.5, 15.5)], pen.k(ink::RED), 2.2);
            pen.thick(&[(15.5, 4.5), (4.5, 15.5)], pen.k(ink::RED), 2.2);
        }
        Icon::Arrange => {
            pen.box_(2.5, 2.5, 11.5, 11.5, ink::SKY);
            pen.box_(8.5, 8.5, 17.5, 17.5, ink::YELLOW);
        }
        Icon::Rotate => {
            pen.arc(10.0, 10.0, 6.5, -2.4, 2.4, c);
            pen.arrow_head((10.0 + 6.5 * (2.4f32).cos(), 10.0 + 6.5 * (2.4f32).sin()), (-0.7, -0.7), 3.0, a);
        }
        _ => return false,
    }
    true
}

fn view_nav(pen: &Pen, icon: Icon) -> bool {
    let (c, a) = (pen.c, pen.a);
    match icon {
        Icon::NormalView => {
            pen.box_(2.0, 3.0, 18.0, 17.0, ink::PAPER);
            pen.frect(2.0, 3.0, 6.0, 17.0, pen.k(a));
            pen.frect(14.0, 3.0, 18.0, 17.0, pen.k(ink::GREY));
            pen.rect(2.0, 3.0, 18.0, 17.0, c);
        }
        Icon::FullPageView => {
            pen.box_(2.0, 3.0, 18.0, 17.0, ink::PAPER);
            for (from, to) in [((6.0, 7.0), (3.5, 4.5)), ((14.0, 7.0), (16.5, 4.5)), ((6.0, 13.0), (3.5, 15.5)), ((14.0, 13.0), (16.5, 15.5))] {
                pen.line_c(&[from, to], a);
            }
        }
        Icon::DockDesktop => {
            pen.box_(2.0, 3.0, 18.0, 15.0, ink::SKY);
            pen.frect(13.0, 3.0, 18.0, 15.0, pen.k(a));
            pen.rect(2.0, 3.0, 18.0, 15.0, c);
            pen.line(&[(7.0, 18.0), (13.0, 18.0)]);
            pen.line(&[(10.0, 15.0), (10.0, 18.0)]);
        }
        Icon::PageColor => {
            pen.page(4.0, 2.0, 16.0, 15.0);
            pen.frect(2.0, 16.5, 7.0, 19.0, pen.k(ink::RED));
            pen.frect(7.5, 16.5, 12.5, 19.0, pen.k(ink::YELLOW));
            pen.frect(13.0, 16.5, 18.0, 19.0, pen.k(ink::BLUE));
        }
        Icon::RuleLines => {
            pen.box_(3.0, 2.0, 17.0, 18.0, ink::PAPER);
            for y in [6.0, 9.5, 13.0, 16.5] {
                pen.line_c(&[(3.0, y), (17.0, y)], pen.k(ink::SKY));
            }
            pen.line_c(&[(6.0, 2.0), (6.0, 18.0)], pen.k(ink::RED));
            pen.rect(3.0, 2.0, 17.0, 18.0, c);
        }
        Icon::HideTitle => {
            pen.page(3.0, 2.0, 17.0, 18.0);
            pen.frect(5.0, 4.5, 12.0, 7.0, pen.k(a));
            pen.lines(5.0, 14.0, &[11.0, 13.5, 16.0]);
            pen.thick(&[(2.0, 2.0), (18.0, 18.0)], pen.k(ink::RED), 1.6);
        }
        Icon::ZoomIn | Icon::ZoomOut | Icon::Search => {
            pen.fcircle(8.5, 8.5, 5.5, pen.k(ink::PAPER));
            pen.circle(8.5, 8.5, 5.5, c);
            pen.thick(&[(12.5, 12.5), (17.5, 17.5)], c, 2.2);
            if icon != Icon::Search {
                pen.line_c(&[(6.0, 8.5), (11.0, 8.5)], a);
            }
            if icon == Icon::ZoomIn {
                pen.line_c(&[(8.5, 6.0), (8.5, 11.0)], a);
            }
        }
        Icon::Zoom100 => {
            pen.box_(2.5, 3.5, 17.5, 16.5, ink::PAPER);
            pen.text(10.0, 10.0, 7.5, "100", c, true);
        }
        Icon::PageWidth => {
            pen.page(5.0, 2.0, 15.0, 18.0);
            pen.line_c(&[(1.5, 10.0), (18.5, 10.0)], a);
            pen.arrow_head((1.0, 10.0), (-1.0, 0.0), 2.5, a);
            pen.arrow_head((19.0, 10.0), (1.0, 0.0), 2.5, a);
        }
        Icon::NewWindow => {
            pen.box_(2.0, 5.0, 14.0, 17.0, ink::PAPER);
            pen.frect(2.0, 5.0, 14.0, 7.5, pen.k(a));
            pen.rect(2.0, 5.0, 14.0, 17.0, c);
            pen.line(&[(6.0, 3.0), (17.5, 3.0), (17.5, 13.0)]);
        }
        Icon::DarkMode => {
            pen.fcircle(10.0, 10.0, 7.0, pen.k(Color32::from_rgb(0x3A, 0x2E, 0x55)));
            pen.fcircle(13.0, 7.5, 5.5, pen.k(ink::YELLOW));
            pen.circle(10.0, 10.0, 7.0, c);
        }
        Icon::NewPage => {
            pen.page(3.5, 2.0, 15.0, 18.0);
            pen.fcircle(15.0, 15.0, 4.0, pen.k(ink::GREEN));
            let w = if pen.colour { ink::PAPER } else { c };
            pen.line_c(&[(13.0, 15.0), (17.0, 15.0)], w);
            pen.line_c(&[(15.0, 13.0), (15.0, 17.0)], w);
        }
        Icon::NewSection => {
            pen.box_(2.0, 6.0, 18.0, 17.0, ink::PAPER);
            pen.box_(3.0, 3.0, 9.0, 6.0, ink::SKY);
            pen.box_(9.5, 3.0, 15.5, 6.0, ink::YELLOW);
        }
        Icon::NewNotebook => {
            notebook_shape(pen, if pen.colour { a } else { c });
        }
        Icon::SectionGroup => {
            pen.box_(5.0, 2.5, 17.0, 13.0, ink::SKY);
            pen.box_(3.0, 5.5, 15.0, 16.0, ink::YELLOW);
            pen.box_(1.5, 8.5, 13.0, 18.0, ink::GREEN);
        }
        Icon::Info => {
            if pen.colour {
                pen.fcircle(10.0, 10.0, 8.0, ink::BLUE);
                pen.text(10.0, 10.5, 12.0, "i", ink::PAPER, true);
            } else {
                pen.circle(10.0, 10.0, 7.5, c);
                pen.text(10.0, 10.5, 12.0, "i", c, true);
            }
        }
        Icon::About => {
            pen.circle(10.0, 10.0, 7.5, c);
            pen.text(10.0, 10.5, 11.0, "i", a, true);
        }
        Icon::Open => {
            pen.folder(2.0, 3.5, 18.0, 16.5);
        }
        Icon::Export => {
            pen.page(2.5, 2.0, 12.5, 18.0);
            pen.line_c(&[(8.0, 10.0), (17.0, 10.0)], a);
            pen.arrow_head((18.5, 10.0), (1.0, 0.0), 3.0, a);
        }
        Icon::Import => {
            pen.page(7.5, 2.0, 17.5, 18.0);
            pen.line_c(&[(2.0, 10.0), (10.5, 10.0)], a);
            pen.arrow_head((12.0, 10.0), (1.0, 0.0), 3.0, a);
        }
        Icon::Options => {
            let teeth = 8;
            for i in 0..teeth {
                let ang = i as f32 / teeth as f32 * std::f32::consts::TAU;
                pen.thick(&[(10.0 + 5.0 * ang.cos(), 10.0 + 5.0 * ang.sin()), (10.0 + 8.0 * ang.cos(), 10.0 + 8.0 * ang.sin())], c, 2.4);
            }
            pen.fcircle(10.0, 10.0, 5.6, c);
            pen.fcircle(10.0, 10.0, 2.4, pen.k(ink::PAPER));
        }
        Icon::Rename => {
            pen.rect(2.0, 6.0, 15.0, 14.0, c);
            pen.line_c(&[(5.0, 8.5), (5.0, 11.5)], a);
            pen.line(&[(17.0, 3.0), (17.0, 17.0)]);
            pen.line(&[(15.5, 3.0), (18.5, 3.0)]);
            pen.line(&[(15.5, 17.0), (18.5, 17.0)]);
        }
        _ => return false,
    }
    true
}

fn chrome(pen: &Pen, icon: Icon) {
    let (c, a) = (pen.c, pen.a);
    match icon {
        Icon::Back => {
            pen.fcircle(10.0, 10.0, 8.0, Color32::TRANSPARENT);
            pen.circle(10.0, 10.0, 8.0, c);
            pen.line(&[(14.0, 10.0), (6.0, 10.0)]);
            pen.line(&[(9.5, 6.5), (6.0, 10.0), (9.5, 13.5)]);
        }
        Icon::Undo | Icon::Redo => {
            let flip = |x: f32| if icon == Icon::Undo { x } else { 20.0 - x };
            let pts: Vec<(f32, f32)> = (0..=12)
                .map(|i| {
                    let t = -std::f32::consts::FRAC_PI_2 + i as f32 / 12.0 * std::f32::consts::PI;
                    (flip(10.5 + 5.5 * t.cos()), 10.5 + 5.0 * t.sin())
                })
                .collect();
            pen.line(&pts);
            pen.line(&[(flip(10.5), 15.5), (flip(6.0), 15.5)]);
            pen.fill(&[(flip(3.0), 5.5), (flip(8.0), 2.0), (flip(8.0), 9.0)], c);
        }
        Icon::More => {
            pen.line(&[(5.0, 6.0), (15.0, 6.0)]);
            pen.fill(&[(5.5, 10.0), (14.5, 10.0), (10.0, 15.0)], c);
        }
        Icon::Dropdown | Icon::ChevronDown => pen.line(&[(5.0, 7.5), (10.0, 12.5), (15.0, 7.5)]),
        Icon::ChevronUp => pen.line(&[(5.0, 12.5), (10.0, 7.5), (15.0, 12.5)]),
        Icon::ChevronLeft => pen.line(&[(12.5, 5.0), (7.5, 10.0), (12.5, 15.0)]),
        Icon::ChevronRight => pen.line(&[(7.5, 5.0), (12.5, 10.0), (7.5, 15.0)]),
        Icon::Plus => {
            pen.thick(&[(10.0, 4.0), (10.0, 16.0)], c, 1.6);
            pen.thick(&[(4.0, 10.0), (16.0, 10.0)], c, 1.6);
        }
        Icon::Pin => {
            pen.poly(&[(7.0, 3.0), (13.0, 3.0), (12.0, 9.0), (8.0, 9.0)], pen.k(a), c);
            pen.line(&[(5.0, 11.0), (15.0, 11.0)]);
            pen.line(&[(6.0, 9.0), (14.0, 9.0), (15.0, 11.0)]);
            pen.line(&[(10.0, 11.0), (10.0, 18.0)]);
        }
        Icon::Close => {
            pen.line(&[(5.0, 5.0), (15.0, 15.0)]);
            pen.line(&[(15.0, 5.0), (5.0, 15.0)]);
        }
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
        _ => {
            // Anything not drawn above: a framed square (never reached for listed icons).
            let _ = a;
            pen.rect(4.0, 4.0, 16.0, 16.0, c);
        }
    }
}

/// A notebook: a coloured cover with a binding strip and a label, in `color`.
fn notebook_shape(pen: &Pen, color: Color32) {
    pen.frect(4.0, 2.0, 16.5, 18.0, color);
    let edge = crate::theme::mix(color, Color32::BLACK, 0.35);
    pen.frect(4.0, 2.0, 6.5, 18.0, edge);
    pen.frect(8.5, 5.0, 14.5, 8.0, pen.k(ink::PAPER));
    pen.frect(16.5, 3.5, 17.5, 16.5, pen.k(ink::PAPER));
}

/// A notebook icon in the notebook's own colour (the notebook pane).
pub fn paint_notebook(painter: &Painter, rect: Rect, color: Color32) {
    let pen = Pen { p: painter, r: rect, c: color, a: color, colour: true, w: 1.25 };
    notebook_shape(&pen, crate::theme::opaque(color));
}

/// NoteCraft's own mark, drawn in code for the title bar: a cream page with section tabs on its
/// edge and a pen nib, in the house style of the app icon.
pub fn paint_app_mark(painter: &Painter, rect: Rect) {
    let pen = Pen { p: painter, r: rect, c: Color32::from_rgb(0x0B, 0x0B, 0x0C), a: crate::theme::APP_COLOR, colour: true, w: 1.1 };
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

    const ALL: &[Icon] = &[
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
        Icon::ToDo,
        Icon::Important,
        Icon::Question,
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
        Icon::Delete,
        Icon::Arrange,
        Icon::Rotate,
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
        Icon::NewPage,
        Icon::NewSection,
        Icon::NewNotebook,
        Icon::SectionGroup,
        Icon::Info,
        Icon::Open,
        Icon::Export,
        Icon::Import,
        Icon::Options,
        Icon::About,
        Icon::Close,
        Icon::Rename,
        Icon::Minimize,
        Icon::Maximize,
        Icon::Restore,
        Icon::WindowClose,
    ];

    /// Every icon draws at every size, in colour and greyed out, without panicking.
    #[test]
    fn every_icon_draws() {
        let ctx = egui::Context::default();
        let mut drawn = 0;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            for &icon in ALL {
                for size in [10.0, 16.0, 20.0, 32.0] {
                    let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(size, size));
                    paint_styled(painter, r, icon, Style::new(Color32::BLACK, crate::theme::APP_COLOR));
                    paint_styled(painter, r, icon, Style::mono(Color32::GRAY));
                }
                drawn += 1;
            }
            paint_notebook(painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(16.0, 16.0)), Color32::RED);
            paint_app_mark(painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(18.0, 18.0)));
        });
        out.textures_delta.clear();
        assert_eq!(drawn, ALL.len());
    }
}
