//! The page canvas: the title line, note containers of rich text ([`text`], [`edit`]), pictures
//! and printouts ([`images`]) and the ink layer ([`ink`]), with rule lines and page colour behind
//! ([`page`]).
//!
//! **Contract with the shell.** The ribbon and panes never reach into canvas internals: they
//! read and set [`CanvasState`] (tool, pen settings, zoom, the pending style) and call [`show`]
//! once per frame for the page area. Everything the canvas changes in the notebook goes through
//! `Session::run` commands (`outline.new`, `text.insert`, `draw.stroke`…). The ribbon aims its
//! formatting buttons with [`selection_params`] / [`format_command`] and shows toggle states
//! with [`current_format`].
//!
//! Units: page positions are points; on screen a point is [`PX_PER_PT`] logical pixels at 100 %
//! zoom (96 dpi, like Office), so `scale = zoom * PX_PER_PT`.

pub mod drop;
pub mod edit;
pub mod images;
pub mod ink;
pub mod layout;
pub mod page;
pub mod pointer;
pub mod shape;
pub mod style;
pub mod table;
pub mod text;
pub mod view;

#[cfg(test)]
mod tests;

use egui::{Pos2, Sense};
use notecraft_doc::{Color, Guid, InkTool, Point};
pub use notecraft_engine::cmd::shapes::ShapeKind;
use notecraft_engine::text::{CellRange, CellRef, Frame};
use notecraft_engine::{CmdError, CmdOutput, Session, TextPos};
use serde_json::{Value, json};

use crate::services::Services;
use crate::theme::Tokens;

/// Logical pixels per page point at 100 % zoom (96 / 72).
pub const PX_PER_PT: f32 = 96.0 / 72.0;

/// What a click or drag on the page does (Home/Draw ribbon tools).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Select and move containers, pictures and ink; click text to edit it.
    Select,
    /// Click anywhere to type (creates a note container). The default.
    #[default]
    Type,
    Pen,
    Pencil,
    Highlighter,
    /// Stroke eraser: removes whole strokes it touches.
    Eraser,
    /// Lasso select ink.
    Lasso,
    /// Draw › Shapes: drag to insert this shape.
    Shape(ShapeKind),
}

impl Tool {
    /// The ink tool behind a drawing tool.
    pub fn ink_tool(self) -> Option<InkTool> {
        match self {
            Tool::Pen => Some(InkTool::Pen),
            Tool::Pencil => Some(InkTool::Pencil),
            Tool::Highlighter => Some(InkTool::Highlighter),
            _ => None,
        }
    }

    pub fn is_drawing(self) -> bool {
        self.ink_tool().is_some()
    }

    /// Tools whose clicks place the caret and create note containers.
    pub fn types(self) -> bool {
        matches!(self, Tool::Type | Tool::Select)
    }
}

/// Colour and width of one drawing tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenStyle {
    pub color: Color,
    /// Points.
    pub width: f32,
}

impl PenStyle {
    pub fn default_for(tool: InkTool) -> PenStyle {
        PenStyle { color: tool.default_color(), width: tool.default_width() }
    }
}

/// The Draw tab's settings for each drawing tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenSettings {
    pub pen: PenStyle,
    pub pencil: PenStyle,
    pub highlighter: PenStyle,
}

impl Default for PenSettings {
    fn default() -> Self {
        PenSettings {
            pen: PenStyle::default_for(InkTool::Pen),
            pencil: PenStyle::default_for(InkTool::Pencil),
            highlighter: PenStyle::default_for(InkTool::Highlighter),
        }
    }
}

impl PenSettings {
    pub fn style(&self, tool: InkTool) -> PenStyle {
        match tool {
            InkTool::Pen => self.pen,
            InkTool::Pencil => self.pencil,
            InkTool::Highlighter => self.highlighter,
        }
    }

    pub fn style_mut(&mut self, tool: InkTool) -> &mut PenStyle {
        match tool {
            InkTool::Pen => &mut self.pen,
            InkTool::Pencil => &mut self.pencil,
            InkTool::Highlighter => &mut self.highlighter,
        }
    }
}

