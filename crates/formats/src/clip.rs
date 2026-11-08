//! Clipboard HTML: paragraphs to an HTML fragment for other apps ([`to_html`]), and HTML from
//! other apps (browsers, word processors) to paragraphs with their basic formatting
//! ([`from_html`]).
//!
//! The reader is a small tolerant tokenizer, not a full HTML parser: it knows the inline
//! formatting tags (b, i, u, s and their synonyms, `<span style>`, `<font>`, links), the blocks
//! that start paragraphs (p, div, li, headings, table rows, `<br>`) and lists. Everything else is
//! read for its text; `<script>`, `<style>`, `<head>` and comments are skipped. Input is hostile:
//! it is cut at [`MAX_HTML_BYTES`], nesting is bounded and so is the paragraph count.
//!
//! Tables: [`blocks_to_html`] writes NoteCraft tables as `<table>`, and [`from_html_blocks`] reads
//! a simple `<table>` (rows of cells; spans are ignored, a table inside a cell is read as text)
//! back as a table. [`from_html`] reads a table's cells as paragraphs.

use std::fmt::Write as _;

use notecraft_doc::page::MAX_INDENT;
use notecraft_doc::table::DEFAULT_COL_WIDTH;
use notecraft_doc::{Block, Cell, Color, ListKind, ParaStyle, Paragraph, Run, Table};

use crate::html::{esc, run_html};

/// Most HTML read, bytes (the rest is ignored).
pub const MAX_HTML_BYTES: usize = 4 * 1024 * 1024;
/// Most paragraphs read from HTML.
pub const MAX_HTML_PARAGRAPHS: usize = 20_000;
/// Deepest element nesting followed; deeper formatting is ignored (the text is kept).
pub const MAX_HTML_DEPTH: usize = 64;

// --- writing --------------------------------------------------------------------------------

/// An HTML fragment of `paragraphs`: headings, paragraphs and lists, with runs' bold, italic,
/// underline, strike, colours, font, size and links.
pub fn to_html(paragraphs: &[Paragraph]) -> String {
    let mut out = String::new();
    let mut open: Option<ListKind> = None;
    for p in paragraphs {
        if p.list != open {
            if let Some(k) = open {
                out.push_str(if k == ListKind::Bullet { "</ul>" } else { "</ol>" });
            }
            if let Some(k) = p.list {
                out.push_str(if k == ListKind::Bullet { "<ul>" } else { "<ol>" });
            }
            open = p.list;
        }
        let tag = match (p.list, p.style.heading_level()) {
            (Some(_), _) => "li".to_owned(),
            (None, Some(l)) => format!("h{}", l.clamp(1, 6)),
            (None, None) if p.style == ParaStyle::Code => "pre".to_owned(),
            (None, None) if p.style == ParaStyle::Quote => "blockquote".to_owned(),
            (None, None) => "p".to_owned(),
        };
        let mut css = String::new();
        if p.indent > 0 {
            let _ = write!(css, "margin-left:{}pt;", u32::from(p.indent) * 18);
        }
        if p.align != notecraft_doc::Align::Left {
            let _ = write!(css, "text-align:{};", p.align.name());
        }
        if css.is_empty() {
            let _ = write!(out, "<{tag}>");
        } else {
            let _ = write!(out, "<{tag} style=\"{}\">", css.trim_end_matches(';'));
        }
        for r in &p.runs {
            run_html(r, &mut out);
        }
        if p.runs.iter().all(|r| r.text.is_empty()) {
            out.push_str("<br>");
        }
        let _ = write!(out, "</{tag}>");
    }
    if let Some(k) = open {
        out.push_str(if k == ListKind::Bullet { "</ul>" } else { "</ol>" });
    }
    out
}

