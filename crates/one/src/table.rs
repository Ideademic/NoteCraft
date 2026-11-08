//! Tables ([MS-ONE] §2.2.27 jcidTableNode, §2.2.28 jcidTableRowNode, §2.2.29
//! jcidTableCellNode): an outline element's content that holds rows, whose cells hold outline
//! elements of their own.
//!
//! - **Table:** ElementChildNodes (the rows), RowCount, ColumnCount, TableBordersVisible,
//!   TableColumnWidths (a count byte, then a 4-byte float per column in half-inches) and
//!   TableColumnsLocked (a count byte, then a bit per column, lowest bit first).
//! - **Row:** ElementChildNodes (the cells).
//! - **Cell:** ElementChildNodes (outline elements, read and written like an outline's),
//!   CellShadingColor, and the outline layout properties an outline carries.
//!
//! The layout of the two arrays was checked against the Apache Tika sample files (data, not
//! code: AGENTS.md, Clean room).

use notecraft_doc::table::{MAX_TABLE_CELLS, MAX_TABLE_ROWS, MIN_COL_WIDTH};
use notecraft_doc::{Cell, Row, Table};
use notecraft_onestore::{Object, PropertySet, PropertyValue};

use crate::ids::{jcid, prop};
use crate::read::{Reader, View};
use crate::util::{from_colorref, hi_to_pt, to_colorref};

/// The table `o` (a jcidTableNode), its cells read with `r`. `depth` is the outline nesting it
/// sits at (nested tables are bounded by the reader's depth limit).
pub(crate) fn read(r: &mut Reader, v: &View<'_>, o: &Object, depth: usize) -> Table {
    let p = &o.props;
    let mut t = Table {
        col_widths: widths(p),
        col_locked: locked(p),
        rows: Vec::new(),
        borders: p.bool(prop::TABLE_BORDERS_VISIBLE).unwrap_or(true),
        indent: 0,
    };
    let mut cells = 0usize;
    for row in v.children(p, prop::ELEMENT_CHILD_NODES) {
        if row.jcid != jcid::TABLE_ROW_NODE {
            r.unsupported = true;
            continue;
        }
        if t.rows.len() >= MAX_TABLE_ROWS {
            r.unsupported = true;
            break;
        }
        let mut out = Row::default();
        for cell in v.children(&row.props, prop::ELEMENT_CHILD_NODES) {
            if cell.jcid != jcid::TABLE_CELL_NODE {
                r.unsupported = true;
                continue;
            }
            cells += 1;
            if cells > MAX_TABLE_CELLS {
                r.unsupported = true;
                break;
            }
            let paragraphs = r.cell_paragraphs(v, &cell.props, depth);
            let shading = cell.props.u32(prop::CELL_SHADING_COLOR).and_then(from_colorref);
            out.cells.push(Cell { paragraphs, shading });
        }
        t.rows.push(out);
    }
    // ColumnCount names columns the rows may not all have; normalizing pads them.
    let declared = p.u32(prop::COLUMN_COUNT).map_or(0, |c| c as usize).min(notecraft_doc::table::MAX_TABLE_COLS);
    if declared > t.col_count() {
        t.col_widths.resize(declared, notecraft_doc::table::DEFAULT_COL_WIDTH);
    }
    t.normalize();
    t
}

/// TableColumnWidths → points.
fn widths(p: &PropertySet) -> Vec<f32> {
    let Some(b) = p.bytes(prop::TABLE_COLUMN_WIDTHS) else { return Vec::new() };
    let n = usize::from(b.first().copied().unwrap_or(0));
    b.get(1..).unwrap_or_default().as_chunks::<4>().0.iter().take(n).map(|c| hi_to_pt(f32::from_le_bytes(*c)).max(MIN_COL_WIDTH)).collect()
}

/// TableColumnsLocked → one flag per column.
fn locked(p: &PropertySet) -> Vec<bool> {
    let Some(b) = p.bytes(prop::TABLE_COLUMNS_LOCKED) else { return Vec::new() };
    let n = usize::from(b.first().copied().unwrap_or(0));
    let bits = b.get(1..).unwrap_or_default();
    let flags: Vec<bool> = (0..n).map(|i| bits.get(i / 8).is_some_and(|byte| byte >> (i % 8) & 1 == 1)).collect();
    // None locked reads as none given, as NoteCraft writes it.
    if flags.iter().any(|f| *f) { flags } else { Vec::new() }
}

