//! Tables in note containers (Insert ▸ Table), as MS-ONE has them: a table is one block of an
//! outline (an outline element whose content is a jcidTableNode), holding rows of cells, and each
//! cell holds paragraphs of rich text.
//!
//! A cell holds paragraphs only: tables don't nest in NoteCraft 0.2 (the type can't express a
//! table inside a cell), so layout, editing and the readers never recurse more than once.
//!
//! The grid is kept rectangular: every row has exactly [`Table::cols`] cells and there is one
//! column width per column ([`Table::normalize`]). Column widths are points.

use serde::{Deserialize, Serialize};

use crate::page::{MAX_INDENT, Paragraph};
use crate::Color;

/// Most rows a table has (hostile files and commands).
pub const MAX_TABLE_ROWS: usize = 2_000;
/// Most columns a table has.
pub const MAX_TABLE_COLS: usize = 64;
/// Most cells a table has (rows × columns).
pub const MAX_TABLE_CELLS: usize = 10_000;
/// Most paragraphs one cell holds.
pub const MAX_CELL_PARAGRAPHS: usize = 1_000;
/// Narrowest column, points (a quarter inch).
pub const MIN_COL_WIDTH: f32 = 18.0;
/// Widest column, points.
pub const MAX_COL_WIDTH: f32 = 2_000.0;
/// Width of a new table's columns, points (an inch).
pub const DEFAULT_COL_WIDTH: f32 = 72.0;
/// Space between a cell's border and its text, left and right, points (the canvas and the PDF
/// lay cells out alike).
pub const CELL_PAD_X: f32 = 4.0;
/// Space between a cell's border and its text, top and bottom, points.
pub const CELL_PAD_Y: f32 = 2.0;
/// Space above and below a table in its container, points.
pub const TABLE_GAP: f32 = 3.0;

/// One block of a note container: a paragraph or a table.
///
/// In JSON a paragraph is written as before (an object of runs and formatting) and a table as
/// `{"table": {…}}`, so older files whose outlines hold `"paragraphs"` read unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Block {
    /// Tried first: it needs the `table` key, which no paragraph has.
    Table {
        table: Table,
    },
    Paragraph(Paragraph),
}

impl Block {
    pub fn table(t: Table) -> Block {
        Block::Table { table: t }
    }

    pub fn as_paragraph(&self) -> Option<&Paragraph> {
        match self {
            Block::Paragraph(p) => Some(p),
            Block::Table { .. } => None,
        }
    }

    pub fn as_paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        match self {
            Block::Paragraph(p) => Some(p),
            Block::Table { .. } => None,
        }
    }

    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Block::Table { table } => Some(table),
            Block::Paragraph(_) => None,
        }
    }

    pub fn as_table_mut(&mut self) -> Option<&mut Table> {
        match self {
            Block::Table { table } => Some(table),
            Block::Paragraph(_) => None,
        }
    }

    pub fn is_table(&self) -> bool {
        matches!(self, Block::Table { .. })
    }

    /// The block's paragraphs: itself, or every cell's in reading order (row by row).
    pub fn paragraphs(&self) -> Box<dyn Iterator<Item = &Paragraph> + '_> {
        match self {
            Block::Paragraph(p) => Box::new(std::iter::once(p)),
            Block::Table { table } => Box::new(table.paragraphs()),
        }
    }

    pub fn paragraphs_mut(&mut self) -> Box<dyn Iterator<Item = &mut Paragraph> + '_> {
        match self {
            Block::Paragraph(p) => Box::new(std::iter::once(p)),
            Block::Table { table } => Box::new(table.rows.iter_mut().flat_map(|r| r.cells.iter_mut()).flat_map(|c| c.paragraphs.iter_mut())),
        }
    }

    /// Plain text: the paragraph's, or the table's rows (cells separated by tabs, one line per
    /// row).
    pub fn text(&self) -> String {
        match self {
            Block::Paragraph(p) => p.text(),
            Block::Table { table } => table.text(),
        }
    }

    /// Indent level (a table's own, or the paragraph's).
    pub fn indent(&self) -> u8 {
        match self {
            Block::Paragraph(p) => p.indent,
            Block::Table { table } => table.indent,
        }
    }
}