/// An HTML fragment of `blocks`: paragraphs as [`to_html`] writes them, tables as `<table>`
/// (each cell's paragraphs inside its `<td>`, shading as its background).
pub fn blocks_to_html(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut run: Vec<Paragraph> = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => run.push(p.clone()),
            Block::Table { table } => {
                out.push_str(&to_html(&run));
                run.clear();
                out.push_str("<table border=\"1\" style=\"border-collapse:collapse\">");
                for row in &table.rows {
                    out.push_str("<tr>");
                    for (c, cell) in row.cells.iter().enumerate() {
                        let mut css = format!("width:{}pt;vertical-align:top", table.col_width(c).round());
                        if let Some(s) = cell.shading {
                            let _ = write!(css, ";background-color:{}", s.to_hex());
                        }
                        let _ = write!(out, "<td style=\"{css}\">{}</td>", to_html(&cell.paragraphs));
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
        }
    }
    out.push_str(&to_html(&run));
    out
}

/// A whole HTML document around a fragment (what the system clipboard gets).
pub fn document(fragment: &str) -> String {
    format!("<html><head><meta charset=\"utf-8\"></head><body><!--StartFragment-->{fragment}<!--EndFragment--></body></html>")
}

/// Escapes text for HTML (re-exported for callers building fragments).
pub fn escape(s: &str) -> String {
    esc(s)
}

// --- reading --------------------------------------------------------------------------------

/// Formatting an open element sets (`None`: unchanged).
#[derive(Clone, Debug, Default)]
struct Fmt {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    color: Option<Color>,
    highlight: Option<Color>,
    font: Option<String>,
    size: Option<f32>,
    link: Option<String>,
}

/// An open element: its name and the formatting it set.
struct Open {
    name: String,
    fmt: Fmt,
}

struct Reader {
    paras: Vec<Paragraph>,
    cur: Paragraph,
    /// Text since the last paragraph break, for collapsing whitespace.
    pending_space: bool,
    stack: Vec<Open>,
    /// Open lists, outermost first.
    lists: Vec<ListKind>,
    /// The current paragraph's block style.
    style: ParaStyle,
    in_li: bool,
    pre: usize,
    /// Read `<table>`s as tables ([`from_html_blocks`]).
    keep_tables: bool,
    /// The table being read.
    table: Option<TableBuild>,
    /// Tables read, with the paragraph index they go before.
    tables: Vec<(usize, Table)>,
}

/// A `<table>` being read: its rows of cells' paragraphs.
struct TableBuild {
    /// Where it goes among the paragraphs.
    at: usize,
    rows: Vec<Vec<Vec<Paragraph>>>,
    /// The open cell's first paragraph.
    cell_from: Option<usize>,
    /// Tables open inside this one (read as text).
    nested: usize,
}

impl TableBuild {
    /// The table: as many columns as the longest row, every cell its paragraphs.
    fn finish(self) -> Option<Table> {
        let rows: Vec<Vec<Vec<Paragraph>>> = self.rows.into_iter().filter(|r| !r.is_empty()).collect();
        let cols = rows.iter().map(Vec::len).max()?;
        let mut t = Table::new(rows.len(), cols, DEFAULT_COL_WIDTH);
        for (r, row) in rows.into_iter().enumerate() {
            for (c, paras) in row.into_iter().enumerate() {
                if let Some(cell) = t.cell_mut(r, c) {
                    *cell = Cell { paragraphs: paras, shading: None };
                }
            }
        }
        t.normalize();
        Some(t)
    }
}

impl Reader {
    fn new() -> Reader {
        Reader {
            paras: Vec::new(),
            cur: Paragraph::default(),
            pending_space: false,
            stack: Vec::new(),
            lists: Vec::new(),
            style: ParaStyle::Normal,
            in_li: false,
            pre: 0,
            keep_tables: false,
            table: None,
            tables: Vec::new(),
        }
    }

    /// Whether `name` opens or closes part of the table being read (not one nested in it).
    fn table_part(&self, name: &str) -> bool {
        self.keep_tables && matches!(name, "table" | "tr" | "td" | "th") && self.table.as_ref().is_none_or(|t| t.nested == 0 || name == "table")
    }

    fn open_table_part(&mut self, name: &str) {
        self.break_para(false);
        match (name, self.table.as_mut()) {
            ("table", None) => self.table = Some(TableBuild { at: self.paras.len(), rows: Vec::new(), cell_from: None, nested: 0 }),
            ("table", Some(t)) => t.nested += 1,
            ("tr", Some(t)) => t.rows.push(Vec::new()),
            (_, Some(t)) => {
                if t.rows.is_empty() {
                    t.rows.push(Vec::new());
                }
                t.cell_from = Some(self.paras.len());
            }
            _ => {}
        }
    }

    fn close_table_part(&mut self, name: &str) {
        self.break_para(false);
        let Some(t) = self.table.as_mut() else { return };
        match name {
            "table" if t.nested > 0 => t.nested -= 1,
            "table" => {
                if let Some(from) = t.cell_from.take() {
                    let paras: Vec<Paragraph> = self.paras.drain(from.min(self.paras.len())..).collect();
                    if let Some(row) = t.rows.last_mut() {
                        row.push(paras);
                    }
                }
                if let Some(t) = self.table.take() {
                    let at = t.at.min(self.paras.len());
                    if let Some(table) = t.finish() {
                        self.tables.push((at, table));
                    }
                }
            }
            "td" | "th" => {
                if let Some(from) = t.cell_from.take() {
                    let paras: Vec<Paragraph> = self.paras.drain(from.min(self.paras.len())..).collect();
                    if let Some(row) = t.rows.last_mut() {
                        row.push(paras);
                    }
                }
            }
            _ => {}
        }
    }

    fn full(&self) -> bool {
        self.paras.len() >= MAX_HTML_PARAGRAPHS
    }

    /// The run formatting in effect: every open element's, innermost last.
    fn run_format(&self) -> Run {
        let mut r = Run::default();
        for o in &self.stack {
            let f = &o.fmt;
            if let Some(b) = f.bold {
                r.bold = b;
            }
            if let Some(b) = f.italic {
                r.italic = b;
            }
            if let Some(b) = f.underline {
                r.underline = b;
            }
            if let Some(b) = f.strike {
                r.strike = b;
            }
            if f.color.is_some() {
                r.color = f.color;
            }
            if f.highlight.is_some() {
                r.highlight = f.highlight;
            }
            if f.font.is_some() {
                r.font.clone_from(&f.font);
            }
            if f.size.is_some() {
                r.size = f.size;
            }
            if f.link.is_some() {
                r.link.clone_from(&f.link);
            }
        }
        r
    }

    fn text(&mut self, raw: &str) {
        if self.full() {
            return;
        }
        let mut s = String::new();
        if self.pre > 0 {
            s.push_str(raw);
            // A preformatted line break is a new paragraph.
            let mut parts = s.split('\n').map(str::to_owned).collect::<Vec<_>>().into_iter();
            if let Some(first) = parts.next() {
                self.push_text(&first);
            }
            for part in parts {
                self.break_para(true);
                self.push_text(&part);
            }
            return;
        }
        // A space carried over from the text before goes with that text (`a <b>b</b>`); one
        // seen here takes this text's formatting.
        let mut carried = self.pending_space;
        for c in raw.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                self.pending_space = true;
            } else {
                if self.pending_space {
                    if !carried && (!s.is_empty() || !self.cur_text_empty()) {
                        s.push(' ');
                    } else if !self.cur_text_empty()
                        && let Some(last) = self.cur.runs.last_mut()
                    {
                        // The space goes with the text before it (`a <b>b</b>`).
                        last.text.push(' ');
                    }
                }
                self.pending_space = false;
                carried = false;
                s.push(if c == '\u{a0}' { ' ' } else { c });
            }
        }
        self.push_text(&s);
    }

    fn cur_text_empty(&self) -> bool {
        self.cur.runs.iter().all(|r| r.text.is_empty())
    }

    fn push_text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let fmt = self.run_format();
        let text: String = s.chars().filter(|c| *c == '\t' || !c.is_control()).collect();
        match self.cur.runs.last_mut() {
            Some(last) if last.same_format(&Run { text: String::new(), ..fmt.clone() }) => last.text.push_str(&text),
            _ => self.cur.runs.push(Run { text, ..fmt }),
        }
    }

    /// Ends the current paragraph. `force` keeps an empty one (a `<br>` on its own line).
    fn break_para(&mut self, force: bool) {
        if self.full() {
            return;
        }
        self.pending_space = false;
        let empty = self.cur_text_empty();
        if empty && !force {
            return;
        }
        let mut p = std::mem::take(&mut self.cur);
        if p.runs.is_empty() {
            p.runs.push(Run::default());
        }
        p.style = self.style;
        if self.in_li
            && let Some(k) = self.lists.last()
        {
            p.list = Some(*k);
        }
        let depth = self.lists.len().saturating_sub(1);
        p.indent = u8::try_from(depth).unwrap_or(MAX_INDENT).min(MAX_INDENT);
        self.paras.push(p);
    }

    fn open(&mut self, name: &str, attrs: &str) {
        if self.table_part(name) {
            self.open_table_part(name);
            if name == "th" && self.stack.len() < MAX_HTML_DEPTH {
                self.stack.push(Open { name: name.to_owned(), fmt: Fmt { bold: Some(true), ..Fmt::default() } });
            }
            return;
        }
        let block = is_block(name);
        if block {
            self.break_para(false);
        }
        match name {
            "br" => {
                self.break_para(true);
                return;
            }
            "ul" | "ol" => {
                if self.lists.len() < MAX_HTML_DEPTH {
                    self.lists.push(if name == "ul" { ListKind::Bullet } else { ListKind::Numbered });
                }
                self.in_li = false;
            }
            "li" => self.in_li = true,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.style = match name {
                    "h1" => ParaStyle::Heading1,
                    "h2" => ParaStyle::Heading2,
                    "h3" => ParaStyle::Heading3,
                    "h4" => ParaStyle::Heading4,
                    "h5" => ParaStyle::Heading5,
                    _ => ParaStyle::Heading6,
                };
            }
            "pre" => {
                self.style = ParaStyle::Code;
                self.pre += 1;
            }
            "blockquote" => self.style = ParaStyle::Quote,
            _ => {}
        }
        if is_void(name) {
            return;
        }
        let mut fmt = Fmt::default();
        match name {
            "b" | "strong" => fmt.bold = Some(true),
            "i" | "em" | "cite" | "var" => fmt.italic = Some(true),
            "u" | "ins" => fmt.underline = Some(true),
            "s" | "strike" | "del" => fmt.strike = Some(true),
            "mark" => fmt.highlight = Some(Color::HIGHLIGHT_YELLOW),
            "code" | "tt" | "kbd" | "samp" => fmt.font = Some("Consolas".to_owned()),
            "a" => fmt.link = attr(attrs, "href").map(|h| decode_entities(&h)).and_then(|h| safe_link(&h)),
            "font" => {
                if let Some(c) = attr(attrs, "color").and_then(|c| parse_css_color(&c)) {
                    fmt.color = Some(c);
                }
                if let Some(f) = attr(attrs, "face").and_then(|f| clean_font(&f)) {
                    fmt.font = Some(f);
                }
            }
            _ => {}
        }
        if let Some(style) = attr(attrs, "style") {
            apply_css(&decode_entities(&style), &mut fmt);
        }
        if self.stack.len() < MAX_HTML_DEPTH {
            self.stack.push(Open { name: name.to_owned(), fmt });
        }
    }

    fn close(&mut self, name: &str) {
        if self.table_part(name) {
            self.close_table_part(name);
            if let Some(i) = self.stack.iter().rposition(|o| o.name == name) {
                self.stack.truncate(i);
            }
            return;
        }
        if is_block(name) {
            self.break_para(false);
        }
        match name {
            "ul" | "ol" => {
                self.lists.pop();
                self.in_li = !self.lists.is_empty();
            }
            "li" => self.in_li = false,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" => self.style = ParaStyle::Normal,
            "pre" => {
                self.pre = self.pre.saturating_sub(1);
                if self.pre == 0 {
                    self.style = ParaStyle::Normal;
                }
            }
            _ => {}
        }
        // Close up to the matching element (tolerating misnested and unclosed tags).
        if let Some(i) = self.stack.iter().rposition(|o| o.name == name) {
            self.stack.truncate(i);
        }
    }

    fn finish(mut self) -> Vec<Paragraph> {
        self.break_para(false);
        let mut out = self.paras;
        // Trailing empty paragraphs (a final <br>) go.
        while out.len() > 1 && out.last().is_some_and(|p| p.runs.iter().all(|r| r.text.is_empty())) {
            out.pop();
        }
        out
    }

    /// The paragraphs with the tables read in their places.
    fn finish_blocks(mut self) -> Vec<Block> {
        // An unclosed table still counts.
        if self.table.is_some() {
            if let Some(t) = self.table.as_mut() {
                t.nested = 0;
            }
            self.close_table_part("table");
        }
        let tables = std::mem::take(&mut self.tables);
        let paras = if tables.is_empty() {
            self.finish()
        } else {
            self.break_para(false);
            self.paras
        };
        let mut out: Vec<Block> = Vec::with_capacity(paras.len() + tables.len());
        let mut ts = tables.into_iter().peekable();
        for (i, p) in paras.into_iter().enumerate() {
            while ts.peek().is_some_and(|(at, _)| *at <= i) {
                if let Some((_, t)) = ts.next() {
                    out.push(Block::table(t));
                }
            }
            out.push(Block::Paragraph(p));
        }
        out.extend(ts.map(|(_, t)| Block::table(t)));
        out
    }
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "li"
            | "ul"
            | "ol"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "tr"
            | "table"
            | "blockquote"
            | "pre"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "dt"
            | "dd"
            | "hr"
            | "figure"
            | "figcaption"
    )
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img" | "meta" | "link" | "input" | "col" | "wbr" | "source" | "area" | "base")
}

