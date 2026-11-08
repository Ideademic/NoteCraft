//! Tables: Insert ▸ Table, typing and formatting in cells, every Table tab command, undo, and
//! hostile parameters aimed at a real table.

use notecraft_doc::{Block, Color, Guid, Outline, Table};
use serde_json::{Value, json};

use crate::text::{CellRef, TextPos};
use crate::{CmdError, Session, ids, registry};

fn gid(v: &Value) -> Guid {
    Guid::parse(v["id"].as_str().unwrap()).unwrap()
}

fn page_session() -> Session {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({"title": "Tables"})).unwrap();
    s
}

fn outline(s: &Session, o: Guid) -> &Outline {
    s.current_page().unwrap().outline(o).unwrap()
}

fn table(s: &Session, o: Guid, block: usize) -> &Table {
    outline(s, o).table(block).unwrap()
}

fn cell(o: Guid, c: (usize, usize, usize)) -> Value {
    json!({"outline": o.to_string(), "cell": CellRef::new(c.0, c.1, c.2).to_json()})
}

fn with(mut v: Value, extra: Value) -> Value {
    if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            m.insert(k.clone(), x.clone());
        }
    }
    v
}

/// A page with one new outline holding a `rows` × `cols` table (block 0).
fn with_table(rows: usize, cols: usize) -> (Session, Guid) {
    let mut s = page_session();
    let out = s.run(ids::INSERT_TABLE, &json!({"rows": rows, "cols": cols})).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(0, 0, 0).to_json());
    (s, gid(&out.value))
}

fn type_in(s: &mut Session, o: Guid, c: (usize, usize, usize), text: &str) {
    s.run(ids::TEXT_INSERT, &with(cell(o, c), json!({"text": text}))).unwrap();
}

#[test]
fn insert_a_table_and_type_in_its_cells() {
    let (mut s, o) = with_table(3, 3);
    let t = table(&s, o, 0);
    assert_eq!((t.row_count(), t.col_count(), t.borders), (3, 3, true));
    type_in(&mut s, o, (0, 0, 0), "Name");
    type_in(&mut s, o, (0, 0, 1), "Qty");
    let out = s.run(ids::TEXT_INSERT, &with(cell(o, (0, 1, 0)), json!({"text": "eggs\nmilk"}))).unwrap();
    assert_eq!(out.value["caret"], TextPos::new(1, 4).to_json(), "newlines make paragraphs in the cell");
    s.run(ids::TEXT_SPLIT_PARAGRAPH, &with(cell(o, (0, 1, 1)), json!({}))).unwrap();
    let t = table(&s, o, 0);
    assert_eq!(t.cell(0, 0).unwrap().text(), "Name");
    assert_eq!(t.cell(1, 0).unwrap().paragraphs.len(), 2);
    assert_eq!(t.cell(1, 1).unwrap().paragraphs.len(), 2, "Enter in a cell is a new paragraph there");
    assert_eq!(outline(&s, o).text(), "Name\tQty\t\neggs milk\t \t\n\t\t");
    // Backspace and Delete stay in the cell.
    s.run(ids::TEXT_DELETE, &with(cell(o, (0, 0, 0)), json!({"at": [0, 4], "backward": true}))).unwrap();
    s.run(ids::TEXT_DELETE, &with(cell(o, (0, 0, 1)), json!({"at": [0, 0], "backward": true}))).unwrap();
    assert_eq!(table(&s, o, 0).cell(0, 0).unwrap().text(), "Nam");
    assert_eq!(table(&s, o, 0).cell(0, 1).unwrap().text(), "Qty");
    // A cell that doesn't exist is an error, not a panic.
    assert!(matches!(s.run(ids::TEXT_INSERT, &with(cell(o, (0, 9, 9)), json!({"text": "x"}))), Err(CmdError::Params(_))));
    assert!(matches!(s.run(ids::TEXT_INSERT, &with(cell(o, (5, 0, 0)), json!({"text": "x"}))), Err(CmdError::Params(_))));
    // Undo takes the typing back, then the table.
    while s.can_undo() && s.current_page().is_some_and(|p| p.outline(o).is_some()) {
        s.run(ids::EDIT_UNDO, &json!({})).unwrap();
    }
    assert!(s.current_page().unwrap().outline(o).is_none());
}

