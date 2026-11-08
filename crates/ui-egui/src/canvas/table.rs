//! Tables on the canvas: the caret in cells, the keys that move between them, cell selections,
//! column borders, the Table tab's commands, and painting the grid.
//!
//! OneNote 2016's keys, as observed: **Tab** goes to the next cell (selecting its text) and in
//! the last cell adds a row; **Shift+Tab** goes back; **Enter** starts a new paragraph in the
//! cell (Enter on an empty last paragraph of the last cell adds a row instead); **Ctrl+Enter**
//! adds a row below; **Ctrl+Alt+R / E** add a column right / left; the arrows cross cell edges
//! and leave the table at its first and last cells. Typing Tab after the text of a plain
//! paragraph turns it into a table of two columns, with the caret in the new cell.

use egui::{Color32, Key, Modifiers, Painter, Pos2, Rect, Shape, Stroke, pos2};
use notecraft_doc::{Outline, Paragraph, Run};
use notecraft_engine::text::{self, CellRange, CellRef, Frame};
use notecraft_engine::{CmdError, CmdOutput, Session, TextPos, ids};
use serde_json::{Value, json};

use super::edit::{self, caret_from};
use super::layout::{OutlineLayout, PageLayout, TableLayout};
use super::style::Ink;
use super::view::View;
use super::{CanvasState, TextCaret, run};
use crate::theme::mix;

/// How near a column border the pointer grabs it, pixels.
pub const BORDER_SLACK: f32 = 3.0;

// --- the caret ---------------------------------------------------------------------------------

/// A caret at the end of outline `o`: in its last cell when it ends with a table.
pub fn caret_at_end(id: notecraft_doc::Guid, o: &Outline) -> TextCaret {
    let last = o.blocks.len().saturating_sub(1);
    if let Some(t) = o.table(last) {
        let cell = CellRef::new(last, t.row_count().saturating_sub(1), t.col_count().saturating_sub(1));
        let end = text::flow(o, Frame::Cell(cell)).map(text::end_of).unwrap_or_default();
        return TextCaret::in_cell(id, cell, end);
    }
    TextCaret::at(id, text::end_of(o))
}

/// Keeps a caret valid in `o`: its cell still exists (else it goes to the table's block, or
/// what is there now), positions clamp, selected cells clamp to the table, and a collapsed
/// caret on a table's block moves into its first cell.
pub fn normalize_caret(c: &mut TextCaret, o: &Outline) {
    if let Some(cell) = c.cell {
        let exists = o.table(cell.block).and_then(|t| t.cell(cell.row, cell.col)).is_some();
        if !exists {
            let pos = text::clamp_ref(o, TextPos::new(cell.block, 0));
            *c = TextCaret { anchor: pos, head: pos, cell: None, cells: None, ..c.clone() };
        }
    } else {
        c.cells = None;
    }
    let Some(fl) = text::flow(o, c.frame()) else { return };
    c.anchor = text::clamp_ref(fl, c.anchor);
    c.head = text::clamp_ref(fl, c.head);
    if let Some(r) = c.cells {
        match o.table(r.block) {
            Some(t) => c.cells = Some(r.clamped(t.row_count(), t.col_count())),
            None => c.cells = None,
        }
    }
    if c.cell.is_none() && c.is_collapsed() && o.table(c.head.para).is_some() {
        let cell = CellRef::new(c.head.para, 0, 0);
        *c = TextCaret { anchor: TextPos::default(), head: TextPos::default(), cell: Some(cell), ..c.clone() };
    }
}

/// Moves a collapsed caret that sits on a table's block (the container's own flow) into the
/// table: its first cell's start (`forward`) or its last cell's end.
pub fn enter_table(state: &mut CanvasState, session: &Session, forward: bool) {
    let Some(c) = state.caret.as_mut() else { return };
    if c.cell.is_some() || !c.is_collapsed() {
        return;
    }
    let Some(o) = session.current_page().and_then(|p| p.outline(c.outline)) else { return };
    let Some(t) = o.table(c.head.para) else { return };
    let block = c.head.para;
    let (cell, pos) = if forward {
        (CellRef::new(block, 0, 0), TextPos::default())
    } else {
        let cell = CellRef::new(block, t.row_count().saturating_sub(1), t.col_count().saturating_sub(1));
        (cell, text::flow(o, Frame::Cell(cell)).map(text::end_of).unwrap_or_default())
    };
    *c = TextCaret { anchor: pos, head: pos, cell: Some(cell), cells: None, goal_x: None, ..c.clone() };
}

