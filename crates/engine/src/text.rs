//! Rich-text editing on the model: caret positions, splitting and merging runs, inserting and
//! deleting text across paragraphs, applying formatting to a character range.
//!
//! Editing happens in a [`Flow`]: a note container's blocks, or one table cell's paragraphs. A
//! position is a block index and a **character** index (Unicode scalar values, as egui's
//! `CCursor` counts them) into that paragraph's text (its runs joined). A table is a block of
//! no characters the caret is never in (its cells are flows of their own, [`CellRef`]): text
//! isn't typed into it, paragraphs don't merge with it, and a deletion that spans it removes
//! it. Every function clamps positions to the flow, so hostile values never index out of
//! bounds.

use notecraft_doc::{Block, Cell, Outline, Paragraph, Run, Tag};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Most characters one paragraph holds (hostile commands): longer insertions are refused.
pub const MAX_PARA_CHARS: usize = 1_000_000;

/// A caret position: paragraph (block) index and character index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TextPos {
    pub para: usize,
    pub char: usize,
}

impl TextPos {
    pub const fn new(para: usize, char: usize) -> TextPos {
        TextPos { para, char }
    }

    /// `{"para": n, "char": n}` or `[para, char]`.
    pub fn from_json(v: &Value) -> Option<TextPos> {
        let (para, char) = match v {
            Value::Object(_) => (json_index(v.get("para"))?, json_index(v.get("char"))),
            Value::Array(a) => (json_index(a.first())?, json_index(a.get(1))),
            _ => return None,
        };
        Some(TextPos::new(para, char.unwrap_or(0)))
    }

    pub fn to_json(self) -> Value {
        serde_json::json!({"para": self.para, "char": self.char})
    }
}

/// A JSON index: a non-negative integer, or a finite float (truncated); huge values saturate.
pub(crate) fn json_index(x: Option<&Value>) -> Option<usize> {
    let n = x.and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f as u64)))?;
    Some(usize::try_from(n).unwrap_or(usize::MAX))
}

/// A table cell in a note container: the table's block index, then row and column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CellRef {
    pub block: usize,
    pub row: usize,
    pub col: usize,
}

impl CellRef {
    pub const fn new(block: usize, row: usize, col: usize) -> CellRef {
        CellRef { block, row, col }
    }

    /// `{"block": n, "row": n, "col": n}` or `[block, row, col]`.
    pub fn from_json(v: &Value) -> Option<CellRef> {
        match v {
            Value::Object(_) => Some(CellRef::new(json_index(v.get("block"))?, json_index(v.get("row"))?, json_index(v.get("col"))?)),
            Value::Array(a) => Some(CellRef::new(json_index(a.first())?, json_index(a.get(1))?, json_index(a.get(2))?)),
            _ => None,
        }
    }

    pub fn to_json(self) -> Value {
        serde_json::json!({"block": self.block, "row": self.row, "col": self.col})
    }
}

/// Which flow of a note container positions are in: its own blocks, or one table cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Frame {
    #[default]
    Top,
    Cell(CellRef),
}

impl Frame {
    /// The `cell` param: absent or `null` for the container's own blocks.
    pub fn from_json(v: Option<&Value>) -> Option<Frame> {
        match v {
            None | Some(Value::Null) => Some(Frame::Top),
            Some(v) => CellRef::from_json(v).map(Frame::Cell),
        }
    }

    /// `null` or the cell.
    pub fn to_json(self) -> Value {
        match self {
            Frame::Top => Value::Null,
            Frame::Cell(c) => c.to_json(),
        }
    }

    pub fn cell(self) -> Option<CellRef> {
        match self {
            Frame::Top => None,
            Frame::Cell(c) => Some(c),
        }
    }
}

/// The flow `frame` names in `o` (`None` for a cell that doesn't exist).
pub fn flow(o: &Outline, frame: Frame) -> Option<&dyn Flow> {
    match frame {
        Frame::Top => Some(o),
        Frame::Cell(c) => o.table(c.block)?.cell(c.row, c.col).map(|c| c as &dyn Flow),
    }
}

