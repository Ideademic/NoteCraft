//! Ink to Shape: recognise a hand-drawn stroke as a line, rectangle, ellipse or triangle and
//! redraw it as a clean polyline (the stroke stays ink, with the same colour, width and tool).
//!
//! Pure geometry, tested with synthetic strokes. A stroke whose ends meet is closed; a closed
//! stroke is classified by how much of its bounding box it covers (rectangle ≈ 1, ellipse ≈ π/4,
//! triangle ≈ ½) and, for an ellipse, by how evenly it keeps its distance from the centre. An
//! open stroke that is nearly straight is a line.

use notecraft_doc::InkPoint;

/// What a stroke was recognised as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Line {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Ellipse {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    /// Apex at the top centre, or at the bottom when `down`.
    Triangle {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        down: bool,
    },
}

/// Strokes smaller than this (points) stay as drawn.
const MIN_SIZE: f32 = 6.0;
/// Points on a redrawn ellipse.
const ELLIPSE_POINTS: usize = 72;

/// Recognises the stroke through `pts`, or `None` when it isn't one of the shapes.
pub fn recognize(pts: &[InkPoint]) -> Option<Shape> {
    let pts: Vec<(f32, f32)> = pts.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| (p.x, p.y)).collect();
    let (first, last) = (*pts.first()?, *pts.last()?);
    if pts.len() < 2 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in &pts {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let size = w.max(h);
    if !size.is_finite() || size < MIN_SIZE {
        return None;
    }
    let length: f32 = pts.windows(2).filter_map(|p| Some(dist(*p.first()?, *p.get(1)?))).sum();
    if length <= 0.0 || !length.is_finite() {
        return None;
    }
    let gap = dist(first, last);
    if gap > 0.25 * size {
        return (gap / length > 0.92).then_some(Shape::Line { x0: first.0, y0: first.1, x1: last.0, y1: last.1 });
    }
    if w.min(h) < 0.15 * size {
        return None;
    }
    // Area covered (shoelace, closing the gap) over the bounding box.
    let n = pts.len();
    let mut a2 = 0.0f64;
    for k in 0..n {
        let (Some(p), Some(q)) = (pts.get(k), pts.get((k + 1) % n)) else { continue };
        a2 += f64::from(p.0) * f64::from(q.1) - f64::from(q.0) * f64::from(p.1);
    }
    let cover = (a2.abs() / 2.0) / (f64::from(w) * f64::from(h));
    let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
    let spread = pts.iter().map(|(x, y)| (((x - cx) / (w / 2.0)).hypot((y - cy) / (h / 2.0)) - 1.0).abs()).sum::<f32>() / n as f32;
    if cover > 0.86 {
        Some(Shape::Rect { x: x0, y: y0, w, h })
    } else if cover > 0.66 && spread < 0.12 {
        Some(Shape::Ellipse { x: x0, y: y0, w, h })
    } else if (0.35..0.66).contains(&cover) {
        // The apex is on the side of the box with fewer points near it.
        let near_top = pts.iter().filter(|(_, y)| *y < y0 + h * 0.15).count();
        let near_bottom = pts.iter().filter(|(_, y)| *y > y1 - h * 0.15).count();
        Some(Shape::Triangle { x: x0, y: y0, w, h, down: near_bottom < near_top })
    } else {
        None
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The shape as clean points at full pressure.
pub fn outline_points(shape: Shape) -> Vec<InkPoint> {
    let pt = |x: f32, y: f32| InkPoint::new(x, y, 1.0);
    match shape {
        Shape::Line { x0, y0, x1, y1 } => vec![pt(x0, y0), pt(x1, y1)],
        Shape::Rect { x, y, w, h } => vec![pt(x, y), pt(x + w, y), pt(x + w, y + h), pt(x, y + h), pt(x, y)],
        Shape::Ellipse { x, y, w, h } => (0..=ELLIPSE_POINTS)
            .map(|i| {
                let a = i as f32 / ELLIPSE_POINTS as f32 * std::f32::consts::TAU;
                pt(x + w / 2.0 + w / 2.0 * a.cos(), y + h / 2.0 + h / 2.0 * a.sin())
            })
            .collect(),
        Shape::Triangle { x, y, w, h, down: false } => vec![pt(x + w / 2.0, y), pt(x + w, y + h), pt(x, y + h), pt(x + w / 2.0, y)],
        Shape::Triangle { x, y, w, h, down: true } => vec![pt(x, y), pt(x + w, y), pt(x + w / 2.0, y + h), pt(x, y)],
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn poly(corners: &[(f32, f32)], per_side: usize) -> Vec<InkPoint> {
        let mut v = vec![];
        for k in 0..corners.len() {
            let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
            for i in 0..per_side {
                let t = i as f32 / per_side as f32;
                let wob = ((i * 7 + k * 3) % 5) as f32 * 0.4 - 0.8;
                v.push(InkPoint::new(a.0 + (b.0 - a.0) * t + wob, a.1 + (b.1 - a.1) * t - wob, 0.5));
            }
        }
        v.push(InkPoint::new(corners[0].0 + 2.0, corners[0].1 + 1.5, 0.5));
        v
    }

    #[test]
    fn shapes_are_recognised() {
        let r = recognize(&poly(&[(100.0, 100.0), (300.0, 100.0), (300.0, 200.0), (100.0, 200.0)], 30)).unwrap();
        assert!(matches!(r, Shape::Rect { w, h, .. } if (w - 200.0).abs() < 4.0 && (h - 100.0).abs() < 4.0), "{r:?}");
        let ell: Vec<InkPoint> = (0..=80)
            .map(|i| {
                let a = i as f32 / 80.0 * std::f32::consts::TAU;
                InkPoint::new(200.0 + 120.0 * a.cos() + (i % 3) as f32 * 0.5, 150.0 + 60.0 * a.sin(), 1.0)
            })
            .collect();
        assert!(matches!(recognize(&ell), Some(Shape::Ellipse { .. })), "{:?}", recognize(&ell));
        assert!(matches!(recognize(&poly(&[(200.0, 50.0), (300.0, 250.0), (100.0, 250.0)], 30)), Some(Shape::Triangle { down: false, .. })));
        assert!(matches!(recognize(&poly(&[(100.0, 50.0), (300.0, 50.0), (200.0, 250.0)], 30)), Some(Shape::Triangle { down: true, .. })));
        let line: Vec<InkPoint> = (0..=40).map(|i| InkPoint::new(10.0 + i as f32 * 5.0, 20.0 + i as f32 * 2.0 + (i % 2) as f32 * 0.6, 1.0)).collect();
        assert!(matches!(recognize(&line), Some(Shape::Line { x0, x1, .. }) if x0 < x1));
        assert_eq!(outline_points(Shape::Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }).len(), 5);
        assert_eq!(outline_points(Shape::Ellipse { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }).len(), ELLIPSE_POINTS + 1);
    }

    #[test]
    fn scribbles_and_garbage_stay_ink() {
        assert_eq!(recognize(&[]), None);
        assert_eq!(recognize(&[InkPoint::new(1.0, 1.0, 1.0)]), None);
        let zig: Vec<InkPoint> = (0..40).map(|i| InkPoint::new(i as f32 * 5.0, if i % 2 == 0 { 0.0 } else { 60.0 }, 1.0)).collect();
        assert_eq!(recognize(&zig), None);
        assert_eq!(recognize(&[InkPoint::new(f32::NAN, 0.0, 1.0), InkPoint::new(f32::INFINITY, 1.0, 1.0)]), None);
        let back: Vec<InkPoint> = (0..=20).chain((0..=20).rev()).map(|i| InkPoint::new(i as f32 * 10.0, 5.0, 1.0)).collect();
        assert_eq!(recognize(&back), None);
    }
}
