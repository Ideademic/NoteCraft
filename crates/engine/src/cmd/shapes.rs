//! Draw › Shapes: the geometry of the shapes NoteCraft inserts (lines, arrows, rectangles,
//! ellipses, polygons and graph axes). A shape is ordinary ink, as OneNote stores its shapes: one
//! or more polylines in page points, drawn with the pen's colour and width. Pure geometry, hand
//! written (proportions after DeckCraft's presets: rounded corners at a sixth of the shorter side,
//! a quarter-side slant for the parallelogram, trapezoid and hexagon).
//!
//! A shape is placed by a box `[x, y, w, h]` from where the drag started (`x`, `y`) to where it
//! ended (`x + w`, `y + h`), so `w` and `h` may be negative. Lines and arrows run from the start
//! to the end; the other shapes fill the box. `constrain` (Shift while dragging) keeps a square
//! box (a square, a circle) or snaps a line to a multiple of 45°.

use std::f32::consts::{FRAC_PI_4, TAU};

/// The shapes of the Shapes gallery, in gallery order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Line,
    Arrow,
    DoubleArrow,
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    RightTriangle,
    Parallelogram,
    Trapezoid,
    Diamond,
    Pentagon,
    Hexagon,
    /// Two axes meeting at the bottom left (a first-quadrant graph).
    Axes2,
    /// Two axes crossing in the middle, arrowheads at their ends (a coordinate plane).
    Axes2Arrows,
    /// Three axes from one origin (x towards the viewer), with arrowheads.
    Axes3,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 16] = [
        ShapeKind::Line,
        ShapeKind::Arrow,
        ShapeKind::DoubleArrow,
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Triangle,
        ShapeKind::RightTriangle,
        ShapeKind::Parallelogram,
        ShapeKind::Trapezoid,
        ShapeKind::Diamond,
        ShapeKind::Pentagon,
        ShapeKind::Hexagon,
        ShapeKind::Axes2,
        ShapeKind::Axes2Arrows,
        ShapeKind::Axes3,
    ];

    /// The id used by `draw.shape`'s `kind`.
    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Line => "line",
            ShapeKind::Arrow => "arrow",
            ShapeKind::DoubleArrow => "doubleArrow",
            ShapeKind::Rectangle => "rectangle",
            ShapeKind::RoundedRectangle => "roundedRectangle",
            ShapeKind::Ellipse => "ellipse",
            ShapeKind::Triangle => "triangle",
            ShapeKind::RightTriangle => "rightTriangle",
            ShapeKind::Parallelogram => "parallelogram",
            ShapeKind::Trapezoid => "trapezoid",
            ShapeKind::Diamond => "diamond",
            ShapeKind::Pentagon => "pentagon",
            ShapeKind::Hexagon => "hexagon",
            ShapeKind::Axes2 => "axes2",
            ShapeKind::Axes2Arrows => "axes2Arrows",
            ShapeKind::Axes3 => "axes3",
        }
    }

    /// The kind called `s` ([`ShapeKind::name`], any case).
    pub fn parse(s: &str) -> Option<ShapeKind> {
        ShapeKind::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(s.trim()))
    }

    /// The name shown in the gallery's tooltips.
    pub fn label(self) -> &'static str {
        match self {
            ShapeKind::Line => "Line",
            ShapeKind::Arrow => "Arrow",
            ShapeKind::DoubleArrow => "Double Arrow",
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::RoundedRectangle => "Rounded Rectangle",
            ShapeKind::Ellipse => "Ellipse",
            ShapeKind::Triangle => "Triangle",
            ShapeKind::RightTriangle => "Right Triangle",
            ShapeKind::Parallelogram => "Parallelogram",
            ShapeKind::Trapezoid => "Trapezoid",
            ShapeKind::Diamond => "Diamond",
            ShapeKind::Pentagon => "Pentagon",
            ShapeKind::Hexagon => "Hexagon",
            ShapeKind::Axes2 => "Graph: X-Y Axes",
            ShapeKind::Axes2Arrows => "Graph: Coordinate Plane",
            ShapeKind::Axes3 => "Graph: 3-D Axes",
        }
    }

    /// Lines and arrows: drawn from the drag's start to its end, not filling a box.
    pub fn is_line(self) -> bool {
        matches!(self, ShapeKind::Line | ShapeKind::Arrow | ShapeKind::DoubleArrow)
    }

    /// Whether every stroke of the shape is a closed outline.
    pub fn is_closed(self) -> bool {
        !self.is_line() && !matches!(self, ShapeKind::Axes2 | ShapeKind::Axes2Arrows | ShapeKind::Axes3)
    }
}