/// [`flow`], mutably.
pub fn flow_mut(o: &mut Outline, frame: Frame) -> Option<&mut dyn Flow> {
    match frame {
        Frame::Top => Some(o),
        Frame::Cell(c) => o.table_mut(c.block)?.cell_mut(c.row, c.col).map(|c| c as &mut dyn Flow),
    }
}

/// A rectangle of cells in one table (inclusive bounds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellRange {
    pub block: usize,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

impl CellRange {
    /// The range from cell `a` to cell `b` (any order; `b`'s block is ignored).
    pub fn span(a: CellRef, b: CellRef) -> CellRange {
        CellRange { block: a.block, rows: (a.row.min(b.row), a.row.max(b.row)), cols: (a.col.min(b.col), a.col.max(b.col)) }
    }

    /// `{"block": n, "rows": [first, last], "cols": [first, last]}` (a single number is one row
    /// or column; bounds in any order).
    pub fn from_json(v: &Value) -> Option<CellRange> {
        let pair = |x: Option<&Value>| -> Option<(usize, usize)> {
            match x? {
                Value::Array(a) => {
                    let lo = json_index(a.first())?;
                    let hi = json_index(a.get(1)).unwrap_or(lo);
                    Some((lo.min(hi), lo.max(hi)))
                }
                n => json_index(Some(n)).map(|i| (i, i)),
            }
        };
        Some(CellRange { block: json_index(v.get("block"))?, rows: pair(v.get("rows"))?, cols: pair(v.get("cols"))? })
    }

    pub fn to_json(self) -> Value {
        serde_json::json!({"block": self.block, "rows": [self.rows.0, self.rows.1], "cols": [self.cols.0, self.cols.1]})
    }

    /// Clamped to a `rows` × `cols` table.
    pub fn clamped(self, rows: usize, cols: usize) -> CellRange {
        let (r, c) = (rows.saturating_sub(1), cols.saturating_sub(1));
        CellRange { block: self.block, rows: (self.rows.0.min(r), self.rows.1.min(r)), cols: (self.cols.0.min(c), self.cols.1.min(c)) }
    }

    /// Every cell, row by row.
    pub fn cells(self) -> impl Iterator<Item = CellRef> {
        let block = self.block;
        let cols = self.cols;
        (self.rows.0..=self.rows.1).flat_map(move |r| (cols.0..=cols.1).map(move |c| CellRef::new(block, r, c)))
    }

    pub fn contains(self, c: CellRef) -> bool {
        c.block == self.block && (self.rows.0..=self.rows.1).contains(&c.row) && (self.cols.0..=self.cols.1).contains(&c.col)
    }
}

/// Paragraphs the caret moves through: a note container's blocks or a table cell's paragraphs.
pub trait Flow {
    /// Blocks in the flow (a table counts as one).
    fn count(&self) -> usize;
    /// The paragraph at `i`; `None` for a table or past the end.
    fn para(&self, i: usize) -> Option<&Paragraph>;
    fn para_mut(&mut self, i: usize) -> Option<&mut Paragraph>;
    /// Inserts a paragraph at `i` (clamped).
    fn insert_para(&mut self, i: usize, p: Paragraph);
    /// Removes blocks `lo..hi` (clamped).
    fn remove_range(&mut self, lo: usize, hi: usize);
    /// Whether block `i` is a table.
    fn is_table(&self, i: usize) -> bool {
        i < self.count() && self.para(i).is_none()
    }
}

impl Flow for Vec<Paragraph> {
    fn count(&self) -> usize {
        self.len()
    }
    fn para(&self, i: usize) -> Option<&Paragraph> {
        self.get(i)
    }
    fn para_mut(&mut self, i: usize) -> Option<&mut Paragraph> {
        self.get_mut(i)
    }
    fn insert_para(&mut self, i: usize, p: Paragraph) {
        let i = i.min(self.len());
        self.insert(i, p);
    }
    fn remove_range(&mut self, lo: usize, hi: usize) {
        let hi = hi.min(self.len());
        self.drain(lo.min(hi)..hi);
    }
}

