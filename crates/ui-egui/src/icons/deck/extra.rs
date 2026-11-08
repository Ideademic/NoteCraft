//! NoteCraft's own icons, drawn in DeckCraft's style with its helpers (24×24 grid, ink line work,
//! fixed accents, tinted fills). Original NoteCraft artwork, `MIT OR Apache-2.0`. Zoom 100%, Page
//! Width, Dark Mode, Thickness, the drop-down arrow and the To Do check box follow WordCraft's
//! designs (storytold/wordcraft, `MIT OR Apache-2.0`), redrawn on this grid.
//!
//! Where a badge sits on another shape, that shape leaves a gap for it, so nothing shows through
//! when the badge is drawn as an outline (disabled or flat).

use egui::Color32;

use super::geometry::{B, C, G, Glyph, K, N, P2, PU, Pen, R, W, Y, arc_pts, t, tf};
use super::{Mode, pen};

/// Icons NoteCraft needs that DeckCraft doesn't draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Extra {
    /// A notebook with its cover in this colour.
    Notebook(Color32),
    /// A new notebook with its cover in this colour.
    NewNotebook(Color32),
    NewPage,
    SectionGroup,
    Import,
    Attach,
    InsertSpace,
    RuleLines,
    DockDesktop,
    Rename,
    Important,
    Question,
    Zoom100,
    PageWidth,
    DarkMode,
    Thickness,
    Dropdown,
    Checkbox,
    Select,
    Delete,
    DateTime,
    Arrange,
    /// A highlight tag: lines of text on a band of this colour (with a marker tip when `true`).
    Marked(Color32, bool),
    /// The Contact tag: a card with a person.
    Contact,
    /// The Address tag: a house.
    Address,
    /// The Phone number tag: a mobile phone.
    Phone,
    /// The Idea tag: a light bulb.
    Idea,
    /// The Critical tag: an exclamation mark in a red disc.
    Critical,
    /// A waste bin with a circular arrow: the notebook recycle bin.
    RecycleBin,
    /// Two pages stacked with a clock badge: page versions.
    PageVersions,
    /// A page with a blue dot: unread.
    Unread,
    /// A page with a green check: mark as read.
    MarkRead,
    /// A page with a pencil and a clock: recent edits.
    RecentEdits,
    /// A clock with a back arrow round it: the History group.
    History,
}

impl Extra {
    /// Every drawing (the notebook in NoteCraft's purple).
    pub const ALL: &'static [Extra] = &[
        Extra::Notebook(super::PURPLE),
        Extra::NewNotebook(crate::theme::APP_COLOR),
        Extra::NewPage,
        Extra::SectionGroup,
        Extra::Import,
        Extra::Attach,
        Extra::InsertSpace,
        Extra::RuleLines,
        Extra::DockDesktop,
        Extra::Rename,
        Extra::Important,
        Extra::Question,
        Extra::Zoom100,
        Extra::PageWidth,
        Extra::DarkMode,
        Extra::Thickness,
        Extra::Dropdown,
        Extra::Checkbox,
        Extra::Select,
        Extra::Delete,
        Extra::DateTime,
        Extra::Arrange,
        Extra::Marked(Color32::from_rgb(0xFF, 0xF2, 0x00), false),
        Extra::Marked(Color32::from_rgb(0xFF, 0xF2, 0x00), true),
        Extra::Contact,
        Extra::Address,
        Extra::Phone,
        Extra::Idea,
        Extra::Critical,
        Extra::RecycleBin,
        Extra::PageVersions,
        Extra::Unread,
        Extra::MarkRead,
        Extra::RecentEdits,
        Extra::History,
    ];
}

/// Paints `icon` into `rect` like [`super::paint_mode`].
pub fn paint_mode(painter: &egui::Painter, rect: egui::Rect, icon: Extra, ink: Color32, mode: Mode) {
    if let Some(pen) = pen(painter, rect, ink, mode) {
        draw(&pen, icon);
    }
}

fn draw(p: &Pen<'_>, icon: Extra) {
    let _ = draw_files(p, icon) || draw_tags_view(p, icon) || draw_more_tags(p, icon) || draw_history(p, icon);
}

