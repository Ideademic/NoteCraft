//! Rich-text editing on the model: caret positions, splitting and merging runs, inserting and
//! deleting text across paragraphs, applying formatting to a character range.
//!
//! A position is a paragraph index and a **character** index (Unicode scalar values, as egui's
//! `CCursor` counts them) into that paragraph's text (its runs joined). Every function clamps
//! positions to the outline, so hostile values never index out of bounds.

use notecraft_doc::{Outline, Paragraph, Run, Tag};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Most characters one paragraph holds (hostile commands): longer insertions are refused.
pub const MAX_PARA_CHARS: usize = 1_000_000;

/// A caret position: paragraph index and character index.
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
        let n = |x: Option<&Value>| x.and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f as u64)));
        let to = |x: u64| usize::try_from(x).unwrap_or(usize::MAX);
        match v {
            Value::Object(_) => Some(TextPos::new(to(n(v.get("para"))?), to(n(v.get("char")).unwrap_or(0)))),
            Value::Array(a) => Some(TextPos::new(to(n(a.first())?), to(n(a.get(1)).unwrap_or(0)))),
            _ => None,
        }
    }

    pub fn to_json(self) -> Value {
        serde_json::json!({"para": self.para, "char": self.char})
    }
}

/// Characters in a paragraph.
pub fn para_len(p: &Paragraph) -> usize {
    p.runs.iter().map(|r| r.text.chars().count()).sum()
}

/// `pos` moved inside `o`: an existing paragraph (the last when past the end) and a character
/// index within it. An outline without paragraphs gets one.
pub fn clamp(o: &mut Outline, pos: TextPos) -> TextPos {
    if o.paragraphs.is_empty() {
        o.paragraphs.push(Paragraph::plain(""));
    }
    let para = pos.para.min(o.paragraphs.len().saturating_sub(1));
    let len = o.paragraphs.get(para).map_or(0, para_len);
    TextPos::new(para, pos.char.min(len))
}

/// [`clamp`] without changing the outline (an empty outline clamps to 0:0).
pub fn clamp_ref(o: &Outline, pos: TextPos) -> TextPos {
    let para = pos.para.min(o.paragraphs.len().saturating_sub(1));
    let len = o.paragraphs.get(para).map_or(0, para_len);
    TextPos::new(para, pos.char.min(len))
}