/// The table node's properties (its rows are `rows`).
pub(crate) fn table_props(t: &Table, rows: Vec<notecraft_onestore::ExtendedGuid>, modified: u32) -> PropertySet {
    let cols = t.col_count().min(255);
    let mut widths = vec![cols as u8];
    for c in 0..cols {
        widths.extend_from_slice(&notecraft_doc::geom::pt_to_half_inch(t.col_width(c)).to_le_bytes());
    }
    let mut locked = vec![cols as u8];
    locked.resize(1 + cols.div_ceil(8), 0);
    for c in 0..cols {
        if t.col_locked.get(c).copied().unwrap_or(false)
            && let Some(byte) = locked.get_mut(1 + c / 8)
        {
            *byte |= 1 << (c % 8);
        }
    }
    let mut p = PropertySet::default();
    p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(modified))
        .push(prop::ROW_COUNT, PropertyValue::U32(u32::try_from(t.row_count()).unwrap_or(u32::MAX)))
        .push(prop::COLUMN_COUNT, PropertyValue::U32(cols as u32))
        .push(prop::TABLE_BORDERS_VISIBLE, PropertyValue::Bool(t.borders))
        .push(prop::TABLE_COLUMN_WIDTHS, PropertyValue::Bytes(widths))
        .push(prop::TABLE_COLUMNS_LOCKED, PropertyValue::Bytes(locked))
        .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(rows));
    p
}

/// A row node's properties.
pub(crate) fn row_props(cells: Vec<notecraft_onestore::ExtendedGuid>, modified: u32) -> PropertySet {
    let mut p = PropertySet::default();
    p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(modified)).push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(cells));
    p
}

