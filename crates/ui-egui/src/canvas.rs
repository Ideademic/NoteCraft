//! The page canvas: the title line, note containers of rich text ([`text`]), pictures and
//! printouts ([`images`]) and the ink layer ([`ink`]), with rule lines and page colour behind.
//!
//! **Contract with the shell.** The ribbon and panes never reach into canvas internals: they
//! read and set [`CanvasState`] (tool, pen settings, zoom) and call [`show`] once per frame for
//! the page area. Everything the canvas changes in the notebook goes through
//! `Session::run` commands (`outline.new`, `text.setParagraphs`, `draw.stroke`…).

pub mod images;
pub mod ink;
pub mod text;

use notecraft_doc::{Color, Guid, InkTool};
use notecraft_engine::Session;

use crate::services::Services;
use crate::theme::Tokens;

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
    /// Selected ink strokes (also mirrored to `Session::ink_selection` for the draw commands).
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

/// The canvas's state between frames. The shell owns one and may set any public field.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasState {
    /// 1.0 = 100 % (see `notecraft_geom::zoom`).
    pub zoom: f32,
    pub tool: Tool,
    pub pens: PenSettings,
    pub selection: CanvasSelection,
    /// The outline whose text is being edited, if any.
    pub editing: Option<Guid>,
    /// The page the state belongs to; a different current page resets selection and editing.
    pub page: Option<Guid>,
    /// Scroll offset of the page within the canvas, logical pixels.
    pub scroll: egui::Vec2,
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
            if tool != Tool::Select && tool != Tool::Lasso {
                self.selection.clear();
            }
        }
        self.tool = tool;
    }

    /// The pen style of the current tool, if it draws.
    pub fn current_pen(&self) -> Option<PenStyle> {
        self.tool.ink_tool().map(|t| self.pens.style(t))
    }
}

/// Draws the current page and handles input on it, filling `ui`'s remaining space.
pub fn show(ui: &mut egui::Ui, state: &mut CanvasState, session: &mut Session, tokens: &Tokens, services: &Services) {
    let _ = services;
    if state.page != session.current.page {
        state.page = session.current.page;
        state.selection.clear();
        state.editing = None;
        state.scroll = egui::Vec2::ZERO;
    }
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, tokens.canvas);
    let Some(page) = session.current_page() else {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, crate::tl!("No page open"), egui::FontId::proportional(14.0), tokens.text_dim);
        ui.allocate_rect(rect, egui::Sense::hover());
        return;
    };
    let zoom = notecraft_geom::zoom::clamp(state.zoom);
    let origin = rect.min + egui::vec2(24.0, 16.0) - state.scroll;
    let bg = page.background.color.map_or(tokens.page, crate::theme::color32);
    painter.rect_filled(rect.shrink2(egui::vec2(12.0, 8.0)), 0.0, bg);
    text::paint_title(&painter, page, origin, zoom, tokens);
    text::paint_outlines(&painter, page, origin, zoom, tokens);
    images::paint(&painter, page, origin, zoom, tokens);
    ink::paint(&painter, page, origin, zoom);
    ui.allocate_rect(rect, egui::Sense::click_and_drag());
}

#[cfg(test)]
mod tests {
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
}
