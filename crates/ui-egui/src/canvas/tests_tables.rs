//! Tables on the canvas and in the window, driven through egui_kittest: Insert › Table's grid
//! picker, typing and Tab between cells (Tab in the last cell adds a row), Enter and Ctrl+Enter,
//! the arrows across cells, a column border dragged wider, cells selected by dragging, and the
//! Table tab's Delete Rows. `table_screenshots` renders a page of tables, the Table tab and the
//! grid picker (light and dark) into `$NOTECRAFT_TABLE_SHOTS` when that is set.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use notecraft_doc::{Block, Cell, Color, ElementKind, Outline, Paragraph, Point, Run, Table};
use notecraft_engine::text::{CellRange, CellRef};
use notecraft_engine::{Session, TextPos, ids};
use serde_json::json;

use super::*;
use crate::NoteApp;
use crate::ribbon::RibbonTab;
use crate::services::Services;
use crate::theme::Tokens;

struct T {
    canvas: CanvasState,
    session: Session,
    tokens: Tokens,
    services: Services,
}

fn harness(t: T) -> Harness<'static, T> {
    Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, t: &mut T| {
            t.tokens.apply(ui.ctx());
            show(ui, &mut t.canvas, &mut t.session, &t.tokens, &t.services);
        },
        t,
    )
}

/// A page holding `o` (and nothing else).
fn with_outline(o: Outline) -> T {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Tables"})).unwrap();
    if let Some(p) = session.current_page_mut() {
        p.elements.push(notecraft_doc::PageElement::new(ElementKind::Outline(o)));
    }
    T { canvas: CanvasState::default(), session, tokens: Tokens::light(), services: Services::default() }
}

fn first_outline(h: &Harness<'_, T>) -> Outline {
    h.state()
        .session
        .current_page()
        .unwrap()
        .elements
        .iter()
        .find_map(|e| match &e.kind {
            ElementKind::Outline(o) => Some(o.clone()),
            _ => None,
        })
        .unwrap()
}

fn table_of(o: &Outline) -> Table {
    o.blocks.iter().find_map(Block::as_table).cloned().unwrap()
}

fn press(h: &mut Harness<'_, T>, p: Pos2, down: bool) {
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: Modifiers::NONE });
    h.step();
}

fn click(h: &mut Harness<'_, T>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
    press(h, p, true);
    press(h, p, false);
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, T>, from: Pos2, to: Pos2) {
    h.event(Event::PointerMoved(from));
    h.step();
    press(h, from, true);
    for i in 1..=4 {
        let p = from + (to - from) * (i as f32 / 4.0);
        h.event(Event::PointerMoved(p));
        h.step();
    }
    press(h, to, false);
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, T>, s: &str) {
    h.event(Event::Text(s.into()));
    h.run_steps(2);
}

fn key(h: &mut Harness<'_, T>, k: Key, m: Modifiers) {
    h.key_press_modifiers(m, k);
    h.run_steps(2);
}

/// A cell's centre on screen.
fn cell_center(h: &Harness<'_, T>, row: usize, col: usize) -> Pos2 {
    let st = h.state();
    let v = view::View::new(&st.canvas, page::sheet_rect(st.canvas.last_rect.unwrap()));
    let lay = layout::page_layout(&h.ctx, &st.canvas, &st.session, &st.tokens, &v);
    let o = &lay.outlines[0];
    o.tables[0].cell_rect(o, &v, row, col).unwrap().center()
}

/// The screen x of a column's right border, and a y inside the table.
fn border(h: &Harness<'_, T>, col: usize) -> Pos2 {
    let st = h.state();
    let v = view::View::new(&st.canvas, page::sheet_rect(st.canvas.last_rect.unwrap()));
    let lay = layout::page_layout(&h.ctx, &st.canvas, &st.session, &st.tokens, &v);
    let o = &lay.outlines[0];
    let r = o.tables[0].cell_rect(o, &v, 0, col).unwrap();
    egui::pos2(r.right(), r.center().y)
}