/// What is selected on the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanvasSelection {
    /// Selected elements (outlines, pictures, printouts, files).
    pub elements: Vec<Guid>,
    /// Selected ink strokes (mirrors `Session::ink_selection`, which the draw commands use).
    pub strokes: Vec<Guid>,
}

impl CanvasSelection {
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.strokes.is_empty()
    }
    pub fn clear(&mut self) {
        self.elements.clear();
        self.strokes.clear();
    }
}

/// Character formatting chosen while the caret has no selection (Ctrl+B before typing): it is
/// passed as `text.insert`'s `style` and forgotten when the caret moves. `None` = unchanged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PendingStyle {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    /// `Some(None)` removes the highlight.
    pub highlight: Option<Option<Color>>,
    /// `Some(None)` is the automatic colour.
    pub color: Option<Option<Color>>,
    pub font: Option<Option<String>>,
    pub size: Option<Option<f32>>,
}

impl PendingStyle {
    pub fn is_empty(&self) -> bool {
        *self == PendingStyle::default()
    }

    /// `text.insert`'s `style` object (only the keys that are set).
    pub fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        let hex = |c: Option<Color>| c.map_or(Value::Null, |c| Value::String(c.to_hex()));
        for (k, v) in [("bold", self.bold), ("italic", self.italic), ("underline", self.underline), ("strike", self.strike)] {
            if let Some(v) = v {
                m.insert(k.into(), Value::Bool(v));
            }
        }
        if let Some(h) = self.highlight {
            m.insert("highlight".into(), hex(h));
        }
        if let Some(c) = self.color {
            m.insert("color".into(), hex(c));
        }
        if let Some(f) = &self.font {
            m.insert("font".into(), f.clone().map_or(Value::Null, Value::String));
        }
        if let Some(s) = self.size {
            m.insert("size".into(), s.map_or(Value::Null, |s| json!(s)));
        }
        Value::Object(m)
    }
}

/// The caret in a note container: `head` is where it blinks, `anchor` the other end of the
/// selection (equal when nothing is selected). Both are in one flow: the container's own
/// paragraphs, or the table cell `cell`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCaret {
    pub outline: Guid,
    pub anchor: TextPos,
    pub head: TextPos,
    /// The undo merge key for typing in this container (its id, or the key it was created with).
    pub merge_key: String,
    /// Remembered x (screen px from the outline's left) for Up/Down.
    pub goal_x: Option<f32>,
    /// The table cell the caret is in (`None`: the container's own paragraphs).
    pub cell: Option<CellRef>,
    /// Whole cells selected (a drag across cells, Table ▸ Select); the caret stays in `cell`.
    pub cells: Option<CellRange>,
}

impl TextCaret {
    pub fn at(outline: Guid, pos: TextPos) -> TextCaret {
        TextCaret { outline, anchor: pos, head: pos, merge_key: outline.to_string(), goal_x: None, cell: None, cells: None }
    }

    /// A caret in table cell `cell`.
    pub fn in_cell(outline: Guid, cell: CellRef, pos: TextPos) -> TextCaret {
        TextCaret { cell: Some(cell), ..TextCaret::at(outline, pos) }
    }

    /// The flow the caret is in.
    pub fn frame(&self) -> Frame {
        self.cell.map_or(Frame::Top, Frame::Cell)
    }

