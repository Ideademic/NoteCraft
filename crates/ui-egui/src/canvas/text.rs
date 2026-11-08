//! Painting note containers: the selection, list markers (drawn in code), tags (the To Do box, and
//! the other tags' icons from [`crate::icons`]), the rich text (bold painted twice only for a face
//! without a bold), and — while a container is hovered, edited or
//! selected — its thin frame with the grip bar on top (drag to move) and the resize arrow at the
//! grip's right end (drag to change the width). Then the caret.

use egui::epaint::{CornerRadius, PathShape};
use egui::text::CCursor;
use egui::{Align2, Color32, FontId, Painter, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
use notecraft_doc::{ListStyle, Page, Tag};

use super::layout::{OutlineLayout, PageLayout};
use super::style::{Ink, MARKER_W, Marker};
use super::view::View;
use super::{CanvasState, pointer};
use crate::theme::{Tokens, mix};

/// Space between the text and the frame, pixels.
pub const FRAME_PAD_X: f32 = 7.0;
pub const FRAME_PAD_Y: f32 = 4.0;
/// Height of the grip bar above the frame, pixels.
pub const GRIP_H: f32 = 9.0;

/// A container's frame, its grip bar and the resize handle at the grip's right end (screen).
pub fn frame_rects(o: &OutlineLayout, v: &View) -> (Rect, Rect, Rect) {
    let text = o.rect(v);
    let frame =
        Rect::from_min_max(pos2(text.left() - FRAME_PAD_X, text.top() - FRAME_PAD_Y), pos2(text.right() + FRAME_PAD_X, text.bottom() + FRAME_PAD_Y));
    let grip = Rect::from_min_max(pos2(frame.left(), frame.top() - GRIP_H), pos2(frame.right(), frame.top()));
    let resize = Rect::from_min_max(pos2(grip.right() - 16.0, grip.top()), pos2(grip.right() + 2.0, grip.bottom()));
    (frame, grip, resize)
}

/// Everything about the note containers on the page.
pub fn paint_outlines(painter: &Painter, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View, lay: &PageLayout, resp: &egui::Response) {
    let ink = Ink::new(tokens, page.background.color);
    let hover = resp.hover_pos().filter(|_| state.tool.types() && !pointer::is_dragging(state));
    let hovered = hover.and_then(|p| {
        lay.outlines.iter().rev().find(|o| {
            let (f, g, _) = frame_rects(o, v);
            f.contains(p) || g.contains(p)
        })
    });
    let hovered_id = hovered.map(|o| o.id);
    for o in &lay.outlines {
        let r = o.rect(v);
        if !r.expand(40.0).intersects(v.clip) {
            continue;
        }
        let editing = state.caret.as_ref().filter(|c| c.outline == o.id);
        if let Some(c) = editing
            && !c.is_collapsed()
        {
            paint_selection(painter, o, v, c.range(), tokens.selection);
        }
        for (i, p) in o.paras.iter().enumerate() {
            let Some(at) = o.galley_origin(v, i) else { continue };
            if at.y > v.clip.bottom() + 4.0 || at.y + p.height < v.clip.top() - 4.0 {
                continue;
            }
            paint_marker(painter, o, v, i, &ink);
            paint_tag(painter, o, v, i, tokens);
            painter.galley(at, p.galley.clone(), ink.text);
            if let Some(b) = &p.bold {
                painter.galley(at + vec2(p.bold_dx, 0.0), b.clone(), ink.text);
                if p.size_px > 26.0 {
                    painter.galley(at + vec2(p.bold_dx * 0.5, 0.0), b.clone(), ink.text);
                }
            }
        }
        let selected = state.selection.elements.contains(&o.id);
        let dragging = pointer::dragged_element(state) == Some(o.id);
        if editing.is_some() || hovered_id == Some(o.id) || selected || dragging {
            paint_frame(painter, o, v, tokens, &ink, editing.is_some() || selected || dragging);
        }
        if let Some(c) = editing
            && let Some(cr) = o.caret_rect(v, c.head)
        {
            paint_caret(painter, cr, &ink, v);
        }
    }
    if let Some(p) = &state.pending {
        let at = v.to_screen(p.at);
        let h = painter.ctx().fonts_mut(|f| f.row_height(&FontId::proportional(super::style::px(11.0, v.scale))));
        paint_caret(painter, Rect::from_min_size(at, vec2(0.0, h)), &ink, v);
    }
}

fn paint_caret(painter: &Painter, r: Rect, ink: &Ink, v: &View) {
    let w = (v.scale * 1.1).clamp(1.0, 3.0);
    let x = r.left().round() + 0.5;
    painter.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], Stroke::new(w, ink.text));
}