/// Largest side of a shape, points (any larger box is clamped).
pub const MAX_SHAPE_SIZE: f32 = 5_000.0;
/// Segments of an ellipse (it has one more point, closing it).
pub const ELLIPSE_SEGMENTS: usize = 64;
/// Segments of each rounded corner.
const CORNER_SEGMENTS: usize = 8;
/// Arrowhead barbs: angle off the shaft, and length limits (points).
const HEAD_ANGLE: f32 = 28.0;
const HEAD_LEN: (f32, f32) = (5.0, 18.0);

/// The box with `w`/`h` clamped to ±[`MAX_SHAPE_SIZE`]; `None` when anything isn't finite.
fn sane([x, y, w, h]: [f32; 4]) -> Option<[f32; 4]> {
    [x, y, w, h].iter().all(|v| v.is_finite()).then(|| {
        let lim = |v: f32| v.clamp(-MAX_SHAPE_SIZE, MAX_SHAPE_SIZE);
        [x.clamp(-1.0e6, 1.0e6), y.clamp(-1.0e6, 1.0e6), lim(w), lim(h)]
    })
}

/// The box Shift gives: a line snapped to the nearest multiple of 45° (same length), any other
/// shape square (the longer side, towards where the drag went).
pub fn constrained(kind: ShapeKind, rect: [f32; 4]) -> [f32; 4] {
    let Some([x, y, w, h]) = sane(rect) else { return rect };
    if kind.is_line() {
        let len = w.hypot(h);
        let a = (h.atan2(w) / FRAC_PI_4).round() * FRAC_PI_4;
        // Exact zeros on the axes, not 1e-8.
        let (c, s) = (a.cos(), a.sin());
        let snap = |v: f32| if v.abs() < 1.0e-4 { 0.0 } else { v };
        return [x, y, snap(len * c), snap(len * s)];
    }
    let side = w.abs().max(h.abs());
    let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
    [x, y, side * sign(w), side * sign(h)]
}

