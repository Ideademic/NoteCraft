//! Tables: Insert ▸ Table and the Table tab (Layout): insert and delete rows and columns,
//! select cells, column widths, shading, borders, sort, and converting between tables and
//! text.
//!
//! Commands name the outline and where in its table they act, in the selection model of
//! [`super::text`]: `"cell": {"block", "row", "col"}` (the caret's cell), `"cells": {"block",
//! "rows": [r0, r1], "cols": [c0, c1]}` (selected cells), or `"block": n` (the whole table).
//! Commands that move the caret return `{"cell": cell, "caret": {"para", "char"}}` (`cell` null
//! for the outline's own blocks); the select commands return `{"cells": range}`.

use notecraft_doc::page::{MAX_ELEMENTS_PER_PAGE, MAX_PARAGRAPHS};
use notecraft_doc::table::{DEFAULT_COL_WIDTH, MAX_TABLE_CELLS, MAX_TABLE_COLS, MAX_TABLE_ROWS};
use notecraft_doc::{Block, Cell, ElementKind, Outline, PageElement, Paragraph, Run, Table};
use serde_json::{Value, json};

use super::text::{frame_param, outline_id, range, range_or_all, with_outline};
use super::{has_page, insert_pos, target_page};
use crate::ids::{
    INSERT_TABLE, TABLE_BORDERS, TABLE_CONVERT_TO_TEXT, TABLE_DELETE, TABLE_DELETE_COLUMN, TABLE_DELETE_ROW, TABLE_FROM_TEXT,
    TABLE_INSERT_COLUMN_LEFT, TABLE_INSERT_COLUMN_RIGHT, TABLE_INSERT_ROW_ABOVE, TABLE_INSERT_ROW_BELOW, TABLE_SELECT_CELL, TABLE_SELECT_COLUMN,
    TABLE_SELECT_ROW, TABLE_SELECT_TABLE, TABLE_SET_COLUMN_WIDTH, TABLE_SHADING, TABLE_SORT,
};
use crate::text::{self, CellRange, CellRef, Frame, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Rows or columns a new table has at most from Insert ▸ Table (the caps of
/// [`notecraft_doc::table`] still apply to the product).
pub const MAX_INSERT_ROWS: usize = MAX_TABLE_ROWS;
pub const MAX_INSERT_COLS: usize = MAX_TABLE_COLS;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(INSERT_TABLE, "Table", insert).when(has_page).params(
            r##"{"rows": number, "cols": number, "colWidth"?: points, "outline"?: guid, "cell"?: cell, "at"?/"start"?: pos (in that outline, at the caret), "x"?: number, "y"?: number (else a new note container below the content), "page"?: guid} -> {"id": guid, "cell": cell, "caret": pos}"##,
        ),
        CommandSpec::new(TABLE_FROM_TEXT, "Convert to Table", from_text).when(has_page).params(
            r##"{"outline": guid, "start"?/"end"?/"para"?/"paras"?: the paragraphs (default: all), "addColumn"?: bool (an empty column after the text: Tab after typing), "page"?: guid} -> {"cell": cell, "caret": pos}"##,
        ),
        CommandSpec::new(TABLE_INSERT_ROW_ABOVE, "Insert Above", |s, p| insert_row(s, p, false)).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "}")),
        CommandSpec::new(TABLE_INSERT_ROW_BELOW, "Insert Below", |s, p| insert_row(s, p, true)).key("Mod+Enter").when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "}")),
        CommandSpec::new(TABLE_INSERT_COLUMN_LEFT, "Insert Left", |s, p| insert_col(s, p, false)).key("Mod+Alt+E").when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "}")),
        CommandSpec::new(TABLE_INSERT_COLUMN_RIGHT, "Insert Right", |s, p| insert_col(s, p, true)).key("Mod+Alt+R").when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "}")),
        CommandSpec::new(TABLE_DELETE_ROW, "Delete Rows", delete_rows).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} (deleting every row deletes the table)")),
        CommandSpec::new(TABLE_DELETE_COLUMN, "Delete Columns", delete_cols).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} (deleting every column deletes the table)")),
        CommandSpec::new(TABLE_DELETE, "Delete Table", delete_table).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "}")),
        CommandSpec::new(TABLE_SELECT_CELL, "Select Cell", |s, p| select(s, p, Sel::Cell)).pure().when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} -> {\"cells\": range}")),
        CommandSpec::new(TABLE_SELECT_ROW, "Select Rows", |s, p| select(s, p, Sel::Row)).pure().when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} -> {\"cells\": range}")),
        CommandSpec::new(TABLE_SELECT_COLUMN, "Select Columns", |s, p| select(s, p, Sel::Column)).pure().when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} -> {\"cells\": range}")),
        CommandSpec::new(TABLE_SELECT_TABLE, "Select Table", |s, p| select(s, p, Sel::Table)).pure().when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} -> {\"cells\": range}")),
        CommandSpec::new(TABLE_SET_COLUMN_WIDTH, "Column Width", set_width).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, r##", "col"?: number (default: the cell's), "width": points (18-2000)}"##)),
        CommandSpec::new(TABLE_SHADING, "Shading", shading).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, r##", "color": "#rrggbb" | null (none)} (the cell, the cells, or with `block` alone every cell)"##)),
        CommandSpec::new(TABLE_BORDERS, "Hide Borders", borders).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, r##", "value"?: bool (borders shown; default: toggle)}"##)),
        CommandSpec::new(TABLE_SORT, "Sort", sort).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, r##", "col"?: number (default: the cell's), "descending"?: bool, "header"?: bool (the first row stays on top)}"##)),
        CommandSpec::new(TABLE_CONVERT_TO_TEXT, "Convert to Text", to_text).when(has_page).params(concat!("{", r##""outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "block"?: number, "page"?: guid"##, "} (a paragraph per row, cells separated by tabs) -> {\"cell\": null, \"caret\": pos}")),
    ]
}

