//! HTML export: clean, semantic markup (headings, paragraphs, lists, To Do checkboxes,
//! `<strong>`/`<em>`…), pictures as `data:` URIs, ink as inline SVG.
//!
//! [`HtmlOptions::positioned`] (the default) keeps the page's layout: every element is placed
//! with inline CSS at its page position inside a `position: relative` page. Without it the
//! content flows in reading order (top to bottom, then left to right).

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::{
    Align, Block, ElementKind, ImageElement, ImageFormat, ListKind, ListStyle, Outline, Page, PageElement, ParaStyle, Paragraph, Printout, Rect, Run,
    Section, Table, Tag,
};

use crate::svg::{ink_bounds, ink_svg, num};

/// Largest attached file inlined as a `data:` link, bytes; bigger ones are listed by name only.
pub const MAX_INLINE_FILE: usize = 20 * 1024 * 1024;

/// How [`page_to_html_with`] lays a page out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HtmlOptions {
    /// Place elements at their page positions (inline CSS); otherwise flow in reading order.
    pub positioned: bool,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        HtmlOptions { positioned: true }
    }
}

const STYLE: &str = "body{font-family:Calibri,Carlito,'Segoe UI',Arial,sans-serif;font-size:11pt;color:#1f1f1f;margin:24px}\
.page{position:relative}\
.page-title{font-size:20pt;font-weight:normal;margin:0 0 4pt 0}\
.page-date{color:#6b6b6b;font-size:9pt;margin:0 0 18pt 0}\
.outline p,.outline li,.outline h2,.outline h3,.outline h4,.outline h5,.outline h6,.outline blockquote,.outline pre{margin:0 0 2pt 0}\
.outline ul,.outline ol{margin:0;padding-left:18pt}\
.todo{margin-right:4pt}\
.printout img,.picture{display:block}\
.para-title{font-size:20pt}\
blockquote{border-left:3px solid #c8c8cc;padding-left:6pt;color:#444}\
pre{font-family:Consolas,'Courier New',monospace}\
.attachment{display:inline-block;padding:4pt 8pt;border:1px solid #c8c8cc}\
nav.toc ol{list-style:none;padding-left:0}\
nav.toc .sub1{padding-left:18pt}nav.toc .sub2{padding-left:36pt}\
table.nc-table{border-collapse:collapse;table-layout:fixed;margin:2pt 0}\
table.nc-table td{vertical-align:top;padding:2pt 4pt;overflow-wrap:anywhere}\
table.nc-table.borders td{border:1px solid #a6a6a6}\
section.page-section{margin-top:36pt;border-top:1px solid #e0e0e0;padding-top:12pt}";

/// One page as a complete HTML document. (Never fails today; `Result` keeps room for limits.)
pub fn page_to_html(page: &Page) -> crate::Result<String> {
    Ok(page_to_html_with(page, HtmlOptions::default()))
}

/// One page as a complete HTML document, laid out as `opts` says.
pub fn page_to_html_with(page: &Page, opts: HtmlOptions) -> String {
    let mut out = head(page.display_title());
    out.push_str(&page_body(page, opts));
    out.push_str("</body>\n</html>\n");
    out
}

/// A section as one HTML document: a table of contents, then each page in order.
pub fn section_to_html(section: &Section) -> crate::Result<String> {
    Ok(section_to_html_with(section, HtmlOptions::default()))
}