/// Whether two flows are cells of the same table.
pub fn same_table(a: Frame, b: Frame) -> bool {
    matches!((a, b), (Frame::Cell(x), Frame::Cell(y)) if x.block == y.block)
}

/// `c` with the cells from its own to `to` selected (the caret stays in its cell).
pub fn span_to(c: TextCaret, to: Frame) -> TextCaret {
    let (Some(from), Some(to)) = (c.cell, to.cell()) else { return c };
    let cells = if from == to { None } else { Some(CellRange::span(from, to)) };
    TextCaret { cells, ..c }
}

/// Selects every cell of the caret's table.
pub fn select_whole_table(c: &mut TextCaret, session: &Session) {
    let Some(cell) = c.cell else { return };
    let Some(t) = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|o| o.table(cell.block)) else { return };
    c.cells = Some(CellRange { block: cell.block, rows: (0, t.row_count().saturating_sub(1)), cols: (0, t.col_count().saturating_sub(1)) });
}

/// The runs and paragraphs the selection covers (for the ribbon's toggle states): the selected
/// cells', or the caret's flow's.
pub fn selection_formats(c: &TextCaret, session: &Session) -> Option<(Vec<Run>, Vec<Paragraph>)> {
    let o = session.current_page()?.outline(c.outline)?;
    if let Some(r) = c.cells {
        let mut runs = Vec::new();
        let mut paras = Vec::new();
        for cell in r.cells() {
            if let Some(fl) = text::flow(o, Frame::Cell(cell)) {
                runs.extend(text::runs_in(fl, TextPos::default(), text::end_of(fl)));
                paras.extend(text::paras_in(fl, 0, usize::MAX).into_iter().cloned());
            }
        }
        return Some((runs, paras));
    }
    let fl = text::flow(o, c.frame())?;
    let (a, b) = c.range();
    let runs = if a == b { fl.para(a.para).map(|pg| text::format_at(pg, a.char)).into_iter().collect() } else { text::runs_in(fl, a, b) };
    let paras = text::paras_in(fl, a.para, b.para).into_iter().cloned().collect();
    Some((runs, paras))
}

/// The selected cells' text: a line per row, cells separated by tabs.
pub fn selected_cells_text(state: &CanvasState, session: &Session) -> Option<String> {
    let c = state.caret.as_ref()?;
    let r = c.cells?;
    let t = session.current_page()?.outline(c.outline)?.table(r.block)?;
    let rows: Vec<String> = (r.rows.0..=r.rows.1)
        .map(|row| {
            (r.cols.0..=r.cols.1).map(|col| t.cell(row, col).map(notecraft_doc::Cell::text).unwrap_or_default()).collect::<Vec<_>>().join("\t")
        })
        .collect();
    Some(rows.join("\n"))
}

// --- commands ----------------------------------------------------------------------------------

/// The caret a table command's output names (`{"cell", "caret"}`), set collapsed.
fn apply_caret(state: &mut CanvasState, out: &CmdOutput) {
    let Some(pos) = caret_from(out) else { return };
    let cell = out.value.get("cell").and_then(CellRef::from_json);
    if let Some(c) = state.caret.as_mut() {
        if let Some(id) = out.value.get("id").and_then(Value::as_str).and_then(notecraft_doc::Guid::parse) {
            c.outline = id;
        }
        *c = TextCaret { anchor: pos, head: pos, cell, cells: None, goal_x: None, ..c.clone() };
    }
    state.reveal_caret = true;
}