/// The tags beyond To Do, Important and Question (original NoteCraft drawings).
fn draw_more_tags(p: &Pen<'_>, icon: Extra) -> bool {
    match icon {
        Extra::Marked(color, marker) => {
            let band = super::geometry::C::Acc(color, 1.0);
            p.rect((2.5, 8.0, 21.5, 17.0), 1.0, band, N);
            for (x0, x1) in [(5.0, 10.5), (12.5, 19.0)] {
                p.seg_w((x0, 12.5), (x1, 12.5), K, 1.3);
            }
            if marker {
                // A marker's chisel tip over the band's right end.
                p.poly(&[(15.5, 2.5), (21.0, 2.5), (21.0, 6.5), (18.5, 8.5), (15.5, 6.5)], band, K);
            }
        }
        Extra::Contact => {
            p.rect((2.5, 5.0, 21.5, 19.0), 1.6, N, K);
            p.person(8.0, 7.0, 0.72, B);
            p.seg((13.0, 10.0), (19.0, 10.0), K);
            p.seg((13.0, 14.0), (17.5, 14.0), K);
        }
        Extra::Address => {
            p.rect((5.5, 10.5, 18.5, 20.5), 0.6, t(B), K);
            p.line(&[(2.5, 12.0), (12.0, 3.5), (21.5, 12.0)], K);
            p.rect((10.3, 14.5, 13.7, 20.5), 0.4, B, N);
        }
        Extra::Phone => {
            p.rect((7.0, 2.5, 17.0, 21.5), 2.2, N, K);
            p.rect((8.8, 5.0, 15.2, 17.0), 0.5, t(B), N);
            p.dot((12.0, 19.3), 0.9, K);
        }
        Extra::Idea => {
            p.circle((12.0, 9.5), 6.2, t(Y), K);
            p.rect((9.5, 15.5, 14.5, 19.0), 0.8, N, K);
            p.seg((10.3, 20.8), (13.7, 20.8), K);
            for (a, b) in [((12.0, 1.0), (12.0, 2.2)), ((4.0, 4.0), (5.0, 5.0)), ((20.0, 4.0), (19.0, 5.0))] {
                p.seg(a, b, Y);
            }
        }
        Extra::Critical => {
            let gc = if p.dis {
                p.circle((12.0, 12.0), 9.8, N, R);
                K
            } else {
                p.circle((12.0, 12.0), 9.8, R, N);
                W
            };
            p.seg_w((12.0, 6.0), (12.0, 13.8), gc, 1.6);
            p.dot((12.0, 17.6), 1.4, gc);
        }
        _ => return false,
    }
    true
}


/// A notebook: a cover in `cover` with a darker spine, a label and the page edges. In ink alone
/// (flat or disabled) it is an outline, so it doesn't turn into a solid block.
fn notebook(p: &Pen<'_>, (x0, y0, x1, y1): (f32, f32, f32, f32), cover: Color32) {
    let w = x1 - x0;
    let spine_x = x0 + w * 0.24;
    let label = (x0 + w * 0.42, y0 + 3.0, x1 - w * 0.14, y0 + 6.5);
    if p.dis {
        p.rline(&[(x1, y0 + 1.5), (x1 + 1.0, y0 + 1.5), (x1 + 1.0, y1 - 1.5), (x1, y1 - 1.5)], 0.5, K.a(0.6));
        p.rect((x0, y0, x1, y1), 1.4, N, K);
        p.seg((spine_x, y0), (spine_x, y1), K);
        p.rect(label, 0.5, N, K);
        return;
    }
    let spine = crate::theme::mix(cover, Color32::BLACK, 0.3);
    // Page edges peeking out on the right.
    p.rect((x1 - 2.0, y0 + 1.5, x1 + 1.0, y1 - 1.5), 0.6, W, K.a(0.6));
    p.rect((x0, y0, x1, y1), 1.4, C::Acc(cover, 1.0), N);
    p.rect((x0, y0, spine_x, y1), 1.4, C::Acc(spine, 1.0), N);
    p.rect(label, 0.5, W, N);
}