/// [`section_to_html`] laid out as `opts` says.
pub fn section_to_html_with(section: &Section, opts: HtmlOptions) -> String {
    let mut out = head(&section.name);
    let _ = write!(out, "<h1>{}</h1>\n<nav class=\"toc\" aria-label=\"Pages\"><ol>", esc(&section.name));
    for (i, p) in section.pages.iter().enumerate() {
        let _ = write!(out, "<li class=\"sub{}\"><a href=\"#page-{}\">{}</a></li>", p.level.min(2), i + 1, esc(p.display_title()));
    }
    out.push_str("</ol></nav>\n");
    for (i, p) in section.pages.iter().enumerate() {
        let _ = write!(out, "<section class=\"page-section\" id=\"page-{}\">", i + 1);
        out.push_str(&page_body(p, opts));
        out.push_str("</section>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

fn head(title: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"generator\" content=\"NoteCraft\">\n<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n",
        esc(title)
    )
}

/// The page's `<article>`: title, date, then its elements and ink.
fn page_body(page: &Page, opts: HtmlOptions) -> String {
    let mut out = String::new();
    out.push_str("<article class=\"page\">\n");
    let _ = writeln!(out, "<h1 class=\"page-title\">{}</h1>", esc(page.title.trim()));
    let _ = writeln!(
        out,
        "<p class=\"page-date\"><time datetime=\"{}\">{}</time></p>",
        page.created.to_rfc3339(),
        page.created.format("%A, %e %B %Y %H:%M")
    );
    let mut els: Vec<&PageElement> = page.elements.iter().collect();
    if opts.positioned {
        // The title block takes the top of the page in the export: content keeps its offsets in
        // a relative box sized to what is on the page.
        let bounds = page.content_bounds().unwrap_or(Rect::ZERO);
        let _ = writeln!(
            out,
            "<div class=\"canvas\" style=\"position:relative;width:{}pt;height:{}pt\">",
            num(bounds.right().max(0.0)),
            num(bounds.bottom().max(0.0))
        );
    } else {
        els.sort_by(|a, b| reading_key(a).partial_cmp(&reading_key(b)).unwrap_or(std::cmp::Ordering::Equal));
        out.push_str("<div class=\"flow\">\n");
    }
    for el in els {
        element_html(el, opts.positioned, &mut out);
    }
    if let Some(b) = ink_bounds(&page.ink) {
        if opts.positioned {
            out.push_str(&ink_svg(&page.ink, b, true));
        } else {
            out.push_str("<figure class=\"drawing\">");
            out.push_str(&ink_svg(&page.ink, b, false));
            out.push_str("</figure>");
        }
        out.push('\n');
    }
    out.push_str("</div>\n</article>\n");
    out
}

/// Reading order: top to bottom, then left to right.
pub(crate) fn reading_key(el: &PageElement) -> (f32, f32) {
    el.pos().map_or((f32::MAX, f32::MAX), |p| (p.y, p.x))
}

fn place(positioned: bool, x: f32, y: f32, w: Option<f32>) -> String {
    if !positioned {
        return String::new();
    }
    let width = w.map(|w| format!(";width:{}pt", num(w))).unwrap_or_default();
    format!(" style=\"position:absolute;left:{}pt;top:{}pt{width}\"", num(x), num(y))
}

fn element_html(el: &PageElement, positioned: bool, out: &mut String) {
    match &el.kind {
        ElementKind::Outline(o) => {
            let _ = write!(out, "<div class=\"outline\"{}>", place(positioned, o.pos.x, o.pos.y, Some(o.width)));
            outline_html(o, out);
            out.push_str("</div>\n");
        }
        ElementKind::Image(img) => {
            if let Some(tag) = img_tag(img, "picture", positioned) {
                out.push_str(&tag);
                out.push('\n');
            }
        }
        ElementKind::Printout(p) => printout_html(p, positioned, out),
        ElementKind::EmbeddedFile { name, bytes, pos } => {
            let _ = write!(out, "<p class=\"attachment-box\"{}>", place(positioned, pos.x, pos.y, None));
            if bytes.len() <= MAX_INLINE_FILE {
                let _ = write!(
                    out,
                    "<a class=\"attachment\" download=\"{}\" href=\"data:application/octet-stream;base64,{}\">\u{1F4CE} {}</a>",
                    esc(name),
                    STANDARD.encode(bytes.as_slice()),
                    esc(name)
                );
            } else {
                let _ = write!(out, "<span class=\"attachment\">\u{1F4CE} {}</span>", esc(name));
            }
            out.push_str("</p>\n");
        }
        ElementKind::Opaque { .. } => out.push_str("<!-- content NoteCraft can't show yet -->\n"),
    }
}

fn printout_html(p: &Printout, positioned: bool, out: &mut String) {
    let _ = write!(out, "<figure class=\"printout\" aria-label=\"{}\">", esc(&p.file_name));
    for (i, page) in p.pages.iter().enumerate() {
        let mut img = page.clone();
        if img.alt.is_empty() {
            img.alt = format!("{}, page {}", p.file_name, i + 1);
        }
        if let Some(tag) = img_tag(&img, "printout-page", positioned) {
            out.push_str(&tag);
        }
    }
    if !positioned {
        let _ = write!(out, "<figcaption>{}</figcaption>", esc(&p.file_name));
    }
    out.push_str("</figure>\n");
}

/// An `<img>` with the picture inline as a `data:` URI; `None` for formats browsers can't show.
fn img_tag(img: &ImageElement, class: &str, positioned: bool) -> Option<String> {
    if img.format == ImageFormat::Other {
        return None;
    }
    let style = if positioned {
        format!(
            " style=\"position:absolute;left:{}pt;top:{}pt;width:{}pt;height:{}pt\"",
            num(img.pos.x),
            num(img.pos.y),
            num(img.size.w),
            num(img.size.h)
        )
    } else {
        format!(" style=\"width:{}pt;height:{}pt\"", num(img.size.w), num(img.size.h))
    };
    Some(format!(
        "<img class=\"{class}\" alt=\"{}\" src=\"data:{};base64,{}\"{style}>",
        esc(&img.alt),
        img.format.mime(),
        STANDARD.encode(img.data.as_slice())
    ))
}

/// An outline's blocks: paragraphs (consecutive list paragraphs of one kind and chosen marker
/// style share a list) and tables.
fn outline_html(o: &Outline, out: &mut String) {
    let mut run: Vec<&Paragraph> = Vec::new();
    for b in &o.blocks {
        match b {
            Block::Paragraph(p) => run.push(p),
            Block::Table { table } => {
                paras_html(&run, out);
                run.clear();
                table_html(table, out);
            }
        }
    }
    paras_html(&run, out);
}

/// A table: fixed column widths, cell shading, borders unless hidden; each cell's paragraphs
/// as in an outline.
fn table_html(t: &Table, out: &mut String) {
    let width: f32 = t.width();
    let class = if t.borders { "nc-table borders" } else { "nc-table" };
    let margin = if t.indent > 0 { format!("margin-left:{}pt;", u32::from(t.indent) * 18) } else { String::new() };
    let _ = write!(out, "<table class=\"{class}\" style=\"{margin}width:{}pt\"><colgroup>", num(width));
    for c in 0..t.col_count() {
        let _ = write!(out, "<col style=\"width:{}pt\">", num(t.col_width(c)));
    }
    out.push_str("</colgroup><tbody>");
    for row in &t.rows {
        out.push_str("<tr>");
        for cell in &row.cells {
            match cell.shading {
                Some(c) => {
                    let _ = write!(out, "<td style=\"background-color:{}\">", c.to_hex());
                }
                None => out.push_str("<td>"),
            }
            let paras: Vec<&Paragraph> = cell.paragraphs.iter().collect();
            paras_html(&paras, out);
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>\n");
}

/// Paragraphs in order: consecutive list paragraphs of one kind and (chosen) marker style
/// share a list.
fn paras_html(paras: &[&Paragraph], out: &mut String) {
    let mut open: Option<(ListKind, Option<ListStyle>)> = None;
    for &p in paras {
        let key = p.list.map(|k| (k, chosen_style(p)));
        if key != open {
            if let Some((k, _)) = open {
                out.push_str(list_close(k));
            }
            if let Some((k, st)) = key {
                list_open(k, st, out);
            }
            open = key;
        }
        para_html(p, out);
    }
    if let Some((k, _)) = open {
        out.push_str(list_close(k));
    }
}

/// The list style the paragraph chose (one of its list's kind), not the level's default.
fn chosen_style(p: &Paragraph) -> Option<ListStyle> {
    p.list_style.filter(|s| Some(s.kind()) == p.list)
}

/// CSS `list-style-type` for a marker style.
fn list_css(style: ListStyle) -> &'static str {
    match style {
        ListStyle::Disc => "disc",
        ListStyle::Circle => "circle",
        ListStyle::Square => "square",
        ListStyle::Dash => "&quot;\u{2013}  &quot;",
        ListStyle::Arrow => "&quot;\u{27A2}  &quot;",
        ListStyle::Decimal => "decimal",
        ListStyle::LowerAlpha => "lower-alpha",
        ListStyle::UpperAlpha => "upper-alpha",
        ListStyle::LowerRoman => "lower-roman",
        ListStyle::UpperRoman => "upper-roman",
    }
}

/// `<ul>` / `<ol>`, with a chosen marker style as CSS and, for numbers, `type`.
fn list_open(k: ListKind, style: Option<ListStyle>, out: &mut String) {
    let tag = if k == ListKind::Bullet { "ul" } else { "ol" };
    let Some(st) = style else {
        let _ = write!(out, "<{tag}>");
        return;
    };
    let ty = match st {
        ListStyle::LowerAlpha => " type=\"a\"",
        ListStyle::UpperAlpha => " type=\"A\"",
        ListStyle::LowerRoman => " type=\"i\"",
        ListStyle::UpperRoman => " type=\"I\"",
        _ => "",
    };
    let _ = write!(out, "<{tag}{ty} style=\"list-style-type:{}\">", list_css(st));
}

fn list_close(k: ListKind) -> &'static str {
    match k {
        ListKind::Bullet => "</ul>",
        ListKind::Numbered => "</ol>",
    }
}

fn para_html(p: &Paragraph, out: &mut String) {
    let (open, close) = if p.list.is_some() {
        ("<li".to_owned(), "</li>".to_owned())
    } else {
        match p.style {
            ParaStyle::Quote => ("<blockquote".to_owned(), "</blockquote>".to_owned()),
            ParaStyle::Code => ("<pre".to_owned(), "</pre>".to_owned()),
            s => match s.heading_level() {
                Some(l) => (format!("<h{}", (l + 1).min(6)), format!("</h{}>", (l + 1).min(6))),
                None if s == ParaStyle::Title => ("<p class=\"para-title\"".to_owned(), "</p>".to_owned()),
                None => ("<p".to_owned(), "</p>".to_owned()),
            },
        }
    };
    out.push_str(&open);
    let mut css = String::new();
    if p.indent > 0 {
        let _ = write!(css, "margin-left:{}pt;", u32::from(p.indent) * 18);
    }
    // A nested item without a chosen style takes its level's marker, as on the canvas.
    if let Some(k) = p.list
        && chosen_style(p).is_none()
        && !p.indent.is_multiple_of(3)
    {
        let _ = write!(css, "list-style-type:{};", list_css(ListStyle::by_level(k, p.indent)));
    }
    if p.align != Align::Left {
        let _ = write!(css, "text-align:{};", p.align.name());
    }
    if !css.is_empty() {
        let _ = write!(out, " style=\"{}\"", css.trim_end_matches(';'));
    }
    out.push('>');
    let mut mark = false;
    match p.tag {
        Some(Tag::ToDo { checked }) => {
            let _ = write!(out, "<input type=\"checkbox\" class=\"todo\" disabled{}> ", if checked { " checked" } else { "" });
        }
        Some(t) => {
            if let Some(sym) = crate::tag_symbol(t) {
                let _ = write!(out, "<span class=\"tag\" aria-label=\"{}\">{sym}</span> ", esc(t.label()));
            } else if let Some(c) = t.highlight() {
                let _ = write!(out, "<mark class=\"tag\" title=\"{}\" style=\"background-color:{}\">", esc(t.label()), c.to_hex());
                mark = true;
            }
        }
        None => {}
    }
    if p.style == ParaStyle::Code && p.list.is_none() {
        out.push_str("<code>");
    }
    for r in &p.runs {
        run_html(r, out);
    }
    if p.style == ParaStyle::Code && p.list.is_none() {
        out.push_str("</code>");
    }
    if mark {
        out.push_str("</mark>");
    }
    if p.runs.iter().all(|r| r.text.is_empty()) {
        out.push_str("<br>");
    }
    out.push_str(&close);
}

fn run_html(r: &Run, out: &mut String) {
    if r.text.is_empty() {
        return;
    }
    let mut close: Vec<&str> = Vec::new();
    if let Some(href) = r.link.as_deref().and_then(safe_href) {
        let _ = write!(out, "<a href=\"{}\">", esc(href));
        close.push("</a>");
    }
    let mut css = String::new();
    if let Some(c) = r.color {
        let _ = write!(css, "color:{};", c.to_hex());
    }
    if let Some(f) = r.font.as_deref() {
        let clean: String = f.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(64).collect();
        if !clean.trim().is_empty() {
            let _ = write!(css, "font-family:'{}';", clean.trim());
        }
    }
    if let Some(s) = r.size.filter(|s| s.is_finite() && *s > 0.0) {
        let _ = write!(css, "font-size:{}pt;", num(s.min(400.0)));
    }
    if !css.is_empty() {
        let _ = write!(out, "<span style=\"{css}\">");
        close.push("</span>");
    }
    if let Some(h) = r.highlight {
        let _ = write!(out, "<mark style=\"background:{}\">", h.to_hex());
        close.push("</mark>");
    }
    for (on, o, c) in [(r.bold, "<strong>", "</strong>"), (r.italic, "<em>", "</em>"), (r.underline, "<u>", "</u>"), (r.strike, "<s>", "</s>")] {
        if on {
            out.push_str(o);
            close.push(c);
        }
    }
    out.push_str(&esc(&r.text));
    for c in close.iter().rev() {
        out.push_str(c);
    }
}

/// Links kept in exports: web, mail and OneNote links only (no `javascript:` and the like).
pub(crate) fn safe_href(href: &str) -> Option<&str> {
    let h = href.trim();
    let lower = h.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "onenote:"].iter().any(|s| lower.starts_with(s)).then_some(h)
}

/// Escapes text for HTML content and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use notecraft_doc::{Color, InkPoint, InkStroke, InkTool, Point, Size};

    use super::*;

    fn sample() -> Page {
        let mut page = Page::new("Plans <&>");
        let mut o = Outline::with_text(Point::new(36.0, 108.0), "Heading\nfirst\nsecond\ndone\nplain");
        o.para_mut(0).unwrap().style = ParaStyle::Heading1;
        o.para_mut(1).unwrap().list = Some(ListKind::Bullet);
        o.para_mut(2).unwrap().list = Some(ListKind::Bullet);
        o.para_mut(2).unwrap().indent = 1;
        o.para_mut(3).unwrap().tag = Some(Tag::ToDo { checked: true });
        o.para_mut(4).unwrap().runs = vec![
            Run { text: "bold".into(), bold: true, ..Run::default() },
            Run::plain(" and "),
            Run { text: "link".into(), link: Some("https://example.com".into()), color: Some(Color::NOTECRAFT), ..Run::default() },
            Run { text: "bad".into(), link: Some("javascript:alert(1)".into()), ..Run::default() },
        ];
        page.elements.push(PageElement::new(ElementKind::Outline(o)));
        page.elements.push(PageElement::new(ElementKind::Image(ImageElement {
            pos: Point::new(400.0, 50.0),
            size: Size::new(10.0, 10.0),
            data: Arc::new(b"\x89PNG\r\n\x1a\nxx".to_vec()),
            format: ImageFormat::Png,
            alt: "a \"pic\"".into(),
        })));
        page.elements.push(PageElement::new(ElementKind::EmbeddedFile {
            name: "notes.txt".into(),
            bytes: Arc::new(b"hi".to_vec()),
            pos: Point::new(0.0, 500.0),
        }));
        page.ink.push(InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(0.0, 0.0, 1.0), InkPoint::new(9.0, 9.0, 1.0)]));
        page
    }

    #[test]
    fn page_html_is_semantic() {
        let html = page_to_html(&sample()).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>Plans &lt;&amp;&gt;</title>"));
        assert!(html.contains("<h2>Heading</h2>"));
        assert!(html.contains("<ul><li>first</li><li style=\"margin-left:18pt;list-style-type:circle\">second</li></ul>"), "{html}");
        assert!(html.contains("<input type=\"checkbox\" class=\"todo\" disabled checked> done"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<a href=\"https://example.com\"><span style=\"color:#c44df0;\">link</span></a>"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("src=\"data:image/png;base64,"));
        assert!(html.contains("alt=\"a &quot;pic&quot;\""));
        assert!(html.contains("<svg class=\"ink\""));
        assert!(html.contains("download=\"notes.txt\""));
        assert!(html.contains("position:absolute;left:36pt;top:108pt;width:288pt"));
        let flow = page_to_html_with(&sample(), HtmlOptions { positioned: false });
        assert!(!flow.contains("position:absolute"));
        // The picture (y = 50) comes before the outline (y = 108) in reading order.
        assert!(flow.find("<img").unwrap() < flow.find("<div class=\"outline\"").unwrap());
    }

    #[test]
    fn alignment_list_styles_and_tags_export() {
        let mut page = Page::new("x");
        let mut o = Outline::with_text(Point::new(0.0, 0.0), "mid\nsq1\nsq2\nrom\nidea\ndef\nleft");
        o.para_mut(0).unwrap().align = Align::Center;
        for i in [1, 2] {
            o.para_mut(i).unwrap().list = Some(ListKind::Bullet);
            o.para_mut(i).unwrap().list_style = Some(ListStyle::Square);
        }
        o.para_mut(2).unwrap().align = Align::Justify;
        o.para_mut(3).unwrap().list = Some(ListKind::Numbered);
        o.para_mut(3).unwrap().list_style = Some(ListStyle::UpperRoman);
        o.para_mut(4).unwrap().tag = Some(Tag::Idea);
        o.para_mut(5).unwrap().tag = Some(Tag::Definition);
        page.elements.push(PageElement::new(ElementKind::Outline(o)));
        let html = page_to_html(&page).unwrap();
        assert!(html.contains("<p style=\"text-align:center\">mid</p>"), "{html}");
        assert!(html.contains("<ul style=\"list-style-type:square\"><li>sq1</li><li style=\"text-align:justify\">sq2</li></ul>"), "{html}");
        assert!(html.contains("<ol type=\"I\" style=\"list-style-type:upper-roman\"><li>rom</li></ol>"), "{html}");
        assert!(html.contains("<span class=\"tag\" aria-label=\"Idea\">\u{1F4A1}</span> idea"), "{html}");
        assert!(html.contains("<mark class=\"tag\" title=\"Definition\" style=\"background-color:#92d050\">def</mark>"), "{html}");
        assert!(html.contains("<p>left</p>"));
    }

    #[test]
    fn section_html_has_toc() {
        let mut s = Section::new("Work", Color::WHITE);
        s.pages.push(sample());
        let mut sub = Page::new("Sub");
        sub.level = 1;
        s.pages.push(sub);
        let html = section_to_html(&s).unwrap();
        assert!(html.contains("<a href=\"#page-1\">Plans &lt;&amp;&gt;</a>"));
        assert!(html.contains("<li class=\"sub1\"><a href=\"#page-2\">Sub</a>"));
        assert!(html.contains("id=\"page-2\""));
    }

    #[test]
    fn escaping() {
        assert_eq!(esc("a<b>&\"'\u{0}"), "a&lt;b&gt;&amp;&quot;&#39;");
        assert_eq!(safe_href(" HTTPS://x "), Some("HTTPS://x"));
        assert_eq!(safe_href("data:text/html,x"), None);
    }
}