impl From<Paragraph> for Block {
    fn from(p: Paragraph) -> Block {
        Block::Paragraph(p)
    }
}

impl From<Table> for Block {
    fn from(t: Table) -> Block {
        Block::table(t)
    }
}

/// A table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    /// One width per column, points.
    pub col_widths: Vec<f32>,
    /// Columns whose width the user set (MS-ONE TableColumnsLocked). NoteCraft draws every
    /// column at its width either way; the flags are kept so files round-trip.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub col_locked: Vec<bool>,
    pub rows: Vec<Row>,
    /// Borders drawn (Table ▸ Hide Borders turns them off; the canvas then shows them dashed
    /// while editing).
    #[serde(default = "yes")]
    pub borders: bool,
    /// Indent level of the table in its container, 0..=[`MAX_INDENT`].
    #[serde(default)]
    pub indent: u8,
}

fn yes() -> bool {
    true
}

/// A table row.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Row {
    pub cells: Vec<Cell>,
}

/// A table cell: paragraphs of rich text and an optional background colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cell {
    pub paragraphs: Vec<Paragraph>,
    /// Table ▸ Shading.
    pub shading: Option<Color>,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell { paragraphs: vec![Paragraph::plain("")], shading: None }
    }
}

impl Cell {
    /// A cell holding `text` (one paragraph per line).
    pub fn with_text(text: &str) -> Cell {
        Cell { paragraphs: text.split('\n').map(Paragraph::plain).collect(), shading: None }
    }

    /// The paragraphs' text joined with newlines.
    pub fn text(&self) -> String {
        self.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.text().is_empty())
    }
}

impl Table {
    /// An empty `rows` × `cols` table (clamped to the caps, at least 1 × 1) with columns
    /// `col_width` points wide.
    pub fn new(rows: usize, cols: usize, col_width: f32) -> Table {
        let cols = cols.clamp(1, MAX_TABLE_COLS);
        let rows = rows.clamp(1, MAX_TABLE_ROWS).min((MAX_TABLE_CELLS / cols).max(1));
        let w = clamp_width(col_width);
        Table {
            col_widths: vec![w; cols],
            col_locked: Vec::new(),
            rows: (0..rows).map(|_| Row { cells: vec![Cell::default(); cols] }).collect(),
            borders: true,
            indent: 0,
        }
    }