fn caret(h: &Harness<'_, T>) -> TextCaret {
    h.state().canvas.caret.clone().unwrap()
}

/// An outline at (100, 160) with "Above", a `rows` × `cols` table, and "Below".
fn sample(rows: usize, cols: usize) -> Outline {
    let mut o = Outline::with_text(Point::new(100.0, 160.0), "Above");
    o.blocks.push(Block::table(Table::new(rows, cols, 90.0)));
    o.blocks.push(Paragraph::plain("Below").into());
    o
}

#[test]
fn typing_tab_and_rows_in_a_table() {
    let mut h = harness(with_outline(sample(2, 2)));
    h.run_steps(3);
    let p = cell_center(&h, 0, 0);
    click(&mut h, p);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 0, 0)), "a click in a cell puts the caret there");
    type_text(&mut h, "one");
    key(&mut h, Key::Tab, Modifiers::NONE);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 0, 1)));
    type_text(&mut h, "two");
    key(&mut h, Key::Tab, Modifiers::NONE);
    type_text(&mut h, "three");
    key(&mut h, Key::Tab, Modifiers::NONE);
    type_text(&mut h, "four");
    assert_eq!(table_of(&first_outline(&h)).row_count(), 2);
    // Tab in the last cell adds a row.
    key(&mut h, Key::Tab, Modifiers::NONE);
    let t = table_of(&first_outline(&h));
    assert_eq!(t.row_count(), 3);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 2, 0)));
    assert_eq!(t.texts(), vec![vec!["one", "two"], vec!["three", "four"], vec!["", ""]]);
    // Shift+Tab goes back, selecting the cell's text: typing replaces it.
    key(&mut h, Key::Tab, Modifiers::SHIFT);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 1, 1)));
    type_text(&mut h, "FOUR");
    assert_eq!(table_of(&first_outline(&h)).cell(1, 1).unwrap().text(), "FOUR");
    // Enter is a new paragraph in the cell; Ctrl+Enter a row below.
    key(&mut h, Key::Enter, Modifiers::NONE);
    type_text(&mut h, "more");
    assert_eq!(table_of(&first_outline(&h)).cell(1, 1).unwrap().text(), "FOUR\nmore");
    key(&mut h, Key::Enter, Modifiers::COMMAND);
    assert_eq!(table_of(&first_outline(&h)).row_count(), 4);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 2, 1)));
    // The text around the table is untouched.
    let o = first_outline(&h);
    assert_eq!((o.para(0).unwrap().text(), o.para(2).unwrap().text()), ("Above".to_owned(), "Below".to_owned()));
}

#[test]
fn arrows_cross_cells_and_leave_the_table() {
    let mut h = harness(with_outline(sample(2, 2)));
    h.run_steps(3);
    let p = cell_center(&h, 0, 0);
    click(&mut h, p);
    type_text(&mut h, "ab");
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!((caret(&h).cell, caret(&h).head), (Some(CellRef::new(1, 0, 1)), TextPos::default()));
    key(&mut h, Key::ArrowLeft, Modifiers::NONE);
    assert_eq!((caret(&h).cell, caret(&h).head), (Some(CellRef::new(1, 0, 0)), TextPos::new(0, 2)));
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(caret(&h).cell, Some(CellRef::new(1, 1, 0)));
    key(&mut h, Key::ArrowDown, Modifiers::NONE);
    assert_eq!((caret(&h).cell, caret(&h).head.para), (None, 2), "down from the last row: the paragraph below");
    key(&mut h, Key::ArrowUp, Modifiers::NONE);
    assert_eq!(caret(&h).cell.map(|c| c.row), Some(1), "back up into the last row");
    for _ in 0..3 {
        key(&mut h, Key::ArrowUp, Modifiers::NONE);
    }
    assert_eq!((caret(&h).cell, caret(&h).head.para), (None, 0), "up from the first row: the paragraph above");
    // Right at the end of "Above" goes into the first cell.
    key(&mut h, Key::End, Modifiers::NONE);
    key(&mut h, Key::ArrowRight, Modifiers::NONE);
    assert_eq!((caret(&h).cell, caret(&h).head), (Some(CellRef::new(1, 0, 0)), TextPos::default()));
}