impl Flow for Vec<Block> {
    fn count(&self) -> usize {
        self.len()
    }
    fn para(&self, i: usize) -> Option<&Paragraph> {
        self.get(i).and_then(Block::as_paragraph)
    }
    fn para_mut(&mut self, i: usize) -> Option<&mut Paragraph> {
        self.get_mut(i).and_then(Block::as_paragraph_mut)
    }
    fn insert_para(&mut self, i: usize, p: Paragraph) {
        let i = i.min(self.len());
        self.insert(i, Block::Paragraph(p));
    }
    fn remove_range(&mut self, lo: usize, hi: usize) {
        let hi = hi.min(self.len());
        self.drain(lo.min(hi)..hi);
    }
}

impl Flow for Outline {
    fn count(&self) -> usize {
        self.blocks.count()
    }
    fn para(&self, i: usize) -> Option<&Paragraph> {
        self.blocks.para(i)
    }
    fn para_mut(&mut self, i: usize) -> Option<&mut Paragraph> {
        self.blocks.para_mut(i)
    }
    fn insert_para(&mut self, i: usize, p: Paragraph) {
        self.blocks.insert_para(i, p);
    }
    fn remove_range(&mut self, lo: usize, hi: usize) {
        self.blocks.remove_range(lo, hi);
    }
}

impl Flow for Cell {
    fn count(&self) -> usize {
        self.paragraphs.count()
    }
    fn para(&self, i: usize) -> Option<&Paragraph> {
        self.paragraphs.para(i)
    }
    fn para_mut(&mut self, i: usize) -> Option<&mut Paragraph> {
        self.paragraphs.para_mut(i)
    }
    fn insert_para(&mut self, i: usize, p: Paragraph) {
        self.paragraphs.insert_para(i, p);
    }
    fn remove_range(&mut self, lo: usize, hi: usize) {
        self.paragraphs.remove_range(lo, hi);
    }
}

/// Every paragraph of `o` in reading order with its flow and index there: the container's own,
/// and at each table its cells' (row by row).
pub fn all_paragraphs(o: &Outline) -> Vec<(Frame, usize, &Paragraph)> {
    let mut out = Vec::new();
    for (b, block) in o.blocks.iter().enumerate() {
        match block {
            Block::Paragraph(p) => out.push((Frame::Top, b, p)),
            Block::Table { table } => {
                for (r, row) in table.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        let f = Frame::Cell(CellRef::new(b, r, c));
                        out.extend(cell.paragraphs.iter().enumerate().map(|(i, p)| (f, i, p)));
                    }
                }
            }
        }
    }
    out
}

/// The paragraphs of blocks `lo..=hi` (tables skipped).
pub fn paras_in<F: Flow + ?Sized>(f: &F, lo: usize, hi: usize) -> Vec<&Paragraph> {
    (lo..=hi.min(f.count().saturating_sub(1))).filter_map(|i| f.para(i)).collect()
}

/// Applies `g` to the paragraphs of blocks `lo..=hi` (tables skipped).
pub fn for_paras_mut<F: Flow + ?Sized>(f: &mut F, lo: usize, hi: usize, g: &mut dyn FnMut(&mut Paragraph)) {
    for i in lo..=hi.min(f.count().saturating_sub(1)) {
        if let Some(p) = f.para_mut(i) {
            g(p);
        }
    }
}

/// Characters of the paragraph at `i` (0 for a table or past the end).
pub fn len_at<F: Flow + ?Sized>(f: &F, i: usize) -> usize {
    f.para(i).map_or(0, para_len)
}

/// Characters in a paragraph.
pub fn para_len(p: &Paragraph) -> usize {
    p.runs.iter().map(|r| r.text.chars().count()).sum()
}