    /// A table of plain text, one row per entry (the longest row sets the column count).
    pub fn from_text(rows: &[Vec<String>], col_width: f32) -> Table {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let mut t = Table::new(rows.len().max(1), cols, col_width);
        for (r, row) in rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                if let Some(cell) = t.cell_mut(r, c) {
                    *cell = Cell::with_text(text);
                }
            }
        }
        t
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Columns: the widest row's cell count (the same for every row after [`Table::normalize`]).
    pub fn col_count(&self) -> usize {
        self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(self.col_widths.len().min(MAX_TABLE_COLS))
    }

    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.rows.get(row)?.cells.get(col)
    }

    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.rows.get_mut(row)?.cells.get_mut(col)
    }

    /// Column `col`'s width, points (the default for a column without one).
    pub fn col_width(&self, col: usize) -> f32 {
        self.col_widths.get(col).copied().map_or(DEFAULT_COL_WIDTH, clamp_width)
    }

    /// The table's width: its columns', points.
    pub fn width(&self) -> f32 {
        (0..self.col_count()).map(|c| self.col_width(c)).sum()
    }

    /// Every cell's paragraphs in reading order.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.paragraphs.iter())
    }

    /// Rows of cell texts (a cell's paragraphs joined with spaces).
    pub fn texts(&self) -> Vec<Vec<String>> {
        self.rows.iter().map(|r| r.cells.iter().map(|c| c.paragraphs.iter().map(Paragraph::text).collect::<Vec<_>>().join(" ")).collect()).collect()
    }

    /// One line per row, cells separated by tabs (plain-text export and search).
    pub fn text(&self) -> String {
        self.texts().iter().map(|r| r.join("\t")).collect::<Vec<_>>().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|r| r.cells.iter().all(Cell::is_empty))
    }

    /// Makes the table safe to lay out and edit (hostile files, JSON and commands): at least one
    /// row and column and at most the caps, every row exactly [`Table::col_count`] cells, one
    /// finite clamped width per column, every cell at least one (and at most
    /// [`MAX_CELL_PARAGRAPHS`]) paragraphs, indents clamped.
    pub fn normalize(&mut self) {
        let cols = self.col_count().clamp(1, MAX_TABLE_COLS);
        let max_rows = (MAX_TABLE_CELLS / cols).clamp(1, MAX_TABLE_ROWS);
        self.rows.truncate(max_rows);
        if self.rows.is_empty() {
            self.rows.push(Row::default());
        }
        for row in &mut self.rows {
            row.cells.truncate(cols);
            while row.cells.len() < cols {
                row.cells.push(Cell::default());
            }
            for cell in &mut row.cells {
                cell.paragraphs.truncate(MAX_CELL_PARAGRAPHS);
                if cell.paragraphs.is_empty() {
                    cell.paragraphs.push(Paragraph::plain(""));
                }
                for p in &mut cell.paragraphs {
                    p.indent = p.indent.min(MAX_INDENT);
                }
            }
        }
        self.col_widths.truncate(cols);
        while self.col_widths.len() < cols {
            self.col_widths.push(DEFAULT_COL_WIDTH);
        }
        for w in &mut self.col_widths {
            *w = clamp_width(*w);
        }
        self.col_locked.truncate(cols);
        self.indent = self.indent.min(MAX_INDENT);
    }

    /// Inserts an empty row at `at` (clamped), its cells shaded like the row it was copied
    /// from (`like`). `false` when the table is full.
    pub fn insert_row(&mut self, at: usize, like: Option<usize>) -> bool {
        let cols = self.col_count().max(1);
        if self.rows.len() >= MAX_TABLE_ROWS || (self.rows.len() + 1).saturating_mul(cols) > MAX_TABLE_CELLS {
            return false;
        }
        let shading = |c: usize| like.and_then(|r| self.cell(r, c)).and_then(|c| c.shading);
        let row = Row { cells: (0..cols).map(|c| Cell { shading: shading(c), ..Cell::default() }).collect() };
        let at = at.min(self.rows.len());
        self.rows.insert(at, row);
        true
    }

    /// Inserts an empty column at `at` (clamped), `width` points wide. `false` when full.
    pub fn insert_col(&mut self, at: usize, width: f32) -> bool {
        let cols = self.col_count();
        if cols >= MAX_TABLE_COLS || self.rows.len().saturating_mul(cols + 1) > MAX_TABLE_CELLS {
            return false;
        }
        let at = at.min(cols);
        for row in &mut self.rows {
            let i = at.min(row.cells.len());
            row.cells.insert(i, Cell::default());
        }
        let i = at.min(self.col_widths.len());
        self.col_widths.insert(i, clamp_width(width));
        if self.col_locked.len() > i {
            self.col_locked.insert(i, false);
        }
        true
    }

    /// Removes row `at`; `false` when it doesn't exist or is the only one.
    pub fn remove_row(&mut self, at: usize) -> bool {
        if self.rows.len() <= 1 || at >= self.rows.len() {
            return false;
        }
        self.rows.remove(at);
        true
    }

    /// Removes column `at`; `false` when it doesn't exist or is the only one.
    pub fn remove_col(&mut self, at: usize) -> bool {
        let cols = self.col_count();
        if cols <= 1 || at >= cols {
            return false;
        }
        for row in &mut self.rows {
            if at < row.cells.len() {
                row.cells.remove(at);
            }
        }
        if at < self.col_widths.len() {
            self.col_widths.remove(at);
        }
        if at < self.col_locked.len() {
            self.col_locked.remove(at);
        }
        true
    }
}