    /// Start and end in document order.
    pub fn range(&self) -> (TextPos, TextPos) {
        if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// A click on empty paper with the Type tool: the caret waits there and the note container is
/// only created when something is typed (so clicking around leaves no empty containers).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingContainer {
    pub at: Point,
    pub merge_key: String,
}

/// The canvas's state between frames. The shell owns one and may set any public field.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasState {
    /// 1.0 = 100 % (see `notecraft_geom::zoom`).
    pub zoom: f32,
    pub tool: Tool,
    pub pens: PenSettings,
    pub selection: CanvasSelection,
    /// The outline whose text is being edited, if any (mirrors `caret`; clearing it ends editing).
    pub editing: Option<Guid>,
    /// The page the state belongs to; a different current page resets selection and editing.
    pub page: Option<Guid>,
    /// Scroll offset of the page within the canvas, logical pixels.
    pub scroll: egui::Vec2,
    /// View › Hide Page Title (the shell copies `UiState::hide_page_title` here).
    pub hide_title: bool,
    /// Minutes east of UTC for the date and time under the title (the host sets it; 0 = UTC).
    pub utc_offset_minutes: i32,
    /// The stroke eraser's radius, points.
    pub eraser_radius: f32,
    /// Formatting for the next typed text (see [`PendingStyle`]).
    pub pending_style: PendingStyle,
    /// The caret and selection in a note container.
    pub caret: Option<TextCaret>,
    /// A container waiting to be created by typing.
    pub pending: Option<PendingContainer>,
    /// What the pointer is doing (drags in progress).
    pub(crate) gesture: pointer::Gesture,
    /// The title's text while it is being edited.
    pub(crate) title_buf: Option<String>,
    /// The keyboard moved the caret: scroll it into view.
    pub(crate) reveal_caret: bool,
    /// Last frame's canvas rect and view origin (tests and the shell's Fit Page Width).
    pub last_rect: Option<egui::Rect>,
    pub(crate) last_origin: Option<Pos2>,
    /// Status messages the canvas wants shown (errors from commands); the shell may drain them.
    pub messages: Vec<String>,
}

impl Default for CanvasState {
    fn default() -> Self {
        CanvasState {
            zoom: 1.0,
            tool: Tool::default(),
            pens: PenSettings::default(),
            selection: CanvasSelection::default(),
            editing: None,
            page: None,
            scroll: egui::Vec2::ZERO,
            hide_title: false,
            utc_offset_minutes: 0,
            eraser_radius: notecraft_engine::cmd::draw::ERASER_RADIUS,
            pending_style: PendingStyle::default(),
            caret: None,
            pending: None,
            gesture: pointer::Gesture::None,
            title_buf: None,
            reveal_caret: false,
            last_rect: None,
            last_origin: None,
            messages: Vec::new(),
        }
    }
}

impl CanvasState {
    /// Sets the zoom, clamped to `notecraft_geom::zoom::{MIN, MAX}`.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = notecraft_geom::zoom::clamp(zoom);
    }

    /// Switches tool, ending text editing and clearing a selection the new tool can't use.
    pub fn set_tool(&mut self, tool: Tool) {
        if tool != self.tool {
            self.editing = None;
            self.pending = None;
            if tool != Tool::Select && tool != Tool::Lasso {
                self.selection.clear();
            }
            self.gesture = pointer::Gesture::None;
        }
        self.tool = tool;
    }

    /// The pen style of the current tool, if it draws.
    pub fn current_pen(&self) -> Option<PenStyle> {
        self.tool.ink_tool().map(|t| self.pens.style(t))
    }

    /// Screen pixels per page point.
    pub fn scale(&self) -> f32 {
        notecraft_geom::zoom::clamp(self.zoom) * PX_PER_PT
    }
}

/// The formatting command params for the caret's selection: `{page, outline, start, end}`;
/// `None` when no note container is being edited.
pub fn selection_params(state: &CanvasState) -> Option<Value> {
    let c = state.caret.as_ref()?;
    let (a, b) = c.range();
    let mut v = json!({"outline": c.outline, "start": a.to_json(), "end": b.to_json()});
    if let Value::Object(m) = &mut v {
        if let Some(page) = state.page {
            m.insert("page".into(), json!(page));
        }
        if let Some(cell) = c.cell {
            m.insert("cell".into(), cell.to_json());
        }
        if let Some(cells) = c.cells {
            m.insert("cells".into(), cells.to_json());
        }
    }
    Some(v)
}

/// The table the caret is in, for the Table tab: the outline, the caret's cell and the selected
/// cells. `None` outside a table.
pub fn table_context(state: &CanvasState) -> Option<(Guid, CellRef, Option<CellRange>)> {
    let c = state.caret.as_ref()?;
    Some((c.outline, c.cell?, c.cells))
}

/// Runs a Table tab command (`table.*`) on the caret's table and moves the caret as it says.
/// `None` when the caret isn't in a table.
pub fn table_command(state: &mut CanvasState, session: &mut Session, id: &str, extra: Value) -> Option<Result<CmdOutput, CmdError>> {
    table::command(state, session, id, extra)
}