fn draw_files(p: &Pen<'_>, icon: Extra) -> bool {
    match icon {
        Extra::Notebook(c) => notebook(p, (4.0, 2.0, 19.0, 22.0), c),
        Extra::NewNotebook(cover) => {
            // The cover stops short of the badge's corner.
            let outline = [(3.0, 2.0), (17.0, 2.0), (17.0, 12.0), (12.0, 12.0), (12.0, 21.0), (3.0, 21.0)];
            if p.dis {
                p.rline(&[(17.0, 3.5), (18.5, 3.5), (18.5, 11.0)], 0.5, K.a(0.6));
                p.rshape(&outline, 1.4, N, K);
                p.seg((6.5, 2.0), (6.5, 21.0), K);
                p.rect((8.5, 5.0, 14.5, 8.5), 0.5, N, K);
            } else {
                let spine = crate::theme::mix(cover, Color32::BLACK, 0.3);
                p.rect((15.5, 3.5, 18.5, 11.5), 0.6, W, K.a(0.6));
                p.rshape(&outline, 1.4, C::Acc(cover, 1.0), N);
                p.rect((3.0, 2.0, 6.5, 21.0), 1.4, C::Acc(spine, 1.0), N);
                p.rect((8.5, 5.0, 14.5, 8.5), 0.5, W, N);
            }
            p.badge((18.0, 18.0), 5.0, G, Glyph::Plus);
        }
        Extra::NewPage => {
            p.rline(&[(11.5, 21.5), (5.0, 21.5), (5.0, 2.5), (14.0, 2.5), (19.0, 7.5), (19.0, 11.5)], 0.8, K);
            p.rline(&[(14.0, 2.5), (14.0, 7.5), (19.0, 7.5)], 0.8, K);
            p.seg_w((8.0, 11.0), (14.5, 11.0), K.a(0.5), 0.85);
            p.seg_w((8.0, 14.5), (11.0, 14.5), K.a(0.5), 0.85);
            p.badge((18.0, 18.0), 5.0, G, Glyph::Plus);
        }
        Extra::SectionGroup => {
            // Three section tabs on a bracket.
            p.rline(&[(4.0, 3.0), (4.0, 21.0)], 0.0, K);
            for (y, c, len) in [(3.5, PU, 15.0), (10.0, B, 13.0), (16.5, G, 11.0)] {
                let x0 = 6.5;
                let x1 = x0 + len;
                p.rshape(&[(x0, y), (x1 - 3.0, y), (x1, y + 2.25), (x1 - 3.0, y + 4.5), (x0, y + 4.5)], 0.6, c, N);
                p.seg_w((4.0, y + 2.25), (x0, y + 2.25), K, 0.9);
            }
        }
        Extra::Import => {
            p.rline(&[(9.0, 8.5), (9.0, 2.5), (21.0, 2.5), (21.0, 21.5), (9.0, 21.5), (9.0, 15.5)], 1.5, K);
            p.arrow((2.0, 12.0), (15.5, 12.0), B);
        }
        Extra::Attach => {
            // A paper clip: right leg, wide bottom loop, left leg, narrow top loop, inner leg.
            let mut pts = vec![(3.5, -3.5)];
            pts.extend(arc_pts((0.0, 5.0), 3.5, 3.5, 0.0, 180.0));
            pts.extend(arc_pts((-1.75, -6.0), 1.75, 1.75, 180.0, 360.0));
            pts.push((0.0, 3.5));
            p.line_w(&tf((12.0, 12.0), 40.0, &pts), K, 1.1);
        }
        Extra::InsertSpace => {
            p.seg((3.0, 3.0), (21.0, 3.0), K);
            p.seg((3.0, 21.0), (21.0, 21.0), K);
            p.seg_w((5.0, 7.0), (11.0, 7.0), K.a(0.5), 0.85);
            p.seg_w((5.0, 17.0), (11.0, 17.0), K.a(0.5), 0.85);
            p.arrow2((16.0, 5.5), (16.0, 18.5), B);
        }
        Extra::RuleLines => {
            for y in [8.0, 12.0, 16.0] {
                p.seg_w((3.5, y), (20.5, y), B, 0.8);
            }
            p.seg_w((7.5, 2.5), (7.5, 21.5), R, 0.8);
            p.rect((2.5, 2.5, 21.5, 21.5), 1.4, N, K);
        }
        Extra::DockDesktop => {
            p.rect((14.5, 3.0, 22.0, 16.5), 0.0, t(PU), N);
            p.seg_w((16.5, 6.5), (20.0, 6.5), PU, 0.8);
            p.seg_w((16.5, 9.5), (20.0, 9.5), PU, 0.8);
            p.seg((14.5, 3.0), (14.5, 16.5), K);
            p.monitor(2.0, 3.0, 22.0, 16.5);
        }
        Extra::Rename => {
            p.rect((2.0, 7.0, 15.0, 17.0), 1.2, N, K);
            p.seg_w((5.0, 12.0), (11.0, 12.0), K.a(0.5), 0.9);
            p.seg((19.0, 4.0), (19.0, 20.0), B);
            p.seg((16.5, 4.0), (21.5, 4.0), B);
            p.seg((16.5, 20.0), (21.5, 20.0), B);
        }
        Extra::DateTime => {
            // A calendar with a clock badge in its corner (the calendar leaves room for it).
            p.rect((3.0, 5.0, 17.5, 9.0), 1.4, R, N);
            p.rline(&[(11.0, 20.0), (3.0, 20.0), (3.0, 5.0), (17.5, 5.0), (17.5, 11.0)], 1.4, K);
            p.seg((7.0, 3.0), (7.0, 6.5), K);
            p.seg((13.5, 3.0), (13.5, 6.5), K);
            for (x, y) in [(6.5, 12.5), (10.0, 12.5), (6.5, 16.0)] {
                p.dot((x, y), 1.0, K);
            }
            p.clock_badge((17.0, 17.0), 5.5, B);
        }
        _ => return false,
    }
    true
}

