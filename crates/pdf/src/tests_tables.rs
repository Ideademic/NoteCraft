//! Tables in print: borders, shading and the cells' text with real fonts, laid out as the
//! canvas lays them out.

use notecraft_doc::{Block, Cell, Color, ElementKind, Outline, Page, PageElement, Paragraph, Point, Run, Table};

use super::tests::extract_text;
use super::*;

/// A page with a shaded, bordered table between two paragraphs, and one with hidden borders.
pub(crate) fn table_page() -> Page {
    let mut t = Table::new(3, 3, 96.0);
    for (r, row) in
        [["Item", "Qty", "Where"], ["Apples", "6", "Market"], ["Bread", "1", "Bakery on the corner of the high street"]].into_iter().enumerate()
    {
        for (c, text) in row.into_iter().enumerate() {
            *t.cell_mut(r, c).unwrap() = Cell::with_text(text);
        }
    }
    for c in 0..3 {
        let cell = t.cell_mut(0, c).unwrap();
        cell.shading = Some(Color::rgb(0xFF, 0xE6, 0x99));
        cell.paragraphs[0].runs[0].bold = true;
    }
    t.cell_mut(2, 2)
        .unwrap()
        .paragraphs
        .push(Paragraph { runs: vec![Run { text: "Ask for rye".into(), italic: true, ..Run::default() }], ..Paragraph::default() });
    let mut o = Outline::with_text(notecraft_doc::page::CONTENT_ORIGIN, "Shopping list");
    o.blocks.push(Block::table(t));
    o.blocks.push(Paragraph::plain("Paid by card.").into());
    let mut hidden = Table::new(1, 2, 80.0);
    hidden.borders = false;
    *hidden.cell_mut(0, 0).unwrap() = Cell::with_text("Borderless");
    *hidden.cell_mut(0, 1).unwrap() = Cell::with_text("cells");
    let only = Outline { pos: Point::new(36.0, 320.0), width: 200.0, blocks: vec![Block::table(hidden)] };
    let mut page = Page::new("Tables in print");
    page.elements.push(PageElement::new(ElementKind::Outline(o)));
    page.elements.push(PageElement::new(ElementKind::Outline(only)));
    page
}

#[test]
fn tables_print_with_borders_shading_and_text() {
    let page = table_page();
    let pdf = export(&Doc::page(&page), &Options::default(), &BundledFonts).unwrap();
    let text = extract_text(&pdf).concat();
    for want in ["Shopping list", "Item", "Qty", "Apples", "Market", "Bakery", "Ask for rye", "Paid by card.", "Borderless", "cells"] {
        assert!(text.contains(want), "{want:?} in {text:?}");
    }
    // The layout: the paragraph after the table sits below every row, and long cell text wraps
    // inside its column.
    let ink = style::Ink::new(None);
    let chars = page.plain_text().chars().collect();
    let mut fs = fonts::FontSet::new(&BundledFonts, &[fonts::StyleKey::new(fonts::DEFAULT_FONT, false, false)], &chars);
    let ElementKind::Outline(o) = &page.elements[0].kind else { panic!("an outline") };
    let lay = text::layout_outline(&mut fs, o, &ink);
    let t = &lay.tables[0];
    assert_eq!((t.col_x.len(), t.row_y.len()), (4, 4));
    assert!((t.col_x[3] - t.col_x[0] - 288.0).abs() < 0.01, "three 96 pt columns");
    let after = lay.paras.iter().find(|p| p.block.galley.text().contains("Paid")).unwrap();
    assert!(after.top >= *t.row_y.last().unwrap(), "{} vs {:?}", after.top, t.row_y);
    let last_row = t.row_y[3] - t.row_y[2];
    let first_row = t.row_y[1] - t.row_y[0];
    assert!(last_row > first_row * 1.5, "the long cell wraps and the row grows: {first_row} vs {last_row}");
    assert!(lay.rect.w >= 288.0);
    // Renders: the shading is drawn.
    let shots = thumbnails(&pdf, 600, 1);
    let img = &shots[0];
    let yellow = img.rgba.chunks(4).filter(|p| p[0] > 240 && p[1] > 210 && p[2] < 180).count();
    assert!(yellow > 50, "shaded header cells ({yellow} yellow pixels)");
}

#[test]
fn hostile_tables_print() {
    let mut t = Table::new(2, 2, f32::NAN);
    t.col_widths = vec![f32::INFINITY, -5.0, 1e30];
    t.rows[0].cells.clear();
    t.indent = 255;
    let o = Outline { pos: Point::new(f32::NAN, 1e30), width: f32::INFINITY, blocks: vec![Block::table(t), Block::table(Table::new(0, 0, 0.0))] };
    let mut page = Page::new("junk");
    page.elements.push(PageElement::new(ElementKind::Outline(o)));
    let pdf = export(&Doc::page(&page), &Options { paper: Paper::A4, ..Options::default() }, &BundledFonts).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

/// Writes PNG renders of the table page to `NOTECRAFT_PDF_SHOTS` (by eye).
#[test]
fn table_samples() {
    let Some(dir) = std::env::var_os("NOTECRAFT_PDF_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = export(&Doc::page(&table_page()), &Options::default(), &SystemFonts).unwrap();
    std::fs::write(dir.join("tables.pdf"), &pdf).unwrap();
    for (i, t) in thumbnails(&pdf, 1400, 1).into_iter().enumerate() {
        image::RgbaImage::from_raw(t.width, t.height, t.rgba).unwrap().save(dir.join(format!("tables-{}.png", i + 1))).unwrap();
    }
}