/// See [`super::table_command`].
pub fn command(state: &mut CanvasState, session: &mut Session, id: &str, extra: Value) -> Option<Result<CmdOutput, CmdError>> {
    let c = state.caret.clone()?;
    c.cell?;
    let mut params = edit::range_params(state, &c);
    if let (Value::Object(m), Value::Object(e)) = (&mut params, &extra) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    session.break_undo_merge();
    let r = session.run(id, &params);
    match &r {
        Ok(out) => {
            if let Some(range) = out.value.get("cells").and_then(CellRange::from_json) {
                select_cells(state, session, range);
            } else {
                apply_caret(state, out);
            }
        }
        Err(e) => state.messages.push(e.to_string()),
    }
    Some(r)
}

/// Selects `range` (one cell: its text).
fn select_cells(state: &mut CanvasState, session: &Session, range: CellRange) {
    let Some(c) = state.caret.as_mut() else { return };
    let single = range.rows.0 == range.rows.1 && range.cols.0 == range.cols.1;
    if single {
        let cell = CellRef::new(range.block, range.rows.0, range.cols.0);
        let end = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|o| text::flow(o, Frame::Cell(cell))).map(text::end_of);
        *c = TextCaret { anchor: TextPos::default(), head: end.unwrap_or_default(), cell: Some(cell), cells: None, ..c.clone() };
    } else {
        c.cells = Some(range);
    }
}

/// See [`super::insert_table`].
pub fn insert(state: &mut CanvasState, session: &mut Session, rows: usize, cols: usize) -> Result<CmdOutput, CmdError> {
    let mut params = json!({"rows": rows, "cols": cols, "page": state.page});
    if let Value::Object(m) = &mut params {
        if let Some(c) = &state.caret {
            m.insert("outline".into(), json!(c.outline));
            m.insert("at".into(), c.range().0.to_json());
            if let Some(cell) = c.cell {
                m.insert("cell".into(), cell.to_json());
            }
        } else if let Some(p) = &state.pending {
            m.insert("x".into(), json!(p.at.x));
            m.insert("y".into(), json!(p.at.y));
        }
    }
    session.break_undo_merge();
    let out = session.run(ids::INSERT_TABLE, &params)?;
    let id = out.value.get("id").and_then(Value::as_str).and_then(notecraft_doc::Guid::parse);
    let cell = out.value.get("cell").and_then(CellRef::from_json);
    if let (Some(id), Some(cell)) = (id, cell) {
        state.pending = None;
        state.selection.clear();
        state.caret = Some(TextCaret::in_cell(id, cell, TextPos::default()));
        state.editing = Some(id);
        state.reveal_caret = true;
    }
    Ok(out)
}

/// Runs a table command on the caret's cell with the typing undo key; moves the caret.
fn run_here(state: &mut CanvasState, session: &mut Session, id: &str) -> bool {
    let Some(c) = state.caret.clone() else { return false };
    let Some(cell) = c.cell else { return false };
    let params = json!({"outline": c.outline, "cell": cell.to_json(), "page": state.page, "mergeKey": c.merge_key});
    match run(state, session, id, params) {
        Some(out) => {
            apply_caret(state, &out);
            true
        }
        None => false,
    }
}

/// Moves to the next (or previous) cell, selecting its text; Tab in the last cell adds a row.
fn move_cell(state: &mut CanvasState, session: &mut Session, forward: bool) {
    let Some(c) = state.caret.clone() else { return };
    let Some(cell) = c.cell else { return };
    let Some(t) = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|o| o.table(cell.block)) else { return };
    let (rows, cols) = (t.row_count(), t.col_count());
    let next = if forward {
        if cell.col + 1 < cols {
            Some(CellRef::new(cell.block, cell.row, cell.col + 1))
        } else if cell.row + 1 < rows {
            Some(CellRef::new(cell.block, cell.row + 1, 0))
        } else {
            None
        }
    } else if cell.col > 0 {
        Some(CellRef::new(cell.block, cell.row, cell.col - 1))
    } else if cell.row > 0 {
        Some(CellRef::new(cell.block, cell.row - 1, cols.saturating_sub(1)))
    } else {
        return;
    };
    session.break_undo_merge();
    let next = match next {
        Some(n) => n,
        None => {
            if !run_here(state, session, ids::TABLE_INSERT_ROW_BELOW) {
                return;
            }
            CellRef::new(cell.block, cell.row + 1, 0)
        }
    };
    select_cells(state, session, CellRange::span(next, next));
    state.reveal_caret = true;
}