#[test]
fn tables_go_at_the_caret_of_an_outline() {
    let mut s = page_session();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "before after"})).unwrap().value);
    let out = s.run(ids::INSERT_TABLE, &json!({"outline": o.to_string(), "at": [0, 7], "rows": 2, "cols": 2})).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(1, 0, 0).to_json());
    let ol = outline(&s, o);
    assert_eq!(ol.blocks.len(), 3);
    assert_eq!((ol.para(0).unwrap().text(), ol.para(2).unwrap().text()), ("before ".to_owned(), "after".to_owned()));
    // From inside a cell, a new table goes after this one: tables don't nest.
    let out = s.run(ids::INSERT_TABLE, &with(cell(o, (1, 0, 0)), json!({"rows": 1, "cols": 1}))).unwrap();
    assert_eq!(out.value["cell"]["block"], 2);
    assert!(outline(&s, o).blocks[2].is_table());
    // A table at the end gets a paragraph to type in after it.
    let p = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 300, "y": 300, "text": "x"})).unwrap().value);
    s.run(ids::INSERT_TABLE, &json!({"outline": p.to_string(), "at": [0, 1], "rows": 1, "cols": 2})).unwrap();
    assert_eq!(outline(&s, p).blocks.len(), 3);
    assert!(outline(&s, p).para(2).is_some());
    // Bad sizes are refused.
    for (r, c) in [(0, 2), (2, 0), (100_000, 1), (1, 1000)] {
        assert!(s.run(ids::INSERT_TABLE, &json!({"rows": r, "cols": c})).is_err(), "{r}×{c}");
    }
    assert!(s.run(ids::INSERT_TABLE, &json!({"rows": 1000, "cols": 64})).is_err(), "more cells than a table holds");
}

#[test]
fn formatting_reaches_cells() {
    let (mut s, o) = with_table(2, 2);
    for (r, c, t) in [(0, 0, "a"), (0, 1, "b"), (1, 0, "c"), (1, 1, "d")] {
        type_in(&mut s, o, (0, r, c), t);
    }
    // In one cell.
    s.run(ids::FORMAT_BOLD, &with(cell(o, (0, 0, 0)), json!({"start": [0, 0], "end": [0, 1]}))).unwrap();
    assert!(table(&s, o, 0).cell(0, 0).unwrap().paragraphs[0].runs[0].bold);
    assert!(!table(&s, o, 0).cell(0, 1).unwrap().paragraphs[0].runs[0].bold);
    // Selected cells: the second row.
    let cells = json!({"outline": o.to_string(), "cells": {"block": 0, "rows": [1, 1], "cols": [0, 1]}});
    s.run(ids::FORMAT_ITALIC, &cells).unwrap();
    s.run(ids::FORMAT_ALIGN, &with(cells.clone(), json!({"align": "center"}))).unwrap();
    let t = table(&s, o, 0);
    assert!(t.rows[1].cells.iter().all(|c| c.paragraphs[0].runs[0].italic && c.paragraphs[0].align == notecraft_doc::Align::Center));
    assert!(!t.cell(0, 0).unwrap().paragraphs[0].runs[0].italic);
    let f = s.run(ids::TEXT_FORMAT_AT, &cells).unwrap().value;
    assert_eq!(f["italic"], true);
    // The whole outline (no range) covers every cell.
    s.run(ids::FORMAT_UNDERLINE, &json!({"outline": o.to_string()})).unwrap();
    assert!(table(&s, o, 0).paragraphs().all(|p| p.runs.iter().all(|r| r.underline)));
    // Bad cell ranges are errors.
    assert!(s.run(ids::FORMAT_BOLD, &json!({"outline": o.to_string(), "cells": {"block": 7, "rows": [0, 0], "cols": [0, 0]}})).is_err());
    assert!(s.run(ids::FORMAT_BOLD, &json!({"outline": o.to_string(), "cells": "x"})).is_err());
}