/// `pos` moved inside `o`: an existing block (the last when past the end) and a character
/// index within it. A flow without blocks gets an empty paragraph.
pub fn clamp<F: Flow + ?Sized>(o: &mut F, pos: TextPos) -> TextPos {
    if o.count() == 0 {
        o.insert_para(0, Paragraph::plain(""));
    }
    clamp_ref(o, pos)
}

/// [`clamp`] without changing the flow (an empty flow clamps to 0:0).
pub fn clamp_ref<F: Flow + ?Sized>(o: &F, pos: TextPos) -> TextPos {
    let para = pos.para.min(o.count().saturating_sub(1));
    TextPos::new(para, pos.char.min(len_at(o, para)))
}

/// The end of the flow.
pub fn end_of<F: Flow + ?Sized>(o: &F) -> TextPos {
    let para = o.count().saturating_sub(1);
    TextPos::new(para, len_at(o, para))
}

/// The nearest paragraph to block `i` (itself, else the next, else the previous); `None` when
/// the flow holds only tables.
pub fn nearest_para<F: Flow + ?Sized>(o: &F, i: usize) -> Option<usize> {
    let n = o.count();
    let i = i.min(n.saturating_sub(1));
    (i..n).find(|&j| o.para(j).is_some()).or_else(|| (0..i).rev().find(|&j| o.para(j).is_some()))
}

/// Byte offset of character `ch` in `s` (the end when past it).
fn byte_at(s: &str, ch: usize) -> usize {
    s.char_indices().nth(ch).map_or(s.len(), |(b, _)| b)
}

/// Splits the run that straddles character `ch` so a run starts there; returns that run's index
/// (`runs.len()` at the end).
pub fn split_at(p: &mut Paragraph, ch: usize) -> usize {
    let mut seen = 0usize;
    let mut i = 0usize;
    while i < p.runs.len() {
        let Some(run) = p.runs.get(i) else { break };
        let n = run.text.chars().count();
        if ch == seen {
            return i;
        }
        if ch < seen + n {
            let b = byte_at(&run.text, ch - seen);
            let tail = Run { text: run.text.get(b..).unwrap_or("").to_owned(), ..run.clone() };
            if let Some(r) = p.runs.get_mut(i) {
                r.text.truncate(b);
            }
            p.runs.insert(i + 1, tail);
            return i + 1;
        }
        seen += n;
        i += 1;
    }
    p.runs.len()
}

/// Drops empty runs (keeping one, for its formatting, when the paragraph is empty) and merges
/// neighbours with the same formatting.
pub fn normalize(p: &mut Paragraph) {
    if p.runs.is_empty() {
        p.runs.push(Run::default());
        return;
    }
    if p.runs.iter().all(|r| r.text.is_empty()) {
        p.runs.truncate(1);
        return;
    }
    p.runs.retain(|r| !r.text.is_empty());
    let mut out: Vec<Run> = Vec::with_capacity(p.runs.len());
    for r in p.runs.drain(..) {
        match out.last_mut() {
            Some(last) if last.same_format(&r) => last.text.push_str(&r.text),
            _ => out.push(r),
        }
    }
    p.runs = out;
}

/// The formatting text typed at `ch` gets: the run before the caret (the first run at the
/// start). A link isn't carried past its end.
pub fn format_at(p: &Paragraph, ch: usize) -> Run {
    let mut seen = 0usize;
    let mut found: Option<(&Run, bool)> = None;
    for r in &p.runs {
        let n = r.text.chars().count();
        if ch > seen && ch <= seen + n {
            found = Some((r, ch == seen + n));
            break;
        }
        seen += n;
    }
    let (run, at_end) = found.or_else(|| p.runs.first().map(|r| (r, false))).unwrap_or((&EMPTY_RUN, false));
    let mut f = Run { text: String::new(), ..run.clone() };
    if at_end {
        f.link = None;
    }
    f
}

static EMPTY_RUN: Run = Run {
    text: String::new(),
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    highlight: None,
    color: None,
    font: None,
    size: None,
    link: None,
};