/// Typing Tab after the text of a plain paragraph (no list, no tag, the caret at its end) makes
/// it a table of two columns, the caret in the new cell. Returns whether it did.
pub fn tab_makes_table(state: &mut CanvasState, session: &mut Session, m: Modifiers) -> bool {
    if m.shift || m.alt || m.command || m.ctrl {
        return false;
    }
    let Some(c) = state.caret.clone() else { return false };
    if c.cell.is_some() || !c.is_collapsed() {
        return false;
    }
    let Some(o) = session.current_page().and_then(|p| p.outline(c.outline)) else { return false };
    let Some(p) = o.para(c.head.para) else { return false };
    let len = text::para_len(p);
    if len == 0 || c.head.char != len || p.list.is_some() || p.tag.is_some() || p.text().trim().is_empty() {
        return false;
    }
    let params = json!({"outline": c.outline, "para": c.head.para, "addColumn": true, "page": state.page, "mergeKey": c.merge_key});
    session.break_undo_merge();
    match run(state, session, ids::TABLE_FROM_TEXT, params) {
        Some(out) => {
            apply_caret(state, &out);
            true
        }
        None => false,
    }
}

/// Backspace at the start of a paragraph just after a table goes into its last cell; Delete at
/// the end of one just before a table goes into its first cell (nothing merges with a table).
/// Returns whether it moved the caret.
pub fn delete_at_edge(state: &mut CanvasState, session: &Session, backward: bool) -> bool {
    let Some(c) = state.caret.clone() else { return false };
    if c.cell.is_some() || !c.is_collapsed() {
        return false;
    }
    let Some(o) = session.current_page().and_then(|p| p.outline(c.outline)) else { return false };
    let len = text::len_at(o, c.head.para);
    let target = if backward && c.head.char == 0 {
        c.head.para.checked_sub(1)
    } else if !backward && c.head.char >= len {
        Some(c.head.para + 1)
    } else {
        None
    };
    let Some(b) = target.filter(|b| o.table(*b).is_some()) else { return false };
    if let Some(cc) = state.caret.as_mut() {
        cc.head = TextPos::new(b, 0);
        cc.anchor = cc.head;
    }
    enter_table(state, session, !backward);
    true
}

/// Backspace / Delete over selected cells: a whole table, whole rows or whole columns go;
/// otherwise the cells' text is cleared. Returns whether there were cells to act on.
pub fn delete_cells(state: &mut CanvasState, session: &mut Session) -> bool {
    let Some(c) = state.caret.clone() else { return false };
    let Some(r) = c.cells else { return false };
    let Some(t) = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|o| o.table(r.block)) else { return false };
    let (rows, cols) = (t.row_count(), t.col_count());
    let all_rows = r.rows == (0, rows.saturating_sub(1));
    let all_cols = r.cols == (0, cols.saturating_sub(1));
    let id = match (all_rows, all_cols) {
        (true, true) => Some(ids::TABLE_DELETE),
        (false, true) => Some(ids::TABLE_DELETE_ROW),
        (true, false) => Some(ids::TABLE_DELETE_COLUMN),
        (false, false) => None,
    };
    session.break_undo_merge();
    match id {
        Some(id) => {
            let params = json!({"outline": c.outline, "cells": r.to_json(), "page": state.page});
            if let Some(out) = run(state, session, id, params) {
                apply_caret(state, &out);
            }
        }
        None => {
            let key = format!("clear:{}", notecraft_doc::Guid::new());
            for cell in r.cells() {
                let end = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|o| text::flow(o, Frame::Cell(cell))).map(text::end_of);
                let Some(end) = end.filter(|e| *e != TextPos::default()) else { continue };
                let params = json!({"outline": c.outline, "cell": cell.to_json(), "start": TextPos::default().to_json(), "end": end.to_json(), "page": state.page, "mergeKey": key});
                run(state, session, ids::TEXT_DELETE, params);
            }
            if let Some(cc) = state.caret.as_mut() {
                let first = CellRef::new(r.block, r.rows.0, r.cols.0);
                *cc = TextCaret { anchor: TextPos::default(), head: TextPos::default(), cell: Some(first), cells: None, ..cc.clone() };
            }
        }
    }
    session.break_undo_merge();
    true
}

