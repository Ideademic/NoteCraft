//! Ink: strokes drawn with a pen, pencil or highlighter (Draw tab).
//!
//! Ink lives at page level, free-floating like OneNote's: [`crate::Page::ink`]. Every point is in
//! **points from the page's top-left corner** (not relative to any outline), so ink drawn over a
//! printout or picture stays where it was drawn when those move. Pressure is `0..=1`, where `1`
//! draws at the stroke's full [`InkStroke::width`].

use notecraft_geom::{Point, Rect, sane, segment_distance};
use serde::{Deserialize, Serialize};

use crate::{Color, Guid};

/// Most points one stroke keeps (hostile files and commands): a long pen stroke has a few hundred.
pub const MAX_POINTS: usize = 5_000;
/// Most ink strokes on one page: drawing more is refused.
pub const MAX_INK_PER_PAGE: usize = 2_000;
/// Widest stroke, points.
pub const MAX_WIDTH: f32 = 100.0;
/// Narrowest stroke, points.
pub const MIN_WIDTH: f32 = 0.1;
/// Lightest pressure drawn: a light touch stays visible.
pub const MIN_PRESSURE: f32 = 0.08;

/// The pen an ink stroke was drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InkTool {
    #[default]
    Pen,
    Pencil,
    Highlighter,
}

impl InkTool {
    pub const ALL: [InkTool; 3] = [InkTool::Pen, InkTool::Pencil, InkTool::Highlighter];

    pub fn parse(s: &str) -> Option<InkTool> {
        match s.to_ascii_lowercase().as_str() {
            "pen" => Some(InkTool::Pen),
            "pencil" => Some(InkTool::Pencil),
            "highlighter" => Some(InkTool::Highlighter),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            InkTool::Pen => "pen",
            InkTool::Pencil => "pencil",
            InkTool::Highlighter => "highlighter",
        }
    }

    /// The colour a new stroke gets (black pen, grey pencil, yellow highlighter).
    pub fn default_color(self) -> Color {
        match self {
            InkTool::Pen => Color::BLACK,
            InkTool::Pencil => Color::rgb(0x59, 0x59, 0x59),
            InkTool::Highlighter => Color::HIGHLIGHT_YELLOW,
        }
    }

    /// The width a new stroke gets, points.
    pub fn default_width(self) -> f32 {
        match self {
            InkTool::Pen => 1.5,
            InkTool::Pencil => 1.0,
            InkTool::Highlighter => 12.0,
        }
    }

    /// How opaque its strokes are drawn: a highlighter's let the text show through, a pencil's
    /// are a little lighter than a pen's. Applied on top of the colour's own alpha.
    pub fn alpha(self) -> f32 {
        match self {
            InkTool::Pen => 1.0,
            InkTool::Pencil => 0.85,
            InkTool::Highlighter => 0.5,
        }
    }

    /// Whether pressure changes the width (the highlighter is a flat marker).
    pub fn uses_pressure(self) -> bool {
        !matches!(self, InkTool::Highlighter)
    }
}

/// One sample of a stroke: page points and pressure `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    /// `0..=1`; `1` is the full stroke width. Input without pressure records `1` (or `0.5`, which
    /// [`InkStroke::sample_width`] also draws at full width when every sample has it).
    #[serde(default = "full_pressure")]
    pub pressure: f32,
}

fn full_pressure() -> f32 {
    1.0
}

impl InkPoint {
    pub const fn new(x: f32, y: f32, pressure: f32) -> Self {
        InkPoint { x, y, pressure }
    }
    pub fn pos(self) -> Point {
        Point { x: self.x, y: self.y }
    }
}

/// One ink stroke.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InkStroke {
    pub id: Guid,
    pub points: Vec<InkPoint>,
    pub color: Color,
    /// Full width (pressure 1), points.
    pub width: f32,
    pub tool: InkTool,
}

impl InkStroke {
    /// A stroke with the tool's default colour and width, sanitized.
    pub fn new(tool: InkTool, points: Vec<InkPoint>) -> InkStroke {
        InkStroke { id: Guid::new(), points, color: tool.default_color(), width: tool.default_width(), tool }.sanitized()
    }

    /// The same stroke with non-finite points dropped, coordinates clamped, at most
    /// [`MAX_POINTS`] points, pressure in `0..=1` and width in [`MIN_WIDTH`]..=[`MAX_WIDTH`]:
    /// safe to draw whatever file or command it came from.
    pub fn sanitized(mut self) -> InkStroke {
        self.points.retain(|p| p.x.is_finite() && p.y.is_finite());
        self.points.truncate(MAX_POINTS);
        for p in &mut self.points {
            p.x = sane(p.x);
            p.y = sane(p.y);
            p.pressure = if p.pressure.is_finite() { p.pressure.clamp(0.0, 1.0) } else { 1.0 };
        }
        self.width = if self.width.is_finite() { self.width.clamp(MIN_WIDTH, MAX_WIDTH) } else { self.tool.default_width() };
        self
    }

    /// True when every sample has the same placeholder pressure (`0.5`, written by devices or
    /// files without pressure): such strokes draw at full width.
    pub fn pressure_is_flat(&self) -> bool {
        !self.points.is_empty() && self.points.iter().all(|p| (p.pressure - 0.5).abs() <= 1.0e-3)
    }

    /// Width of one sample: full width at pressure 1, never thinner than [`MIN_PRESSURE`] of it.
    /// Highlighters and flat-pressure strokes draw at full width.
    pub fn sample_width(&self, pressure: f32) -> f32 {
        if !self.tool.uses_pressure() || self.pressure_is_flat() {
            return self.width;
        }
        let p = if pressure.is_finite() { pressure.clamp(MIN_PRESSURE, 1.0) } else { 1.0 };
        self.width * p
    }

