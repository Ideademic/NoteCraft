//! The pointer with the Type and Select tools, OneNote's way: a click on empty paper puts the
//! caret there (a note container appears when you type); a click in a container places the caret
//! (Shift extends, double-click takes a word, triple-click the paragraph) and a drag selects;
//! the grip bar moves the container, the arrow at its right end changes its width; a click on a
//! To Do box ticks it. Pictures (and, with Select, printouts) are selected by a click, dragged to
//! move and resized from their corners (keeping their shape). With Select, a click on ink
//! selects the stroke and a drag moves it. The drawing tools are in [`super::ink`].
//!
//! Drags show a preview and commit one command on release (`outline.move`, `outline.resize`,
//! `draw.moveSelection`): one undo step each.

use egui::{CursorIcon, Pos2, Rect, Ui};
use notecraft_doc::{Guid, InkPoint, InkTool, Point};
use notecraft_engine::{Session, TextPos, ids};
use serde_json::json;

use super::layout::{self, PageLayout};
use super::view::View;
use super::{CanvasState, PendingContainer, TextCaret, Tool, edit, images, ink, run, text};

/// A drag in progress.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Gesture {
    #[default]
    None,
    /// Selecting text in a container.
    SelectText { outline: Guid },
    /// Moving an element: where the pointer was pressed and is now (page points).
    Move { id: Guid, from: Point, to: Point },
    /// Changing a container's width.
    Resize { id: Guid, start_w: f32, from_x: f32, to_x: f32 },
    /// Resizing a picture from corner `corner` (0 top-left, clockwise): the rect so far.
    ImageResize { id: Guid, corner: u8, start: notecraft_doc::Rect, rect: notecraft_doc::Rect },
    /// Drawing a stroke (page points with pressure).
    Ink { tool: InkTool, points: Vec<InkPoint> },
    /// The eraser: the last point erased from and its undo key.
    Erase { key: String, last: Point },
    /// Drawing a lasso loop.
    Lasso { points: Vec<Point> },
    /// Dragging the ink selection.
    InkMove { from: Point, to: Point },
}

/// Whether a drag is under way.
pub fn is_dragging(state: &CanvasState) -> bool {
    !matches!(state.gesture, Gesture::None)
}

/// The element a move or resize drag is changing.
pub fn dragged_element(state: &CanvasState) -> Option<Guid> {
    match &state.gesture {
        Gesture::Move { id, .. } | Gesture::Resize { id, .. } | Gesture::ImageResize { id, .. } => Some(*id),
        _ => None,
    }
}

/// An element's position and outline width as the drag in progress would leave them.
pub fn preview_geometry(state: &CanvasState, id: Guid, pos: Point, width: f32) -> (Point, f32) {
    match &state.gesture {
        Gesture::Move { id: d, from, to } if *d == id => (Point::new(pos.x + to.x - from.x, pos.y + to.y - from.y), width),
        Gesture::Resize { id: d, start_w, from_x, to_x } if *d == id => (pos, (start_w + to_x - from_x).max(36.0)),
        _ => (pos, width),
    }
}

/// A picture's rect as the drag in progress would leave it.
pub fn preview_rect(state: &CanvasState, id: Guid, r: notecraft_doc::Rect) -> notecraft_doc::Rect {
    match &state.gesture {
        Gesture::Move { id: d, from, to } if *d == id => r.translate(to.x - from.x, to.y - from.y),
        Gesture::ImageResize { id: d, rect, .. } if *d == id => *rect,
        _ => r,
    }
}

/// The ink selection's offset while it is dragged.
pub fn ink_drag_offset(state: &CanvasState) -> (f32, f32) {
    match &state.gesture {
        Gesture::InkMove { from, to } => (to.x - from.x, to.y - from.y),
        _ => (0.0, 0.0),
    }
}

struct Ptr {
    pressed: bool,
    down: bool,
    released: bool,
    pos: Option<Pos2>,
    clicks: u8,
    shift: bool,
}