// --- keys --------------------------------------------------------------------------------------

/// The galley row of character `ch` and how many rows the paragraph has.
fn row_info(o: &OutlineLayout, pos: TextPos) -> Option<(usize, usize)> {
    let p = o.paras.get(pos.para)?;
    let mut base = 0;
    let last = p.galley.rows.len().saturating_sub(1);
    for (i, r) in p.galley.rows.iter().enumerate() {
        let n = r.char_count_including_newline().0;
        if pos.char < base + n || i == last {
            return Some((i, p.galley.rows.len()));
        }
        base += n;
    }
    Some((0, 1))
}

/// The caret in `target` (a cell or the container) nearest to screen x `x` on its first
/// (`top`) or last line.
fn land(target: &OutlineLayout, v: &View, x: f32, top: bool) -> TextPos {
    let r = target.rect(v);
    let y = if top {
        target.paras.first().map_or(r.top(), |p| r.top() + p.top + 1.0)
    } else {
        target.paras.last().map_or(r.bottom(), |p| r.top() + p.top + p.height - 1.0)
    };
    target.hit(v, pos2(x, y))
}

/// Puts a collapsed caret in `frame` at `pos`.
fn go(state: &mut CanvasState, frame: Frame, pos: TextPos, goal_x: Option<f32>) {
    if let Some(c) = state.caret.as_mut() {
        *c = TextCaret { anchor: pos, head: pos, cell: frame.cell(), cells: None, goal_x, ..c.clone() };
    }
    state.reveal_caret = true;
}

/// Leaves table `t` (block `block`) upwards or downwards into the block before or after it.
fn leave_table(state: &mut CanvasState, session: &Session, o: &OutlineLayout, block: usize, x: f32, down: bool, v: &View) -> bool {
    let Some(ol) = session.current_page().and_then(|p| p.outline(o.id)) else { return false };
    let target = if down { block + 1 } else { block.wrapping_sub(1) };
    if target >= ol.blocks.len() {
        return false;
    }
    if ol.table(target).is_some() {
        // Another table: its first or last row.
        if let Some(c) = state.caret.as_mut() {
            c.cell = None;
            c.head = TextPos::new(target, 0);
            c.anchor = c.head;
        }
        enter_table(state, session, down);
        return true;
    }
    let pos = o.paras.get(target).map_or(TextPos::new(target, 0), |p| {
        let r = o.rect(v);
        let y = if down { r.top() + p.top + 1.0 } else { r.top() + p.top + p.height - 1.0 };
        o.hit(v, pos2(x, y))
    });
    go(state, Frame::Top, pos, Some(x - o.rect(v).left()));
    true
}