/// Inserts a `rows` × `cols` table at the caret (or, without one, in a new note container below
/// the page's content, or where the paper was clicked), and puts the caret in its first cell.
pub fn insert_table(state: &mut CanvasState, session: &mut Session, rows: usize, cols: usize) -> Result<CmdOutput, CmdError> {
    table::insert(state, session, rows, cols)
}

/// The pending style (Ctrl+B with nothing selected), for the ribbon to change.
pub fn pending_style_mut(state: &mut CanvasState) -> &mut PendingStyle {
    &mut state.pending_style
}

/// Runs a formatting command (`format.*`) on the caret's selection, like the canvas's own
/// shortcuts: a collapsed caret outside a word changes the pending style instead (character
/// formats), and the waiting container of a click on empty paper takes the pending style too.
/// `None` when there is nothing to format (no caret).
pub fn format_command(state: &mut CanvasState, session: &mut Session, id: &str, extra: Value) -> Option<Result<CmdOutput, CmdError>> {
    edit::format(state, session, id, extra)
}

/// Formatting at the caret for the ribbon's toggle states. `None` fields are mixed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurrentFormat {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub highlight: Option<Option<Color>>,
    pub color: Option<Option<Color>>,
    pub font: Option<Option<String>>,
    pub size: Option<Option<f32>>,
    pub style: Option<notecraft_doc::ParaStyle>,
    pub list: Option<Option<notecraft_doc::ListKind>>,
    /// The list marker style in effect (the level's default included).
    pub list_style: Option<Option<notecraft_doc::ListStyle>>,
    pub align: Option<notecraft_doc::Align>,
    pub tag: Option<Option<notecraft_doc::Tag>>,
}

/// What the caret would type with, or the common formatting of the selection (the pending style
/// included). `None` when nothing is being edited.
pub fn current_format(state: &CanvasState, session: &Session) -> Option<CurrentFormat> {
    edit::current_format(state, session)
}

/// Draws the current page and handles input on it, filling `ui`'s remaining space.
pub fn show(ui: &mut egui::Ui, state: &mut CanvasState, session: &mut Session, tokens: &Tokens, services: &Services) {
    let rect = ui.available_rect_before_wrap();
    let id = ui.scope_id().with("nc_canvas");
    crate::fonts::sync(ui.ctx());
    sync(state, session);
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    state.last_rect = Some(rect);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, tokens.canvas);
    if session.current_page().is_none() {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, crate::tl!("No page open"), egui::FontId::proportional(14.0), tokens.text_dim);
        return;
    }
    let sheet = page::sheet_rect(rect);
    view::scroll_and_zoom(ui, state, session, sheet, &resp);
    let v = view::View::new(state, sheet);
    state.last_origin = Some(v.origin);
    drop::handle(ui, state, session, services, &v);
    // Lay out for input, and again whenever a command changed the page.
    let mut lay = layout::page_layout(ui.ctx(), state, session, tokens, &v);
    let title = if state.hide_title { None } else { session.current_page().map(|p| page::title_rect(ui.ctx(), p, &v)) };
    let rev = session.revision;
    pointer::handle(ui, state, session, &v, &lay, &resp, title);
    if session.revision != rev {
        lay = layout::page_layout(ui.ctx(), state, session, tokens, &v);
    }
    let rev = session.revision;
    edit::keyboard(ui, state, session, services, &v, &lay, id);
    edit::keep_caret_valid(state, session);
    if session.revision != rev {
        lay = layout::page_layout(ui.ctx(), state, session, tokens, &v);
    }
    if state.reveal_caret {
        edit::caret_visible(state, &v, &lay, sheet);
    }
    if let Some(pg) = session.current_page() {
        page::paint_background(&painter, pg, tokens, &v, sheet);
        page::paint_title(&painter, ui.ctx(), state, pg, tokens, &v);
        images::paint(ui.ctx(), &painter, state, pg, tokens, &v);
        ink::paint_layer(&painter, state, pg, tokens, &v, true);
        text::paint_outlines(&painter, state, pg, tokens, &v, &lay, &resp);
        ink::paint_layer(&painter, state, pg, tokens, &v, false);
        shape::paint_preview(&painter, state, pg, tokens, &v);
        images::paint_selection(&painter, state, pg, tokens, &v);
    }
    ink::paint_overlay(&painter, state, session, tokens, &v, &resp);
    drop::paint_hover(ui, &painter, tokens, rect);
    // Widgets on top of the paper: the title box and the scroll bars.
    page::title_editor(ui, state, session, tokens, &v);
    view::paint_scrollbars(ui, state, session, tokens, sheet, &v, &lay);
    pointer::cursor(ui, state, session, &v, &lay, &resp);
}

