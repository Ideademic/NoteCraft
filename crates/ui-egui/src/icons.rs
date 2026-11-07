//! Icons drawn in code (no image assets, nothing from Microsoft). Phase 2 (shell agent) draws
//! each one; for now every icon is a framed square.

use egui::{Color32, Painter, Rect, Stroke, StrokeKind};

/// Every icon the UI uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    NewPage,
    NewSection,
    NewNotebook,
    Undo,
    Redo,
    Search,
    Bold,
    Italic,
    Underline,
    Strike,
    Highlight,
    FontColor,
    Bullets,
    Numbering,
    Indent,
    Outdent,
    ToDo,
    Picture,
    Printout,
    Attach,
    DateTime,
    Type,
    Select,
    Lasso,
    Eraser,
    Pen,
    Pencil,
    Highlighter,
    RuleLines,
    PageColor,
    ZoomIn,
    ZoomOut,
    Back,
}

/// Paints `icon` filling `rect` in `color`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let _ = icon;
    painter.rect_stroke(rect.shrink(rect.width() * 0.2), 1.0, Stroke::new(1.0, color), StrokeKind::Inside);
}