/// The character range `[a, b)` of paragraph `i` covered by `start..end`.
pub fn para_span<F: Flow + ?Sized>(o: &F, i: usize, start: TextPos, end: TextPos) -> (usize, usize) {
    let len = len_at(o, i);
    let a = if i == start.para { start.char.min(len) } else { 0 };
    let b = if i == end.para { end.char.min(len) } else { len };
    (a, b.max(a))
}

/// Applies `f` to every run (split as needed) between `start` and `end`; returns whether any
/// character was covered.
pub fn apply_range<F: Flow + ?Sized>(o: &mut F, start: TextPos, end: TextPos, f: &mut dyn FnMut(&mut Run)) -> bool {
    let mut any = false;
    for i in start.para..=end.para.min(o.count().saturating_sub(1)) {
        let (a, b) = para_span(o, i, start, end);
        let Some(p) = o.para_mut(i) else { continue };
        if a == b {
            continue;
        }
        let ia = split_at(p, a);
        let ib = split_at(p, b);
        for r in p.runs.get_mut(ia..ib).into_iter().flatten() {
            f(r);
            any = true;
        }
        normalize(p);
    }
    any
}

/// The runs (with their covered text) between `start` and `end`, without changing anything.
pub fn runs_in<F: Flow + ?Sized>(o: &F, start: TextPos, end: TextPos) -> Vec<Run> {
    let mut out = Vec::new();
    for i in start.para..=end.para.min(o.count().saturating_sub(1)) {
        let (a, b) = para_span(o, i, start, end);
        let Some(p) = o.para(i) else { continue };
        let mut seen = 0usize;
        for r in &p.runs {
            let n = r.text.chars().count();
            let (lo, hi) = (a.max(seen), b.min(seen + n));
            if lo < hi {
                let text: String = r.text.chars().skip(lo - seen).take(hi - lo).collect();
                out.push(Run { text, ..r.clone() });
            }
            seen += n;
        }
    }
    out
}

/// Inserts `text` (newlines start new paragraphs, which take the paragraph's list, indent and
/// style; a To Do tag starts unchecked) at `at` with formatting `fmt`. Returns the caret after
/// the inserted text. Nothing is inserted at a table block.
pub fn insert<F: Flow + ?Sized>(o: &mut F, at: TextPos, text: &str, fmt: &Run) -> TextPos {
    let at = clamp(o, at);
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or("").replace('\r', "");
    let rest: Vec<String> = lines.map(|l| l.replace('\r', "")).collect();
    let Some(p) = o.para_mut(at.para) else { return at };
    let i = split_at(p, at.char);
    let tail: Vec<Run> = if rest.is_empty() { Vec::new() } else { p.runs.drain(i..).collect() };
    if !first.is_empty() {
        p.runs.insert(i.min(p.runs.len()), Run { text: first.clone(), ..fmt.clone() });
    }
    if p.runs.is_empty() {
        p.runs.push(Run { text: String::new(), ..fmt.clone() });
    }
    normalize(p);
    if rest.is_empty() {
        return TextPos::new(at.para, at.char + first.chars().count());
    }
    let template = continuation(p);
    let mut para = at.para;
    let n = rest.len();
    for (k, line) in rest.into_iter().enumerate() {
        let mut np = template.clone();
        np.runs = vec![Run { text: line, ..fmt.clone() }];
        if k + 1 == n {
            np.runs.extend(tail.iter().cloned());
        }
        normalize(&mut np);
        para += 1;
        o.insert_para(para, np);
    }
    let last_line_len = o.para(para).map_or(0, |p| para_len(p).saturating_sub(tail.iter().map(|r| r.text.chars().count()).sum::<usize>()));
    TextPos::new(para, last_line_len)
}

/// A new paragraph following `p` (Enter): same list (and list style), indent, alignment and
/// style (a heading is followed by normal text), a To Do tag unchecked, no runs.
pub fn continuation(p: &Paragraph) -> Paragraph {
    let style =
        if p.style.heading_level().is_some() || p.style == notecraft_doc::ParaStyle::Title { notecraft_doc::ParaStyle::Normal } else { p.style };
    let tag = match p.tag {
        Some(Tag::ToDo { .. }) => Some(Tag::ToDo { checked: false }),
        other => other,
    };
    Paragraph { runs: Vec::new(), indent: p.indent, list: p.list, list_style: p.list_style, tag, style, align: p.align, lang: p.lang.clone() }
}

