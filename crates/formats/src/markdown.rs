//! Markdown export (CommonMark plus GitHub's task lists and strikethrough).
//!
//! The title is a `#` heading; outline headings go one level below it. Outlines follow in
//! reading order, separated by blank lines. Pictures and printout pages are `data:` images; ink
//! is one SVG picture. Underline, colours and fonts have no Markdown form and are dropped.

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_doc::{ElementKind, ImageElement, ImageFormat, ListKind, Outline, Page, PageElement, ParaStyle, Paragraph, Run, Tag};

use crate::html::{reading_key, safe_href};
use crate::svg::{ink_bounds, ink_svg};

/// One page as Markdown. (Never fails today; `Result` keeps room for limits.)
pub fn page_to_markdown(page: &Page) -> crate::Result<String> {
    Ok(page_markdown(page))
}

fn page_markdown(page: &Page) -> String {
    let mut out = String::new();
    let title = page.title.trim();
    let _ = writeln!(out, "# {}\n", if title.is_empty() { "Untitled page".to_owned() } else { escape_line(title) });
    let _ = writeln!(out, "*{}*\n", page.created.format("%A, %e %B %Y %H:%M"));
    let mut els: Vec<&PageElement> = page.elements.iter().collect();
    els.sort_by(|a, b| reading_key(a).partial_cmp(&reading_key(b)).unwrap_or(std::cmp::Ordering::Equal));
    for el in els {
        let block = element_md(el);
        if !block.trim().is_empty() {
            out.push_str(block.trim_end());
            out.push_str("\n\n");
        }
    }
    if let Some(b) = ink_bounds(&page.ink) {
        let svg = ink_svg(&page.ink, b, false);
        let _ = writeln!(out, "![Ink](data:image/svg+xml;base64,{})\n", STANDARD.encode(svg.as_bytes()));
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn element_md(el: &PageElement) -> String {
    match &el.kind {
        ElementKind::Outline(o) => outline_md(o),
        ElementKind::Image(img) => image_md(img, &img.alt),
        ElementKind::Printout(p) => p
            .pages
            .iter()
            .enumerate()
            .map(|(i, pg)| image_md(pg, &if pg.alt.is_empty() { format!("{}, page {}", p.file_name, i + 1) } else { pg.alt.clone() }))
            .collect::<Vec<_>>()
            .join("\n\n"),
        ElementKind::EmbeddedFile { name, .. } => format!("\u{1F4CE} {}", escape_inline(name)),
        ElementKind::Opaque { .. } => String::new(),
    }
}

fn image_md(img: &ImageElement, alt: &str) -> String {
    if img.format == ImageFormat::Other {
        return String::new();
    }
    let alt: String = alt.chars().filter(|c| !matches!(c, '[' | ']' | '\n' | '\r')).collect();
    format!("![{alt}](data:{};base64,{})", img.format.mime(), STANDARD.encode(img.data.as_slice()))
}

fn outline_md(o: &Outline) -> String {
    let mut out = String::new();
    let mut prev: Option<&Paragraph> = None;
    let mut number = 0u32;
    let mut in_code = false;
    for p in &o.paragraphs {
        let code = p.style == ParaStyle::Code && p.list.is_none();
        if code != in_code {
            out.push_str("```\n");
            in_code = code;
        }
        if code {
            out.push_str(&p.text().replace("```", "` ` `"));
            out.push('\n');
            prev = Some(p);
            continue;
        }
        // A blank line between blocks, none inside a list.
        let todo = |x: &Paragraph| matches!(x.tag, Some(Tag::ToDo { .. }));
        if let Some(q) = prev
            && !(q.list.is_some() && p.list.is_some())
            && !(todo(q) && todo(p))
        {
            out.push('\n');
        }
        number = if p.list == Some(ListKind::Numbered) && prev.is_some_and(|q| q.list == Some(ListKind::Numbered)) { number + 1 } else { 1 };
        para_md(p, number, &mut out);
        out.push('\n');
        prev = Some(p);
    }
    if in_code {
        out.push_str("```\n");
    }
    out
}

fn para_md(p: &Paragraph, number: u32, out: &mut String) {
    let indent = "  ".repeat(usize::from(p.indent));
    let text: String = p.runs.iter().map(run_md).collect();
    let checkbox = match p.tag {
        Some(Tag::ToDo { checked }) => Some(if checked { "[x] " } else { "[ ] " }),
        _ => None,
    };
    match (p.list, checkbox) {
        (Some(ListKind::Numbered), c) => {
            let _ = write!(out, "{indent}{number}. {}{text}", c.unwrap_or(""));
        }
        (Some(ListKind::Bullet), c) => {
            let _ = write!(out, "{indent}- {}{text}", c.unwrap_or(""));
        }
        (None, Some(c)) => {
            let _ = write!(out, "{indent}- {c}{text}");
        }
        (None, None) => {
            let prefix = match p.style {
                ParaStyle::Quote => "> ".to_owned(),
                s => s.heading_level().map(|l| format!("{} ", "#".repeat(usize::from(l + 1).min(6)))).unwrap_or_default(),
            };
            let important = if p.tag == Some(Tag::Important) { "\u{2605} " } else { "" };
            let _ = write!(out, "{indent}{prefix}{important}{text}");
        }
    }
}

/// One run with its emphasis; surrounding spaces go outside the markers so they still apply.
fn run_md(r: &Run) -> String {
    if r.text.is_empty() {
        return String::new();
    }
    let core = r.text.trim();
    if core.is_empty() {
        return r.text.clone();
    }
    let lead = r.text.get(..r.text.len() - r.text.trim_start().len()).unwrap_or("");
    let trail = r.text.get(r.text.trim_end().len()..).unwrap_or("");
    let mut s = escape_inline(core);
    if r.strike {
        s = format!("~~{s}~~");
    }
    if r.italic {
        s = format!("*{s}*");
    }
    if r.bold {
        s = format!("**{s}**");
    }
    if let Some(href) = r.link.as_deref().and_then(safe_href) {
        s = format!("[{s}](<{}>)", href.replace(['<', '>', ' '], ""));
    }
    format!("{lead}{s}{trail}")
}

/// Backslash-escapes Markdown punctuation inside text.
fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '~' | '|' | '#') {
            out.push('\\');
        }
        if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

fn escape_line(s: &str) -> String {
    escape_inline(&s.replace(['\n', '\r'], " "))
}

#[cfg(test)]
mod tests {
    use notecraft_doc::Point;

    use super::*;

    #[test]
    fn markdown_keeps_structure() {
        let mut page = Page::new("Trip");
        let mut o = Outline::with_text(Point::new(36.0, 108.0), "Packing\nsocks\nhat\nbook tickets\nfirst\nsecond\nquote\nx = 1\n*literal*");
        o.paragraphs[0].style = ParaStyle::Heading1;
        o.paragraphs[1].list = Some(ListKind::Bullet);
        o.paragraphs[2].list = Some(ListKind::Bullet);
        o.paragraphs[2].indent = 1;
        o.paragraphs[3].tag = Some(Tag::ToDo { checked: false });
        o.paragraphs[4].list = Some(ListKind::Numbered);
        o.paragraphs[5].list = Some(ListKind::Numbered);
        o.paragraphs[6].style = ParaStyle::Quote;
        o.paragraphs[7].style = ParaStyle::Code;
        o.paragraphs[1].runs =
            vec![Run { text: "socks ".into(), bold: true, ..Run::default() }, Run { text: "now".into(), strike: true, ..Run::default() }];
        page.elements.push(PageElement::new(ElementKind::Outline(o)));
        let md = page_to_markdown(&page).unwrap();
        assert!(md.starts_with("# Trip\n"), "{md}");
        assert!(md.contains("## Packing"), "{md}");
        assert!(md.contains("- **socks** ~~now~~\n  - hat"), "{md}");
        assert!(md.contains("- [ ] book tickets"), "{md}");
        assert!(md.contains("1. first\n2. second"), "{md}");
        assert!(md.contains("> quote"), "{md}");
        assert!(md.contains("```\nx = 1\n```"), "{md}");
        assert!(md.contains("\\*literal\\*"), "{md}");
        assert!(md.ends_with("\\*\n") && !md.ends_with("\n\n"), "{md:?}");
    }

    #[test]
    fn links_are_filtered() {
        let ok = Run { text: "site".into(), link: Some("https://a.b/c".into()), ..Run::default() };
        assert_eq!(run_md(&ok), "[site](<https://a.b/c>)");
        let bad = Run { text: "x".into(), link: Some("javascript:1".into()), ..Run::default() };
        assert_eq!(run_md(&bad), "x");
        assert_eq!(run_md(&Run::plain("  ")), "  ");
    }
}