/// The caret command output.
fn caret(cell: Option<CellRef>, at: TextPos) -> CmdOutput {
    CmdOutput::value(json!({"cell": cell.map_or(Value::Null, CellRef::to_json), "caret": at.to_json()}))
}

/// Where in a table a command acts: the table's block and the selected cells (the caret's cell
/// alone, the `cells` range, or every cell for `block` alone), clamped to the table.
struct Target {
    block: usize,
    cells: CellRange,
    /// The caret's cell, when one was named.
    cell: Option<CellRef>,
}

fn target(params: &Value, o: &Outline) -> Result<Target, CmdError> {
    let cell = frame_param(params)?.cell();
    let range = match params.get("cells").filter(|v| !v.is_null()) {
        Some(v) => Some(CellRange::from_json(v).ok_or_else(|| CmdError::Params("`cells` must be {\"block\", \"rows\", \"cols\"}".into()))?),
        None => None,
    };
    let block = range.map(|r| r.block).or(cell.map(|c| c.block)).or(p::index(params, "block"));
    let block = block.ok_or_else(|| CmdError::Params("`cell`, `cells` or `block` is required".into()))?;
    let t = o.table(block).ok_or_else(|| CmdError::Params(format!("block {block} is not a table")))?;
    let (rows, cols) = (t.row_count(), t.col_count());
    let cells = match (range, cell) {
        (Some(r), _) => r,
        (None, Some(c)) => CellRange::span(c, c),
        (None, None) => CellRange { block, rows: (0, usize::MAX), cols: (0, usize::MAX) },
    }
    .clamped(rows, cols);
    let cell = cell.map(|c| CellRef::new(block, c.row.min(rows.saturating_sub(1)), c.col.min(cols.saturating_sub(1))));
    Ok(Target { block, cells: CellRange { block, ..cells }, cell })
}

/// Runs `f` on the table a command names; unchanged when `f` reports so.
fn with_table(s: &mut Session, params: &Value, f: impl FnOnce(&mut Outline, &Target) -> CmdResult) -> CmdResult {
    with_outline(s, params, |o| {
        let t = target(params, o)?;
        f(o, &t)
    })
}

/// The table at `t.block` (checked by [`target`]).
fn table_mut<'a>(o: &'a mut Outline, t: &Target) -> Result<&'a mut Table, CmdError> {
    o.table_mut(t.block).ok_or_else(|| CmdError::Params(format!("block {} is not a table", t.block)))
}

/// The end of a cell's text.
fn cell_end(o: &Outline, c: CellRef) -> TextPos {
    text::flow(o, Frame::Cell(c)).map(text::end_of).unwrap_or_default()
}

// --- Insert ▸ Table --------------------------------------------------------------------------

