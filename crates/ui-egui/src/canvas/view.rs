//! Page points ↔ screen, scrolling (wheel, scroll bars), zooming (Ctrl+wheel, pinch) and how far
//! the page reaches: an open sheet that grows to fit its content.

use egui::{Pos2, Rect, Sense, Ui, Vec2, pos2, vec2};
use notecraft_doc::Point;
use notecraft_engine::Session;

use super::CanvasState;
use super::layout::PageLayout;
use crate::theme::Tokens;

/// Empty paper kept beyond the content to the right and below (points), so there is always room
/// to click and start a new note.
pub const PAPER_MARGIN: f32 = 360.0;
/// The same to the right, points.
pub const PAPER_MARGIN_X: f32 = 72.0;
/// Scroll bar thickness, pixels.
const BAR: f32 = 10.0;

/// Where the page sits on screen this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Screen position of the page's (0, 0).
    pub origin: Pos2,
    /// Screen pixels per page point.
    pub scale: f32,
    /// The visible page area.
    pub clip: Rect,
}

impl View {
    pub fn new(state: &CanvasState, sheet: Rect) -> View {
        View { origin: sheet.min - state.scroll, scale: state.scale(), clip: sheet }
    }

    pub fn to_screen(&self, p: Point) -> Pos2 {
        pos2(self.origin.x + p.x * self.scale, self.origin.y + p.y * self.scale)
    }

    pub fn pt(&self, x: f32, y: f32) -> Pos2 {
        self.to_screen(Point::new(x, y))
    }

    pub fn to_page(&self, s: Pos2) -> Point {
        let k = if self.scale > 0.0 { self.scale } else { 1.0 };
        Point::new((s.x - self.origin.x) / k, (s.y - self.origin.y) / k)
    }

    pub fn len(&self, pts: f32) -> f32 {
        pts * self.scale
    }

    pub fn rect(&self, r: notecraft_doc::Rect) -> Rect {
        Rect::from_min_size(self.pt(r.x, r.y), vec2(r.w * self.scale, r.h * self.scale))
    }
}

/// The page's extent in points (content plus margin, never smaller than the view).
pub fn extent(session: &Session, lay: Option<&PageLayout>, view_size: Vec2, scale: f32) -> Vec2 {
    let mut max = vec2(600.0, 400.0);
    if let Some(b) = session.current_page().and_then(|p| p.content_bounds()) {
        max = max.max(vec2(b.right(), b.bottom()));
    }
    if let Some(l) = lay {
        for o in &l.outlines {
            max = max.max(vec2(o.pos.x + o.width, o.pos.y + o.height_pt()));
        }
    }
    let k = scale.max(0.01);
    (max + vec2(PAPER_MARGIN_X, PAPER_MARGIN)).max(view_size / k)
}

/// Wheel scrolling and Ctrl+wheel / pinch zoom over the page (zoom keeps the point under the
/// pointer still). The scroll offset is clamped to the page's extent.
pub fn scroll_and_zoom(ui: &Ui, state: &mut CanvasState, session: &Session, sheet: Rect, resp: &egui::Response) {
    let hovered = resp.contains_pointer();
    let (zoom_delta, scroll, pointer) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
    if hovered && (zoom_delta - 1.0).abs() > 1.0e-4 {
        let before = View::new(state, sheet);
        let anchor_screen = pointer.unwrap_or(sheet.center());
        let anchor = before.to_page(anchor_screen);
        state.set_zoom(state.zoom * zoom_delta);
        let k = state.scale();
        // Keep `anchor` under the pointer: origin + anchor * k = anchor_screen.
        state.scroll = vec2(sheet.min.x + anchor.x * k - anchor_screen.x, sheet.min.y + anchor.y * k - anchor_screen.y);
    } else if hovered && scroll != Vec2::ZERO {
        state.scroll -= scroll;
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    }
    clamp_scroll(state, session, sheet);
}