/// The value of attribute `key` in a tag's attribute text.
fn attr(attrs: &str, key: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let bytes = attrs.as_bytes();
    let mut from = 0;
    while let Some(off) = lower.get(from..)?.find(key) {
        let i = from + off;
        from = i + key.len();
        let before_ok = i == 0 || bytes.get(i - 1).is_some_and(|b| b.is_ascii_whitespace());
        let mut j = i + key.len();
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        if !before_ok || bytes.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        let rest = attrs.get(j..)?;
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest.get(1..).and_then(|r| r.split(q).next()).unwrap_or(""),
            _ => rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or(""),
        };
        return Some(value.chars().take(4096).collect());
    }
    None
}

/// Links kept from pasted HTML.
fn safe_link(h: &str) -> Option<String> {
    let h = h.trim();
    let lower = h.to_ascii_lowercase();
    ["http://", "https://", "mailto:", "notecraft:", "onenote:", "file:"]
        .iter()
        .any(|s| lower.starts_with(s))
        .then(|| h.chars().filter(|c| !c.is_control()).take(2048).collect())
}

fn clean_font(f: &str) -> Option<String> {
    let first = f.split(',').next().unwrap_or("").trim().trim_matches(|c| c == '"' || c == '\'').trim();
    let s: String = first.chars().filter(|c| !c.is_control()).take(64).collect();
    let generic = ["serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui", "inherit", "initial"];
    (!s.is_empty() && !generic.contains(&s.to_ascii_lowercase().as_str())).then_some(s)
}