fn read(ui: &Ui) -> Ptr {
    ui.input(|i| {
        let clicks = if i.pointer.button_triple_clicked(egui::PointerButton::Primary) {
            3
        } else if i.pointer.button_double_clicked(egui::PointerButton::Primary) {
            2
        } else {
            1
        };
        Ptr {
            pressed: i.pointer.primary_pressed(),
            down: i.pointer.primary_down(),
            released: i.pointer.primary_released(),
            pos: i.pointer.interact_pos().or(i.pointer.latest_pos()),
            clicks,
            shift: i.modifiers.shift,
        }
    })
}

/// Handles this frame's pointer on the page.
pub fn handle(ui: &mut Ui, state: &mut CanvasState, session: &mut Session, v: &View, lay: &PageLayout, resp: &egui::Response, title: Option<Rect>) {
    let p = read(ui);
    let Some(at) = p.pos else { return };
    let on_title = title.is_some_and(|t| t.contains(at)) && state.tool.types();
    let starts = p.pressed && resp.hovered() && v.clip.contains(at) && !on_title;
    if !state.tool.types() {
        ink::pointer(ui, state, session, v, (starts, p.down, p.released, at), resp);
        return;
    }
    // Double and triple clicks arrive as a second/third press: extend the caret's selection.
    if p.clicks > 1
        && starts
        && let Some(c) = state.caret.clone()
        && let Some(o) = lay.outline(c.outline)
        && o.rect(v).expand(text::FRAME_PAD_X).contains(at)
    {
        let pos = o.hit(v, at);
        let (a, b) = if p.clicks == 3 { para_range(session, c.outline, pos) } else { word_range(session, c.outline, pos) };
        set_caret(state, TextCaret { anchor: a, head: b, ..c });
        state.gesture = Gesture::None;
        return;
    }
    if starts {
        press(ui, state, session, v, lay, at, p.shift);
    } else if p.down {
        drag(state, session, v, lay, at);
    }
    if p.released {
        release(state, session);
    }
}

fn set_caret(state: &mut CanvasState, c: TextCaret) {
    state.editing = Some(c.outline);
    state.caret = Some(c);
}

fn word_range(session: &Session, outline: Guid, pos: TextPos) -> (TextPos, TextPos) {
    let Some(o) = session.current_page().and_then(|p| p.outline(outline)) else { return (pos, pos) };
    if let Some(r) = notecraft_engine::text::word_at(o, pos) {
        return r;
    }
    // At a word's edge (or on a space): the word after, else the one before.
    let text: Vec<char> = o.paragraphs.get(pos.para).map(|p| p.text().chars().collect()).unwrap_or_default();
    let is_word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '\'');
    let ch = pos.char.min(text.len());
    let (mut a, mut b) = (ch, ch);
    if is_word(text.get(ch)) {
        while is_word(text.get(b)) {
            b += 1;
        }
    } else if ch > 0 && is_word(text.get(ch - 1)) {
        while a > 0 && is_word(text.get(a - 1)) {
            a -= 1;
        }
    } else {
        // Whitespace: select the run of spaces.
        while b < text.len() && !is_word(text.get(b)) {
            b += 1;
        }
    }
    (TextPos::new(pos.para, a), TextPos::new(pos.para, b))
}

fn para_range(session: &Session, outline: Guid, pos: TextPos) -> (TextPos, TextPos) {
    let len =
        session.current_page().and_then(|p| p.outline(outline)).and_then(|o| o.paragraphs.get(pos.para)).map_or(0, notecraft_engine::text::para_len);
    (TextPos::new(pos.para, 0), TextPos::new(pos.para, len))
}

