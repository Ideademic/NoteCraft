//! Tables in HTML (`<table>`), Markdown (pipe tables) and plain text (tab-separated rows).

use notecraft_doc::{Block, Cell, Color, ElementKind, Outline, Page, PageElement, Paragraph, Point, Run, Table};

use crate::{page_to_html, page_to_markdown, page_to_text};

fn page() -> Page {
    let mut t = Table::new(2, 3, 90.0);
    for (r, row) in [["Name", "Qty", "Note"], ["a|b", "2", ""]].into_iter().enumerate() {
        for (c, text) in row.into_iter().enumerate() {
            *t.cell_mut(r, c).unwrap() = Cell::with_text(text);
        }
    }
    let head = t.cell_mut(0, 0).unwrap();
    head.shading = Some(Color::rgb(0xFF, 0xE6, 0x99));
    head.paragraphs[0].runs[0].bold = true;
    t.cell_mut(1, 2).unwrap().paragraphs = vec![
        Paragraph::plain("first"),
        Paragraph { runs: vec![Run { text: "second".into(), italic: true, ..Run::default() }], ..Paragraph::default() },
    ];
    let mut o = Outline::with_text(Point::new(36.0, 108.0), "Before");
    o.blocks.push(Block::table(t));
    o.blocks.push(Paragraph::plain("After").into());
    let mut p = Page::new("Tables");
    p.elements.push(PageElement::new(ElementKind::Outline(o)));
    p
}

#[test]
fn html_has_a_real_table() {
    let html = page_to_html(&page()).unwrap();
    assert!(html.contains("<table class=\"nc-table borders\""), "{html}");
    assert!(html.contains("<col style=\"width:90pt\">"), "{html}");
    assert!(html.contains("<td style=\"background-color:#ffe699\"><p><strong>Name</strong></p></td>"), "{html}");
    assert!(html.contains("<td><p>a|b</p></td>"), "{html}");
    assert!(html.contains("<p>first</p><p><em>second</em></p>"), "{html}");
    let (before, after) = (html.find("Before").unwrap(), html.find("After").unwrap());
    let table = html.find("<table").unwrap();
    assert!(before < table && table < after, "in reading order");
    let mut hidden = page();
    if let ElementKind::Outline(o) = &mut hidden.elements[0].kind {
        o.table_mut(1).unwrap().borders = false;
    }
    assert!(page_to_html(&hidden).unwrap().contains("<table class=\"nc-table\""));
}

#[test]
fn markdown_has_a_pipe_table() {
    let md = page_to_markdown(&page()).unwrap();
    assert!(md.contains("Before\n\n| **Name** | Qty | Note |\n|---|---|---|\n| a\\|b | 2 | first<br>*second* |\n\nAfter"), "{md}");
}

#[test]
fn text_has_tab_separated_rows() {
    let txt = page_to_text(&page()).unwrap();
    assert_eq!(txt, "Tables\nBefore\nName\tQty\tNote\na|b\t2\tfirst second\nAfter");
}
