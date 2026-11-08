//! Drawing primitives on a Krilla surface, in page points (y down).

use craft_pdf::write::backend as k;
use k::geom::{Path, PathBuilder, Transform};
use k::num::NormalizedF32;
use k::paint::{Fill, FillRule, LineCap, LineJoin, Stroke};
use k::surface::Surface;
use notecraft_doc::Color;

use crate::ink::Piece;

/// Bézier handle length for a quarter circle.
const KAPPA: f32 = 0.552_284_8;

pub fn rgb(c: Color) -> k::color::rgb::Color {
    k::color::rgb::Color::new(c.r, c.g, c.b)
}

fn opacity(a: f32) -> NormalizedF32 {
    NormalizedF32::new(if a.is_finite() { a.clamp(0.0, 1.0) } else { 1.0 }).unwrap_or(NormalizedF32::ONE)
}

pub fn fill(c: Color, alpha: f32) -> Fill {
    Fill { paint: rgb(c).into(), opacity: opacity(alpha * f32::from(c.a) / 255.0), rule: FillRule::NonZero }
}

pub fn stroke(c: Color, width: f32) -> Stroke {
    Stroke {
        paint: rgb(c).into(),
        width: if width.is_finite() { width.clamp(0.05, 200.0) } else { 1.0 },
        line_cap: LineCap::Butt,
        line_join: LineJoin::Miter,
        opacity: opacity(f32::from(c.a) / 255.0),
        ..Stroke::default()
    }
}

/// Adds a disc (a full circle, positive winding with y down).
pub fn disc(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
    let k = r * KAPPA;
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.close();
}

pub fn polygon(pb: &mut PathBuilder, pts: &[(f32, f32)]) {
    let Some(((x, y), rest)) = pts.split_first() else { return };
    pb.move_to(*x, *y);
    for (x, y) in rest {
        pb.line_to(*x, *y);
    }
    pb.close();
}

pub fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    polygon(&mut pb, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]);
    pb.finish()
}

pub fn fill_path(s: &mut Surface, path: &Path, f: Fill) {
    s.set_stroke(None);
    s.set_fill(Some(f));
    s.draw_path(path);
    s.set_fill(None);
}

pub fn stroke_path(s: &mut Surface, path: &Path, st: Stroke) {
    s.set_fill(None);
    s.set_stroke(Some(st));
    s.draw_path(path);
    s.set_stroke(None);
}

pub fn fill_rect(s: &mut Surface, x: f32, y: f32, w: f32, h: f32, c: Color) {
    if let Some(p) = rect_path(x, y, w, h) {
        fill_path(s, &p, fill(c, 1.0));
    }
}

pub fn stroke_rect(s: &mut Surface, x: f32, y: f32, w: f32, h: f32, width: f32, c: Color) {
    if let Some(p) = rect_path(x, y, w, h) {
        stroke_path(s, &p, stroke(c, width));
    }
}

pub fn line(s: &mut Surface, a: (f32, f32), b: (f32, f32), width: f32, c: Color) {
    let mut pb = PathBuilder::new();
    pb.move_to(a.0, a.1);
    pb.line_to(b.0, b.1);
    if let Some(p) = pb.finish() {
        stroke_path(s, &p, stroke(c, width));
    }
}

/// A polyline with round joins and caps.
pub fn polyline(s: &mut Surface, pts: &[(f32, f32)], width: f32, c: Color) {
    let Some(((x, y), rest)) = pts.split_first() else { return };
    let mut pb = PathBuilder::new();
    pb.move_to(*x, *y);
    for (x, y) in rest {
        pb.line_to(*x, *y);
    }
    if let Some(p) = pb.finish() {
        stroke_path(s, &p, Stroke { line_cap: LineCap::Round, line_join: LineJoin::Round, ..stroke(c, width) });
    }
}

pub fn fill_disc(s: &mut Surface, cx: f32, cy: f32, r: f32, c: Color) {
    let mut pb = PathBuilder::new();
    disc(&mut pb, cx, cy, r);
    if let Some(p) = pb.finish() {
        fill_path(s, &p, fill(c, 1.0));
    }
}

pub fn stroke_disc(s: &mut Surface, cx: f32, cy: f32, r: f32, width: f32, c: Color) {
    let mut pb = PathBuilder::new();
    disc(&mut pb, cx, cy, r);
    if let Some(p) = pb.finish() {
        stroke_path(s, &p, stroke(c, width));
    }
}

/// An ink stroke's outline pieces as one filled path.
pub fn fill_pieces(s: &mut Surface, pieces: &[Piece], c: Color, alpha: f32) {
    let mut pb = PathBuilder::new();
    for p in pieces {
        match p {
            Piece::Poly(v) => polygon(&mut pb, v),
            Piece::Disc(x, y, r) => disc(&mut pb, *x, *y, *r),
        }
    }
    if let Some(path) = pb.finish() {
        fill_path(s, &path, Fill { paint: rgb(c).into(), opacity: opacity(alpha), rule: FillRule::NonZero });
    }
}

pub fn translate(x: f32, y: f32) -> Transform {
    Transform::from_translate(x, y)
}