fn apply_css(style: &str, fmt: &mut Fmt) {
    for decl in style.split(';').take(64) {
        let Some((k, v)) = decl.split_once(':') else { continue };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_end_matches("!important").trim();
        let lv = v.to_ascii_lowercase();
        match k.as_str() {
            "font-weight" => {
                fmt.bold = Some(lv == "bold" || lv == "bolder" || lv.parse::<u32>().is_ok_and(|w| w >= 600));
            }
            "font-style" => fmt.italic = Some(lv.contains("italic") || lv.contains("oblique")),
            "text-decoration" | "text-decoration-line" => {
                if lv.contains("underline") {
                    fmt.underline = Some(true);
                }
                if lv.contains("line-through") {
                    fmt.strike = Some(true);
                }
                if lv == "none" {
                    fmt.underline = Some(false);
                    fmt.strike = Some(false);
                }
            }
            "color" => {
                if let Some(c) = parse_css_color(v) {
                    fmt.color = Some(c);
                }
            }
            "background-color" | "background" => {
                // White and transparent backgrounds are the page, not a highlight.
                if let Some(c) = parse_css_color(v).filter(|c| c.a > 0 && !(c.r > 0xF0 && c.g > 0xF0 && c.b > 0xF0)) {
                    fmt.highlight = Some(c);
                }
            }
            "font-family" => {
                if let Some(f) = clean_font(v) {
                    fmt.font = Some(f);
                }
            }
            "font-size" => {
                if let Some(s) = parse_size(&lv) {
                    fmt.size = Some(s);
                }
            }
            _ => {}
        }
    }
}