#[test]
fn rows_and_columns() {
    let (mut s, o) = with_table(2, 2);
    type_in(&mut s, o, (0, 0, 0), "top");
    let out = s.run(ids::TABLE_INSERT_ROW_BELOW, &cell(o, (0, 0, 1))).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(0, 1, 1).to_json());
    let out = s.run(ids::TABLE_INSERT_ROW_ABOVE, &cell(o, (0, 0, 0))).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(0, 0, 0).to_json());
    assert_eq!(table(&s, o, 0).row_count(), 4);
    assert_eq!(table(&s, o, 0).cell(1, 0).unwrap().text(), "top");
    s.run(ids::TABLE_INSERT_COLUMN_RIGHT, &cell(o, (0, 1, 1))).unwrap();
    s.run(ids::TABLE_INSERT_COLUMN_LEFT, &cell(o, (0, 1, 0))).unwrap();
    let t = table(&s, o, 0);
    assert_eq!(t.col_count(), 4);
    assert_eq!(t.cell(1, 1).unwrap().text(), "top");
    assert!(t.rows.iter().all(|r| r.cells.len() == 4));
    // Delete the selected rows and columns.
    s.run(ids::TABLE_DELETE_ROW, &json!({"outline": o.to_string(), "cells": {"block": 0, "rows": [0, 1], "cols": [0, 0]}})).unwrap();
    s.run(ids::TABLE_DELETE_COLUMN, &cell(o, (0, 0, 3))).unwrap();
    let t = table(&s, o, 0);
    assert_eq!((t.row_count(), t.col_count()), (2, 3));
    assert!(!t.text().contains("top"));
    // Deleting every column deletes the table; the caret lands in the outline's paragraph.
    let out = s.run(ids::TABLE_DELETE_COLUMN, &json!({"outline": o.to_string(), "cells": {"block": 0, "rows": [0, 0], "cols": [0, 99]}})).unwrap();
    assert_eq!(out.value["cell"], Value::Null);
    assert!(!outline(&s, o).has_tables());
    assert_eq!(outline(&s, o).blocks.len(), 1, "an empty paragraph is left");
    s.run(ids::EDIT_UNDO, &json!({})).unwrap();
    assert!(outline(&s, o).has_tables());
    s.run(ids::TABLE_DELETE, &json!({"outline": o.to_string(), "block": 0})).unwrap();
    assert!(!outline(&s, o).has_tables());
}

#[test]
fn select_width_shading_borders() {
    let (mut s, o) = with_table(3, 4);
    let sel = |s: &mut Session, id: &str| s.run(id, &cell(o, (0, 1, 2))).unwrap().value["cells"].clone();
    assert_eq!(sel(&mut s, ids::TABLE_SELECT_CELL), json!({"block": 0, "rows": [1, 1], "cols": [2, 2]}));
    assert_eq!(sel(&mut s, ids::TABLE_SELECT_ROW), json!({"block": 0, "rows": [1, 1], "cols": [0, 3]}));
    assert_eq!(sel(&mut s, ids::TABLE_SELECT_COLUMN), json!({"block": 0, "rows": [0, 2], "cols": [2, 2]}));
    assert_eq!(sel(&mut s, ids::TABLE_SELECT_TABLE), json!({"block": 0, "rows": [0, 2], "cols": [0, 3]}));
    assert!(!s.can_undo() || s.run(ids::EDIT_UNDO, &json!({})).is_ok(), "selecting changes nothing");
    let (mut s, o) = with_table(3, 4);
    s.run(ids::TABLE_SET_COLUMN_WIDTH, &with(cell(o, (0, 0, 1)), json!({"width": 150.5}))).unwrap();
    s.run(ids::TABLE_SET_COLUMN_WIDTH, &json!({"outline": o.to_string(), "block": 0, "col": 99, "width": -5})).unwrap();
    let t = table(&s, o, 0);
    assert_eq!(t.col_width(1), 150.5);
    assert_eq!(t.col_width(3), notecraft_doc::table::MIN_COL_WIDTH, "clamped, and the column index too");
    assert_eq!(t.col_locked.get(1), Some(&true));
    assert!(s.run(ids::TABLE_SET_COLUMN_WIDTH, &cell(o, (0, 0, 0))).is_err(), "`width` is required");
    s.run(ids::TABLE_SHADING, &json!({"outline": o.to_string(), "cells": {"block": 0, "rows": [0, 0], "cols": [0, 3]}, "color": "#ffe699"})).unwrap();
    let t = table(&s, o, 0);
    assert!(t.rows[0].cells.iter().all(|c| c.shading == Some(Color::rgb(0xFF, 0xE6, 0x99))));
    assert!(t.rows[1].cells.iter().all(|c| c.shading.is_none()));
    assert!(s.run(ids::TABLE_SHADING, &cell(o, (0, 0, 0))).is_err(), "`color` is required");
    s.run(ids::TABLE_SHADING, &json!({"outline": o.to_string(), "block": 0, "color": null})).unwrap();
    assert!(table(&s, o, 0).paragraphs().count() == 12 && table(&s, o, 0).rows.iter().flat_map(|r| &r.cells).all(|c| c.shading.is_none()));
    s.run(ids::TABLE_BORDERS, &cell(o, (0, 0, 0))).unwrap();
    assert!(!table(&s, o, 0).borders, "toggled off");
    assert!(!s.run(ids::TABLE_BORDERS, &with(cell(o, (0, 0, 0)), json!({"value": false}))).unwrap().changed);
    s.run(ids::TABLE_BORDERS, &with(cell(o, (0, 0, 0)), json!({"value": true}))).unwrap();
    assert!(table(&s, o, 0).borders);
}