#[test]
fn tab_after_text_makes_a_table() {
    let mut h = harness(with_outline(Outline::with_text(Point::new(100.0, 160.0), "")));
    h.run_steps(3);
    let p = h.state().canvas.last_rect.unwrap().min + vec2(200.0, 230.0);
    let o = {
        let st = h.state();
        let v = view::View::new(&st.canvas, page::sheet_rect(st.canvas.last_rect.unwrap()));
        v.to_screen(Point::new(110.0, 165.0))
    };
    let _ = p;
    click(&mut h, o);
    type_text(&mut h, "Name");
    key(&mut h, Key::Tab, Modifiers::NONE);
    let ol = first_outline(&h);
    let t = table_of(&ol);
    assert_eq!((t.row_count(), t.col_count()), (1, 2));
    assert_eq!(t.cell(0, 0).unwrap().text(), "Name");
    assert_eq!(caret(&h).cell, Some(CellRef::new(0, 0, 1)), "the caret in the new cell");
    type_text(&mut h, "Phone");
    assert_eq!(table_of(&first_outline(&h)).cell(0, 1).unwrap().text(), "Phone");
}

#[test]
fn dragging_a_border_resizes_its_column() {
    let mut h = harness(with_outline(sample(2, 3)));
    h.run_steps(3);
    let b = border(&h, 0);
    drag(&mut h, b, b + vec2(40.0, 0.0));
    let t = table_of(&first_outline(&h));
    let px_per_pt = h.state().canvas.scale();
    assert!((t.col_width(0) - (90.0 + 40.0 / px_per_pt)).abs() < 1.5, "{} wide", t.col_width(0));
    assert_eq!(t.col_width(1), 90.0, "the next column keeps its width");
    // One drag is one undo step.
    h.state_mut().session.run(ids::EDIT_UNDO, &json!({})).unwrap();
    h.run_steps(2);
    assert_eq!(table_of(&first_outline(&h)).col_width(0), 90.0);
}

#[test]
fn dragging_across_cells_selects_them_and_backspace_deletes_whole_rows() {
    let mut o = sample(3, 2);
    if let Some(t) = o.table_mut(1) {
        *t = Table::from_text(&[vec!["a".into(), "b".into()], vec!["c".into(), "d".into()], vec!["e".into(), "f".into()]], 90.0);
    }
    let mut h = harness(with_outline(o));
    h.run_steps(3);
    let (a, b) = (cell_center(&h, 1, 0), cell_center(&h, 1, 1));
    drag(&mut h, a, b);
    assert_eq!(caret(&h).cells, Some(CellRange { block: 1, rows: (1, 1), cols: (0, 1) }));
    // Formatting reaches the selected cells.
    let st = h.state_mut();
    let _ = format_command(&mut st.canvas, &mut st.session, ids::FORMAT_BOLD, json!({}));
    assert!(table_of(&first_outline(&h)).rows[1].cells.iter().all(|c| c.paragraphs[0].runs[0].bold));
    key(&mut h, Key::Backspace, Modifiers::NONE);
    assert_eq!(table_of(&first_outline(&h)).row_count(), 2, "a whole row selected goes");
    // Ctrl+A twice in a cell selects the table; Delete removes it.
    let p = cell_center(&h, 0, 0);
    click(&mut h, p);
    key(&mut h, Key::A, Modifiers::COMMAND);
    key(&mut h, Key::A, Modifiers::COMMAND);
    assert!(caret(&h).cells.is_some());
    key(&mut h, Key::Delete, Modifiers::NONE);
    assert!(!first_outline(&h).has_tables());
}