/// The end of the outline.
pub fn end_of(o: &Outline) -> TextPos {
    let para = o.paragraphs.len().saturating_sub(1);
    TextPos::new(para, o.paragraphs.get(para).map_or(0, para_len))
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
pub fn para_span(o: &Outline, i: usize, start: TextPos, end: TextPos) -> (usize, usize) {
    let len = o.paragraphs.get(i).map_or(0, para_len);
    let a = if i == start.para { start.char.min(len) } else { 0 };
    let b = if i == end.para { end.char.min(len) } else { len };
    (a, b.max(a))
}

/// Applies `f` to every run (split as needed) between `start` and `end`; returns whether any
/// character was covered.
pub fn apply_range(o: &mut Outline, start: TextPos, end: TextPos, f: &mut dyn FnMut(&mut Run)) -> bool {
    let mut any = false;
    for i in start.para..=end.para.min(o.paragraphs.len().saturating_sub(1)) {
        let (a, b) = para_span(o, i, start, end);
        let Some(p) = o.paragraphs.get_mut(i) else { continue };
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
pub fn runs_in(o: &Outline, start: TextPos, end: TextPos) -> Vec<Run> {
    let mut out = Vec::new();
    for i in start.para..=end.para.min(o.paragraphs.len().saturating_sub(1)) {
        let (a, b) = para_span(o, i, start, end);
        let Some(p) = o.paragraphs.get(i) else { continue };
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
/// the inserted text.
pub fn insert(o: &mut Outline, at: TextPos, text: &str, fmt: &Run) -> TextPos {
    let at = clamp(o, at);
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or("").replace('\r', "");
    let rest: Vec<String> = lines.map(|l| l.replace('\r', "")).collect();
    let Some(p) = o.paragraphs.get_mut(at.para) else { return at };
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
        o.paragraphs.insert(para.min(o.paragraphs.len()), np);
    }
    let last_line_len = o.paragraphs.get(para).map_or(0, |p| para_len(p).saturating_sub(tail.iter().map(|r| r.text.chars().count()).sum::<usize>()));
    TextPos::new(para, last_line_len)
}

/// A new paragraph following `p` (Enter): same list (and list style), indent, alignment and
/// style (a heading is followed by normal text), a To Do tag unchecked, no runs.
fn continuation(p: &Paragraph) -> Paragraph {
    let style =
        if p.style.heading_level().is_some() || p.style == notecraft_doc::ParaStyle::Title { notecraft_doc::ParaStyle::Normal } else { p.style };
    let tag = match p.tag {
        Some(Tag::ToDo { .. }) => Some(Tag::ToDo { checked: false }),
        other => other,
    };
    Paragraph { runs: Vec::new(), indent: p.indent, list: p.list, list_style: p.list_style, tag, style, align: p.align }
}

/// Deletes the text between `start` and `end` (merging the end paragraph into the start one);
/// returns the caret (at `start`).
pub fn delete(o: &mut Outline, start: TextPos, end: TextPos) -> TextPos {
    let (mut start, mut end) = (clamp(o, start), clamp(o, end));
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    if start == end {
        return start;
    }
    let tail: Vec<Run> = match o.paragraphs.get_mut(end.para) {
        Some(p) => {
            let i = split_at(p, end.char);
            p.runs.drain(i..).collect()
        }
        None => Vec::new(),
    };
    let Some(p) = o.paragraphs.get_mut(start.para) else { return start };
    let i = split_at(p, start.char);
    let removed_fmt = p.runs.get(i).map(|r| Run { text: String::new(), ..r.clone() });
    p.runs.truncate(i);
    p.runs.extend(tail);
    if p.runs.is_empty() {
        p.runs.push(removed_fmt.unwrap_or_default());
    }
    normalize(p);
    if end.para > start.para {
        let lo = start.para.saturating_add(1).min(o.paragraphs.len());
        let hi = end.para.saturating_add(1).min(o.paragraphs.len());
        o.paragraphs.drain(lo..hi);
    }
    start
}

/// Splits the paragraph at `at` (Enter). An empty list or To Do paragraph loses its bullet or
/// tag instead, as in word processors. Returns the caret.
pub fn split_paragraph(o: &mut Outline, at: TextPos) -> TextPos {
    let at = clamp(o, at);
    let Some(p) = o.paragraphs.get_mut(at.para) else { return at };
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
    let next = at.para.saturating_add(1).min(o.paragraphs.len());
    o.paragraphs.insert(next, np);
    TextPos::new(next, 0)
}

/// Joins paragraph `para` with the next one; returns the caret at the join.
pub fn merge_paragraphs(o: &mut Outline, para: usize) -> Option<TextPos> {
    let next_i = para.checked_add(1).filter(|n| *n < o.paragraphs.len())?;
    let next = o.paragraphs.remove(next_i);
    let p = o.paragraphs.get_mut(para)?;
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
pub fn word_at(o: &Outline, at: TextPos) -> Option<(TextPos, TextPos)> {
    let p = o.paragraphs.get(at.para)?;
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
    use notecraft_doc::{ListKind, ParaStyle, Point};

    use super::*;

    fn outline(text: &str) -> Outline {
        Outline::with_text(Point::ZERO, text)
    }

    #[test]
    fn insert_and_delete_across_paragraphs() {
        let mut o = outline("héllo\nworld");
        let bold = Run { bold: true, ..Run::default() };
        let c = insert(&mut o, TextPos::new(0, 2), "XY", &bold);
        assert_eq!(c, TextPos::new(0, 4));
        assert_eq!(o.paragraphs[0].text(), "héXYllo");
        assert_eq!(o.paragraphs[0].runs.len(), 3);
        assert!(o.paragraphs[0].runs[1].bold);
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
        assert_eq!(o.paragraphs[0].runs.len(), 1, "an empty paragraph keeps a run");
    }

    #[test]
    fn split_merge_and_continuations() {
        let mut o = outline("ab");
        o.paragraphs[0].list = Some(ListKind::Bullet);
        o.paragraphs[0].tag = Some(Tag::ToDo { checked: true });
        let c = split_paragraph(&mut o, TextPos::new(0, 2));
        assert_eq!(c, TextPos::new(1, 0));
        assert_eq!(o.paragraphs[1].list, Some(ListKind::Bullet));
        assert_eq!(o.paragraphs[1].tag, Some(Tag::ToDo { checked: false }));
        // Enter on the empty bullet ends the list.
        split_paragraph(&mut o, c);
        assert_eq!(o.paragraphs.len(), 2);
        assert_eq!((o.paragraphs[1].list, o.paragraphs[1].tag), (None, None));
        assert_eq!(merge_paragraphs(&mut o, 0), Some(TextPos::new(0, 2)));
        assert_eq!(o.paragraphs.len(), 1);
        assert_eq!(merge_paragraphs(&mut o, 0), None);
        let mut h = outline("Title");
        h.paragraphs[0].style = ParaStyle::Heading1;
        split_paragraph(&mut h, TextPos::new(0, 5));
        assert_eq!(h.paragraphs[1].style, ParaStyle::Normal);
        split_paragraph(&mut h, TextPos::new(0, 2));
        assert_eq!(h.paragraphs[1].style, ParaStyle::Heading1);
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
        assert!(format_at(&o.paragraphs[0], 5).italic);
        assert!(!format_at(&o.paragraphs[0], 0).italic);
        assert_eq!(TextPos::from_json(&serde_json::json!({"para": 2, "char": 3})), Some(TextPos::new(2, 3)));
        assert_eq!(TextPos::from_json(&serde_json::json!([1, 1e30])), Some(TextPos::new(1, usize::MAX)));
        assert_eq!(TextPos::from_json(&serde_json::json!("x")), None);
    }
}