#[test]
fn sort_and_convert() {
    let (mut s, o) = with_table(4, 2);
    for (r, name, n) in [(0, "Item", "Count"), (1, "pears", "10"), (2, "Apples", "2"), (3, "figs", "")] {
        type_in(&mut s, o, (0, r, 0), name);
        if !n.is_empty() {
            type_in(&mut s, o, (0, r, 1), n);
        }
    }
    let col = |s: &Session, c: usize| table(s, o, 0).rows.iter().map(|r| r.cells[c].text()).collect::<Vec<_>>();
    s.run(ids::TABLE_SORT, &with(cell(o, (0, 1, 0)), json!({"header": true}))).unwrap();
    assert_eq!(col(&s, 0), vec!["Item", "Apples", "figs", "pears"], "the header stays, case is ignored");
    s.run(ids::TABLE_SORT, &with(cell(o, (0, 1, 1)), json!({"header": true, "descending": true}))).unwrap();
    assert_eq!(col(&s, 1), vec!["Count", "10", "2", ""], "numbers by value, empty cells last");
    let out = s.run(ids::TABLE_CONVERT_TO_TEXT, &cell(o, (0, 0, 0))).unwrap();
    assert_eq!(out.value["caret"], TextPos::new(0, 0).to_json());
    let ol = outline(&s, o);
    assert!(!ol.has_tables());
    assert_eq!(ol.text(), "Item\tCount\npears\t10\nApples\t2\nfigs\t");
    // And back: paragraphs with tabs become a table again.
    let out = s.run(ids::TABLE_FROM_TEXT, &json!({"outline": o.to_string(), "paras": [0, 3]})).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(0, 0, 0).to_json());
    let t = table(&s, o, 0);
    assert_eq!((t.row_count(), t.col_count()), (4, 2));
    assert_eq!(t.cell(2, 0).unwrap().text(), "Apples");
    // Typing "abc" then Tab: a table of two columns, the caret in the new cell.
    let p = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 300, "y": 300, "text": "abc"})).unwrap().value);
    let out = s.run(ids::TABLE_FROM_TEXT, &json!({"outline": p.to_string(), "para": 0, "addColumn": true})).unwrap();
    assert_eq!(out.value["cell"], CellRef::new(0, 0, 1).to_json());
    let t = table(&s, p, 0);
    assert_eq!((t.col_count(), t.cell(0, 0).unwrap().text()), (2, "abc".to_owned()));
    assert!(s.run(ids::TABLE_FROM_TEXT, &json!({"outline": p.to_string(), "para": 0})).is_err(), "already a table");
}

#[test]
fn blocks_around_tables_edit_safely() {
    let mut s = page_session();
    let o = gid(&s.run(ids::OUTLINE_NEW, &json!({"x": 36, "y": 108, "text": "one\ntwo"})).unwrap().value);
    s.run(ids::INSERT_TABLE, &json!({"outline": o.to_string(), "at": [0, 3], "rows": 1, "cols": 1})).unwrap();
    // Backspace at the start of the paragraph after the table doesn't merge into it.
    let out = s.run(ids::TEXT_DELETE, &json!({"outline": o.to_string(), "at": [2, 0], "backward": true})).unwrap();
    assert!(!out.changed);
    assert!(outline(&s, o).blocks[1].is_table());
    // Typing at the table's block does nothing; deleting across it removes it.
    s.run(ids::TEXT_INSERT, &json!({"outline": o.to_string(), "at": [1, 0], "text": "x"})).unwrap();
    assert_eq!(outline(&s, o).text(), "one\n\ntwo");
    s.run(ids::TEXT_DELETE, &json!({"outline": o.to_string(), "start": [0, 3], "end": [2, 0]})).unwrap();
    assert_eq!(outline(&s, o).text(), "onetwo");
    assert!(!outline(&s, o).has_tables());
}