fn app_harness(app: NoteApp) -> Harness<'static, NoteApp> {
    Harness::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut NoteApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

fn app_table(h: &Harness<'_, NoteApp>) -> Option<Table> {
    h.state().session.current_page()?.elements.iter().find_map(|e| match &e.kind {
        ElementKind::Outline(o) => o.blocks.iter().find_map(Block::as_table).cloned(),
        _ => None,
    })
}

#[test]
fn insert_a_table_from_the_grid_picker_and_use_the_table_tab() {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Grid"})).unwrap();
    let mut h = app_harness(NoteApp::new(session, Services::default()));
    h.run_steps(3);
    h.get_by_label("Insert").click();
    h.run_steps(3);
    h.get_by_label("Table").click();
    h.run_steps(3);
    h.get_by_label("3×3").hover();
    h.run_steps(2);
    h.get_by_label("3×3").click();
    h.run_steps(4);
    let t = app_table(&h).expect("a table");
    assert_eq!((t.row_count(), t.col_count()), (3, 3));
    assert_eq!(h.state().ui.ribbon_tab, RibbonTab::Table, "the Table tab shows");
    // Type, Tab to the next cell.
    h.event(Event::Text("x".into()));
    h.run_steps(2);
    h.key_press(Key::Tab);
    h.run_steps(2);
    h.event(Event::Text("y".into()));
    h.run_steps(2);
    let t = app_table(&h).unwrap();
    assert_eq!((t.cell(0, 0).unwrap().text(), t.cell(0, 1).unwrap().text()), ("x".to_owned(), "y".to_owned()));
    // Table › Delete Rows takes the caret's row.
    h.get_by_label("Delete Rows").click();
    h.run_steps(3);
    assert_eq!(app_table(&h).unwrap().row_count(), 2);
    // Hide Borders.
    h.get_by_label("Hide Borders").click();
    h.run_steps(3);
    assert!(!app_table(&h).unwrap().borders);
    // Leaving the table puts the tab away.
    h.state_mut().canvas.caret = None;
    h.state_mut().canvas.editing = None;
    h.run_steps(3);
    assert_ne!(h.state().ui.ribbon_tab, RibbonTab::Table);
    assert!(h.query_by_label("Table Tools: Layout").is_none());
}

#[test]
fn insert_table_dialog() {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Dialog"})).unwrap();
    let mut h = app_harness(NoteApp::new(session, Services::default()));
    h.run_steps(3);
    crate::ribbon::table::open_dialog(&h.ctx);
    h.run_steps(3);
    let _ = h.get_by_label("Number of columns:");
    h.get_by_label("OK").click();
    h.run_steps(4);
    let t = app_table(&h).expect("a 3×3 table from the dialog's defaults");
    assert_eq!((t.row_count(), t.col_count()), (3, 3));
}

/// A page of tables for the screenshots: shading, a hidden-border table, rich text in cells.
fn shot_page(session: &mut Session) {
    let mut t = Table::new(4, 3, 110.0);
    for (r, row) in
        [["Task", "Owner", "Due"], ["Draft the plan", "Ana", "Mon"], ["Review", "Sam", "Wed"], ["Ship it", "Everyone", "Fri"]].into_iter().enumerate()
    {
        for (c, text) in row.into_iter().enumerate() {
            *t.cell_mut(r, c).unwrap() = Cell::with_text(text);
        }
    }
    for c in 0..3 {
        let cell = t.cell_mut(0, c).unwrap();
        cell.shading = Some(Color::rgb(0xDD, 0xEB, 0xF7));
        cell.paragraphs[0].runs[0].bold = true;
    }
    t.cell_mut(3, 2).unwrap().shading = Some(Color::rgb(0xFF, 0xE6, 0x99));
    t.cell_mut(2, 0).unwrap().paragraphs.push(Paragraph {
        runs: vec![Run { text: "with notes".into(), italic: true, color: Some(Color::rgb(0xC0, 0x30, 0x30)), ..Run::default() }],
        list: Some(notecraft_doc::ListKind::Bullet),
        ..Paragraph::default()
    });
    let mut o = Outline::with_text(Point::new(60.0, 120.0), "This week");
    if let Some(p) = o.para_mut(0) {
        p.style = notecraft_doc::ParaStyle::Heading2;
    }
    o.blocks.push(Block::table(t));
    o.blocks.push(Paragraph::plain("Tables keep their columns, shading and borders.").into());
    o.width = 400.0;
    let mut hidden = Table::from_text(&[vec!["Borders hidden".into(), "(dashed while editing)".into()], vec!["Name".into(), "Phone".into()]], 120.0);
    hidden.borders = false;
    let o2 = Outline { pos: Point::new(60.0, 400.0), width: 240.0, blocks: vec![Block::table(hidden)] };
    if let Some(p) = session.current_page_mut() {
        p.title = "Tables".into();
        p.elements.push(notecraft_doc::PageElement::new(ElementKind::Outline(o)));
        p.elements.push(notecraft_doc::PageElement::new(ElementKind::Outline(o2)));
    }
}