/// The selection behind the text, row by row.
pub fn paint_selection(
    painter: &Painter,
    o: &OutlineLayout,
    v: &View,
    (a, b): (notecraft_engine::TextPos, notecraft_engine::TextPos),
    fill: Color32,
) {
    for i in a.para..=b.para {
        let Some(p) = o.paras.get(i) else { break };
        let Some(origin) = o.galley_origin(v, i) else { break };
        let s = if i == a.para { a.char } else { 0 };
        let e = if i == b.para { b.char } else { p.chars };
        let mut base = 0usize;
        for row in &p.galley.rows {
            let n = row.char_count_including_newline().0;
            let (rs, re) = (s.max(base), e.min(base + row.char_count_excluding_newline().0));
            if rs < re || (rs == re && i != b.para && re == base + n && s <= re) {
                for xr in row.x_ranges(egui::text::CharIndex(rs - base)..egui::text::CharIndex(re - base)) {
                    let mut rr =
                        Rect::from_min_max(pos2(row.pos.x + xr.min, row.min_y()), pos2(row.pos.x + xr.max, row.max_y())).translate(origin.to_vec2());
                    if i != b.para && re >= base + row.char_count_excluding_newline().0 {
                        // The paragraph break is selected too.
                        rr.max.x += p.size_px * 0.3;
                    }
                    painter.rect_filled(rr, 0.0, fill);
                }
            }
            base += n;
        }
        if p.chars == 0 && i != b.para {
            let cr = p.galley.pos_from_cursor(CCursor::new(0)).translate(origin.to_vec2());
            painter.rect_filled(Rect::from_min_size(cr.min, vec2(p.size_px * 0.3, cr.height())), 0.0, fill);
        }
    }
}

fn marker_color(o: &OutlineLayout, i: usize, ink: &Ink) -> Color32 {
    // The marker takes the colour of the paragraph's first character.
    o.paras.get(i).and_then(|p| p.galley.job.sections.first()).map_or(ink.text, |s| s.format.color)
}

fn paint_marker(painter: &Painter, o: &OutlineLayout, v: &View, i: usize, ink: &Ink) {
    let Some(p) = o.paras.get(i) else { return };
    let Some(m) = &p.marker else { return };
    let top = v.to_screen(o.pos);
    let (a, b) = p.first_row();
    let cy = top.y + (a + b) / 2.0;
    let color = marker_color(o, i, ink);
    match m {
        Marker::Bullet(style) => {
            let r = (p.size_px * 0.15).clamp(1.5, 12.0);
            let c = pos2(top.x + p.marker_x + MARKER_W * v.scale * 0.4, cy);
            let w = (r * 0.4).max(1.0);
            match style {
                ListStyle::Circle => {
                    painter.circle_stroke(c, r - 0.5, Stroke::new((r * 0.35).max(1.0), color));
                }
                ListStyle::Square => {
                    painter.rect_filled(Rect::from_center_size(c, vec2(r * 1.7, r * 1.7)), 0.0, color);
                }
                ListStyle::Dash => {
                    painter.line_segment([pos2(c.x - r * 1.4, c.y), pos2(c.x + r * 1.4, c.y)], Stroke::new(w, color));
                }
                ListStyle::Arrow => {
                    // A solid arrowhead pointing at the text.
                    let s = r * 1.5;
                    let pts = vec![pos2(c.x - s * 0.7, c.y - s), pos2(c.x + s * 0.9, c.y), pos2(c.x - s * 0.7, c.y + s), pos2(c.x - s * 0.2, c.y)];
                    painter.add(Shape::convex_polygon(vec![pts[0], pts[1], pts[3]], color, Stroke::NONE));
                    painter.add(Shape::convex_polygon(vec![pts[3], pts[1], pts[2]], color, Stroke::NONE));
                }
                _ => {
                    painter.circle_filled(c, r, color);
                }
            }
        }
        Marker::Number(label) => {
            let font = p.galley.job.sections.first().map_or_else(|| FontId::proportional(p.size_px), |s| s.format.font_id.clone());
            let x = top.x + p.text_x - v.len(4.0);
            let row_top = top.y + a;
            painter.text(pos2(x, row_top + (b - a) / 2.0), Align2::RIGHT_CENTER, label, font, color);
        }
    }
}

