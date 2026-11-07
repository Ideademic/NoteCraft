//! The ink layer: drawing strokes with pressure, the eraser and the lasso (phase 2, canvas
//! agent). For now it paints existing strokes as polylines.

use egui::{Painter, Pos2, Stroke, pos2};
use notecraft_doc::Page;

use crate::theme::color32;

pub(crate) fn paint(painter: &Painter, page: &Page, origin: Pos2, zoom: f32) {
    for s in &page.ink {
        let alpha = (f32::from(s.color.a) * s.tool.alpha()).round().clamp(0.0, 255.0) as u8;
        let color = color32(s.color.with_alpha(alpha));
        let pts: Vec<Pos2> = s.points.iter().map(|p| pos2(origin.x + p.x * zoom, origin.y + p.y * zoom)).collect();
        painter.line(pts, Stroke::new(s.width * zoom, color));
    }
}