/// The polylines (page points) of `kind` in `rect` (see the module notes); empty when the box
/// isn't finite or is smaller than half a point both ways.
pub fn shape_strokes(kind: ShapeKind, rect: [f32; 4], constrain: bool) -> Vec<Vec<[f32; 2]>> {
    let Some(r) = sane(rect) else { return Vec::new() };
    let [x, y, w, h] = if constrain { constrained(kind, r) } else { r };
    if w.abs().max(h.abs()) < 0.5 {
        return Vec::new();
    }
    let (a, b) = ([x, y], [x + w, y + h]);
    // The box, normalised: left, top, right, bottom.
    let (x0, y0, x1, y1) = (x.min(x + w), y.min(y + h), x.max(x + w), y.max(y + h));
    let (bw, bh) = (x1 - x0, y1 - y0);
    let (cx, cy) = (x0 + bw / 2.0, y0 + bh / 2.0);
    let ss = bw.min(bh);
    let closed = |pts: &[[f32; 2]]| -> Vec<[f32; 2]> { pts.iter().chain(pts.first()).copied().collect() };
    let head = |tail: [f32; 2], tip: [f32; 2]| arrow_head(tail, tip).to_vec();
    match kind {
        ShapeKind::Line => vec![vec![a, b]],
        ShapeKind::Arrow => vec![vec![a, b], head(a, b)],
        ShapeKind::DoubleArrow => vec![vec![a, b], head(a, b), head(b, a)],
        ShapeKind::Rectangle => vec![closed(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]])],
        ShapeKind::RoundedRectangle => vec![rounded_rect(x0, y0, x1, y1, ss / 6.0)],
        ShapeKind::Ellipse => vec![
            (0..=ELLIPSE_SEGMENTS)
                .map(|i| {
                    // Start at the top, like a hand draws it; the last point is the first.
                    let t = (i % ELLIPSE_SEGMENTS) as f32 / ELLIPSE_SEGMENTS as f32 * TAU - TAU / 4.0;
                    [cx + bw / 2.0 * t.cos(), cy + bh / 2.0 * t.sin()]
                })
                .collect(),
        ],
        ShapeKind::Triangle => vec![closed(&[[cx, y0], [x1, y1], [x0, y1]])],
        ShapeKind::RightTriangle => vec![closed(&[[x0, y0], [x1, y1], [x0, y1]])],
        ShapeKind::Parallelogram => {
            let off = (ss / 4.0).min(bw / 2.0);
            vec![closed(&[[x0 + off, y0], [x1, y0], [x1 - off, y1], [x0, y1]])]
        }
        ShapeKind::Trapezoid => {
            let off = (ss / 4.0).min(bw / 2.0);
            vec![closed(&[[x0 + off, y0], [x1 - off, y0], [x1, y1], [x0, y1]])]
        }
        ShapeKind::Diamond => vec![closed(&[[cx, y0], [x1, cy], [cx, y1], [x0, cy]])],
        ShapeKind::Pentagon => vec![closed(&regular(5, x0, y0, bw, bh))],
        ShapeKind::Hexagon => {
            let off = (ss / 4.0).min(bw / 2.0);
            vec![closed(&[[x0 + off, y0], [x1 - off, y0], [x1, cy], [x1 - off, y1], [x0 + off, y1], [x0, cy]])]
        }
        ShapeKind::Axes2 => vec![vec![[x0, y0], [x0, y1]], vec![[x0, y1], [x1, y1]]],
        ShapeKind::Axes2Arrows => {
            let (l, r, t, bt) = ([x0, cy], [x1, cy], [cx, y0], [cx, y1]);
            vec![vec![l, r], vec![bt, t], head(l, r), head(r, l), head(bt, t), head(t, bt)]
        }
        ShapeKind::Axes3 => {
            // The origin left of centre and low; z up, y right, x down to the left (towards you).
            let o = [x0 + bw * 0.38, y0 + bh * 0.62];
            let (z, yy, xx) = ([o[0], y0], [x1, o[1]], [x0, y1]);
            vec![vec![o, z], vec![o, yy], vec![o, xx], head(o, z), head(o, yy), head(o, xx)]
        }
    }
}

/// An arrowhead at `tip` for a shaft from `tail`: one barb end, the tip, the other barb end (an
/// open V, as a pen draws it). Its barbs are a fifth of the shaft, 5 to 18 points, never more
/// than half the shaft.
pub fn arrow_head(tail: [f32; 2], tip: [f32; 2]) -> [[f32; 2]; 3] {
    let len = (tip[0] - tail[0]).hypot(tip[1] - tail[1]);
    if !(len.is_finite() && len > 0.0) {
        return [tip, tip, tip];
    }
    let barb = (len * 0.2).clamp(HEAD_LEN.0, HEAD_LEN.1).min(len / 2.0);
    let back = (tail[1] - tip[1]).atan2(tail[0] - tip[0]);
    let end = |sign: f32| {
        let a = back + sign * HEAD_ANGLE.to_radians();
        [tip[0] + barb * a.cos(), tip[1] + barb * a.sin()]
    };
    [end(1.0), tip, end(-1.0)]
}