/// Deletes the text between `start` and `end` (merging the end paragraph into the start one,
/// and removing the blocks between, tables included); returns the caret (at `start`).
pub fn delete<F: Flow + ?Sized>(o: &mut F, start: TextPos, end: TextPos) -> TextPos {
    let (mut start, mut end) = (clamp(o, start), clamp(o, end));
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    if start == end {
        return start;
    }
    if o.para(start.para).is_none() {
        // A range starting on a table: it goes with the rest; the caret takes its place.
        let tail = o.para(end.para).map(|p| {
            let mut p = p.clone();
            let i = split_at(&mut p, end.char);
            p.runs.drain(..i);
            normalize(&mut p);
            p
        });
        o.remove_range(start.para, end.para.saturating_add(1));
        o.insert_para(start.para, tail.unwrap_or_else(|| Paragraph::plain("")));
        return TextPos::new(start.para, 0);
    }
    let tail: Vec<Run> = match o.para_mut(end.para) {
        Some(p) => {
            let i = split_at(p, end.char);
            p.runs.drain(i..).collect()
        }
        None => Vec::new(),
    };
    let Some(p) = o.para_mut(start.para) else { return start };
    let i = split_at(p, start.char);
    let removed_fmt = p.runs.get(i).map(|r| Run { text: String::new(), ..r.clone() });
    p.runs.truncate(i);
    p.runs.extend(tail);
    if p.runs.is_empty() {
        p.runs.push(removed_fmt.unwrap_or_default());
    }
    normalize(p);
    if end.para > start.para {
        o.remove_range(start.para.saturating_add(1), end.para.saturating_add(1));
    }
    start
}

/// Splits the paragraph at `at` (Enter). An empty list or To Do paragraph loses its bullet or
/// tag instead, as in word processors. Returns the caret.
pub fn split_paragraph<F: Flow + ?Sized>(o: &mut F, at: TextPos) -> TextPos {
    let at = clamp(o, at);
    let Some(p) = o.para_mut(at.para) else { return at };
    if para_len(p) == 0 && (p.list.is_some() || p.tag.is_some()) {
        p.list = None;
        p.tag = None;
        return at;
    }
    let fmt = format_at(p, at.char);
    let i = split_at(p, at.char);
    let tail: Vec<Run> = p.runs.drain(i..).collect();
    if p.runs.is_empty() {
        p.runs.push(fmt.clone());
    }
    normalize(p);
    let mut np = continuation(p);
    // Splitting a heading in the middle keeps both halves headings.
    if !tail.is_empty() {
        np.style = p.style;
    }
    np.runs = if tail.is_empty() { vec![fmt] } else { tail };
    normalize(&mut np);
    let next = at.para.saturating_add(1).min(o.count());
    o.insert_para(next, np);
    TextPos::new(next, 0)
}

/// Joins paragraph `para` with the next one; returns the caret at the join. `None` when either
/// is missing or is a table.
pub fn merge_paragraphs<F: Flow + ?Sized>(o: &mut F, para: usize) -> Option<TextPos> {
    let next_i = para.checked_add(1).filter(|n| *n < o.count())?;
    o.para(para)?;
    let next = o.para(next_i)?.clone();
    o.remove_range(next_i, next_i + 1);
    let p = o.para_mut(para)?;
    let at = TextPos::new(para, para_len(p));
    let was_empty = at.char == 0;
    if was_empty {
        p.runs.clear();
    }
    p.runs.extend(next.runs);
    normalize(p);
    Some(at)
}