#[test]
fn table_screenshots() {
    let Ok(dir) = std::env::var("NOTECRAFT_TABLE_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for dark in [false, true] {
        let name = if dark { "dark" } else { "light" };
        for (shot, layout) in [
            ("page", None),
            ("table-tab", Some(crate::ribbon::display::RibbonLayout::Classic)),
            ("simplified-table-tab", Some(crate::ribbon::display::RibbonLayout::Simplified)),
            ("grid-picker", None),
        ] {
            let mut session = Session::new_default();
            session.run(ids::PAGE_NEW, &json!({"title": "Tables"})).unwrap();
            shot_page(&mut session);
            let mut app = NoteApp::new(session, Services::default());
            app.ui.prefs.appearance = if dark { crate::Appearance::Dark } else { crate::Appearance::Light };
            if let Some(l) = layout {
                app.ui.ribbon_layout = l;
            }
            let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_pixels_per_point(2.0).wgpu().build_ui_state(
                |ui, app: &mut NoteApp| {
                    let ctx = ui.ctx().clone();
                    app.logic(&ctx);
                    app.ui(ui);
                },
                app,
            );
            h.set_render_cursor(false);
            h.run_steps(4);
            match shot {
                "table-tab" | "simplified-table-tab" | "page" => {
                    // The caret in a cell of the second container (hidden borders show dashed).
                    let id = h.state().session.current_page().unwrap().elements[1].id;
                    let cell = CellRef::new(0, 1, 1);
                    h.state_mut().canvas.caret = Some(TextCaret::in_cell(id, cell, TextPos::new(0, 5)));
                    h.state_mut().canvas.editing = Some(id);
                    if shot != "page" {
                        h.state_mut().ui.ribbon_tab = RibbonTab::Table;
                    }
                }
                _ => {
                    h.get_by_label("Insert").click();
                    h.run_steps(3);
                    h.get_by_label("Table").click();
                    h.run_steps(3);
                    h.get_by_label("4×3").hover();
                }
            }
            h.run_steps(6);
            let img = h.render().unwrap();
            img.save(dir.join(format!("tables-{shot}-{name}.png"))).unwrap();
        }
    }
}

/// The screen point just right of character `ch` of paragraph `para` in cell `cell` (the app's
/// canvas).
fn cell_spot(h: &Harness<'_, NoteApp>, outline: notecraft_doc::Guid, cell: CellRef, para: usize, ch: usize) -> Pos2 {
    let st = h.state();
    let v = view::View::new(&st.canvas, page::sheet_rect(st.canvas.last_rect.unwrap()));
    let lay = layout::page_layout(&h.ctx, &st.canvas, &st.session, &st.tokens, &v);
    let r = lay.outline(outline).unwrap().frame_caret_rect(&v, notecraft_engine::text::Frame::Cell(cell), TextPos::new(para, ch)).unwrap();
    egui::pos2(r.left() + 2.0, r.center().y)
}

