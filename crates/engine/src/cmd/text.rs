//! Fine-grained typing in a note container: insert, delete, Enter (split), join paragraphs,
//! and a query for the formatting at the caret or selection (the ribbon's toggle states).
//!
//! # The selection model
//! Commands that act on text name the outline and a range:
//!
//! ```json
//! {"page"?: guid, "outline": guid, "start": {"para": 0, "char": 3}, "end": {"para": 1, "char": 0}}
//! ```
//!
//! `char` counts Unicode scalar values (egui's `CCursor`), `para` indexes the outline's
//! paragraphs. Positions are clamped to the outline, `start` and `end` may be in either order,
//! and `end` defaults to `start`. Also accepted: `"id"` for `"outline"`; `"at"` for a caret;
//! `"para": n` (that whole paragraph); `"paras": [first, last]`; the older
//! `{"para": n, "start": c0, "end": c1}`. No range means the whole outline.

use notecraft_doc::page::MAX_PARAGRAPHS;
use notecraft_doc::{Color, Guid, Outline, Run};
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::{TEXT_DELETE, TEXT_FORMAT_AT, TEXT_INSERT, TEXT_MERGE_PARAGRAPHS, TEXT_SPLIT_PARAGRAPH};
use crate::text::{self, MAX_PARA_CHARS, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Most characters inserted by one command.
pub const MAX_INSERT_CHARS: usize = 1_000_000;
/// Largest font size, points.
pub const MAX_FONT_SIZE: f32 = 400.0;
/// Smallest font size, points.
pub const MIN_FONT_SIZE: f32 = 1.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(TEXT_INSERT, "Insert Text", insert)
            .when(has_page)
            .params(r##"{"outline": guid, "at"?: {"para","char"} (default: end), "start"?/"end"?: pos (typing over a selection), "text": string, "style"?: {"bold"?, "italic"?, "underline"?, "strike"?: bool, "highlight"?, "color"?: "#rrggbb"|null, "font"?: string|null, "size"?: number|null, "link"?: string|null}, "mergeKey"?: string, "page"?: guid}"##),
        CommandSpec::new(TEXT_DELETE, "Delete Text", delete)
            .when(has_page)
            .params(r##"{"outline": guid, "start": pos, "end": pos} | {"outline": guid, "at": pos, "backward"?: bool (default true), "unit"?: "char"|"word", "count"?: number}, "mergeKey"?: string, "page"?: guid"##),
        CommandSpec::new(TEXT_SPLIT_PARAGRAPH, "New Paragraph", split)
            .when(has_page)
            .params(r##"{"outline": guid, "at"?: pos (default: end), "start"?/"end"?: pos (replaced first), "mergeKey"?: string, "page"?: guid}"##),
        CommandSpec::new(TEXT_MERGE_PARAGRAPHS, "Join Paragraphs", merge)
            .when(has_page)
            .params(r##"{"outline": guid, "para": number (joined with the next one), "mergeKey"?: string, "page"?: guid}"##),
        CommandSpec::new(TEXT_FORMAT_AT, "Formatting at Selection", format_at)
            .pure()
            .when(has_page)
            .params(r##"{"outline": guid, "start"?: pos, "end"?: pos, "page"?: guid}"##),
    ]
}

// --- the selection model -------------------------------------------------------------------

/// `params.outline`, else `params.id`.
pub(crate) fn outline_id(params: &Value) -> Result<Guid, CmdError> {
    match p::guid(params, "outline")? {
        Some(id) => Ok(id),
        None => p::guid(params, "id")?.ok_or_else(|| CmdError::Params("`outline` (GUID) is required".into())),
    }
}

/// The range a command names (see the module docs), ordered and clamped to `o`. `None` when no
/// range was given.
pub(crate) fn range(params: &Value, o: &Outline) -> Option<(TextPos, TextPos)> {
    let (a, b) = raw_range(params)?;
    let (a, b) = (text::clamp_ref(o, a), text::clamp_ref(o, b));
    Some(if b < a { (b, a) } else { (a, b) })
}

/// The range, or the whole outline.
pub(crate) fn range_or_all(params: &Value, o: &Outline) -> (TextPos, TextPos) {
    range(params, o).unwrap_or((TextPos::default(), text::end_of(o)))
}

fn raw_range(params: &Value) -> Option<(TextPos, TextPos)> {
    if let Some(start) = params.get("start").and_then(TextPos::from_json) {
        let end = params.get("end").and_then(TextPos::from_json).unwrap_or(start);
        return Some((start, end));
    }
    if let Some(at) = params.get("at").and_then(TextPos::from_json) {
        return Some((at, at));
    }
    let para = p::index(params, "para");
    if let Some(c0) = p::index(params, "start") {
        let para = para.unwrap_or(0);
        let c1 = p::index(params, "end").unwrap_or(c0);
        return Some((TextPos::new(para, c0), TextPos::new(para, c1)));
    }
    if let Some(arr) = params.get("paras").and_then(Value::as_array) {
        let n = |i: usize| arr.get(i).and_then(Value::as_u64).map(|n| usize::try_from(n).unwrap_or(usize::MAX));
        let first = n(0)?;
        let last = n(1).unwrap_or(first);
        let (first, last) = (first.min(last), first.max(last));
        return Some((TextPos::new(first, 0), TextPos::new(last, usize::MAX)));
    }
    para.map(|i| (TextPos::new(i, 0), TextPos::new(i, usize::MAX)))
}

/// Runs `f` on the named outline of the target page; touches the page when it changed.
pub(crate) fn with_outline(s: &mut Session, params: &Value, f: impl FnOnce(&mut Outline) -> CmdResult) -> CmdResult {
    let id = outline_id(params)?;
    let page = target_page(s, params)?;
    let outline = page.outline_mut(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    if outline.paragraphs.is_empty() {
        outline.paragraphs.push(notecraft_doc::Paragraph::plain(""));
    }
    let out = f(outline)?;
    if out.changed {
        page.touch();
    }
    Ok(out)
}

fn caret(c: TextPos) -> CmdOutput {
    CmdOutput::value(json!({"caret": c.to_json()}))
}

// --- run formatting from JSON ------------------------------------------------------------------

/// A font name: trimmed, controls dropped, at most 64 characters.
pub(crate) fn clean_font(s: &str) -> Option<String> {
    let f: String = s.trim().chars().filter(|c| !c.is_control()).take(64).collect();
    (!f.is_empty()).then_some(f)
}

pub(crate) fn clamp_size(v: f32) -> f32 {
    v.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// `base` with the formatting keys present in `style` applied.
pub(crate) fn apply_style(style: &Value, mut base: Run) -> Result<Run, CmdError> {
    if !style.is_object() {
        return Err(CmdError::Params("`style` must be an object".into()));
    }
    for (k, field) in [("bold", &mut base.bold), ("italic", &mut base.italic), ("underline", &mut base.underline), ("strike", &mut base.strike)] {
        if let Some(b) = p::bool(style, k) {
            *field = b;
        }
    }
    if style.get("highlight").is_some() {
        base.highlight = p::color(style, "highlight")?;
    }
    if style.get("color").is_some() {
        base.color = p::color(style, "color")?;
    }
    if let Some(f) = style.get("font") {
        base.font = f.as_str().and_then(clean_font);
    }
    if let Some(sz) = style.get("size") {
        base.size = sz.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).map(clamp_size);
    }
    if let Some(l) = style.get("link") {
        base.link = l.as_str().map(|l| l.trim().chars().filter(|c| !c.is_control()).take(2048).collect::<String>()).filter(|l| !l.is_empty());
    }
    Ok(base)
}

// --- commands --------------------------------------------------------------------------------

fn insert(s: &mut Session, params: &Value) -> CmdResult {
    let txt = p::req_str(params, "text")?;
    let n = txt.chars().count();
    if n > MAX_INSERT_CHARS {
        return Err(CmdError::Params(format!("at most {MAX_INSERT_CHARS} characters at once")));
    }
    let lines = txt.matches('\n').count();
    with_outline(s, params, |o| {
        if o.paragraphs.len().saturating_add(lines) > MAX_PARAGRAPHS {
            return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
        }
        let (start, end) = range(params, o).unwrap_or_else(|| {
            let e = text::end_of(o);
            (e, e)
        });
        let at = text::delete(o, start, end);
        if o.paragraphs.get(at.para).map_or(0, text::para_len).saturating_add(n) > MAX_PARA_CHARS {
            return Err(CmdError::Failed("this paragraph is as long as it can be".into()));
        }
        let base = o.paragraphs.get(at.para).map(|pg| text::format_at(pg, at.char)).unwrap_or_default();
        let fmt = match params.get("style") {
            Some(st) if !st.is_null() => apply_style(st, base)?,
            _ => base,
        };
        if txt.is_empty() && start == end {
            return Ok(CmdOutput::query(json!({"caret": at.to_json()})));
        }
        Ok(caret(text::insert(o, at, txt, &fmt)))
    })
}

fn delete(s: &mut Session, params: &Value) -> CmdResult {
    with_outline(s, params, |o| {
        if params.get("start").is_some() {
            let (a, b) = range(params, o).ok_or_else(|| CmdError::Params("`start` must be {\"para\", \"char\"}".into()))?;
            if a == b {
                return Ok(CmdOutput::query(json!({"caret": a.to_json()})));
            }
            return Ok(caret(text::delete(o, a, b)));
        }
        let at = params.get("at").and_then(TextPos::from_json).ok_or_else(|| CmdError::Params("`start`/`end` or `at` is required".into()))?;
        let at = text::clamp(o, at);
        let backward = p::bool(params, "backward").unwrap_or(true);
        let count = p::index(params, "count").unwrap_or(1).clamp(1, MAX_INSERT_CHARS);
        let word = p::str(params, "unit") == Some("word");
        delete_step(o, at, backward, count, word)
    })
}

/// Backspace / Delete from a caret: characters (or a word), joining paragraphs at their edges.
/// Backspace at the start of a list paragraph removes the bullet first, then the indent.
fn delete_step(o: &mut Outline, at: TextPos, backward: bool, count: usize, word: bool) -> CmdResult {
    let len = o.paragraphs.get(at.para).map_or(0, text::para_len);
    if backward {
        if at.char == 0 {
            let Some(pg) = o.paragraphs.get_mut(at.para) else { return Ok(CmdOutput::unchanged()) };
            if pg.list.is_some() || pg.tag.is_some() {
                pg.list = None;
                pg.tag = None;
                return Ok(caret(at));
            }
            if pg.indent > 0 {
                pg.indent -= 1;
                return Ok(caret(at));
            }
            let Some(prev) = at.para.checked_sub(1) else { return Ok(CmdOutput::query(json!({"caret": at.to_json()}))) };
            return Ok(text::merge_paragraphs(o, prev).map_or_else(CmdOutput::unchanged, caret));
        }
        let from = if word { word_start(o, at) } else { TextPos::new(at.para, at.char.saturating_sub(count)) };
        return Ok(caret(text::delete(o, from, at)));
    }
    if at.char >= len {
        return Ok(text::merge_paragraphs(o, at.para).map_or_else(|| CmdOutput::query(json!({"caret": at.to_json()})), caret));
    }
    let to = if word { word_end(o, at) } else { TextPos::new(at.para, at.char.saturating_add(count).min(len)) };
    Ok(caret(text::delete(o, at, to)))
}

fn para_chars(o: &Outline, para: usize) -> Vec<char> {
    o.paragraphs.get(para).map(|p| p.text().chars().collect()).unwrap_or_default()
}

/// Where Ctrl+Backspace deletes back to: over spaces, then over the word.
fn word_start(o: &Outline, at: TextPos) -> TextPos {
    let chars = para_chars(o, at.para);
    let mut i = at.char.min(chars.len());
    while i > 0 && chars.get(i - 1).is_some_and(|c| c.is_whitespace()) {
        i -= 1;
    }
    let word = chars.get(i.wrapping_sub(1)).is_some_and(|c| c.is_alphanumeric());
    while i > 0 && chars.get(i - 1).is_some_and(|c| !c.is_whitespace() && (c.is_alphanumeric() == word)) {
        i -= 1;
    }
    TextPos::new(at.para, i)
}

/// Where Ctrl+Delete deletes up to: over the word, then over spaces.
fn word_end(o: &Outline, at: TextPos) -> TextPos {
    let chars = para_chars(o, at.para);
    let mut i = at.char.min(chars.len());
    let word = chars.get(i).is_some_and(|c| c.is_alphanumeric());
    while chars.get(i).is_some_and(|c| !c.is_whitespace() && (c.is_alphanumeric() == word)) {
        i += 1;
    }
    while chars.get(i).is_some_and(|c| c.is_whitespace()) {
        i += 1;
    }
    TextPos::new(at.para, i)
}

fn split(s: &mut Session, params: &Value) -> CmdResult {
    with_outline(s, params, |o| {
        if o.paragraphs.len() >= MAX_PARAGRAPHS {
            return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
        }
        let (a, b) = range(params, o).unwrap_or_else(|| {
            let e = text::end_of(o);
            (e, e)
        });
        let at = text::delete(o, a, b);
        Ok(caret(text::split_paragraph(o, at)))
    })
}

fn merge(s: &mut Session, params: &Value) -> CmdResult {
    let para = p::index(params, "para").ok_or_else(|| CmdError::Params("`para` (number) is required".into()))?;
    with_outline(s, params, |o| Ok(text::merge_paragraphs(o, para).map_or_else(CmdOutput::unchanged, caret)))
}

/// A value that is the same for every run, or `"mixed"`.
fn common<T: PartialEq + Clone>(vals: impl Iterator<Item = T>, to_json: impl Fn(T) -> Value) -> Value {
    let mut first: Option<T> = None;
    for v in vals {
        match &first {
            None => first = Some(v),
            Some(f) if *f == v => {}
            Some(_) => return Value::String("mixed".into()),
        }
    }
    first.map_or(Value::Null, to_json)
}

fn format_at(s: &mut Session, params: &Value) -> CmdResult {
    let id = outline_id(params)?;
    let page = target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let (a, b) = range(params, o).unwrap_or((TextPos::default(), TextPos::default()));
    let runs: Vec<Run> =
        if a == b { o.paragraphs.get(a.para).map(|pg| text::format_at(pg, a.char)).into_iter().collect() } else { text::runs_in(o, a, b) };
    let paras: Vec<&notecraft_doc::Paragraph> =
        o.paragraphs.get(a.para..=b.para.min(o.paragraphs.len().saturating_sub(1))).map(|s| s.iter().collect()).unwrap_or_default();
    let color = |c: Option<Color>| c.map_or(Value::Null, |c| Value::String(c.to_hex()));
    let v = json!({
        "bold": common(runs.iter().map(|r| r.bold), Value::Bool),
        "italic": common(runs.iter().map(|r| r.italic), Value::Bool),
        "underline": common(runs.iter().map(|r| r.underline), Value::Bool),
        "strike": common(runs.iter().map(|r| r.strike), Value::Bool),
        "highlight": common(runs.iter().map(|r| r.highlight), color),
        "color": common(runs.iter().map(|r| r.color), color),
        "font": common(runs.iter().map(|r| r.font.clone()), |f| f.map_or(Value::Null, Value::String)),
        "size": common(runs.iter().map(|r| r.size), |sz| sz.map_or(Value::Null, |v| json!(v))),
        "link": common(runs.iter().map(|r| r.link.clone()), |f| f.map_or(Value::Null, Value::String)),
        "style": common(paras.iter().map(|pg| pg.style), |st| Value::String(st.name().into())),
        "list": common(paras.iter().map(|pg| pg.list), |l| serde_json::to_value(l).unwrap_or(Value::Null)),
        "indent": common(paras.iter().map(|pg| pg.indent), |i| json!(i)),
        "tag": common(paras.iter().map(|pg| pg.tag), |t| serde_json::to_value(t).unwrap_or(Value::Null)),
    });
    Ok(CmdOutput::query(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_parse_every_form() {
        let o = Outline::with_text(notecraft_doc::Point::ZERO, "abc\ndefg\nh");
        let r = |v: Value| range(&v, &o);
        assert_eq!(r(json!({"start": {"para": 1, "char": 9}, "end": {"para": 0, "char": 1}})), Some((TextPos::new(0, 1), TextPos::new(1, 4))));
        assert_eq!(r(json!({"at": [2, 0]})), Some((TextPos::new(2, 0), TextPos::new(2, 0))));
        assert_eq!(r(json!({"para": 1, "start": 1, "end": 3})), Some((TextPos::new(1, 1), TextPos::new(1, 3))));
        assert_eq!(r(json!({"paras": [2, 0]})), Some((TextPos::new(0, 0), TextPos::new(2, 1))));
        assert_eq!(r(json!({"para": 1})), Some((TextPos::new(1, 0), TextPos::new(1, 4))));
        assert_eq!(r(json!({"para": 99})), Some((TextPos::new(2, 0), TextPos::new(2, 1))));
        assert_eq!(r(json!({})), None);
        assert_eq!(range_or_all(&json!({}), &o), (TextPos::new(0, 0), TextPos::new(2, 1)));
    }

    #[test]
    fn styles_parse() {
        let r = apply_style(&json!({"bold": true, "color": "#ff0000", "size": 1e9, "font": " Arial ", "highlight": null}), Run::default()).unwrap();
        assert!(r.bold);
        assert_eq!(r.color, Color::parse_hex("#ff0000"));
        assert_eq!(r.size, Some(MAX_FONT_SIZE));
        assert_eq!(r.font.as_deref(), Some("Arial"));
        assert!(apply_style(&json!({"color": 5}), Run::default()).is_err());
        assert!(apply_style(&json!([]), Run::default()).is_err());
    }
}