fn draw_tags_view(p: &Pen<'_>, icon: Extra) -> bool {
    match icon {
        Extra::Important => p.star((12.0, 12.8), 10.0, Y, N),
        Extra::Question => {
            let gc = if p.dis {
                p.circle((12.0, 12.0), 9.8, N, PU);
                K
            } else {
                p.circle((12.0, 12.0), 9.8, PU, N);
                W
            };
            let mut q = arc_pts((12.0, 9.6), 3.2, 3.0, 180.0, 400.0);
            q.push((12.0, 13.2));
            q.push((12.0, 14.6));
            p.line_w(&q, gc, 1.1);
            p.dot((12.0, 17.8), 1.3, gc);
        }
        Extra::Zoom100 => {
            // A magnifier reading 1:1 (path-drawn, so it holds up at 16 px).
            p.magnifier((10.0, 10.0), 7.5, N, K);
            p.digit(1, 4.6, 6.8, 6.4, B);
            p.dot((10.0, 8.6), 0.75, B);
            p.dot((10.0, 11.6), 0.75, B);
            p.digit(1, 11.2, 6.8, 6.4, B);
        }
        Extra::PageWidth => {
            p.page(3.0, 2.0, 21.0, 22.0, N, K);
            p.seg_w((6.5, 8.0), (13.0, 8.0), K.a(0.5), 0.85);
            p.arrow2((6.0, 14.0), (18.0, 14.0), B);
        }
        Extra::DarkMode => {
            p.shape(&crescent((11.0, 13.0), 8.5, (16.0, 8.0), 7.0), t(Y), K);
            p.sparkle((18.5, 5.0), 3.2, Y);
        }
        Extra::Thickness => {
            p.seg_w((4.0, 5.0), (20.0, 5.0), K, 0.7);
            p.seg_w((4.0, 11.0), (20.0, 11.0), K, 1.4);
            p.seg_w((4.0, 18.5), (20.0, 18.5), B, 2.8);
        }
        Extra::Dropdown => p.poly(&[(5.5, 8.5), (18.5, 8.5), (12.0, 16.0)], K, N),
        Extra::Checkbox => {
            p.rect((3.5, 3.5, 20.5, 20.5), 1.8, N, K);
            p.line_w(&[(7.5, 12.5), (10.8, 15.8), (16.5, 8.5)], G, 1.3);
        }
        Extra::Select => {
            p.dashed(&[(2.5, 2.5), (15.5, 2.5), (15.5, 7.0)], B, 2.2, 1.6);
            p.dashed(&[(2.5, 2.5), (2.5, 14.5), (7.0, 14.5)], B, 2.2, 1.6);
            p.cursor(9.5, 6.0, 0.95);
        }
        Extra::Delete => {
            p.seg_w((5.5, 5.5), (18.5, 18.5), R, 1.4);
            p.seg_w((18.5, 5.5), (5.5, 18.5), R, 1.4);
        }
        Extra::Arrange => {
            // Three layers, front one in colour, each cut where the one in front sits.
            p.rline(&[(11.5, 6.5), (11.5, 2.5), (2.5, 2.5), (2.5, 11.5), (6.5, 11.5)], 1.0, K);
            p.rline(&[(16.5, 11.5), (16.5, 7.5), (7.5, 7.5), (7.5, 16.5), (11.5, 16.5)], 1.0, B);
            p.rect((12.5, 12.5, 21.5, 21.5), 1.0, B, B);
        }
        _ => return false,
    }
    true
}

