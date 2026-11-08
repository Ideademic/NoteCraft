//! Ink strokes as filled vector outlines: the width follows the pen pressure sample by sample
//! (the canvas's tapers), pens and pencils have round ends and joins, a highlighter has flat
//! ends. One stroke is one path of same-direction pieces (a quadrilateral per segment, a disc
//! per joint) filled with the non-zero rule, so their union is painted once and a see-through
//! highlighter doesn't darken where pieces overlap.

use notecraft_doc::{Color, InkStroke, InkTool};

use crate::style::{Ink, mix};

/// One piece of a stroke's outline, page points.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// A closed polygon, counter-clockwise on screen (positive shoelace area with y down).
    Poly(Vec<(f32, f32)>),
    /// A disc: centre and radius.
    Disc(f32, f32, f32),
}

/// Thinnest half-width drawn, points.
const MIN_HALF: f32 = 0.05;

/// The colour and opacity a stroke prints with (the canvas's `stroke_color`).
pub fn stroke_paint(s: &InkStroke, ink: &Ink) -> (Color, f32) {
    let base = ink.adapt(s.color.with_alpha(255));
    let base = if s.tool == InkTool::Pencil { mix(base, ink.paper, 0.18) } else { base };
    let a = (f32::from(s.color.a) / 255.0 * s.tool.alpha()).clamp(0.0, 1.0);
    (base, a)
}

fn area(p: &[(f32, f32)]) -> f32 {
    p.iter().zip(p.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum::<f32>() / 2.0
}

fn positive(mut p: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    if area(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// The pieces of `s` (sanitized points: the model clamps them).
pub fn outline(s: &InkStroke) -> Vec<Piece> {
    let mut pts: Vec<(f32, f32, f32)> = Vec::with_capacity(s.points.len());
    for p in &s.points {
        if !(p.x.is_finite() && p.y.is_finite()) {
            continue;
        }
        let hw = (s.sample_width(p.pressure) / 2.0).max(MIN_HALF);
        if pts.last().is_some_and(|l| (l.0 - p.x).hypot(l.1 - p.y) < 0.05) {
            continue;
        }
        pts.push((p.x, p.y, if hw.is_finite() { hw } else { MIN_HALF }));
    }
    let round = s.tool != InkTool::Highlighter;
    let mut out = Vec::new();
    match pts.as_slice() {
        [] => return out,
        [(x, y, hw)] => {
            if round {
                out.push(Piece::Disc(*x, *y, *hw));
            } else {
                let (w, h) = (hw * 0.3, *hw);
                out.push(Piece::Poly(positive(vec![(x - w, y - h), (x + w, y - h), (x + w, y + h), (x - w, y + h)])));
            }
            return out;
        }
        _ => {}
    }
    for w in pts.windows(2) {
        let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy);
        if len <= 1.0e-6 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        out.push(Piece::Poly(positive(vec![
            (a.0 + nx * a.2, a.1 + ny * a.2),
            (b.0 + nx * b.2, b.1 + ny * b.2),
            (b.0 - nx * b.2, b.1 - ny * b.2),
            (a.0 - nx * a.2, a.1 - ny * a.2),
        ])));
    }
    let last = pts.len() - 1;
    for (i, p) in pts.iter().enumerate() {
        let end = i == 0 || i == last;
        if !end || round {
            out.push(Piece::Disc(p.0, p.1, p.2));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_doc::InkPoint;

    #[test]
    fn pressure_tapers_and_ends() {
        let s = InkStroke::new(InkTool::Pen, vec![InkPoint::new(0.0, 0.0, 1.0), InkPoint::new(10.0, 0.0, 0.2), InkPoint::new(20.0, 5.0, 1.0)]);
        let pieces = outline(&s);
        // Two segments, three discs (round ends and the joint).
        assert_eq!(pieces.iter().filter(|p| matches!(p, Piece::Poly(_))).count(), 2);
        let discs: Vec<f32> = pieces.iter().filter_map(|p| if let Piece::Disc(_, _, r) = p { Some(*r) } else { None }).collect();
        assert_eq!(discs.len(), 3);
        assert!(discs[1] < discs[0], "lighter pressure, thinner");
        for p in &pieces {
            if let Piece::Poly(v) = p {
                assert!(area(v) > 0.0, "every piece winds the same way");
            }
        }
        let hl = InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(0.0, 0.0, 0.5), InkPoint::new(10.0, 0.0, 0.5)]);
        assert!(outline(&hl).iter().all(|p| matches!(p, Piece::Poly(_))), "flat ends");
        let (c, a) = stroke_paint(&hl, &Ink::new(None));
        assert_eq!(c, Color::HIGHLIGHT_YELLOW);
        assert_eq!(a, 0.5);
        assert!(outline(&InkStroke::new(InkTool::Pen, vec![])).is_empty());
    }
}
