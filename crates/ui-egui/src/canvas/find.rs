//! Find on the page (Ctrl+F): the shell's find bar runs `text.find` and hands the matches here
//! ([`Highlights`]); the canvas paints every match behind the text, the current one stronger, and
//! scrolls the current one into view when it changes. Matches in the title are counted by the
//! bar but not painted (the title is its own text box).

use egui::{Color32, Painter, Rect, Stroke, StrokeKind};
use notecraft_doc::Guid;
use notecraft_engine::TextPos;
use notecraft_engine::text::{CellRef, Frame};

use super::CanvasState;
use super::layout::PageLayout;
use super::view::{self, View};
use crate::theme::Tokens;

/// One match: the container (`None` for the title), its table cell (if in one), the paragraph
/// and the characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub outline: Option<Guid>,
    pub cell: Option<CellRef>,
    pub para: usize,
    pub start: usize,
    pub end: usize,
}

impl Match {
    /// From one of `text.find`'s matches.
    pub fn from_json(v: &serde_json::Value) -> Option<Match> {
        let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).map(|x| usize::try_from(x).unwrap_or(usize::MAX));
        let outline = match v.get("outline") {
            Some(serde_json::Value::String(s)) => Some(Guid::parse(s)?),
            _ => None,
        };
        let cell = v.get("cell").and_then(CellRef::from_json);
        Some(Match { outline, cell, para: n("para")?, start: n("start")?, end: n("end")? })
    }

    /// As `text.replace`'s `at`.
    pub fn to_json(self) -> serde_json::Value {
        serde_json::json!({"outline": self.outline, "cell": self.cell.map(CellRef::to_json), "para": self.para, "start": self.start, "end": self.end})
    }

    fn frame(self) -> Frame {
        self.cell.map_or(Frame::Top, Frame::Cell)
    }
}

/// What the find bar has found on the current page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Highlights {
    pub matches: Vec<Match>,
    /// Index into `matches` of the current one.
    pub current: Option<usize>,
    /// The current match changed: scroll it into view once.
    pub reveal: bool,
}

/// All matches, then the current one, behind the text.
pub fn paint(painter: &Painter, state: &CanvasState, tokens: &Tokens, v: &View, lay: &PageLayout) {
    let Some(h) = &state.find else { return };
    let (all, current) = colors(tokens);
    for (i, m) in h.matches.iter().enumerate() {
        let Some(id) = m.outline else { continue };
        let Some(o) = lay.outline(id).and_then(|o| o.frame(m.frame())) else { continue };
        let is_current = h.current == Some(i);
        let range = (TextPos::new(m.para, m.start), TextPos::new(m.para, m.end));
        super::text::paint_selection(painter, o, v, range, if is_current { current } else { all });
        if is_current && let Some(r) = rect_of(o, v, *m) {
            painter.rect_stroke(r.expand(1.0), 1.0, Stroke::new(1.0, tokens.accent), StrokeKind::Outside);
        }
    }
}

/// The highlight fills: every match, and the current one.
fn colors(tokens: &Tokens) -> (Color32, Color32) {
    if tokens.dark {
        (Color32::from_rgba_unmultiplied(0xD8, 0xB4, 0x2A, 0x70), Color32::from_rgba_unmultiplied(0xF0, 0x8C, 0x28, 0xC0))
    } else {
        (Color32::from_rgba_unmultiplied(0xFF, 0xE8, 0x5C, 0xC8), Color32::from_rgba_unmultiplied(0xFF, 0xA8, 0x3C, 0xE0))
    }
}

/// The screen rect of a match's first character to its last (one row's worth when it wraps).
fn rect_of(o: &super::layout::OutlineLayout, v: &View, m: Match) -> Option<Rect> {
    let a = o.caret_rect(v, TextPos::new(m.para, m.start))?;
    let b = o.caret_rect(v, TextPos::new(m.para, m.end))?;
    Some(a.union(b))
}

/// Scrolls the current match into view after the bar moved to it.
pub fn reveal_current(state: &mut CanvasState, v: &View, lay: &PageLayout, sheet: Rect) {
    let Some(h) = state.find.as_mut() else { return };
    if !std::mem::take(&mut h.reveal) {
        return;
    }
    let Some(m) = h.current.and_then(|i| h.matches.get(i)).copied() else { return };
    let r = match m.outline {
        Some(id) => lay.outline(id).and_then(|o| o.frame(m.frame())).and_then(|o| rect_of(o, v, m)),
        // The title: the top of the page.
        None => Some(Rect::from_min_size(v.pt(0.0, 0.0), egui::vec2(1.0, 1.0))),
    };
    if let Some(r) = r {
        view::reveal(state, sheet, r);
    }
}