/// Keeps the state in step with the session: another page resets selection and editing; the
/// shell clearing `editing` (a tool change) ends editing; the ink selection follows the session.
fn sync(state: &mut CanvasState, session: &mut Session) {
    if state.page != session.current.page {
        // `state.page` is still the old page here: an empty container there is cleaned up.
        edit::leave(state, session, None);
        state.page = session.current.page;
        state.selection.clear();
        state.editing = None;
        state.caret = None;
        state.pending = None;
        state.title_buf = None;
        state.gesture = pointer::Gesture::None;
        state.scroll = egui::Vec2::ZERO;
        state.pending_style = PendingStyle::default();
    }
    if state.caret.is_some() && state.editing != state.caret.as_ref().map(|c| c.outline) {
        // The shell cleared `editing` (tool change) or pointed it elsewhere.
        let wanted = state.editing;
        edit::leave(state, session, None);
        if let Some(id) = wanted
            && session.current_page().and_then(|p| p.outline(id)).is_some()
        {
            state.caret = session.current_page().and_then(|p| p.outline(id)).map(|o| table::caret_at_end(id, o));
            state.editing = Some(id);
        }
    } else if state.caret.is_none() && state.editing.is_some() {
        let id = state.editing;
        let exists = id.is_some_and(|id| session.current_page().and_then(|p| p.outline(id)).is_some());
        if exists && let Some(id) = id {
            state.caret = session.current_page().and_then(|p| p.outline(id)).map(|o| table::caret_at_end(id, o));
        } else {
            state.editing = None;
        }
    }
    if !state.tool.types() {
        state.pending = None;
    }
    let ink_sel: Vec<Guid> = session.ink_selection.clone();
    if state.selection.strokes != ink_sel {
        if state.selection.strokes.is_empty() && !ink_sel.is_empty() && state.tool != Tool::Lasso && state.tool != Tool::Select {
            // The shell cleared the canvas copy: let the session's go too.
            session.ink_selection.clear();
        } else {
            state.selection.strokes = ink_sel;
        }
    }
}

/// Runs a command, keeping its error for the shell's status bar.
pub(crate) fn run(state: &mut CanvasState, session: &mut Session, id: &str, params: Value) -> Option<CmdOutput> {
    match session.run(id, &params) {
        Ok(out) => Some(out),
        Err(e) => {
            log::info!("canvas {id}: {e}");
            if state.messages.len() < 8 {
                state.messages.push(e.to_string());
            }
            None
        }
    }
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn tools_and_pens() {
        let mut s = CanvasState::default();
        assert_eq!(s.tool, Tool::Type);
        s.selection.elements.push(Guid::new());
        s.set_tool(Tool::Highlighter);
        assert!(s.selection.is_empty());
        assert_eq!(s.current_pen().map(|p| p.color), Some(InkTool::Highlighter.default_color()));
        s.pens.style_mut(InkTool::Pen).width = 3.0;
        assert_eq!(s.pens.style(InkTool::Pen).width, 3.0);
        s.set_zoom(99.0);
        assert_eq!(s.zoom, notecraft_geom::zoom::MAX);
        assert!(Tool::Pencil.is_drawing() && !Tool::Lasso.is_drawing());
    }

    #[test]
    fn pending_style_json() {
        let p = PendingStyle { bold: Some(true), highlight: Some(None), size: Some(Some(14.0)), ..PendingStyle::default() };
        assert_eq!(p.to_json(), json!({"bold": true, "highlight": null, "size": 14.0}));
        assert!(PendingStyle::default().is_empty());
    }
}
