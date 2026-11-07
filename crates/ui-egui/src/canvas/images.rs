//! Pictures, printouts and attached files: textures, placement, resize handles (phase 2, canvas
//! agent). For now it outlines where each one sits.

use egui::{Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use notecraft_doc::{ElementKind, Page};

use crate::theme::Tokens;

pub(crate) fn paint(painter: &Painter, page: &Page, origin: Pos2, zoom: f32, tokens: &Tokens) {
    for el in &page.elements {
        if matches!(el.kind, ElementKind::Outline(_)) {
            continue;
        }
        if let Some(b) = el.bounds() {
            let r = Rect::from_min_size(pos2(origin.x + b.x * zoom, origin.y + b.y * zoom), vec2(b.w * zoom, b.h * zoom));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, tokens.container_border), StrokeKind::Inside);
        }
    }
}