/// A CSS length as points (`pt`, `px`), clamped to 1–400.
fn parse_size(v: &str) -> Option<f32> {
    let (num, k) = match v.strip_suffix("pt") {
        Some(n) => (n, 1.0),
        None => (v.strip_suffix("px")?, 0.75),
    };
    let n: f32 = num.trim().parse().ok()?;
    let pt = n * k;
    (pt.is_finite() && pt > 0.0).then(|| (pt * 2.0).round().max(2.0) / 2.0).map(|s| s.clamp(1.0, 400.0))
}

/// `#rgb`, `#rrggbb`, `rgb(r, g, b)` / `rgba(…)` and a few colour names.
fn parse_css_color(v: &str) -> Option<Color> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let hex: String = hex.chars().take(6).collect();
        return match hex.len() {
            3 => {
                let d: Vec<u8> = hex.chars().filter_map(|c| c.to_digit(16)).map(|x| (x as u8) * 17).collect();
                match d.as_slice() {
                    [r, g, b] => Some(Color::rgb(*r, *g, *b)),
                    _ => None,
                }
            }
            6 => Color::parse_hex(&format!("#{hex}")),
            _ => None,
        };
    }
    if let Some(inner) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let inner = inner.trim_end_matches(')');
        let parts: Vec<&str> = inner.split([',', ' ', '/']).filter(|s| !s.trim().is_empty()).take(4).collect();
        let c = |i: usize| parts.get(i).and_then(|s| s.trim().parse::<f32>().ok()).map(|f| f.clamp(0.0, 255.0) as u8);
        let a = parts.get(3).and_then(|s| s.trim().parse::<f32>().ok()).map_or(255, |f| (f.clamp(0.0, 1.0) * 255.0) as u8);
        return Some(Color::rgba(c(0)?, c(1)?, c(2)?, a));
    }
    Some(match v.as_str() {
        "black" => Color::rgb(0, 0, 0),
        "white" => Color::rgb(255, 255, 255),
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "orange" => Color::rgb(255, 165, 0),
        "purple" => Color::rgb(128, 0, 128),
        "gray" | "grey" => Color::rgb(128, 128, 128),
        _ => return None,
    })
}