/// A cell node's properties (its outline elements are `oes`), with the outline layout
/// properties OneNote writes on cells.
pub(crate) fn cell_props(cell: &Cell, oes: Vec<notecraft_onestore::ExtendedGuid>, modified: u32, indents: PropertyValue) -> PropertySet {
    let mut p = PropertySet::default();
    p.push(prop::LAST_MODIFIED_TIME, PropertyValue::U32(modified))
        .push(prop::ELEMENT_CHILD_NODES, PropertyValue::Ids(oes))
        .push(prop::RG_OUTLINE_INDENT_DISTANCE, indents)
        .push(prop::BODY_TEXT_ALIGNMENT, PropertyValue::U8(0))
        .push(prop::OUTLINE_ELEMENT_CHILD_LEVEL, PropertyValue::U8(1))
        .push(prop::ENFORCE_OUTLINE_STRUCTURE, PropertyValue::Bool(true));
    if let Some(c) = cell.shading {
        p.push(prop::CELL_SHADING_COLOR, PropertyValue::U32(to_colorref(c)));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_and_locks_decode_like_the_samples() {
        let mut p = PropertySet::default();
        // Two columns of 1.03 and 2.03 half-inches; the second locked (as a sample file has it).
        let mut w = vec![2u8];
        w.extend_from_slice(&1.03f32.to_le_bytes());
        w.extend_from_slice(&2.03f32.to_le_bytes());
        p.push(prop::TABLE_COLUMN_WIDTHS, PropertyValue::Bytes(w));
        p.push(prop::TABLE_COLUMNS_LOCKED, PropertyValue::Bytes(vec![2, 2]));
        let ws = widths(&p);
        assert_eq!(ws.len(), 2);
        assert!((ws[0] - 1.03 * 36.0).abs() < 0.01 && (ws[1] - 2.03 * 36.0).abs() < 0.01, "{ws:?}");
        assert_eq!(locked(&p), vec![false, true]);
        // Truncated and hostile arrays never panic.
        let mut bad = PropertySet::default();
        bad.push(prop::TABLE_COLUMN_WIDTHS, PropertyValue::Bytes(vec![200, 1, 2]));
        bad.push(prop::TABLE_COLUMNS_LOCKED, PropertyValue::Bytes(vec![20]));
        assert!(widths(&bad).is_empty());
        assert!(locked(&bad).is_empty(), "no column locked");
        let mut nan = PropertySet::default();
        let mut w = vec![1u8];
        w.extend_from_slice(&f32::NAN.to_le_bytes());
        nan.push(prop::TABLE_COLUMN_WIDTHS, PropertyValue::Bytes(w));
        assert!(widths(&nan).iter().all(|w| w.is_finite()));
    }

    /// A page whose outline holds text, a rich table and more text.
    fn table_section() -> notecraft_doc::Section {
        use notecraft_doc::{Block, Color, ElementKind, ListKind, Outline, Page, PageElement, Paragraph, Point, Run, Section, Tag};
        let mut t = Table::new(3, 3, 80.0);
        t.col_widths = vec![60.0, 120.5, 90.0];
        t.col_locked = vec![false, true, false];
        t.borders = false;
        t.indent = 1;
        for (r, row) in [["Name", "Qty", "Notes"], ["eggs", "12", ""], ["milk", "2", "semi-skimmed"]].into_iter().enumerate() {
            for (c, text) in row.into_iter().enumerate() {
                *t.cell_mut(r, c).unwrap() = Cell::with_text(text);
            }
        }
        let head = t.cell_mut(0, 0).unwrap();
        head.shading = Some(Color::rgb(0xFF, 0xE6, 0x99));
        head.paragraphs[0].runs[0].bold = true;
        let notes = t.cell_mut(2, 2).unwrap();
        notes.paragraphs.push(Paragraph {
            runs: vec![Run { text: "from the farm".into(), italic: true, ..Run::default() }],
            list: Some(ListKind::Bullet),
            ..Paragraph::default()
        });
        notes.paragraphs.push(Paragraph { tag: Some(Tag::ToDo { checked: true }), ..Paragraph::plain("buy") });
        t.cell_mut(1, 2).unwrap().shading = Some(Color::rgb(0xC6, 0xEF, 0xCE));
        let mut o = Outline::with_text(Point::new(36.0, 108.0), "Shopping");
        o.blocks.push(Block::table(t));
        o.blocks.push(Paragraph::plain("after the table").into());
        let only = Outline { pos: Point::new(400.0, 108.0), width: 200.0, blocks: vec![Block::table(Table::new(1, 2, 72.0))] };
        let mut page = Page::new("Tables");
        page.elements.push(PageElement::new(ElementKind::Outline(o)));
        page.elements.push(PageElement::new(ElementKind::Outline(only)));
        let mut s = Section::new("Tables", Color::rgb(1, 2, 3));
        s.pages.push(page);
        s
    }

    fn outlines(s: &notecraft_doc::Section) -> Vec<notecraft_doc::Outline> {
        s.pages[0]
            .elements
            .iter()
            .filter_map(|e| match &e.kind {
                notecraft_doc::ElementKind::Outline(o) => Some(o.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn tables_round_trip_through_one_files() {
        let s = table_section();
        let back = crate::read_section(&crate::write_section(&s).unwrap()).unwrap();
        assert!(!back.source.unsupported_content, "tables are real content now");
        let (a, b) = (outlines(&s), outlines(&back));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.blocks.len(), y.blocks.len());
            for (bx, by) in x.blocks.iter().zip(&y.blocks) {
                match (bx.as_table(), by.as_table()) {
                    (Some(tx), Some(ty)) => {
                        assert_eq!(tx.rows, ty.rows, "cells, rich text, lists, tags and shading");
                        assert_eq!((tx.borders, tx.indent, &tx.col_locked), (ty.borders, ty.indent, &ty.col_locked));
                        for (wx, wy) in tx.col_widths.iter().zip(&ty.col_widths) {
                            assert!((wx - wy).abs() < 0.01, "{wx} vs {wy}");
                        }
                    }
                    (None, None) => assert_eq!(bx, by),
                    _ => panic!("a block changed kind: {bx:?} → {by:?}"),
                }
            }
        }
        // Writing what was read gives the same again.
        let again = crate::read_section(&crate::write_section(&back).unwrap()).unwrap();
        assert_eq!(outlines(&again)[0].blocks, outlines(&back)[0].blocks);
    }

    #[test]
    fn hostile_table_objects_read_safely() {
        // Every cell's properties junk, rows of the wrong type: the reader keeps a rectangle.
        let s = table_section();
        let bytes = crate::write_section(&s).unwrap();
        let mut store = notecraft_onestore::read(&bytes).unwrap();
        for sp in &mut store.spaces {
            for o in &mut sp.objects {
                match o.jcid {
                    jcid::TABLE_NODE => {
                        o.props.props.retain(|(id, _)| id.0 & 0x03FF_FFFF != prop::TABLE_COLUMN_WIDTHS & 0x03FF_FFFF);
                        o.props.push(prop::TABLE_COLUMN_WIDTHS, PropertyValue::Bytes(vec![255, 0, 0]));
                        o.props.push(prop::COLUMN_COUNT, PropertyValue::U32(u32::MAX));
                    }
                    jcid::TABLE_CELL_NODE => {
                        o.props.props.retain(|(id, _)| id.0 & 0x03FF_FFFF != prop::CELL_SHADING_COLOR & 0x03FF_FFFF);
                        o.props.push(prop::CELL_SHADING_COLOR, PropertyValue::U32(u32::MAX));
                    }
                    _ => {}
                }
            }
        }
        let back = crate::read::section(&store, "x").unwrap();
        for o in outlines(&back) {
            for t in o.blocks.iter().filter_map(notecraft_doc::Block::as_table) {
                let cols = t.col_count();
                assert!(cols <= notecraft_doc::table::MAX_TABLE_COLS);
                assert!(t.rows.iter().all(|r| r.cells.len() == cols && r.cells.iter().all(|c| c.shading.is_none())));
            }
        }
    }

    #[test]
    fn corpus_tables_read_as_tables() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/tika");
        let mut found = 0;
        for name in ["testOneNote1.one", "testOneNote2.one"] {
            let Ok(bytes) = std::fs::read(dir.join(name)) else {
                eprintln!("corpus/ not fetched (scripts/fetch-corpus.sh): skipped");
                return;
            };
            let s = crate::read_section(&bytes).unwrap();
            let tables: Vec<Table> = s
                .pages
                .iter()
                .flat_map(|p| p.elements.iter())
                .filter_map(|e| match &e.kind {
                    notecraft_doc::ElementKind::Outline(o) => Some(o.blocks.iter().filter_map(|b| b.as_table().cloned()).collect::<Vec<_>>()),
                    _ => None,
                })
                .flatten()
                .collect();
            eprintln!("{name}: {} tables, {:?}", tables.len(), tables.iter().map(|t| (t.row_count(), t.col_count())).collect::<Vec<_>>());
            assert!(!tables.is_empty(), "{name} has tables");
            assert!(tables.iter().any(|t| !t.is_empty()), "{name}: the cells have text");
            found += tables.len();
            // Saved and read again, the tables are the same.
            let again = crate::read_section(&crate::write_section(&s).unwrap()).unwrap();
            let count = |s: &notecraft_doc::Section| s.pages.iter().map(|p| p.plain_text()).collect::<Vec<_>>();
            assert_eq!(count(&again), count(&s), "{name}");
        }
        // The samples have nine tables, four of them inside other tables' cells: those are
        // flattened to text (NoteCraft's tables don't nest), and the section says so.
        assert!(found >= 5, "{found} tables in the samples");
    }

    #[test]
    fn props_round_trip() {
        let mut t = Table::new(2, 9, 50.0);
        t.col_locked = vec![true, false, false, false, false, false, false, false, true];
        let p = table_props(&t, Vec::new(), 0);
        assert_eq!(p.u32(prop::ROW_COUNT), Some(2));
        assert_eq!(p.u32(prop::COLUMN_COUNT), Some(9));
        assert_eq!(locked(&p), t.col_locked);
        assert!(widths(&p).iter().all(|w| (w - 50.0).abs() < 0.01));
    }
}