fn size_param(params: &Value, key: &str, max: usize) -> Result<usize, CmdError> {
    let n = p::index(params, key).ok_or_else(|| CmdError::Params(format!("`{key}` (number) is required")))?;
    if n == 0 || n > max {
        return Err(CmdError::Params(format!("`{key}` must be 1-{max}")));
    }
    Ok(n)
}

fn insert(s: &mut Session, params: &Value) -> CmdResult {
    let rows = size_param(params, "rows", MAX_INSERT_ROWS)?;
    let cols = size_param(params, "cols", MAX_INSERT_COLS)?;
    if rows.saturating_mul(cols) > MAX_TABLE_CELLS {
        return Err(CmdError::Params(format!("a table holds at most {MAX_TABLE_CELLS} cells")));
    }
    let width = p::f32(params, "colWidth").unwrap_or(DEFAULT_COL_WIDTH);
    let table = Table::new(rows, cols, width);
    if params.get("outline").is_some() || params.get("id").is_some() {
        let id = outline_id(params)?;
        let frame = frame_param(params)?;
        return with_outline(s, params, |o| {
            if o.blocks.len() >= MAX_PARAGRAPHS {
                return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
            }
            let block = match frame {
                // Tables don't nest: one asked for in a cell goes after its table.
                Frame::Cell(c) => {
                    let b = c.block.saturating_add(1).min(o.blocks.len());
                    o.blocks.insert(b, Block::table(table));
                    b
                }
                Frame::Top => place_table(o, params, table),
            };
            if block + 1 == o.blocks.len() && o.blocks.len() > 1 {
                // Somewhere to type below it.
                o.blocks.push(Paragraph::plain("").into());
            }
            let mut out = caret(Some(CellRef::new(block, 0, 0)), TextPos::default());
            if let Value::Object(m) = &mut out.value {
                m.insert("id".into(), json!(id));
            }
            Ok(out)
        });
    }
    let page = target_page(s, params)?;
    if page.elements.len() >= MAX_ELEMENTS_PER_PAGE {
        return Err(CmdError::Failed("this page holds as many items as it can".into()));
    }
    let pos = insert_pos(params, page);
    let mut o = Outline::with_paragraphs(pos, Vec::new());
    o.blocks.push(Block::table(table));
    o.width = o.display_width();
    let el = PageElement::new(ElementKind::Outline(o));
    let id = el.id;
    page.elements.push(el);
    page.touch();
    Ok(CmdOutput::value(json!({"id": id, "cell": CellRef::new(0, 0, 0).to_json(), "caret": TextPos::default().to_json()})))
}

/// Puts `table` at the caret in the outline's own blocks: in place of an empty paragraph, before
/// or after the caret's paragraph at its start or end, or between its halves. Returns its block.
fn place_table(o: &mut Outline, params: &Value, table: Table) -> usize {
    let at = range(params, o).map_or_else(|| text::end_of(o), |(a, _)| a);
    let at = text::clamp(o, at);
    let Some(p) = o.para(at.para) else {
        // The caret names a table: after it.
        let b = at.para.saturating_add(1).min(o.blocks.len());
        o.blocks.insert(b, Block::table(table));
        return b;
    };
    let len = text::para_len(p);
    let plain = p.list.is_none() && p.tag.is_none();
    if len == 0 && plain {
        if let Some(slot) = o.blocks.get_mut(at.para) {
            *slot = Block::table(table);
        }
        return at.para;
    }
    let b = if at.char == 0 {
        at.para
    } else if at.char >= len {
        at.para + 1
    } else {
        text::split_paragraph(o, at).para
    };
    let b = b.min(o.blocks.len());
    o.blocks.insert(b, Block::table(table));
    b
}

/// Paragraph `p`'s runs split at tabs: one paragraph per piece, formatting kept.
fn split_at_tabs(p: &Paragraph) -> Vec<Paragraph> {
    let mut out = vec![Paragraph { runs: Vec::new(), indent: 0, list: None, list_style: None, tag: None, ..p.clone() }];
    for r in &p.runs {
        for (i, piece) in r.text.split('\t').enumerate() {
            if i > 0 {
                out.push(Paragraph { runs: Vec::new(), indent: 0, list: None, list_style: None, tag: None, ..p.clone() });
            }
            if let Some(last) = out.last_mut()
                && !piece.is_empty()
            {
                last.runs.push(Run { text: piece.to_owned(), ..r.clone() });
            }
        }
    }
    for q in &mut out {
        text::normalize(q);
    }
    out
}