#[test]
fn table_commands_survive_hostile_params() {
    let junk = [
        json!({"cell": {"block": u64::MAX, "row": -1, "col": "x"}}),
        json!({"cell": [0, u64::MAX, u64::MAX], "text": "x", "width": f64::NAN, "color": "#nothex"}),
        json!({"cells": {"block": 0, "rows": [u64::MAX, 0], "cols": [5, u64::MAX]}, "color": "#123456", "width": 1e30}),
        json!({"cells": {"block": 0, "rows": "all"}, "col": u64::MAX, "descending": "yes", "header": 1}),
        json!({"block": 0, "col": 1e300, "width": -1e300, "value": "no", "rows": 1e9, "cols": -3}),
        json!({"block": 1, "at": [u64::MAX, u64::MAX], "rows": 2, "cols": 2, "addColumn": true, "paras": [u64::MAX, 0]}),
        json!({"cell": [0, 0, 0], "at": [99, 99], "start": [5, 5], "end": [0, 0], "rows": 64, "cols": 64}),
    ];
    for spec in registry().all().iter().filter(|s| {
        s.id.starts_with("table.")
            || s.id == ids::INSERT_TABLE
            || s.id.starts_with("text.")
            || s.id.starts_with("format.")
            || s.id.starts_with("link.")
            || s.id.starts_with("proof.")
    }) {
        for params in &junk {
            let (mut s, o) = with_table(2, 3);
            s.run(ids::OUTLINE_NEW, &json!({"x": 1, "y": 1, "text": "a\tb"})).unwrap();
            let params = with(params.clone(), json!({"outline": o.to_string()}));
            let r = s.run(spec.id, &params);
            assert!(!matches!(&r, Err(CmdError::Failed(m)) if m.starts_with("internal error")), "{} panicked with {params}", spec.id);
            // Whatever happened, the table is still a rectangle.
            if let Some(t) = s.current_page().unwrap().outline(o).and_then(|ol| ol.blocks.iter().find_map(Block::as_table)) {
                let cols = t.col_count();
                assert!(t.rows.iter().all(|r| r.cells.len() == cols) && t.col_widths.len() == cols, "{} broke the grid with {params}", spec.id);
            }
        }
    }
}

#[test]
fn spelling_links_and_word_count_reach_cells() {
    let (mut s, o) = with_table(1, 2);
    type_in(&mut s, o, (0, 0, 1), "a brwn fox");
    // Spelling: the cell's misspelling names the cell, and applying a fix works there.
    let mut issues = Vec::new();
    for _ in 0..400 {
        let out = s.run(ids::PROOF_CHECK, &json!({})).unwrap().value;
        issues = out["issues"].as_array().cloned().unwrap_or_default();
        if !issues.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let issue = issues.first().cloned().expect("a misspelling in the cell");
    assert_eq!((issue["cell"].clone(), issue["word"].clone()), (CellRef::new(0, 0, 1).to_json(), json!("brwn")));
    let sug = s.run(ids::PROOF_SUGGESTIONS, &with(cell(o, (0, 0, 1)), json!({"at": [0, 3]}))).unwrap().value;
    assert_eq!(sug["word"], "brwn");
    s.run(ids::PROOF_APPLY, &with(cell(o, (0, 0, 1)), json!({"para": 0, "start": 2, "end": 6, "text": "brown", "word": "brwn"}))).unwrap();
    assert_eq!(table(&s, o, 0).cell(0, 1).unwrap().text(), "a brown fox");
    // Links in a cell.
    s.run(ids::LINK_SET, &with(cell(o, (0, 0, 1)), json!({"start": [0, 2], "end": [0, 7], "url": "example.com"}))).unwrap();
    let got = s.run(ids::LINK_GET, &with(cell(o, (0, 0, 1)), json!({"at": [0, 3]}))).unwrap().value;
    assert_eq!(got["url"], "https://example.com");
    type_in(&mut s, o, (0, 0, 0), "[[Elsewhere]]");
    let out = s.run(ids::LINK_WIKI, &with(cell(o, (0, 0, 0)), json!({"at": [0, 13]}))).unwrap().value;
    assert_eq!(out["created"], true);
    assert_eq!(table(&s, o, 0).cell(0, 0).unwrap().text(), "Elsewhere");
    // Word count and proofing language see cells.
    let wc = s.run(ids::REVIEW_WORD_COUNT, &json!({})).unwrap().value;
    assert!(wc["words"].as_u64().unwrap() >= 4, "{wc}");
    s.run(ids::PROOF_SET_LANGUAGE, &json!({"outline": o.to_string(), "lang": "en-GB"})).unwrap();
    assert!(table(&s, o, 0).paragraphs().all(|p| p.lang.as_deref() == Some("en-GB")));
}
