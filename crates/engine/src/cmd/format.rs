//! Home tab formatting, on the selection model of [`super::text`]:
//! `{"page"?: guid, "outline": guid, "start": {"para","char"}, "end": {"para","char"}}`.
//!
//! - **Character formatting** (bold … font) covers the characters between `start` and `end`.
//!   A collapsed selection (a caret) formats the word around it; with no word there nothing
//!   changes and the output is `{"collapsed": true}`, so the canvas applies the format to what
//!   is typed next (pass it as `text.insert`'s `style`). No range: the whole outline.
//! - **Toggles** (bold, italic, underline, strike, bullets, numbering, To Do) work like Word's:
//!   when everything selected already has it, it is removed; otherwise it is applied to all.
//!   `"value": true|false` sets it instead of toggling.
//! - **Paragraph formatting** (style, lists, indent, tags) covers every paragraph the range
//!   touches.

use notecraft_doc::page::MAX_INDENT;
use notecraft_doc::{Color, ListKind, Outline, ParaStyle, Paragraph, Run, Tag};
use serde_json::{Value, json};

use super::has_page;
use super::text::{clamp_size, clean_font, range, range_or_all, with_outline};
use crate::ids::{
    FORMAT_BOLD, FORMAT_BULLETS, FORMAT_CLEAR, FORMAT_COLOR, FORMAT_FONT, FORMAT_HIGHLIGHT, FORMAT_INDENT, FORMAT_ITALIC, FORMAT_NUMBERING,
    FORMAT_OUTDENT, FORMAT_SIZE, FORMAT_STRIKE, FORMAT_STYLE, FORMAT_TAG_TODO, FORMAT_TOGGLE_TODO, FORMAT_UNDERLINE,
};
use crate::text::{self, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

macro_rules! sel {
    ($extra:literal) => {
        concat!(r##"{"outline": guid, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid"##, $extra, "}")
    };
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(FORMAT_BOLD, "Bold", bold).key("Mod+B").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_ITALIC, "Italic", italic).key("Mod+I").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_UNDERLINE, "Underline", underline).key("Mod+U").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_STRIKE, "Strikethrough", strike).key("Mod+-").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_HIGHLIGHT, "Text Highlight Color", highlight)
            .key("Mod+Alt+H")
            .when(has_page)
            .params(sel!(r##", "color"?: "#rrggbb" (default yellow; toggles off when all already have it) | null (remove)"##)),
        CommandSpec::new(FORMAT_COLOR, "Font Color", color).when(has_page).params(sel!(r##", "color": "#rrggbb" | null (automatic)"##)),
        CommandSpec::new(FORMAT_SIZE, "Font Size", size).when(has_page).params(sel!(r##", "size": number (1-400) | null (style default)"##)),
        CommandSpec::new(FORMAT_FONT, "Font", font).when(has_page).params(sel!(r##", "font": string | null (default font)"##)),
        CommandSpec::new(FORMAT_CLEAR, "Clear All Formatting", clear).key("Mod+Shift+N").when(has_page).params(sel!("")),
        CommandSpec::new(FORMAT_STYLE, "Styles", style)
            .when(has_page)
            .params(sel!(r##", "style": "normal"|"heading1".."heading6"|"title"|"quote"|"code""##)),
        CommandSpec::new(FORMAT_BULLETS, "Bullets", bullets).key("Mod+.").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_NUMBERING, "Numbering", numbering).key("Mod+/").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_INDENT, "Increase Indent", indent).key("Alt+Shift+Right").when(has_page).params(sel!("")),
        CommandSpec::new(FORMAT_OUTDENT, "Decrease Indent", outdent).key("Alt+Shift+Left").when(has_page).params(sel!("")),
        CommandSpec::new(FORMAT_TAG_TODO, "To Do Tag", tag_todo).key("Mod+1").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_TOGGLE_TODO, "Check To Do", toggle_todo).when(has_page).params(sel!(r##", "checked"?: bool"##)),
    ]
}

// --- character formatting ------------------------------------------------------------------

/// The characters a character command covers: the range, the word at a collapsed caret, or the
/// whole outline. `None`: a caret outside any word.
fn char_range(params: &Value, o: &Outline) -> Option<(TextPos, TextPos)> {
    match range(params, o) {
        Some((a, b)) if a == b => text::word_at(o, a),
        Some(r) => Some(r),
        None => Some(range_or_all(params, o)),
    }
}

fn collapsed() -> CmdResult {
    Ok(CmdOutput::query(json!({"collapsed": true})))
}

/// Applies `f` to the covered characters; unchanged when it changed nothing.
fn apply_chars(s: &mut Session, params: &Value, f: &dyn Fn(&Outline, TextPos, TextPos) -> Box<dyn FnMut(&mut Run)>) -> CmdResult {
    with_outline(s, params, |o| {
        let Some((a, b)) = char_range(params, o) else { return collapsed() };
        let before = o.paragraphs.clone();
        let mut g = f(o, a, b);
        text::apply_range(o, a, b, &mut *g);
        Ok(if o.paragraphs == before { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

/// A toggle: on unless everything covered already has it (or `value` says).
fn toggle(s: &mut Session, params: &Value, get: fn(&Run) -> bool, set: fn(&mut Run, bool)) -> CmdResult {
    let value = p::bool(params, "value");
    apply_chars(s, params, &|o, a, b| {
        let on = value.unwrap_or_else(|| {
            let runs = text::runs_in(o, a, b);
            runs.is_empty() || !runs.iter().all(get)
        });
        Box::new(move |r: &mut Run| set(r, on))
    })
}

fn bold(s: &mut Session, params: &Value) -> CmdResult {
    toggle(s, params, |r| r.bold, |r, v| r.bold = v)
}

fn italic(s: &mut Session, params: &Value) -> CmdResult {
    toggle(s, params, |r| r.italic, |r, v| r.italic = v)
}

fn underline(s: &mut Session, params: &Value) -> CmdResult {
    toggle(s, params, |r| r.underline, |r, v| r.underline = v)
}

fn strike(s: &mut Session, params: &Value) -> CmdResult {
    toggle(s, params, |r| r.strike, |r, v| r.strike = v)
}

fn highlight(s: &mut Session, params: &Value) -> CmdResult {
    let explicit = params.get("color").is_some();
    let c = if explicit { p::color(params, "color")? } else { Some(Color::HIGHLIGHT_YELLOW) };
    apply_chars(s, params, &|o, a, b| {
        let runs = text::runs_in(o, a, b);
        // The button toggles: pressing it again over highlighted text removes the highlight.
        let value = if !explicit && !runs.is_empty() && runs.iter().all(|r| r.highlight == c) { None } else { c };
        Box::new(move |r: &mut Run| r.highlight = value)
    })
}

fn color(s: &mut Session, params: &Value) -> CmdResult {
    if params.get("color").is_none() {
        return Err(CmdError::Params("`color` (#rrggbb or null) is required".into()));
    }
    let c = p::color(params, "color")?;
    apply_chars(s, params, &|_, _, _| Box::new(move |r: &mut Run| r.color = c))
}

fn size(s: &mut Session, params: &Value) -> CmdResult {
    let v = match params.get("size") {
        Some(Value::Null) => None,
        Some(_) => Some(clamp_size(p::req_f32(params, "size")?)),
        None => return Err(CmdError::Params("`size` (number or null) is required".into())),
    };
    apply_chars(s, params, &|_, _, _| Box::new(move |r: &mut Run| r.size = v))
}

fn font(s: &mut Session, params: &Value) -> CmdResult {
    let v = match params.get("font") {
        Some(Value::Null) => None,
        Some(Value::String(f)) => Some(clean_font(f).ok_or_else(|| CmdError::Params("`font` must not be empty".into()))?),
        _ => return Err(CmdError::Params("`font` (string or null) is required".into())),
    };
    apply_chars(s, params, &|_, _, _| {
        let v = v.clone();
        Box::new(move |r: &mut Run| r.font = v.clone())
    })
}

/// Clear All Formatting: plain runs (links kept), normal paragraphs (lists and tags kept).
fn clear(s: &mut Session, params: &Value) -> CmdResult {
    with_outline(s, params, |o| {
        let (a, b) = match range(params, o) {
            Some((a, b)) if a == b => text::word_at(o, a).unwrap_or((a, b)),
            Some(r) => r,
            None => range_or_all(params, o),
        };
        let before = o.paragraphs.clone();
        text::apply_range(o, a, b, &mut |r| *r = Run { text: std::mem::take(&mut r.text), link: r.link.take(), ..Run::default() });
        for pg in paras_mut(o, a, b) {
            pg.style = ParaStyle::Normal;
        }
        Ok(if o.paragraphs == before { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

// --- paragraph formatting ------------------------------------------------------------------

fn paras_mut(o: &mut Outline, a: TextPos, b: TextPos) -> impl Iterator<Item = &mut Paragraph> {
    let hi = b.para.min(o.paragraphs.len().saturating_sub(1));
    o.paragraphs.get_mut(a.para..=hi).into_iter().flatten()
}

/// Applies `f` to every paragraph the range touches (all of them without one).
fn apply_paras(s: &mut Session, params: &Value, f: &dyn Fn(&mut [Paragraph])) -> CmdResult {
    with_outline(s, params, |o| {
        let (a, b) = range_or_all(params, o);
        let hi = b.para.min(o.paragraphs.len().saturating_sub(1));
        let Some(slice) = o.paragraphs.get_mut(a.para..=hi) else { return Ok(CmdOutput::unchanged()) };
        let before = slice.to_vec();
        f(slice);
        Ok(if slice == before.as_slice() { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

fn style(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::req_str(params, "style")?;
    let st = ParaStyle::parse(name).ok_or_else(|| CmdError::Params(format!("unknown style `{}`", name.chars().take(20).collect::<String>())))?;
    apply_paras(s, params, &|ps| {
        for pg in ps {
            pg.style = st;
        }
    })
}

fn list(s: &mut Session, params: &Value, kind: ListKind) -> CmdResult {
    let value = p::bool(params, "value");
    apply_paras(s, params, &|ps| {
        let on = value.unwrap_or_else(|| !ps.iter().all(|pg| pg.list == Some(kind)));
        for pg in ps {
            pg.list = on.then_some(kind);
        }
    })
}

fn bullets(s: &mut Session, params: &Value) -> CmdResult {
    list(s, params, ListKind::Bullet)
}

fn numbering(s: &mut Session, params: &Value) -> CmdResult {
    list(s, params, ListKind::Numbered)
}

fn indent(s: &mut Session, params: &Value) -> CmdResult {
    apply_paras(s, params, &|ps| {
        for pg in ps {
            pg.indent = pg.indent.saturating_add(1).min(MAX_INDENT);
        }
    })
}

fn outdent(s: &mut Session, params: &Value) -> CmdResult {
    apply_paras(s, params, &|ps| {
        for pg in ps {
            pg.indent = pg.indent.saturating_sub(1);
        }
    })
}

fn is_todo(pg: &Paragraph) -> bool {
    matches!(pg.tag, Some(Tag::ToDo { .. }))
}

/// To Do tag (Ctrl+1): adds an unchecked To Do to the paragraphs (keeping checked ones as they
/// are), or removes it when all already have one.
fn tag_todo(s: &mut Session, params: &Value) -> CmdResult {
    let value = p::bool(params, "value");
    apply_paras(s, params, &|ps| {
        let on = value.unwrap_or_else(|| !ps.iter().all(is_todo));
        for pg in ps {
            if !on {
                if is_todo(pg) {
                    pg.tag = None;
                }
            } else if !is_todo(pg) {
                pg.tag = Some(Tag::ToDo { checked: false });
            }
        }
    })
}

/// Checks or unchecks the To Do boxes in the range (clicking the box: `para` alone). Paragraphs
/// without a To Do tag are left alone. Without `checked`, all become checked unless all already
/// are.
fn toggle_todo(s: &mut Session, params: &Value) -> CmdResult {
    let value = p::bool(params, "checked");
    apply_paras(s, params, &|ps| {
        let all_checked = ps.iter().filter(|pg| is_todo(pg)).all(|pg| pg.tag == Some(Tag::ToDo { checked: true }));
        let checked = value.unwrap_or(!all_checked);
        for pg in ps.iter_mut().filter(|pg| is_todo(pg)) {
            pg.tag = Some(Tag::ToDo { checked });
        }
    })
}