fn from_text(s: &mut Session, params: &Value) -> CmdResult {
    let add = p::bool(params, "addColumn").unwrap_or(false);
    with_outline(s, params, |o| {
        let (a, b) = range_or_all(params, o);
        let (lo, hi) = (a.para, b.para.min(o.blocks.len().saturating_sub(1)));
        if (lo..=hi).any(|i| text::Flow::is_table(o, i)) {
            return Err(CmdError::Params("the range holds a table".into()));
        }
        let rows: Vec<Vec<Paragraph>> = (lo..=hi).filter_map(|i| o.para(i)).map(split_at_tabs).collect();
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1) + usize::from(add);
        if rows.is_empty() || cols > MAX_TABLE_COLS || rows.len() > MAX_TABLE_ROWS || rows.len().saturating_mul(cols) > MAX_TABLE_CELLS {
            return Err(CmdError::Params("too many rows or columns for a table".into()));
        }
        let indent = o.para(lo).map_or(0, |p| p.indent);
        let mut t = Table::new(rows.len(), cols, DEFAULT_COL_WIDTH);
        t.indent = indent;
        for (r, cells) in rows.iter().enumerate() {
            for (c, para) in cells.iter().enumerate() {
                if let Some(cell) = t.cell_mut(r, c) {
                    *cell = Cell { paragraphs: vec![para.clone()], shading: None };
                }
            }
        }
        o.blocks.drain(lo..=hi);
        o.blocks.insert(lo.min(o.blocks.len()), Block::table(t));
        if lo + 1 == o.blocks.len() {
            o.blocks.push(Paragraph::plain("").into());
        }
        let first = rows.first().map_or(1, Vec::len);
        let at = if add { CellRef::new(lo, 0, first.min(cols - 1)) } else { CellRef::new(lo, 0, 0) };
        Ok(caret(Some(at), cell_end(o, at)))
    })
}

// --- Table ▸ Insert / Delete -----------------------------------------------------------------

fn insert_row(s: &mut Session, params: &Value, below: bool) -> CmdResult {
    with_table(s, params, |o, t| {
        let at = if below { t.cells.rows.1 + 1 } else { t.cells.rows.0 };
        let like = if below { t.cells.rows.1 } else { t.cells.rows.0 };
        if !table_mut(o, t)?.insert_row(at, Some(like)) {
            return Err(CmdError::Failed("this table holds as many rows as it can".into()));
        }
        let col = t.cell.map_or(t.cells.cols.0, |c| c.col);
        Ok(caret(Some(CellRef::new(t.block, at, col)), TextPos::default()))
    })
}

fn insert_col(s: &mut Session, params: &Value, right: bool) -> CmdResult {
    with_table(s, params, |o, t| {
        let at = if right { t.cells.cols.1 + 1 } else { t.cells.cols.0 };
        let tb = table_mut(o, t)?;
        let width = tb.col_width(if right { t.cells.cols.1 } else { t.cells.cols.0 });
        if !tb.insert_col(at, width) {
            return Err(CmdError::Failed("this table holds as many columns as it can".into()));
        }
        let row = t.cell.map_or(t.cells.rows.0, |c| c.row);
        Ok(caret(Some(CellRef::new(t.block, row, at)), TextPos::default()))
    })
}

/// Removes the table block (an empty paragraph takes its place when nothing else is left);
/// the caret goes to the nearest paragraph.
fn remove_table(o: &mut Outline, block: usize) -> CmdResult {
    if block < o.blocks.len() {
        o.blocks.remove(block);
    }
    let at = match text::nearest_para(o, block.saturating_sub(usize::from(block >= o.blocks.len()))) {
        Some(i) => i,
        None => {
            let i = block.min(o.blocks.len());
            o.blocks.insert(i, Paragraph::plain("").into());
            i
        }
    };
    let ch = if at < block { text::len_at(o, at) } else { 0 };
    Ok(caret(None, TextPos::new(at, ch)))
}