/// The word around `at` (letters, digits, `_` and apostrophes), or `None` when the caret isn't
/// inside one.
pub fn word_at<F: Flow + ?Sized>(o: &F, at: TextPos) -> Option<(TextPos, TextPos)> {
    let p = o.para(at.para)?;
    let chars: Vec<char> = p.text().chars().collect();
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_' || *c == '\'';
    let ch = at.char.min(chars.len());
    let left = ch.checked_sub(1).and_then(|i| chars.get(i)).is_some_and(is_word);
    let right = chars.get(ch).is_some_and(is_word);
    // Inside a word only: at its edge the format applies to what is typed next.
    if !(left && right) {
        return None;
    }
    let mut a = ch;
    while a > 0 && chars.get(a - 1).is_some_and(is_word) {
        a -= 1;
    }
    let mut b = ch;
    while chars.get(b).is_some_and(is_word) {
        b += 1;
    }
    Some((TextPos::new(at.para, a), TextPos::new(at.para, b)))
}

#[cfg(test)]
mod tests {
    use notecraft_doc::{ListKind, ParaStyle, Point, Table};

    use super::*;

    fn outline(text: &str) -> Outline {
        Outline::with_text(Point::ZERO, text)
    }

    fn p(o: &Outline, i: usize) -> &Paragraph {
        o.para(i).unwrap()
    }

    #[test]
    fn insert_and_delete_across_paragraphs() {
        let mut o = outline("héllo\nworld");
        let bold = Run { bold: true, ..Run::default() };
        let c = insert(&mut o, TextPos::new(0, 2), "XY", &bold);
        assert_eq!(c, TextPos::new(0, 4));
        assert_eq!(p(&o, 0).text(), "héXYllo");
        assert_eq!(p(&o, 0).runs.len(), 3);
        assert!(p(&o, 0).runs[1].bold);
        let c = insert(&mut o, TextPos::new(0, 1), "a\nb\nc", &Run::default());
        assert_eq!(o.text(), "ha\nb\ncéXYllo\nworld");
        assert_eq!(c, TextPos::new(2, 1));
        let c = delete(&mut o, TextPos::new(0, 1), TextPos::new(2, 1));
        assert_eq!(c, TextPos::new(0, 1));
        assert_eq!(o.text(), "héXYllo\nworld");
        // Reversed, out-of-range positions clamp.
        delete(&mut o, TextPos::new(99, 99), TextPos::new(0, 7));
        assert_eq!(o.text(), "héXYllo");
        delete(&mut o, TextPos::new(0, 0), TextPos::new(0, 99));
        assert_eq!(o.text(), "");
        assert_eq!(p(&o, 0).runs.len(), 1, "an empty paragraph keeps a run");
    }

    #[test]
    fn split_merge_and_continuations() {
        let mut o = outline("ab");
        o.para_mut(0).unwrap().list = Some(ListKind::Bullet);
        o.para_mut(0).unwrap().tag = Some(Tag::ToDo { checked: true });
        let c = split_paragraph(&mut o, TextPos::new(0, 2));
        assert_eq!(c, TextPos::new(1, 0));
        assert_eq!(p(&o, 1).list, Some(ListKind::Bullet));
        assert_eq!(p(&o, 1).tag, Some(Tag::ToDo { checked: false }));
        // Enter on the empty bullet ends the list.
        split_paragraph(&mut o, c);
        assert_eq!(o.blocks.len(), 2);
        assert_eq!((p(&o, 1).list, p(&o, 1).tag), (None, None));
        assert_eq!(merge_paragraphs(&mut o, 0), Some(TextPos::new(0, 2)));
        assert_eq!(o.blocks.len(), 1);
        assert_eq!(merge_paragraphs(&mut o, 0), None);
        let mut h = outline("Title");
        h.para_mut(0).unwrap().style = ParaStyle::Heading1;
        split_paragraph(&mut h, TextPos::new(0, 5));
        assert_eq!(p(&h, 1).style, ParaStyle::Normal);
        split_paragraph(&mut h, TextPos::new(0, 2));
        assert_eq!(p(&h, 1).style, ParaStyle::Heading1);
        assert_eq!(h.text(), "Ti\ntle\n");
    }