fn app_click(h: &mut Harness<'_, NoteApp>, p: Pos2, button: PointerButton, mods: Modifiers) {
    h.event(Event::PointerMoved(p));
    h.step();
    if mods != Modifiers::NONE {
        h.event(Event::ModifiersChanged(mods));
    }
    h.event(Event::PointerButton { pos: p, button, pressed: true, modifiers: mods });
    h.step();
    h.event(Event::PointerButton { pos: p, button, pressed: false, modifiers: mods });
    if mods != Modifiers::NONE {
        h.event(Event::ModifiersChanged(Modifiers::NONE));
    }
    h.run_steps(3);
}

#[test]
fn spelling_menus_and_links_work_in_cells() {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Review in tables"})).unwrap();
    let mut t = Table::new(1, 2, 160.0);
    *t.cell_mut(0, 0).unwrap() = Cell::with_text("I like ovr cats");
    let o = Outline { pos: Point::new(80.0, 150.0), width: 340.0, blocks: vec![Block::table(t)] };
    let el = notecraft_doc::PageElement::new(ElementKind::Outline(o));
    let oid = el.id;
    session.current_page_mut().unwrap().elements.push(el);
    let mut h = app_harness(NoteApp::new(session, Services::default()));
    h.run_steps(3);
    let first = CellRef::new(0, 0, 0);
    // The misspelling in the cell is underlined.
    for _ in 0..400 {
        h.step();
        if !h.state().canvas.review.marks.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(h.state().canvas.review.marks, vec![(oid, notecraft_engine::text::Frame::Cell(first), 0, 7, 10)]);
    // Its right-click menu fixes it, in the cell.
    let p = cell_spot(&h, oid, first, 0, 8);
    app_click(&mut h, p, PointerButton::Secondary, Modifiers::NONE);
    assert_eq!(h.state().canvas.caret.as_ref().and_then(|c| c.cell), Some(first), "the caret goes into the cell");
    let _ = h.get_by_label("Possible spelling mistake");
    h.get_by_label("over").click();
    h.run_steps(4);
    let cell_text = |h: &Harness<'_, NoteApp>, c: usize| app_table(h).unwrap().cell(0, c).unwrap().text();
    assert_eq!(cell_text(&h, 0), "I like over cats");
    // The Spelling pane finds a misspelling in a cell and selects it there.
    h.state_mut().session.run(ids::TEXT_INSERT, &json!({"outline": oid, "cell": [0, 0, 1], "text": "brwn"})).unwrap();
    h.key_press(Key::F7);
    h.run_steps(4);
    let c = h.state().canvas.caret.clone().unwrap();
    assert_eq!((c.cell, c.range()), (Some(CellRef::new(0, 0, 1)), (TextPos::new(0, 0), TextPos::new(0, 4))));
    h.key_press(Key::F7);
    h.run_steps(2);
    // Typing a wiki link in a cell makes it, and Ctrl+click on it in the cell jumps.
    let host = h.state().session.current.page;
    let p = cell_spot(&h, oid, CellRef::new(0, 0, 1), 0, 4);
    app_click(&mut h, p, PointerButton::Primary, Modifiers::NONE);
    for s in [" see ", "[[Cell page]", "]"] {
        h.event(Event::Text(s.into()));
        h.run_steps(2);
    }
    assert_eq!(cell_text(&h, 1), "brwn see Cell page");
    let target = h.state().session.current_section().unwrap().pages.iter().find(|p| p.title == "Cell page").map(|p| p.id).expect("made");
    assert_eq!(h.state().session.current.page, host);
    let p = cell_spot(&h, oid, CellRef::new(0, 0, 1), 0, 12);
    app_click(&mut h, p, PointerButton::Primary, Modifiers::COMMAND);
    assert_eq!(h.state().session.current.page, Some(target), "Ctrl+click in a cell follows the link");
}