/// Keys with a meaning in a table cell. Returns whether the key was used.
pub fn key(state: &mut CanvasState, session: &mut Session, lay: &PageLayout, v: &View, key: Key, m: Modifiers) -> bool {
    let Some(c) = state.caret.clone() else { return false };
    let Some(cell) = c.cell else { return false };
    let cmd = m.command || m.ctrl;
    match key {
        Key::Tab if !cmd && !m.alt => {
            move_cell(state, session, !m.shift);
            return true;
        }
        Key::Enter if cmd => {
            session.break_undo_merge();
            run_here(state, session, ids::TABLE_INSERT_ROW_BELOW);
            return true;
        }
        Key::R | Key::E if cmd && m.alt => {
            session.break_undo_merge();
            run_here(state, session, if key == Key::R { ids::TABLE_INSERT_COLUMN_RIGHT } else { ids::TABLE_INSERT_COLUMN_LEFT });
            return true;
        }
        Key::Enter if !m.shift && c.is_collapsed() && c.cells.is_none() => return enter_in_cell(state, session, cell),
        _ => {}
    }
    if m.shift || !c.is_collapsed() || c.cells.is_some() {
        return false;
    }
    let Some(o) = lay.outline(c.outline) else { return false };
    let Some(t) = o.table(cell.block) else { return false };
    let Some(cl) = t.cell(cell.row, cell.col) else { return false };
    let (rows, cols) = (t.rows(), t.cols());
    let Some(fl) = session.current_page().and_then(|p| p.outline(c.outline)).and_then(|ol| text::flow(ol, Frame::Cell(cell))) else { return false };
    let end = text::end_of(fl);
    let x = cl.caret_rect(v, c.head).map_or(cl.rect(v).left(), |r| r.left());
    let x = c.goal_x.map_or(x, |g| o.rect(v).left() + g);
    match key {
        Key::ArrowLeft if !cmd && c.head == TextPos::default() => {
            if cell.col > 0 || cell.row > 0 {
                let prev =
                    if cell.col > 0 { CellRef::new(cell.block, cell.row, cell.col - 1) } else { CellRef::new(cell.block, cell.row - 1, cols - 1) };
                let pos =
                    session.current_page().and_then(|p| p.outline(c.outline)).and_then(|ol| text::flow(ol, Frame::Cell(prev))).map(text::end_of);
                go(state, Frame::Cell(prev), pos.unwrap_or_default(), None);
                true
            } else {
                let prev = cell.block.checked_sub(1);
                match prev {
                    Some(b) => {
                        let len = session.current_page().and_then(|p| p.outline(c.outline)).map_or(0, |ol| text::len_at(ol, b));
                        go(state, Frame::Top, TextPos::new(b, len), None);
                        enter_table(state, session, false);
                        true
                    }
                    None => true,
                }
            }
        }
        Key::ArrowRight if !cmd && c.head == end => {
            if cell.col + 1 < cols || cell.row + 1 < rows {
                let next =
                    if cell.col + 1 < cols { CellRef::new(cell.block, cell.row, cell.col + 1) } else { CellRef::new(cell.block, cell.row + 1, 0) };
                go(state, Frame::Cell(next), TextPos::default(), None);
                true
            } else {
                let len = session.current_page().and_then(|p| p.outline(c.outline)).map_or(0, |ol| ol.blocks.len());
                if cell.block + 1 < len {
                    go(state, Frame::Top, TextPos::new(cell.block + 1, 0), None);
                    enter_table(state, session, true);
                }
                true
            }
        }
        Key::ArrowUp | Key::ArrowDown => {
            let down = key == Key::ArrowDown;
            let Some((row, n)) = row_info(cl, c.head) else { return false };
            let at_edge = if down { c.head.para + 1 >= cl.paras.len() && row + 1 >= n } else { c.head.para == 0 && row == 0 };
            if !at_edge {
                return false;
            }
            let goal = Some(x - o.rect(v).left());
            let next_row = if down { cell.row + 1 } else { cell.row.wrapping_sub(1) };
            match t.cell(next_row, cell.col) {
                Some(target) => {
                    let pos = land(target, v, x, down);
                    go(state, Frame::Cell(CellRef::new(cell.block, next_row, cell.col)), pos, goal);
                }
                None => {
                    if !leave_table(state, session, o, cell.block, x, down, v) {
                        let pos = if down { end } else { TextPos::default() };
                        go(state, Frame::Cell(cell), pos, goal);
                    }
                }
            }
            true
        }
        _ => false,
    }
}

/// Enter in a cell: a new paragraph there, except on an empty last paragraph at the end of the
/// table's last cell, which adds a row (the empty paragraph goes).
fn enter_in_cell(state: &mut CanvasState, session: &mut Session, cell: CellRef) -> bool {
    let Some(c) = state.caret.clone() else { return false };
    let Some(o) = session.current_page().and_then(|p| p.outline(c.outline)) else { return false };
    let Some(t) = o.table(cell.block) else { return false };
    let last = cell.row + 1 == t.row_count() && cell.col + 1 == t.col_count();
    let Some(fl) = text::flow(o, Frame::Cell(cell)) else { return false };
    let at_end = c.head == text::end_of(fl);
    let empty_last =
        fl.count() > 1 && text::len_at(fl, c.head.para) == 0 && fl.para(c.head.para).is_some_and(|p| p.list.is_none() && p.tag.is_none());
    if !(last && at_end && empty_last) {
        return false;
    }
    let params =
        json!({"outline": c.outline, "cell": cell.to_json(), "at": c.head.to_json(), "backward": true, "page": state.page, "mergeKey": c.merge_key});
    run(state, session, ids::TEXT_DELETE, params);
    run_here(state, session, ids::TABLE_INSERT_ROW_BELOW);
    true
}