/// The To Do box (ticked in the accent colour), and the other tags' icons. The highlight tags
/// have no icon: their colour is behind the text.
fn paint_tag(painter: &Painter, o: &OutlineLayout, v: &View, i: usize, tokens: &Tokens) {
    let Some(p) = o.paras.get(i) else { return };
    let Some(tag) = p.tag.filter(|t| t.has_icon()) else { return };
    let top = v.to_screen(o.pos);
    let (a, b) = p.first_row();
    let side = (p.size_px * 0.85).clamp(6.0, 60.0);
    let c = pos2(top.x + p.tag_x + super::style::TAG_W * v.scale / 2.0, top.y + (a + b) / 2.0);
    match tag {
        Tag::ToDo { checked } => {
            let r = Rect::from_center_size(c, vec2(side, side));
            let border = if tokens.dark { Color32::from_rgb(0x9A, 0xA8, 0xC0) } else { Color32::from_rgb(0x6B, 0x7A, 0x96) };
            let fill = if tokens.dark { Color32::from_rgb(0x30, 0x30, 0x34) } else { Color32::WHITE };
            painter.rect(r, CornerRadius::same(1), fill, Stroke::new(1.0, border), StrokeKind::Inside);
            if checked {
                let s = side;
                let pts = vec![
                    pos2(r.left() + s * 0.2, r.top() + s * 0.52),
                    pos2(r.left() + s * 0.42, r.top() + s * 0.74),
                    pos2(r.left() + s * 0.84, r.top() + s * 0.2),
                ];
                painter.add(Shape::Path(PathShape::line(pts, Stroke::new((s * 0.15).max(1.5), tokens.accent))));
            }
        }
        other => {
            if let Some(icon) = crate::icons::tag_icon(other) {
                let ink = if tokens.dark { Color32::from_rgb(0xE6, 0xE6, 0xEA) } else { Color32::from_rgb(0x44, 0x44, 0x4A) };
                crate::icons::paint_styled(painter, Rect::from_center_size(c, vec2(side * 1.15, side * 1.15)), icon, crate::icons::Style::new(ink, tokens.accent));
            }
        }

    }
}

fn paint_frame(painter: &Painter, o: &OutlineLayout, v: &View, tokens: &Tokens, ink: &Ink, strong: bool) {
    let (frame, grip, resize) = frame_rects(o, v);
    let line = if ink.dark { mix(ink.paper, ink.text, 0.35) } else { tokens.container_border };
    painter.rect_stroke(frame, 0.0, Stroke::new(1.0, line), StrokeKind::Inside);
    let grip_fill = if strong { mix(ink.paper, line, 0.55) } else { mix(ink.paper, line, 0.35) };
    painter.rect(grip, 0.0, grip_fill, Stroke::new(1.0, line), StrokeKind::Inside);
    // The grip's dots in the middle.
    let dot = mix(grip_fill, ink.text, 0.45);
    let cy = grip.center().y;
    for k in -3..=3 {
        painter.circle_filled(pos2(grip.center().x + k as f32 * 4.0, cy), 0.9, dot);
    }
    // The width arrow at the right end.
    let rc = resize.center();
    let s = Stroke::new(1.0, dot);
    painter.line_segment([pos2(rc.x - 5.0, rc.y), pos2(rc.x + 5.0, rc.y)], s);
    for dir in [-1.0f32, 1.0] {
        let tip = pos2(rc.x + dir * 5.0, rc.y);
        painter.line_segment([tip, pos2(tip.x - dir * 2.5, rc.y - 2.5)], s);
        painter.line_segment([tip, pos2(tip.x - dir * 2.5, rc.y + 2.5)], s);
    }
}