    #[test]
    fn apply_and_inspect_ranges() {
        let mut o = outline("one two\nthree");
        let any = apply_range(&mut o, TextPos::new(0, 4), TextPos::new(1, 2), &mut |r| r.italic = true);
        assert!(any);
        let runs = runs_in(&o, TextPos::new(0, 0), end_of(&o));
        assert_eq!(
            runs.iter().map(|r| (r.text.as_str(), r.italic)).collect::<Vec<_>>(),
            vec![("one ", false), ("two", true), ("th", true), ("ree", false)]
        );
        assert_eq!(word_at(&o, TextPos::new(0, 5)), Some((TextPos::new(0, 4), TextPos::new(0, 7))));
        assert_eq!(word_at(&o, TextPos::new(0, 3)), None, "at the end of a word");
        assert_eq!(word_at(&outline("a  b"), TextPos::new(0, 2)), None);
        assert!(format_at(p(&o, 0), 5).italic);
        assert!(!format_at(p(&o, 0), 0).italic);
        assert_eq!(TextPos::from_json(&serde_json::json!({"para": 2, "char": 3})), Some(TextPos::new(2, 3)));
        assert_eq!(TextPos::from_json(&serde_json::json!([1, 1e30])), Some(TextPos::new(1, usize::MAX)));
        assert_eq!(TextPos::from_json(&serde_json::json!("x")), None);
        assert_eq!(CellRef::from_json(&serde_json::json!([1, 2, 3])), Some(CellRef::new(1, 2, 3)));
        assert_eq!(CellRef::from_json(&serde_json::json!({"block": 1, "row": 2})), None);
        assert_eq!(CellRef::from_json(&CellRef::new(4, 5, 6).to_json()), Some(CellRef::new(4, 5, 6)));
    }

    #[test]
    fn tables_are_solid_blocks() {
        let mut o = outline("before\nafter");
        o.blocks.insert(1, Block::table(Table::new(2, 2, 72.0)));
        // Nothing is typed into a table block, and paragraphs don't merge with it.
        assert_eq!(insert(&mut o, TextPos::new(1, 0), "x", &Run::default()), TextPos::new(1, 0));
        assert_eq!(merge_paragraphs(&mut o, 0), None);
        assert_eq!(merge_paragraphs(&mut o, 1), None);
        assert_eq!(split_paragraph(&mut o, TextPos::new(1, 0)), TextPos::new(1, 0));
        assert_eq!(nearest_para(&o, 1), Some(2));
        assert!(o.is_table(1) && !o.is_table(0) && !o.is_table(9));
        // Formatting across it skips it.
        let end = end_of(&o);
        apply_range(&mut o, TextPos::new(0, 0), end, &mut |r| r.bold = true);
        assert!(o.table(1).unwrap().paragraphs().all(|p| p.runs.iter().all(|r| !r.bold)));
        // A deletion that spans it removes it.
        let c = delete(&mut o, TextPos::new(0, 3), TextPos::new(2, 2));
        assert_eq!(c, TextPos::new(0, 3));
        assert_eq!(o.text(), "befter");
        assert!(!o.has_tables());
        // One that starts on it removes it too, keeping the end's tail.
        let mut o = outline("x\nyz");
        o.blocks.insert(0, Block::table(Table::new(1, 1, 72.0)));
        assert_eq!(delete(&mut o, TextPos::new(0, 0), TextPos::new(2, 1)), TextPos::new(0, 0));
        assert_eq!(o.text(), "z");
        // Cells are flows too.
        let mut cell = Cell::default();
        let c = insert(&mut cell, TextPos::new(0, 0), "a\nb", &Run::default());
        assert_eq!((c, cell.text()), (TextPos::new(1, 1), "a\nb".to_owned()));
        let only_table = Outline { pos: Point::ZERO, width: 100.0, blocks: vec![Block::table(Table::new(1, 1, 72.0))] };
        assert_eq!(nearest_para(&only_table, 0), None);
    }
}