fn delete_rows(s: &mut Session, params: &Value) -> CmdResult {
    with_table(s, params, |o, t| {
        let tb = table_mut(o, t)?;
        let (r0, r1) = t.cells.rows;
        if r0 == 0 && r1 + 1 >= tb.row_count() {
            return remove_table(o, t.block);
        }
        tb.rows.drain(r0..=r1.min(tb.row_count().saturating_sub(1)));
        let row = r0.min(tb.row_count().saturating_sub(1));
        let col = t.cell.map_or(t.cells.cols.0, |c| c.col);
        Ok(caret(Some(CellRef::new(t.block, row, col)), TextPos::default()))
    })
}

fn delete_cols(s: &mut Session, params: &Value) -> CmdResult {
    with_table(s, params, |o, t| {
        let tb = table_mut(o, t)?;
        let (c0, c1) = t.cells.cols;
        if c0 == 0 && c1 + 1 >= tb.col_count() {
            return remove_table(o, t.block);
        }
        for c in (c0..=c1).rev() {
            tb.remove_col(c);
        }
        let col = c0.min(tb.col_count().saturating_sub(1));
        let row = t.cell.map_or(t.cells.rows.0, |c| c.row);
        Ok(caret(Some(CellRef::new(t.block, row, col)), TextPos::default()))
    })
}

fn delete_table(s: &mut Session, params: &Value) -> CmdResult {
    with_table(s, params, |o, t| remove_table(o, t.block))
}

// --- Table ▸ Select --------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Sel {
    Cell,
    Row,
    Column,
    Table,
}

fn select(s: &mut Session, params: &Value, what: Sel) -> CmdResult {
    let id = outline_id(params)?;
    let page = target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let t = target(params, o)?;
    let tb = o.table(t.block).ok_or_else(|| CmdError::Params(format!("block {} is not a table", t.block)))?;
    let (last_r, last_c) = (tb.row_count().saturating_sub(1), tb.col_count().saturating_sub(1));
    let r = t.cells;
    let out = match what {
        Sel::Cell => r,
        Sel::Row => CellRange { cols: (0, last_c), ..r },
        Sel::Column => CellRange { rows: (0, last_r), ..r },
        Sel::Table => CellRange { block: t.block, rows: (0, last_r), cols: (0, last_c) },
    };
    Ok(CmdOutput::query(json!({"cells": out.to_json()})))
}

// --- Table ▸ Format --------------------------------------------------------------------------

