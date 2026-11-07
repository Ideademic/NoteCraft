//! NoteCraft geometry and units.
//!
//! The model ([`notecraft-doc`]) keeps every position in **points** (1/72 inch, `f32`) measured
//! from the page's top-left corner, y growing downwards. File formats use other units, converted
//! here:
//!
//! - **Half-inches**: MS-ONE stores outline and picture offsets and widths in half-inch
//!   increments (1 half-inch = 36 pt).
//! - **Ink units**: ink coordinates are in 0.01 mm (HIMETRIC), 1 unit = 72 / 2540 pt.
//!
//! On screen, page points become pixels through a [`zoom`] factor (1.0 = 100 %, where one point
//! is drawn as one logical pixel, as egui counts them).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod units;
pub mod zoom;

use serde::{Deserialize, Serialize};

pub use units::{
    PT_PER_HALF_INCH, PT_PER_INCH, PT_PER_INK_UNIT, PT_PER_MM, half_inch_to_pt, inch_to_pt, ink_to_pt, mm_to_pt, pt_to_half_inch, pt_to_inch,
    pt_to_ink, pt_to_mm, pt_to_px, px_to_pt,
};

/// Largest coordinate the model keeps, in points (about 350 m): hostile files and commands are
/// clamped to it so later arithmetic stays finite.
pub const MAX_COORD: f32 = 1.0e6;

/// A finite value clamped to ±[`MAX_COORD`]; NaN and infinities become `0`.
pub fn sane(v: f32) -> f32 {
    if v.is_finite() { v.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 }
}

/// A position in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }

    /// Both coordinates finite and within ±[`MAX_COORD`].
    pub fn sanitized(self) -> Self {
        Point { x: sane(self.x), y: sane(self.y) }
    }

    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Point { x: self.x + dx, y: self.y + dy }
    }

    pub fn distance(self, other: Point) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// A width and height in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const ZERO: Size = Size { w: 0.0, h: 0.0 };

    pub const fn new(w: f32, h: f32) -> Self {
        Size { w, h }
    }

    /// Finite, non-negative and within [`MAX_COORD`].
    pub fn sanitized(self) -> Self {
        Size { w: sane(self.w).max(0.0), h: sane(self.h).max(0.0) }
    }

    /// The largest size with this one's aspect ratio that fits in `bound` (never larger than
    /// itself). A zero-sized input stays zero.
    pub fn fit_within(self, bound: Size) -> Size {
        if self.w <= 0.0 || self.h <= 0.0 {
            return Size::ZERO;
        }
        let s = (bound.w / self.w).min(bound.h / self.h).min(1.0);
        if s.is_finite() && s > 0.0 { Size { w: self.w * s, h: self.h * s } } else { Size::ZERO }
    }
}

/// An axis-aligned rectangle in points: top-left corner plus size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    pub fn from_min_size(min: Point, size: Size) -> Self {
        Rect { x: min.x, y: min.y, w: size.w, h: size.h }
    }

    /// The rectangle spanning two corners, in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        let (x0, x1) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
        let (y0, y1) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
        Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    /// The bounding box of `points`; `None` when there are none.
    pub fn bounding(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut it = points.into_iter();
        let first = it.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in it {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
    }

    pub fn min(self) -> Point {
        Point { x: self.x, y: self.y }
    }
    pub fn max(self) -> Point {
        Point { x: self.right(), y: self.bottom() }
    }
    pub fn size(self) -> Size {
        Size { w: self.w, h: self.h }
    }
    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn center(self) -> Point {
        Point { x: self.x + self.w / 2.0, y: self.y + self.h / 2.0 }
    }

    /// Inclusive of the edges.
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// Overlap, touching edges included.
    pub fn intersects(self, o: Rect) -> bool {
        self.x <= o.right() && o.x <= self.right() && self.y <= o.bottom() && o.y <= self.bottom()
    }

    pub fn union(self, o: Rect) -> Rect {
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        Rect { x: x0, y: y0, w: self.right().max(o.right()) - x0, h: self.bottom().max(o.bottom()) - y0 }
    }

    /// Grown by `d` on every side (shrunk for negative `d`, never below zero size).
    pub fn expand(self, d: f32) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: (self.w + 2.0 * d).max(0.0), h: (self.h + 2.0 * d).max(0.0) }
    }

    pub fn translate(self, dx: f32, dy: f32) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, ..self }
    }
}

/// Distance from `p` to the segment `a`–`b`.
pub fn segment_distance(a: Point, b: Point, p: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(Point { x: a.x + t * dx, y: a.y + t * dy })
}

/// Whether `p` lies inside the closed polygon `poly` (even-odd rule). Fewer than three vertices
/// enclose nothing. Used by the lasso.
pub fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for (i, a) in poly.iter().enumerate() {
        if let Some(b) = poly.get(j)
            && ((a.y > p.y) != (b.y > p.y))
        {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_basics() {
        let r = Rect::from_corners(Point::new(10.0, 20.0), Point::new(0.0, 0.0));
        assert_eq!(r, Rect::new(0.0, 0.0, 10.0, 20.0));
        assert!(r.contains(Point::new(10.0, 20.0)));
        assert!(!r.contains(Point::new(10.1, 5.0)));
        assert_eq!(r.center(), Point::new(5.0, 10.0));
        assert!(r.intersects(Rect::new(10.0, 20.0, 5.0, 5.0)));
        assert!(!r.intersects(Rect::new(11.0, 0.0, 5.0, 5.0)));
        assert_eq!(r.union(Rect::new(20.0, -5.0, 1.0, 1.0)), Rect::new(0.0, -5.0, 21.0, 25.0));
        assert_eq!(r.expand(-100.0).size(), Size::ZERO);
        assert_eq!(r.translate(1.0, 2.0).min(), Point::new(1.0, 2.0));
    }

    #[test]
    fn bounding_box() {
        assert_eq!(Rect::bounding([]), None);
        let b = Rect::bounding([Point::new(3.0, 1.0), Point::new(-1.0, 4.0), Point::new(2.0, 2.0)]);
        assert_eq!(b, Some(Rect::new(-1.0, 1.0, 4.0, 3.0)));
    }

    #[test]
    fn hostile_numbers_are_sanitized() {
        assert_eq!(Point::new(f32::NAN, f32::INFINITY).sanitized(), Point::ZERO);
        assert_eq!(Point::new(1e30, -1e30).sanitized(), Point::new(MAX_COORD, -MAX_COORD));
        assert_eq!(Size::new(-5.0, f32::NAN).sanitized(), Size::ZERO);
    }

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(Size::new(200.0, 100.0).fit_within(Size::new(100.0, 100.0)), Size::new(100.0, 50.0));
        assert_eq!(Size::new(20.0, 10.0).fit_within(Size::new(100.0, 100.0)), Size::new(20.0, 10.0));
        assert_eq!(Size::new(0.0, 10.0).fit_within(Size::new(100.0, 100.0)), Size::ZERO);
    }

    #[test]
    fn segment_and_polygon() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        assert_eq!(segment_distance(a, b, Point::new(5.0, 3.0)), 3.0);
        assert_eq!(segment_distance(a, b, Point::new(13.0, 4.0)), 5.0);
        assert_eq!(segment_distance(a, a, Point::new(3.0, 4.0)), 5.0);
        let sq = [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)];
        assert!(point_in_polygon(Point::new(5.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(15.0, 5.0), &sq));
        assert!(!point_in_polygon(Point::new(5.0, 5.0), &sq[..2]));
    }
}