fn press(ui: &Ui, state: &mut CanvasState, session: &mut Session, v: &View, lay: &PageLayout, at: Pos2, shift: bool) {
    let page_pt = v.to_page(at);
    // 1. Handles of the selected picture.
    if let Some((id, corner, r)) = images::handle_at(state, session, v, at) {
        state.gesture = Gesture::ImageResize { id, corner, start: r, rect: r };
        return;
    }
    // 2. Note containers (top-most first).
    for o in lay.outlines.iter().rev() {
        let (frame, grip, resize) = text::frame_rects(o, v);
        let shown = state.caret.as_ref().is_some_and(|c| c.outline == o.id) || frame.contains(at) || grip.contains(at);
        if !shown {
            continue;
        }
        if resize.contains(at) {
            state.gesture = Gesture::Resize { id: o.id, start_w: o.width, from_x: page_pt.x, to_x: page_pt.x };
            return;
        }
        if grip.contains(at) {
            state.selection.elements = vec![o.id];
            state.gesture = Gesture::Move { id: o.id, from: page_pt, to: page_pt };
            return;
        }
        if !frame.contains(at) {
            continue;
        }
        for i in 0..o.paras.len() {
            if layout::todo_box(o, v, i).is_some_and(|r| r.expand(2.0).contains(at)) {
                let page = state.page;
                run(state, session, ids::FORMAT_TOGGLE_TODO, json!({"outline": o.id, "para": i, "page": page}));
                return;
            }
        }
        let pos = o.hit(v, at);
        let same = state.caret.as_ref().is_some_and(|c| c.outline == o.id);
        if !same {
            edit::leave(state, session, Some(o.id));
        }
        state.pending = None;
        state.selection.elements.clear();
        clear_ink_selection(state, session);
        let caret = match state.caret.clone() {
            Some(c) if same && shift => TextCaret { head: pos, goal_x: None, ..c },
            Some(c) if same => TextCaret { anchor: pos, head: pos, goal_x: None, ..c },
            _ => TextCaret::at(o.id, pos),
        };
        state.pending_style = super::PendingStyle::default();
        session.break_undo_merge();
        set_caret(state, caret);
        state.gesture = Gesture::SelectText { outline: o.id };
        ui.memory_mut(|m| m.request_focus(ui.scope_id().with("nc_canvas")));
        return;
    }
    // 3. Pictures, attached files and printouts.
    if let Some(id) = images::element_at(state, session, v, at) {
        edit::leave(state, session, None);
        state.pending = None;
        clear_ink_selection(state, session);
        state.selection.elements = vec![id];
        state.gesture = Gesture::Move { id, from: page_pt, to: page_pt };
        return;
    }
    // 4. Ink, with Select.
    if state.tool == Tool::Select
        && let Some(stroke) = ink::stroke_at(session, v, at)
    {
        edit::leave(state, session, None);
        state.pending = None;
        state.selection.elements.clear();
        if !session.ink_selection.contains(&stroke)
            && let Some(out) = run(state, session, ids::DRAW_LASSO_SELECT, json!({"ids": [stroke], "add": shift}))
        {
            state.selection.strokes = ids_of(&out.value);
        }
        state.gesture = Gesture::InkMove { from: page_pt, to: page_pt };
        return;
    }
    // 5. Empty paper: the caret waits there for a new container.
    edit::leave(state, session, None);
    state.selection.elements.clear();
    clear_ink_selection(state, session);
    state.pending_style = super::PendingStyle::default();
    if page_pt.y > 0.0 && page_pt.x > 0.0 {
        state.pending = Some(PendingContainer { at: page_pt, merge_key: format!("new:{}", Guid::new()) });
        ui.memory_mut(|m| m.request_focus(ui.scope_id().with("nc_canvas")));
    }
}

pub(crate) fn ids_of(v: &serde_json::Value) -> Vec<Guid> {
    v.get("ids").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().and_then(Guid::parse)).collect()).unwrap_or_default()
}

pub(crate) fn clear_ink_selection(state: &mut CanvasState, session: &mut Session) {
    if !session.ink_selection.is_empty() {
        run(state, session, ids::DRAW_LASSO_SELECT, json!({"points": []}));
    }
    state.selection.strokes.clear();
}

fn drag(state: &mut CanvasState, session: &Session, v: &View, lay: &PageLayout, at: Pos2) {
    let p = v.to_page(at);
    match &mut state.gesture {
        Gesture::SelectText { outline } => {
            let id = *outline;
            if let Some(o) = lay.outline(id) {
                let pos = o.hit(v, at);
                if let Some(c) = state.caret.as_mut().filter(|c| c.outline == id) {
                    c.head = pos;
                    c.goal_x = None;
                }
            }
        }
        Gesture::Move { to, .. } | Gesture::InkMove { to, .. } => *to = p,
        Gesture::Resize { to_x, .. } => *to_x = p.x,
        Gesture::ImageResize { corner, start, rect, .. } => *rect = images::resized(*start, *corner, p),
        _ => {}
    }
    let _ = session;
}