/// A column width clamped to [`MIN_COL_WIDTH`]..=[`MAX_COL_WIDTH`] (junk → the default).
pub fn clamp_width(w: f32) -> f32 {
    if w.is_finite() { w.clamp(MIN_COL_WIDTH, MAX_COL_WIDTH) } else { DEFAULT_COL_WIDTH }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Outline;
    use crate::Point;

    #[test]
    fn new_tables_are_rectangular_and_capped() {
        let t = Table::new(3, 4, 50.0);
        assert_eq!((t.row_count(), t.col_count()), (3, 4));
        assert_eq!(t.width(), 200.0);
        let big = Table::new(usize::MAX, usize::MAX, f32::NAN);
        assert_eq!(big.col_count(), MAX_TABLE_COLS);
        assert!(big.row_count() * big.col_count() <= MAX_TABLE_CELLS);
        assert_eq!(big.col_width(0), DEFAULT_COL_WIDTH);
        assert_eq!(Table::new(0, 0, 1.0).col_width(0), MIN_COL_WIDTH);
    }

    #[test]
    fn normalize_fixes_ragged_hostile_tables() {
        let mut t = Table {
            col_widths: vec![f32::INFINITY],
            col_locked: vec![true; 9],
            rows: vec![Row { cells: vec![Cell::default(), Cell { paragraphs: Vec::new(), shading: None }, Cell::default()] }, Row::default()],
            borders: false,
            indent: 200,
        };
        t.normalize();
        assert_eq!(t.col_count(), 3);
        assert!(t.rows.iter().all(|r| r.cells.len() == 3));
        assert!(t.rows.iter().flat_map(|r| &r.cells).all(|c| !c.paragraphs.is_empty()));
        assert_eq!(t.col_widths, vec![DEFAULT_COL_WIDTH; 3]);
        assert_eq!(t.col_locked.len(), 3);
        assert_eq!(t.indent, MAX_INDENT);
        let mut empty = Table { col_widths: Vec::new(), col_locked: Vec::new(), rows: Vec::new(), borders: true, indent: 0 };
        empty.normalize();
        assert_eq!((empty.row_count(), empty.col_count()), (1, 1));
    }

    #[test]
    fn rows_and_columns_insert_and_remove() {
        let mut t = Table::new(2, 2, 72.0);
        t.cell_mut(0, 0).unwrap().shading = Some(Color::rgb(1, 2, 3));
        assert!(t.insert_row(1, Some(0)));
        assert_eq!(t.cell(1, 0).unwrap().shading, Some(Color::rgb(1, 2, 3)));
        assert!(t.insert_col(99, 40.0));
        assert_eq!((t.row_count(), t.col_count(), t.col_widths.len()), (3, 3, 3));
        assert_eq!(t.col_width(2), 40.0);
        assert!(t.remove_row(0) && t.remove_col(0));
        assert!(!t.remove_row(99) && !t.remove_col(99));
        let mut one = Table::new(1, 1, 72.0);
        assert!(!one.remove_row(0) && !one.remove_col(0), "the last row and column stay");
    }

    #[test]
    fn blocks_serialize_and_old_outlines_read() {
        let mut t = Table::from_text(&[vec!["a".into(), "b".into()], vec!["c".into()]], 60.0);
        t.normalize();
        assert_eq!(t.text(), "a\tb\nc\t");
        let o = Outline {
            pos: Point::new(1.0, 2.0),
            width: 300.0,
            blocks: vec![Block::Paragraph(Paragraph::plain("before")), Block::table(t), Block::Paragraph(Paragraph::plain("after"))],
        };
        let j = serde_json::to_string(&o).unwrap();
        let back: Outline = serde_json::from_str(&j).unwrap();
        assert_eq!(back, o, "{j}");
        assert_eq!(back.paragraphs().count(), 2 + 4);
        assert_eq!(o.text(), "before\na\tb\nc\t\nafter");
        // 0.1 JSON: an outline of paragraphs.
        let old: Outline = serde_json::from_str(r#"{"pos":{"x":0,"y":0},"width":200,"paragraphs":[{"runs":[{"text":"hi","bold":true}]},{"runs":[]}]}"#).unwrap();
        assert_eq!(old.blocks.len(), 2);
        assert!(old.blocks.iter().all(|b| !b.is_table()));
        assert_eq!(old.text(), "hi\n");
    }
}
