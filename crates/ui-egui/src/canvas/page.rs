//! The paper: page colour, rule lines and grids (light blue, with a red margin line beside rule
//! lines, as on ruled paper), and the title block: the editable title over a thin rule, with the
//! date and time the page was created under it in grey.

use egui::{Align2, FontId, Margin, Painter, Rect, Stroke, TextEdit, Ui, pos2, vec2};
use notecraft_doc::Page;
use notecraft_doc::page::TITLE_ORIGIN;
use notecraft_engine::Session;
use serde_json::json;

use super::style::{self, Ink};
use super::view::View;
use super::{CanvasState, edit};
use crate::theme::{Tokens, mix};

/// Title font size, points.
pub const TITLE_PT: f32 = 20.0;
/// Shortest title rule, points.
pub const TITLE_RULE_MIN: f32 = 330.0;
/// Date line font size, points.
pub const DATE_PT: f32 = 9.5;
/// Where the red margin line of rule lines sits, points from the left.
pub const MARGIN_X: f32 = 24.0;

/// The page's area inside the canvas: the section-tinted band shows as a thin frame.
pub fn sheet_rect(rect: Rect) -> Rect {
    Rect::from_min_max(pos2(rect.left() + 4.0, rect.top() + 3.0), pos2(rect.right() - 4.0, rect.bottom())).intersect(rect)
}

/// Paper colour and rule lines.
pub fn paint_background(painter: &Painter, page: &Page, tokens: &Tokens, v: &View, sheet: Rect) {
    let ink = Ink::new(tokens, page.background.color);
    painter.rect_filled(sheet, 0.0, ink.paper);
    let Some(spacing) = page.background.rule.spacing() else { return };
    let (line, margin) = if page.background.color.is_some() {
        (mix(ink.paper, tokens.blue, 0.22), mix(ink.paper, tokens.red, 0.35))
    } else {
        (tokens.rule_line, tokens.rule_margin)
    };
    let step = v.len(spacing);
    if step < 3.0 {
        return;
    }
    let grid = page.background.rule.is_grid();
    let stroke = Stroke::new(1.0, line);
    // Horizontal lines from the top of the page down.
    let first = ((sheet.top() - v.origin.y) / step).floor().max(1.0) as i64;
    let last = ((sheet.bottom() - v.origin.y) / step).ceil() as i64;
    for k in first..=last.min(first + 2000) {
        let y = (v.origin.y + k as f32 * step).round() + 0.5;
        painter.line_segment([pos2(sheet.left(), y), pos2(sheet.right(), y)], stroke);
    }
    if grid {
        let first = ((sheet.left() - v.origin.x) / step).floor().max(1.0) as i64;
        let last = ((sheet.right() - v.origin.x) / step).ceil() as i64;
        for k in first..=last.min(first + 2000) {
            let x = (v.origin.x + k as f32 * step).round() + 0.5;
            painter.line_segment([pos2(x, sheet.top()), pos2(x, sheet.bottom())], stroke);
        }
    } else {
        let x = (v.origin.x + v.len(MARGIN_X)).round() + 0.5;
        painter.line_segment([pos2(x, sheet.top()), pos2(x, sheet.bottom())], Stroke::new(1.0, margin));
    }
}

/// The title's text box and the rule under it, on screen.
pub fn title_rect(ctx: &egui::Context, page: &Page, v: &View) -> Rect {
    let font = FontId::proportional(style::px(TITLE_PT, v.scale));
    let w = ctx.fonts_mut(|f| f.layout_no_wrap(page.title.clone(), font.clone(), egui::Color32::BLACK).size().x);
    let h = ctx.fonts_mut(|f| f.row_height(&font));
    let width = (w + v.len(12.0)).max(v.len(TITLE_RULE_MIN));
    Rect::from_min_size(v.pt(TITLE_ORIGIN.x, TITLE_ORIGIN.y), vec2(width, h + v.len(4.0)))
}