fn release(state: &mut CanvasState, session: &mut Session) {
    let g = std::mem::take(&mut state.gesture);
    let page = state.page;
    match g {
        Gesture::Move { id, from, to } => {
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            if dx.abs() + dy.abs() > 0.25
                && let Some(pos) = session.current_page().and_then(|pg| pg.element(id)).and_then(|e| e.pos())
            {
                let (x, y) = ((pos.x + dx).max(0.0), (pos.y + dy).max(0.0));
                run(state, session, ids::OUTLINE_MOVE, json!({"id": id, "x": x, "y": y, "page": page}));
            }
        }
        Gesture::Resize { id, start_w, from_x, to_x } => {
            let w = (start_w + to_x - from_x).max(36.0);
            if (w - start_w).abs() > 0.25 {
                run(state, session, ids::OUTLINE_RESIZE, json!({"id": id, "width": w, "page": page}));
            }
        }
        Gesture::ImageResize { id, start, rect, .. } => {
            if rect != start {
                run(state, session, ids::OUTLINE_RESIZE, json!({"id": id, "width": rect.w, "height": rect.h, "page": page}));
                if (rect.x, rect.y) != (start.x, start.y) {
                    run(state, session, ids::OUTLINE_MOVE, json!({"id": id, "x": rect.x, "y": rect.y, "page": page}));
                }
            }
        }
        Gesture::InkMove { from, to } => {
            let (dx, dy) = (to.x - from.x, to.y - from.y);
            if dx.abs() + dy.abs() > 0.25 {
                run(state, session, ids::DRAW_MOVE_SELECTION, json!({"dx": dx, "dy": dy}));
            }
        }
        other => state.gesture = other,
    }
    if matches!(state.gesture, Gesture::SelectText { .. }) {
        state.gesture = Gesture::None;
    }
}

/// The pointer's shape over the page.
pub fn cursor(ui: &Ui, state: &CanvasState, session: &Session, v: &View, lay: &PageLayout, resp: &egui::Response) {
    if !resp.hovered() && !is_dragging(state) {
        return;
    }
    let Some(at) = resp.hover_pos().or_else(|| ui.input(|i| i.pointer.latest_pos())) else { return };
    let icon = match &state.gesture {
        Gesture::Move { .. } | Gesture::InkMove { .. } => CursorIcon::Grabbing,
        Gesture::Resize { .. } => CursorIcon::ResizeHorizontal,
        Gesture::ImageResize { corner, .. } => corner_cursor(*corner),
        Gesture::SelectText { .. } => CursorIcon::Text,
        Gesture::Ink { .. } | Gesture::Lasso { .. } => CursorIcon::Crosshair,
        Gesture::Erase { .. } => CursorIcon::None,
        Gesture::None => match state.tool {
            Tool::Pen | Tool::Pencil | Tool::Highlighter | Tool::Lasso => CursorIcon::Crosshair,
            Tool::Eraser => CursorIcon::None,
            Tool::Type | Tool::Select => hover_icon(state, session, v, lay, at),
        },
    };
    ui.ctx().set_cursor_icon(icon);
}

fn corner_cursor(corner: u8) -> CursorIcon {
    if corner.is_multiple_of(2) { CursorIcon::ResizeNwSe } else { CursorIcon::ResizeNeSw }
}

fn hover_icon(state: &CanvasState, session: &Session, v: &View, lay: &PageLayout, at: Pos2) -> CursorIcon {
    if let Some(c) = images::corner_hover(state, session, v, at) {
        return corner_cursor(c);
    }
    for o in lay.outlines.iter().rev() {
        let (frame, grip, resize) = text::frame_rects(o, v);
        if resize.contains(at) {
            return CursorIcon::ResizeHorizontal;
        }
        if grip.contains(at) {
            return CursorIcon::Grab;
        }
        if frame.contains(at) {
            if (0..o.paras.len()).any(|i| layout::todo_box(o, v, i).is_some_and(|r| r.expand(2.0).contains(at))) {
                return CursorIcon::PointingHand;
            }
            return CursorIcon::Text;
        }
    }
    if images::over_picture(state, session, v, at) {
        return CursorIcon::Move;
    }
    CursorIcon::Text
}