/// A closed rounded rectangle, corners of radius `r` (at most half the shorter side), clockwise
/// from the end of the top-left corner.
fn rounded_rect(x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> Vec<[f32; 2]> {
    let r = r.clamp(0.0, ((x1 - x0).min(y1 - y0) / 2.0).max(0.0));
    let mut v = Vec::with_capacity(4 * (CORNER_SEGMENTS + 1) + 1);
    // Corner centres and the angle each arc starts at (y down: -90° is up).
    for (ccx, ccy, start) in [(x1 - r, y0 + r, -90.0f32), (x1 - r, y1 - r, 0.0), (x0 + r, y1 - r, 90.0), (x0 + r, y0 + r, 180.0)] {
        for k in 0..=CORNER_SEGMENTS {
            let a = (start + 90.0 * k as f32 / CORNER_SEGMENTS as f32).to_radians();
            v.push([ccx + r * a.cos(), ccy + r * a.sin()]);
        }
    }
    if let Some(f) = v.first().copied() {
        v.push(f);
    }
    v
}

/// A regular `n`-gon with a corner at the top, stretched to fill the box.
fn regular(n: usize, x0: f32, y0: f32, w: f32, h: f32) -> Vec<[f32; 2]> {
    let unit: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU - TAU / 4.0;
            [a.cos(), a.sin()]
        })
        .collect();
    let (mut ux0, mut uy0, mut ux1, mut uy1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for [ux, uy] in &unit {
        (ux0, uy0, ux1, uy1) = (ux0.min(*ux), uy0.min(*uy), ux1.max(*ux), uy1.max(*uy));
    }
    let (uw, uh) = ((ux1 - ux0).max(1.0e-6), (uy1 - uy0).max(1.0e-6));
    unit.iter().map(|[ux, uy]| [x0 + (ux - ux0) / uw * w, y0 + (uy - uy0) / uh * h]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(strokes: &[Vec<[f32; 2]>]) -> [f32; 4] {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in strokes.iter().flatten() {
            (x0, y0, x1, y1) = (x0.min(p[0]), y0.min(p[1]), x1.max(p[0]), y1.max(p[1]));
        }
        [x0, y0, x1, y1]
    }

    #[test]
    fn closed_shapes_close_and_fill_their_box() {
        let rect = [100.0, 50.0, 200.0, 120.0];
        for kind in ShapeKind::ALL {
            let s = shape_strokes(kind, rect, false);
            assert!(!s.is_empty(), "{kind:?}");
            assert!(s.iter().all(|st| st.len() >= 2), "{kind:?}");
            // Inside the box, but for arrowheads, which may stick out at a line's ends.
            let [x0, y0, x1, y1] = bounds(&s);
            let pad = if kind.is_closed() { 0.01 } else { 20.0 };
            assert!(x0 >= 100.0 - pad && y0 >= 50.0 - pad && x1 <= 300.0 + pad && y1 <= 170.0 + pad, "{kind:?}: {x0} {y0} {x1} {y1}");
            if kind.is_closed() {
                assert_eq!(s.len(), 1, "{kind:?}");
                let (f, l) = (s[0][0], *s[0].last().unwrap());
                assert!((f[0] - l[0]).abs() < 1e-3 && (f[1] - l[1]).abs() < 1e-3, "{kind:?} closes");
                // A closed shape touches all four sides of its box.
                assert!((x0 - 100.0).abs() < 0.01 && (y0 - 50.0).abs() < 0.01 && (x1 - 300.0).abs() < 0.01 && (y1 - 170.0).abs() < 0.01, "{kind:?}");
            }
            assert!(s.iter().flatten().all(|p| p[0].is_finite() && p[1].is_finite()));
            assert_eq!(ShapeKind::parse(kind.name()), Some(kind));
        }
        let count = |k| shape_strokes(k, rect, false).iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(count(ShapeKind::Rectangle), vec![5]);
        assert_eq!(count(ShapeKind::Ellipse), vec![ELLIPSE_SEGMENTS + 1]);
        assert_eq!(count(ShapeKind::Triangle), vec![4]);
        assert_eq!(count(ShapeKind::Diamond), vec![5]);
        assert_eq!(count(ShapeKind::Pentagon), vec![6]);
        assert_eq!(count(ShapeKind::Hexagon), vec![7]);
        assert_eq!(count(ShapeKind::RoundedRectangle), vec![4 * (CORNER_SEGMENTS + 1) + 1]);
        assert_eq!(count(ShapeKind::Line), vec![2]);
        assert_eq!(count(ShapeKind::Arrow), vec![2, 3]);
        assert_eq!(count(ShapeKind::DoubleArrow), vec![2, 3, 3]);
        assert_eq!(count(ShapeKind::Axes2), vec![2, 2]);
        assert_eq!(shape_strokes(ShapeKind::Axes2Arrows, rect, false).len(), 6);
        assert_eq!(shape_strokes(ShapeKind::Axes3, rect, false).len(), 6);
        assert_eq!(ShapeKind::parse("ELLIPSE"), Some(ShapeKind::Ellipse));
        assert_eq!(ShapeKind::parse("blob"), None);
    }

    #[test]
    fn lines_follow_the_drag_and_boxes_normalise() {
        // Dragged up and to the left: the line runs that way, the box is the same box.
        let s = shape_strokes(ShapeKind::Arrow, [300.0, 200.0, -100.0, -50.0], false);
        assert_eq!(s[0], vec![[300.0, 200.0], [200.0, 150.0]]);
        assert_eq!(s[1][1], [200.0, 150.0], "the head is at the end of the drag");
        assert_eq!(
            shape_strokes(ShapeKind::Rectangle, [300.0, 200.0, -100.0, -50.0], false),
            shape_strokes(ShapeKind::Rectangle, [200.0, 150.0, 100.0, 50.0], false)
        );
        // Heads stay a sensible size.
        let [b1, tip, _] = arrow_head([0.0, 0.0], [1000.0, 0.0]);
        assert!((b1[0] - tip[0]).hypot(b1[1] - tip[1]) <= HEAD_LEN.1 + 0.01);
        let [b1, tip, _] = arrow_head([0.0, 0.0], [4.0, 0.0]);
        assert!((b1[0] - tip[0]).hypot(b1[1] - tip[1]) <= 2.01);
    }

    #[test]
    fn constrain_squares_boxes_and_snaps_lines() {
        let [_, _, w, h] = constrained(ShapeKind::Ellipse, [0.0, 0.0, 120.0, -40.0]);
        assert_eq!((w, h), (120.0, -120.0));
        let e = shape_strokes(ShapeKind::Ellipse, [0.0, 0.0, 120.0, -40.0], true);
        let [x0, y0, x1, y1] = bounds(&e);
        assert!(((x1 - x0) - (y1 - y0)).abs() < 0.01 && y1 <= 0.01, "a circle above the start");
        for (dx, dy, want) in [(100.0, 8.0, (100.3, 0.0)), (100.0, 90.0, (95.2, 95.2)), (-5.0, 80.0, (0.0, 80.2)), (-90.0, -100.0, (-95.2, -95.2))] {
            let [_, _, w, h] = constrained(ShapeKind::Line, [10.0, 10.0, dx, dy]);
            assert!((w - want.0).abs() < 0.3 && (h - want.1).abs() < 0.3, "{dx},{dy} -> {w},{h}");
        }
    }

    #[test]
    fn hostile_boxes_give_nothing_or_bounded_shapes() {
        for kind in ShapeKind::ALL {
            for r in [[f32::NAN, 0.0, 10.0, 10.0], [0.0, 0.0, f32::INFINITY, 1.0], [0.0, 0.0, 0.1, 0.2]] {
                assert!(shape_strokes(kind, r, true).is_empty(), "{kind:?} {r:?}");
            }
            let s = shape_strokes(kind, [1e30, -1e30, 1e30, -1e30], true);
            assert!(s.iter().flatten().all(|p| p[0].is_finite() && p[1].is_finite()));
            let [x0, y0, x1, y1] = bounds(&s);
            assert!(x1 - x0 <= MAX_SHAPE_SIZE + 40.0 && y1 - y0 <= MAX_SHAPE_SIZE + 40.0);
            // A flat box still draws (a rectangle as thin as a line).
            assert!(!shape_strokes(kind, [0.0, 0.0, 50.0, 0.0], false).is_empty());
        }
    }
}