/// Replaces character references (`&amp;`, `&#233;`, `&#xE9;`, a few names).
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(rest.get(..i).unwrap_or(""));
        let tail = rest.get(i..).unwrap_or("");
        let end = tail.get(..tail.len().min(12)).and_then(|t| t.find(';'));
        let decoded = end.and_then(|e| {
            let name = tail.get(1..e)?;
            let c = if let Some(num) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
            } else if let Some(num) = name.strip_prefix('#') {
                num.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    "ndash" => Some('\u{2013}'),
                    "mdash" => Some('\u{2014}'),
                    "hellip" => Some('\u{2026}'),
                    "lsquo" => Some('\u{2018}'),
                    "rsquo" => Some('\u{2019}'),
                    "ldquo" => Some('\u{201c}'),
                    "rdquo" => Some('\u{201d}'),
                    "bull" => Some('\u{2022}'),
                    "copy" => Some('\u{a9}'),
                    _ => None,
                }
            };
            Some((c?, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(if c == '\0' { '\u{FFFD}' } else { c });
                rest = tail.get(len..).unwrap_or("");
            }
            None => {
                out.push('&');
                rest = tail.get(1..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out
}

/// The part of `html` to read: the fragment between `<!--StartFragment-->` and
/// `<!--EndFragment-->` when marked (Windows' CF_HTML, browsers), else the `<body>`, else all.
fn fragment(html: &str) -> &str {
    if let Some(s) = html.find("<!--StartFragment-->") {
        let from = s + "<!--StartFragment-->".len();
        let to = html.get(from..).and_then(|r| r.find("<!--EndFragment-->")).map_or(html.len(), |e| from + e);
        return html.get(from..to).unwrap_or("");
    }
    // CF_HTML's header (Version:…StartHTML:…) without fragment markers: skip to the markup.
    if html.starts_with("Version:") { html.find('<').and_then(|i| html.get(i..)).unwrap_or("") } else { html }
}

/// Paragraphs with their basic formatting from HTML another app put on the clipboard.
pub fn from_html(html: &str) -> Vec<Paragraph> {
    read(html, false).finish()
}

/// [`from_html`] with simple `<table>`s read as tables.
pub fn from_html_blocks(html: &str) -> Vec<Block> {
    read(html, true).finish_blocks()
}

fn read(html: &str, keep_tables: bool) -> Reader {
    let mut cut = html.len().min(MAX_HTML_BYTES);
    while !html.is_char_boundary(cut) {
        cut -= 1;
    }
    let src = fragment(html.get(..cut).unwrap_or(""));
    let mut r = Reader::new();
    r.keep_tables = keep_tables;
    let mut rest = src;
    // Elements whose content isn't text.
    let mut skip: Option<String> = None;
    while !rest.is_empty() && !r.full() {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() {
                r.text(&decode_entities(rest));
            }
            break;
        };
        if lt > 0 && skip.is_none() {
            r.text(&decode_entities(rest.get(..lt).unwrap_or("")));
        }
        rest = rest.get(lt..).unwrap_or("");
        if rest.starts_with("<!--") {
            rest = rest.find("-->").and_then(|e| rest.get(e + 3..)).unwrap_or("");
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').and_then(|e| rest.get(e + 1..)).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else {
            // An unfinished tag: the rest is text.
            if skip.is_none() {
                r.text(&decode_entities(rest.get(1..).unwrap_or("")));
            }
            break;
        };
        let inner = rest.get(1..gt).unwrap_or("");
        rest = rest.get(gt + 1..).unwrap_or("");
        let closing = inner.starts_with('/');
        let body = inner.trim_start_matches('/');
        let name_end = body.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(body.len());
        let name: String = body.get(..name_end).unwrap_or("").chars().take(16).collect::<String>().to_ascii_lowercase();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-') {
            if skip.is_none() {
                r.text(&decode_entities(&format!("<{inner}>")));
            }
            continue;
        }
        if let Some(s) = &skip {
            if closing && *s == name {
                skip = None;
            }
            continue;
        }
        if !closing && matches!(name.as_str(), "script" | "style" | "head" | "title" | "template" | "noscript" | "svg") {
            skip = Some(name);
            continue;
        }
        let attrs = body.get(name_end..).unwrap_or("");
        if closing {
            r.close(&name);
        } else {
            r.open(&name, attrs);
            if attrs.trim_end().ends_with('/') && !is_void(&name) {
                r.close(&name);
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(ps: &[Paragraph]) -> Vec<String> {
        ps.iter().map(Paragraph::text).collect()
    }

    #[test]
    fn reads_basic_formatting() {
        let ps = from_html(
            "<h2>Plan</h2><p>Some <b>bold</b>, <i>italic</i>, <u>under</u> and <s>gone</s> text.<br>Next line</p>\
             <ul><li>one</li><li>two<ul><li>deep</li></ul></li></ul><ol><li>first</li></ol>\
             <p><a href=\"https://example.com\">a link</a> <a href=\"javascript:alert(1)\">bad</a></p>",
        );
        assert_eq!(texts(&ps), ["Plan", "Some bold, italic, under and gone text.", "Next line", "one", "two", "deep", "first", "a link bad"]);
        assert_eq!(ps[0].style, ParaStyle::Heading2);
        let runs = &ps[1].runs;
        assert!(runs.iter().any(|r| r.bold && r.text == "bold"));
        assert!(runs.iter().any(|r| r.italic && r.text == "italic"));
        assert!(runs.iter().any(|r| r.underline && r.text == "under"), "{runs:?}");
        assert!(runs.iter().any(|r| r.strike && r.text == "gone"));
        assert_eq!((ps[3].list, ps[3].indent), (Some(ListKind::Bullet), 0));
        assert_eq!((ps[5].list, ps[5].indent), (Some(ListKind::Bullet), 1));
        assert_eq!(ps[6].list, Some(ListKind::Numbered));
        assert_eq!(ps[7].runs[0].link.as_deref(), Some("https://example.com"));
        assert!(ps[7].runs.iter().all(|r| r.link.as_deref() != Some("javascript:alert(1)")));
    }

    #[test]
    fn styles_entities_and_fragments() {
        let html = "Version:0.9\r\nStartHTML:0000\r\n<html><head><style>p{color:red}</style></head><body>junk<!--StartFragment-->\
            <span style=\"font-weight:700;color:#ff0000;font-size:14pt;font-family:'Georgia', serif\">A&amp;B &lt;x&gt; &#233;&#x41;&nbsp;z</span>\
            <!--EndFragment--></body></html>";
        let ps = from_html(html);
        assert_eq!(texts(&ps), ["A&B <x> \u{e9}A z"]);
        let r = &ps[0].runs[0];
        assert!(r.bold);
        assert_eq!(r.color, Some(Color::rgb(255, 0, 0)));
        assert_eq!(r.size, Some(14.0));
        assert_eq!(r.font.as_deref(), Some("Georgia"));
        // Whitespace collapses like HTML; scripts are skipped.
        assert_eq!(texts(&from_html("<p>  a \n  b  </p><script>alert(1)</script>")), ["a b"]);
        assert_eq!(texts(&from_html("plain text")), ["plain text"]);
    }

    #[test]
    fn tables_read_and_write() {
        let html = "<p>before</p><table><tr><th>Name</th><th>Qty</th></tr><tr><td>eggs <b>big</b></td><td><p>12</p><p>dozen</p></td></tr></table><p>after</p>";
        let blocks = from_html_blocks(html);
        assert_eq!(blocks.len(), 3, "{blocks:?}");
        let t = blocks[1].as_table().unwrap();
        assert_eq!((t.row_count(), t.col_count()), (2, 2));
        assert_eq!(t.texts(), vec![vec!["Name", "Qty"], vec!["eggs big", "12 dozen"]]);
        assert!(t.cell(0, 0).unwrap().paragraphs[0].runs[0].bold, "header cells are bold");
        assert!(t.cell(1, 0).unwrap().paragraphs[0].runs.iter().any(|r| r.bold && r.text == "big"));
        assert_eq!(blocks[2].text(), "after");
        // Ragged rows, a nested table (as text), an unclosed table: never a panic.
        let b = from_html_blocks("<table><tr><td>a</td></tr><tr><td>b</td><td><table><tr><td>x</td></tr></table></td></tr>");
        let t = b.iter().find_map(Block::as_table).unwrap();
        assert_eq!((t.row_count(), t.col_count()), (2, 2));
        assert!(t.cell(1, 1).unwrap().text().contains('x'));
        // Without tables: their cells are paragraphs, as before.
        assert_eq!(texts(&from_html(html)).join("|"), "before|NameQty|eggs big|12|dozen|after");
        // And written back out.
        let out = blocks_to_html(&blocks);
        assert!(out.contains("<table") && out.contains("<td style=\"width:72pt;vertical-align:top\"><p><strong>Name</strong></p></td>"), "{out}");
        let again = from_html_blocks(&out);
        assert_eq!(again.iter().find_map(Block::as_table).map(Table::texts), Some(t_texts(&blocks)));
    }

    fn t_texts(b: &[Block]) -> Vec<Vec<String>> {
        b.iter().find_map(Block::as_table).map(Table::texts).unwrap_or_default()
    }

    #[test]
    fn hostile_html_never_panics() {
        let deep = "<b><i>".repeat(10_000) + "x";
        assert_eq!(texts(&from_html(&deep)), ["x"]);
        for junk in ["<", "<<<>>>", "&#xFFFFFFFFFF;", "&#99999999999999999999;", "<a href=", "<p", "</p></p></ul>", "<!--", "<b>é", "&", "<\u{e9}>"]
        {
            let _ = from_html(junk);
            let _ = from_html_blocks(junk);
        }
        let many = "<p>x</p>".repeat(30_000);
        assert_eq!(from_html(&many).len(), MAX_HTML_PARAGRAPHS);
        let ul = "<ul><li>".repeat(200) + "x";
        let ps = from_html(&ul);
        assert!(ps.iter().all(|p| p.indent <= MAX_INDENT));
        let tables = "<table><tr><td>".repeat(500) + "x" + &"</td></tr>".repeat(3) + &"<table><tr><td>y".repeat(100);
        let _ = from_html_blocks(&tables);
        let big = "<table>".to_owned() + &"<tr>".repeat(5000) + &"<td>x</td>".repeat(20_000);
        let b = from_html_blocks(&big);
        assert!(b.iter().filter_map(Block::as_table).all(|t| t.row_count() * t.col_count() <= notecraft_doc::table::MAX_TABLE_CELLS));
    }

    #[test]
    fn writes_html_that_reads_back() {
        let ps = vec![
            Paragraph { style: ParaStyle::Heading1, ..Paragraph::plain("Title") },
            Paragraph { runs: vec![Run { text: "bold <tag>".into(), bold: true, ..Run::default() }], ..Paragraph::default() },
            Paragraph { list: Some(ListKind::Bullet), ..Paragraph::plain("item") },
        ];
        let html = to_html(&ps);
        assert!(html.contains("<h1>") && html.contains("&lt;tag&gt;") && html.contains("<ul><li>"), "{html}");
        let back = from_html(&document(&html));
        assert_eq!(texts(&back), ["Title", "bold <tag>", "item"]);
        assert_eq!(back[0].style, ParaStyle::Heading1);
        assert!(back[1].runs[0].bold);
        assert_eq!(back[2].list, Some(ListKind::Bullet));
    }
}