// --- the pointer -------------------------------------------------------------------------------

/// The table column border under screen point `at` in container `o`: its table's block and the
/// column whose right edge it is.
pub fn border_at(o: &OutlineLayout, v: &View, at: Pos2) -> Option<(usize, usize)> {
    o.tables.iter().find_map(|t| t.border_at(o, v, at, BORDER_SLACK).map(|i| (t.block, i - 1)))
}

// --- painting ----------------------------------------------------------------------------------

/// A shading colour as drawn: on dark paper it is darkened towards the paper, so the light
/// text drawn there stays readable (as OneNote's dark mode does).
pub fn shading_color(ink: &Ink, c: Color32) -> Color32 {
    if ink.dark { mix(ink.paper, c, 0.35) } else { c }
}

/// Cell shading, under everything else.
pub fn paint_shading(painter: &Painter, o: &OutlineLayout, t: &TableLayout, v: &View, ink: &Ink) {
    for (r, row) in t.shading.iter().enumerate() {
        for (c, shade) in row.iter().enumerate() {
            let (Some(s), Some(rect)) = (shade, t.cell_rect(o, v, r, c)) else { continue };
            if rect.intersects(v.clip) {
                painter.rect_filled(rect, 0.0, shading_color(ink, crate::theme::color32(*s)));
            }
        }
    }
}

/// Fills the selected cells: the caret's cell range, or every cell of the tables a selection in
/// the container's own blocks covers.
pub fn paint_selected_cells(painter: &Painter, o: &OutlineLayout, v: &View, c: &TextCaret, fill: Color32) {
    let fill_cell = |t: &TableLayout, r: usize, col: usize| {
        if let Some(rect) = t.cell_rect(o, v, r, col) {
            painter.rect_filled(rect.shrink(0.5), 0.0, fill);
        }
    };
    if let Some(range) = c.cells {
        if let Some(t) = o.table(range.block) {
            for cell in range.cells() {
                fill_cell(t, cell.row, cell.col);
            }
        }
        return;
    }
    if c.cell.is_some() || c.is_collapsed() {
        return;
    }
    let (a, b) = c.range();
    for t in o.tables.iter().filter(|t| (a.para..=b.para).contains(&t.block)) {
        for r in 0..t.rows() {
            for col in 0..t.cols() {
                fill_cell(t, r, col);
            }
        }
    }
}

/// The grid: solid borders, or — borders hidden — light dashed lines while the container is
/// being edited, hovered or selected (OneNote shows hidden borders while you work in a table).
pub fn paint_grid(painter: &Painter, o: &OutlineLayout, t: &TableLayout, v: &View, ink: &Ink, shown: bool) {
    let r = t.rect(o, v);
    if !r.expand(4.0).intersects(v.clip) || (!t.borders && !shown) {
        return;
    }
    let left = v.to_screen(o.pos);
    let lines = |paint: &dyn Fn(Pos2, Pos2)| {
        for y in &t.row_y {
            let y = (left.y + y).round() + 0.5;
            paint(pos2(r.left(), y), pos2(r.right(), y));
        }
        for x in &t.col_x {
            let x = (left.x + x).round() + 0.5;
            paint(pos2(x, r.top()), pos2(x, r.bottom()));
        }
    };
    if t.borders {
        let stroke = Stroke::new(1.0, mix(ink.paper, ink.text, 0.45));
        lines(&|a, b| {
            painter.line_segment([a, b], stroke);
        });
    } else {
        let stroke = Stroke::new(1.0, mix(ink.paper, ink.text, 0.28));
        lines(&|a, b| {
            painter.extend(Shape::dashed_line(&[a, b], stroke, 3.0, 3.0));
        });
    }
}

/// The cells' rectangles of table `t` on screen (tests and the shell).
pub fn cell_rects(o: &OutlineLayout, t: &TableLayout, v: &View) -> Vec<Vec<Rect>> {
    (0..t.rows()).map(|r| (0..t.cols()).filter_map(|c| t.cell_rect(o, v, r, c)).collect()).collect()
}
