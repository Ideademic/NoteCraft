//! Note containers: rich text layout, the caret, typing (phase 2, canvas agent). For now it
//! paints the title and each outline's plain text.

use egui::{Align2, FontId, Painter, Pos2, Stroke, pos2, vec2};
use notecraft_doc::page::TITLE_ORIGIN;
use notecraft_doc::{ElementKind, Page};

use crate::theme::Tokens;

/// The title line, its rule and the created date/time under it.
pub(crate) fn paint_title(painter: &Painter, page: &Page, origin: Pos2, zoom: f32, tokens: &Tokens) {
    let at = origin + vec2(TITLE_ORIGIN.x, TITLE_ORIGIN.y) * zoom;
    painter.text(at, Align2::LEFT_TOP, page.title.as_str(), FontId::proportional(20.0 * zoom), tokens.page_text);
    let rule_y = at.y + 30.0 * zoom;
    painter.line_segment([pos2(at.x, rule_y), pos2(at.x + 360.0 * zoom, rule_y)], Stroke::new(1.0, tokens.title_rule));
    let when = page.created.format("%A, %d %B %Y   %H:%M").to_string();
    painter.text(pos2(at.x, rule_y + 4.0 * zoom), Align2::LEFT_TOP, when, FontId::proportional(10.0 * zoom), tokens.text_dim);
}

/// Each outline's paragraphs as plain lines (placeholder for the rich text editor).
pub(crate) fn paint_outlines(painter: &Painter, page: &Page, origin: Pos2, zoom: f32, tokens: &Tokens) {
    for el in &page.elements {
        if let ElementKind::Outline(o) = &el.kind {
            let mut y = origin.y + o.pos.y * zoom;
            for p in &o.paragraphs {
                let size = p.style.font_size() * zoom;
                painter.text(pos2(origin.x + o.pos.x * zoom, y), Align2::LEFT_TOP, p.text(), FontId::proportional(size), tokens.page_text);
                y += size * 1.5;
            }
        }
    }
}