/// Keeps the scroll offset on the page.
pub fn clamp_scroll(state: &mut CanvasState, session: &Session, sheet: Rect) {
    let k = state.scale();
    let ext = extent(session, None, sheet.size(), k) * k;
    let max = (ext - sheet.size()).max(Vec2::ZERO);
    let fix = |v: f32, m: f32| if v.is_finite() { v.clamp(0.0, m) } else { 0.0 };
    state.scroll = vec2(fix(state.scroll.x, max.x), fix(state.scroll.y, max.y));
}

/// Scrolls so `r` (screen) is inside the sheet, with a little room around it.
pub fn reveal(state: &mut CanvasState, sheet: Rect, r: Rect) {
    let pad = 24.0;
    if r.bottom() > sheet.bottom() - pad {
        state.scroll.y += r.bottom() - (sheet.bottom() - pad);
    } else if r.top() < sheet.top() + pad {
        state.scroll.y -= (sheet.top() + pad) - r.top();
    }
    if r.right() > sheet.right() - pad {
        state.scroll.x += r.right() - (sheet.right() - pad);
    } else if r.left() < sheet.left() + pad {
        state.scroll.x -= (sheet.left() + pad) - r.left();
    }
    state.scroll = state.scroll.max(Vec2::ZERO);
}

/// Thin flat scroll bars along the right and bottom edges, draggable. Hidden when the page fits.
pub fn paint_scrollbars(ui: &mut Ui, state: &mut CanvasState, session: &Session, tokens: &Tokens, sheet: Rect, v: &View, lay: &PageLayout) {
    let ext = extent(session, Some(lay), sheet.size(), v.scale) * v.scale;
    let painter = ui.painter_at(sheet);
    for vertical in [true, false] {
        let (len, view_len, off) = if vertical { (ext.y, sheet.height(), state.scroll.y) } else { (ext.x, sheet.width(), state.scroll.x) };
        if len <= view_len + 1.0 {
            continue;
        }
        let track = if vertical {
            Rect::from_min_max(pos2(sheet.right() - BAR, sheet.top()), pos2(sheet.right(), sheet.bottom() - BAR))
        } else {
            Rect::from_min_max(pos2(sheet.left(), sheet.bottom() - BAR), pos2(sheet.right() - BAR, sheet.bottom()))
        };
        let track_len = if vertical { track.height() } else { track.width() };
        let thumb_len = (view_len / len * track_len).clamp(24.0, track_len);
        let travel = (track_len - thumb_len).max(1.0);
        let max_off = (len - view_len).max(1.0);
        let at = (off / max_off).clamp(0.0, 1.0) * travel;
        let thumb = if vertical {
            Rect::from_min_size(pos2(track.left() + 2.0, track.top() + at), vec2(BAR - 4.0, thumb_len))
        } else {
            Rect::from_min_size(pos2(track.left() + at, track.top() + 2.0), vec2(thumb_len, BAR - 4.0))
        };
        let id = ui.scope_id().with(("nc_canvas_bar", vertical));
        let r = ui.interact(track, id, Sense::click_and_drag());
        if r.dragged() {
            let d = if vertical { r.drag_delta().y } else { r.drag_delta().x };
            let delta = d / travel * max_off;
            if vertical {
                state.scroll.y = (state.scroll.y + delta).clamp(0.0, max_off);
            } else {
                state.scroll.x = (state.scroll.x + delta).clamp(0.0, max_off);
            }
        } else if r.clicked()
            && let Some(p) = r.interact_pointer_pos()
        {
            // Page up / down towards the click.
            let before = if vertical { p.y < thumb.top() } else { p.x < thumb.left() };
            let step = if before { -view_len * 0.9 } else { view_len * 0.9 };
            if vertical {
                state.scroll.y = (state.scroll.y + step).clamp(0.0, max_off);
            } else {
                state.scroll.x = (state.scroll.x + step).clamp(0.0, max_off);
            }
        }
        let fill = if r.hovered() || r.dragged() { tokens.border_strong } else { tokens.border };
        painter.rect_filled(thumb, crate::theme::RADIUS_SMALL, fill);
    }
}