    /// Bounding box of the points (not including the stroke width); `None` when empty.
    pub fn bbox(&self) -> Option<Rect> {
        Rect::bounding(self.points.iter().map(|p| p.pos()))
    }

    /// Bounding box grown by half the width: what it covers on the page.
    pub fn bounds(&self) -> Option<Rect> {
        self.bbox().map(|r| r.expand(self.width / 2.0))
    }

    /// Distance from `p` to the stroke's centre line; `None` when it has no points.
    pub fn distance(&self, p: Point) -> Option<f32> {
        let first = self.points.first()?;
        if self.points.len() == 1 {
            return Some(first.pos().distance(p));
        }
        self.points.windows(2).filter_map(|w| Some(segment_distance(w.first()?.pos(), w.get(1)?.pos(), p))).filter(|d| d.is_finite()).reduce(f32::min)
    }

    /// Whether a click at `p` hits the stroke: within half its width plus `tolerance`.
    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        self.distance(p).is_some_and(|d| d <= self.width / 2.0 + tolerance.max(0.0))
    }

    /// Whether a lasso `polygon` takes the stroke: at least half of its points lie inside.
    pub fn in_lasso(&self, polygon: &[Point]) -> bool {
        if self.points.is_empty() {
            return false;
        }
        let inside = self.points.iter().filter(|p| notecraft_geom::point_in_polygon(p.pos(), polygon)).count();
        inside * 2 >= self.points.len()
    }

    /// Moves every point by (`dx`, `dy`), clamped.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        let (dx, dy) = (sane(dx), sane(dy));
        for p in &mut self.points {
            p.x = sane(p.x + dx);
            p.y = sane(p.y + dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> InkStroke {
        InkStroke::new(InkTool::Pen, vec![InkPoint::new(0.0, 0.0, 1.0), InkPoint::new(10.0, 0.0, 0.5)])
    }

    #[test]
    fn tool_defaults() {
        assert_eq!(InkTool::Highlighter.default_color(), Color::HIGHLIGHT_YELLOW);
        assert_eq!(InkTool::Highlighter.alpha(), 0.5);
        assert!(InkTool::Highlighter.default_width() > InkTool::Pen.default_width());
        assert_eq!(InkTool::parse("PENCIL"), Some(InkTool::Pencil));
        assert_eq!(InkTool::parse("brush"), None);
        for t in InkTool::ALL {
            assert_eq!(InkTool::parse(t.name()), Some(t));
        }
    }

    #[test]
    fn hostile_strokes_are_sanitized() {
        let mut pts = vec![InkPoint::new(f32::NAN, 1.0, 1.0), InkPoint::new(1e30, -1e30, 7.0), InkPoint::new(1.0, 2.0, f32::NAN)];
        pts.extend(std::iter::repeat_n(InkPoint::new(3.0, 3.0, 0.5), MAX_POINTS * 2));
        let s = InkStroke { id: Guid::new(), points: pts, color: Color::BLACK, width: f32::INFINITY, tool: InkTool::Pen }.sanitized();
        assert_eq!(s.points.len(), MAX_POINTS);
        assert_eq!(s.points[0], InkPoint::new(notecraft_geom::MAX_COORD, -notecraft_geom::MAX_COORD, 1.0));
        assert_eq!(s.points[1].pressure, 1.0);
        assert_eq!(s.width, InkTool::Pen.default_width());
    }

    #[test]
    fn distance_hit_and_bbox() {
        let s = line();
        assert_eq!(s.distance(Point::new(5.0, 3.0)), Some(3.0));
        assert!(s.hit(Point::new(5.0, 1.0), 0.5));
        assert!(!s.hit(Point::new(5.0, 3.0), 0.5));
        assert_eq!(s.bbox(), Some(Rect::new(0.0, 0.0, 10.0, 0.0)));
        let empty = InkStroke::new(InkTool::Pen, vec![]);
        assert_eq!(empty.distance(Point::ZERO), None);
        assert_eq!(empty.bbox(), None);
        let dot = InkStroke::new(InkTool::Pen, vec![InkPoint::new(3.0, 4.0, 1.0)]);
        assert_eq!(dot.distance(Point::ZERO), Some(5.0));
    }

    #[test]
    fn pressure_widths() {
        let s = line();
        assert_eq!(s.sample_width(1.0), s.width);
        assert_eq!(s.sample_width(0.0), s.width * MIN_PRESSURE);
        let flat = InkStroke::new(InkTool::Pen, vec![InkPoint::new(0.0, 0.0, 0.5), InkPoint::new(1.0, 0.0, 0.5)]);
        assert_eq!(flat.sample_width(0.5), flat.width);
        let hl = InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(0.0, 0.0, 0.2)]);
        assert_eq!(hl.sample_width(0.2), hl.width);
    }

    #[test]
    fn lasso_and_translate() {
        let mut s = line();
        let poly = [Point::new(-1.0, -1.0), Point::new(11.0, -1.0), Point::new(11.0, 1.0), Point::new(-1.0, 1.0)];
        assert!(s.in_lasso(&poly));
        s.translate(100.0, 0.0);
        assert!(!s.in_lasso(&poly));
        assert_eq!(s.points[0].x, 100.0);
    }

    #[test]
    fn serde_round_trip() {
        let s = line();
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<InkStroke>(&j).unwrap(), s);
        let p: InkPoint = serde_json::from_str(r#"{"x":1,"y":2}"#).unwrap();
        assert_eq!(p.pressure, 1.0);
    }
}