fn set_width(s: &mut Session, params: &Value) -> CmdResult {
    let w = p::req_f32(params, "width")?;
    let w = notecraft_doc::table::clamp_width(w);
    with_table(s, params, |o, t| {
        let col = p::index(params, "col").or(t.cell.map(|c| c.col)).unwrap_or(t.cells.cols.0);
        let tb = table_mut(o, t)?;
        let n = tb.col_count();
        let col = col.min(n.saturating_sub(1));
        tb.normalize();
        let before = (tb.col_widths.get(col).copied(), tb.col_locked.get(col).copied());
        if let Some(cw) = tb.col_widths.get_mut(col) {
            *cw = w;
        }
        if tb.col_locked.len() < n {
            tb.col_locked.resize(n, false);
        }
        if let Some(l) = tb.col_locked.get_mut(col) {
            *l = true;
        }
        let after = (tb.col_widths.get(col).copied(), tb.col_locked.get(col).copied());
        Ok(if before == after { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

fn shading(s: &mut Session, params: &Value) -> CmdResult {
    if params.get("color").is_none() {
        return Err(CmdError::Params("`color` (#rrggbb or null) is required".into()));
    }
    let c = p::color(params, "color")?;
    with_table(s, params, |o, t| {
        let tb = table_mut(o, t)?;
        let mut changed = false;
        for cr in t.cells.cells() {
            if let Some(cell) = tb.cell_mut(cr.row, cr.col)
                && cell.shading != c
            {
                cell.shading = c;
                changed = true;
            }
        }
        Ok(if changed { CmdOutput::ok() } else { CmdOutput::unchanged() })
    })
}

fn borders(s: &mut Session, params: &Value) -> CmdResult {
    let value = p::bool(params, "value");
    with_table(s, params, |o, t| {
        let tb = table_mut(o, t)?;
        let on = value.unwrap_or(!tb.borders);
        if tb.borders == on {
            return Ok(CmdOutput::unchanged());
        }
        tb.borders = on;
        Ok(CmdOutput::value(json!({"borders": on})))
    })
}

/// A sort key: a number when the text reads as one, else the text without case.
fn sort_key(cell: &Cell) -> (u8, f64, String) {
    let t = cell.text();
    let t = t.trim();
    match t.replace(',', "").parse::<f64>() {
        Ok(n) if n.is_finite() => (0, n, String::new()),
        _ => (if t.is_empty() { 2 } else { 1 }, 0.0, t.to_lowercase()),
    }
}

fn sort(s: &mut Session, params: &Value) -> CmdResult {
    let desc = p::bool(params, "descending").unwrap_or(false);
    let header = p::bool(params, "header").unwrap_or(false);
    with_table(s, params, |o, t| {
        let tb = table_mut(o, t)?;
        let col = p::index(params, "col").or(t.cell.map(|c| c.col)).unwrap_or(t.cells.cols.0).min(tb.col_count().saturating_sub(1));
        let skip = usize::from(header).min(tb.rows.len());
        let before = tb.rows.clone();
        let rest = tb.rows.get_mut(skip..).unwrap_or_default();
        let key = |r: &notecraft_doc::Row| r.cells.get(col).map_or((2, 0.0, String::new()), sort_key);
        rest.sort_by(|a, b| {
            let (x, y) = (key(a), key(b));
            let ord = x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| x.2.cmp(&y.2));
            // Numbers before text, empty cells last, whichever way the rest sorts.
            x.0.cmp(&y.0).then(if desc { ord.reverse() } else { ord })
        });
        Ok(if tb.rows == before { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

fn to_text(s: &mut Session, params: &Value) -> CmdResult {
    with_table(s, params, |o, t| {
        let tb = o.table(t.block).cloned().ok_or_else(|| CmdError::Params(format!("block {} is not a table", t.block)))?;
        let mut paras = Vec::with_capacity(tb.rows.len());
        for row in &tb.rows {
            let mut p = Paragraph { indent: tb.indent, ..Paragraph::default() };
            for (c, cell) in row.cells.iter().enumerate() {
                if c > 0 {
                    let fmt = p.runs.last().map(|r| Run { text: String::new(), link: None, ..r.clone() }).unwrap_or_default();
                    p.runs.push(Run { text: "\t".into(), ..fmt });
                }
                for (i, cp) in cell.paragraphs.iter().enumerate() {
                    if i > 0 {
                        p.runs.push(Run::plain(" "));
                    }
                    if p.runs.is_empty() {
                        p.style = cp.style;
                        p.align = cp.align;
                    }
                    p.runs.extend(cp.runs.iter().filter(|r| !r.text.is_empty()).cloned());
                }
            }
            text::normalize(&mut p);
            paras.push(Block::Paragraph(p));
        }
        let b = t.block.min(o.blocks.len());
        o.blocks.splice(b..(b + 1).min(o.blocks.len()), paras);
        Ok(caret(None, TextPos::new(b, 0)))
    })
}

#[cfg(test)]
mod tests {
    use notecraft_doc::Point;

    use super::*;

    #[test]
    fn tabs_split_into_cells_keeping_formats() {
        let p =
            Paragraph { runs: vec![Run::plain("a\tb"), Run { text: "c\t".into(), bold: true, ..Run::default() }], indent: 2, ..Paragraph::default() };
        let cells = split_at_tabs(&p);
        assert_eq!(cells.iter().map(Paragraph::text).collect::<Vec<_>>(), vec!["a", "bc", ""]);
        assert!(cells[1].runs[1].bold && !cells[1].runs[0].bold);
        assert!(cells.iter().all(|c| c.indent == 0));
    }

    #[test]
    fn tables_go_at_the_caret() {
        let mut o = Outline::with_text(Point::ZERO, "one\n\nthree");
        assert_eq!(place_table(&mut o, &json!({"at": [1, 0]}), Table::new(1, 1, 72.0)), 1);
        assert!(o.blocks[1].is_table() && o.blocks.len() == 3, "the empty paragraph is replaced");
        assert_eq!(place_table(&mut o, &json!({"at": [0, 1]}), Table::new(1, 1, 72.0)), 1);
        assert_eq!(o.text().lines().next(), Some("o"), "{:?}", o.text());
        assert_eq!(place_table(&mut o, &json!({"at": [99, 99]}), Table::new(1, 1, 72.0)), o.blocks.len() - 1);
        let sk = |s: &str| sort_key(&Cell::with_text(s));
        assert!(sk("2") < sk("10") && sk("10") < sk("apple") && sk("Apple") == sk("apple") && sk("zebra") < sk(""));
    }
}