/// The History tab's icons (original NoteCraft drawings).
fn draw_history(p: &Pen<'_>, icon: Extra) -> bool {
    match icon {
        Extra::RecycleBin => {
            // Lid and handle, a tapered bin with ribs, a green circular arrow on its front.
            p.seg((3.5, 5.5), (20.5, 5.5), K);
            p.rline(&[(9.0, 5.5), (9.5, 3.0), (14.5, 3.0), (15.0, 5.5)], 0.6, K);
            p.rshape(&[(5.0, 5.5), (19.0, 5.5), (17.5, 21.0), (6.5, 21.0)], 1.0, t(B), K);
            let mut arc = arc_pts((12.0, 13.5), 3.6, 3.6, 200.0, 470.0);
            if let Some(&(x, y)) = arc.last() {
                arc.push((x + 1.6, y - 0.6));
            }
            p.line_w(&arc, G, 1.0);
            p.poly(&[(7.6, 11.4), (10.4, 11.0), (8.6, 13.4)], G, N);
        }
        Extra::PageVersions => {
            // An older page behind, the current one in front, a clock badge.
            p.rline(&[(7.0, 4.5), (7.0, 2.0), (17.0, 2.0), (17.0, 4.5)], 0.6, K.a(0.6));
            p.rline(&[(4.5, 7.0), (4.5, 4.5), (19.5, 4.5), (19.5, 7.0)], 0.6, K.a(0.8));
            p.rline(&[(12.0, 22.0), (3.0, 22.0), (3.0, 7.0), (21.0, 7.0), (21.0, 13.0)], 1.0, K);
            p.seg_w((6.0, 11.0), (17.0, 11.0), K.a(0.5), 0.85);
            p.seg_w((6.0, 14.5), (11.0, 14.5), K.a(0.5), 0.85);
            p.clock_badge((17.5, 18.0), 5.0, B);
        }
        Extra::Unread => {
            p.page(4.0, 2.0, 18.5, 22.0, N, K);
            for y in [9.0, 12.5, 16.0] {
                p.seg_w((7.0, y), (13.0, y), K.a(0.5), 0.85);
            }
            p.circle((18.0, 17.5), 4.2, B, N);
        }
        Extra::MarkRead => {
            p.page(4.0, 2.0, 18.5, 22.0, N, K);
            for y in [9.0, 12.5] {
                p.seg_w((7.0, y), (13.0, y), K.a(0.5), 0.85);
            }
            p.badge((18.0, 18.0), 5.0, G, Glyph::Check);
        }
        Extra::RecentEdits => {
            p.page(3.0, 2.0, 17.5, 22.0, N, K);
            p.seg_w((6.0, 8.5), (12.5, 8.5), K.a(0.5), 0.85);
            p.seg_w((6.0, 12.0), (10.0, 12.0), K.a(0.5), 0.85);
            p.clock_badge((17.0, 17.0), 5.5, Y);
        }
        Extra::History => {
            let mut arc = arc_pts((12.0, 12.0), 8.5, 8.5, 160.0, 470.0);
            arc.reverse();
            p.line_w(&arc, K, 1.0);
            p.poly(&[(1.8, 11.2), (6.4, 11.2), (4.1, 14.4)], K, N);
            p.line_w(&[(12.0, 7.0), (12.0, 12.0), (15.5, 14.0)], B, 1.0);
        }
        _ => return false,
    }
    true
}

/// The crescent left of circle `(c1, r1)` once circle `(c2, r2)` is cut away from it.
fn crescent(c1: P2, r1: f32, c2: P2, r2: f32) -> Vec<P2> {
    let (dx, dy) = (c2.0 - c1.0, c2.1 - c1.1);
    let d = (dx * dx + dy * dy).sqrt().max(1e-3);
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let (ux, uy) = (dx / d, dy / d);
    let m = (c1.0 + a * ux, c1.1 + a * uy);
    let p1 = (m.0 - h * uy, m.1 + h * ux);
    let p2 = (m.0 + h * uy, m.1 - h * ux);
    let ang = |c: P2, q: P2| (q.1 - c.1).atan2(q.0 - c.0).to_degrees();
    // Outer arc on circle 1 the long way round, away from circle 2.
    let (a1, mut a2) = (ang(c1, p1), ang(c1, p2));
    if a2 < a1 {
        a2 += 360.0;
    }
    let mut pts = arc_pts(c1, r1, r1, a1, a2);
    // Inner arc on circle 2 back from p2 to p1 inside circle 1.
    let (b2, mut b1) = (ang(c2, p2), ang(c2, p1));
    if b1 > b2 {
        b1 -= 360.0;
    }
    let mut inner = arc_pts(c2, r2, r2, b2, b1);
    inner.pop();
    if !inner.is_empty() {
        inner.remove(0);
    }
    pts.extend(inner);
    pts
}