fn date_text(page: &Page, offset_minutes: i32) -> (String, String) {
    // Checked: a hostile file's date near chrono's limits (or a junk offset) must not overflow.
    let minutes = offset_minutes.clamp(-14 * 60, 14 * 60);
    match chrono::FixedOffset::east_opt(minutes * 60) {
        Some(offset) => {
            let t = page.created.with_timezone(&offset);
            (t.format("%A, %B %-d, %Y").to_string(), t.format("%-I:%M %p").to_string())
        }
        None => (page.created.format("%A, %B %-d, %Y").to_string(), page.created.format("%-I:%M %p").to_string()),
    }
}

/// The rule under the title and the creation date and time.
pub fn paint_title(painter: &Painter, ctx: &egui::Context, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View) {
    if state.hide_title {
        return;
    }
    let ink = Ink::new(tokens, page.background.color);
    let r = title_rect(ctx, page, v);
    let rule_y = r.bottom().round() + 0.5;
    let rule = if ink.dark { mix(ink.paper, ink.text, 0.35) } else { tokens.title_rule };
    painter.line_segment([pos2(r.left(), rule_y), pos2(r.right(), rule_y)], Stroke::new(1.0, rule));
    let (date, time) = date_text(page, state.utc_offset_minutes);
    let font = FontId::proportional(style::px(DATE_PT, v.scale));
    let grey = if ink.dark { mix(ink.paper, ink.text, 0.6) } else { mix(ink.paper, egui::Color32::from_rgb(0x40, 0x40, 0x40), 0.8) };
    let at = pos2(r.left(), rule_y + v.len(5.0));
    let dr = painter.text(at, Align2::LEFT_TOP, date, font.clone(), grey);
    painter.text(pos2(dr.right() + v.len(22.0), at.y), Align2::LEFT_TOP, time, font, grey);
}

/// The editable title (an egui text box without a frame on the title line). Edits rename the
/// page (`page.rename`, one undo step while typing); Enter or Esc leaves it.
pub fn title_editor(ui: &mut Ui, state: &mut CanvasState, session: &mut Session, tokens: &Tokens, v: &View) -> Option<egui::Response> {
    if state.hide_title {
        return None;
    }
    let page = session.current_page()?;
    let page_id = page.id;
    let ink = Ink::new(tokens, page.background.color);
    let rect = title_rect(ui.ctx(), page, v).intersect(v.clip);
    if rect.width() <= 1.0 || rect.height() <= 1.0 {
        return None;
    }
    let id = ui.scope_id().with(("nc_title", page_id));
    let focused = ui.memory(|m| m.has_focus(id));
    if !focused || state.title_buf.is_none() {
        state.title_buf = Some(page.title.clone());
    }
    let mut buf = state.title_buf.clone().unwrap_or_default();
    let font = FontId::proportional(style::px(TITLE_PT, v.scale));
    let edit = TextEdit::singleline(&mut buf)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(Margin::ZERO)
        .font(font)
        .text_color(ink.text)
        .desired_width(rect.width());
    let resp = ui.put(rect, edit);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, crate::tl!("Page title")));
    if resp.gained_focus() {
        edit::leave(state, session, None);
    }
    if resp.changed() {
        let title: String = buf.chars().filter(|c| !c.is_control()).take(1000).collect();
        let _ = super::run(
            state,
            session,
            notecraft_engine::ids::PAGE_RENAME,
            json!({"id": page_id, "title": title, "mergeKey": format!("title:{page_id}")}),
        );
        buf = title;
    }
    if resp.lost_focus() {
        session.break_undo_merge();
    }
    state.title_buf = Some(buf);
    Some(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_like_onenote() {
        let p = Page::new("x");
        let (d, t) = date_text(&p, 60);
        assert!(d.contains(", 20"), "{d}");
        assert!(t.ends_with("AM") || t.ends_with("PM"), "{t}");
    }

    #[test]
    fn dates_at_chronos_limits_do_not_overflow() {
        let mut p = Page::new("x");
        for (when, offset) in [(chrono::DateTime::<chrono::Utc>::MAX_UTC, i32::MAX), (chrono::DateTime::<chrono::Utc>::MIN_UTC, i32::MIN)] {
            p.created = when;
            let (d, _) = date_text(&p, offset);
            assert!(!d.is_empty());
        }
    }
}
