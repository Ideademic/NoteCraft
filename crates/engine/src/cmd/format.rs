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
//! - **Tables:** `cell` puts the range in a table cell, `cells` covers whole cells, and a range
//!   in the outline's own blocks covers every table it spans (see [`super::text`]).

use notecraft_doc::Block;
use notecraft_doc::page::MAX_INDENT;
use notecraft_doc::{Align, Color, ListKind, ListStyle, Outline, ParaStyle, Paragraph, Run, Tag};
use serde_json::{Value, json};

use super::has_page;
use super::text::{clamp_size, clean_font, frame_param, range, targets, with_outline};
use crate::ids::{
    FORMAT_ALIGN, FORMAT_BOLD, FORMAT_BULLETS, FORMAT_CLEAR, FORMAT_COLOR, FORMAT_FONT, FORMAT_HIGHLIGHT, FORMAT_INDENT, FORMAT_ITALIC,
    FORMAT_NUMBERING, FORMAT_OUTDENT, FORMAT_SIZE, FORMAT_STRIKE, FORMAT_STYLE, FORMAT_TAG, FORMAT_TAG_TODO, FORMAT_TOGGLE_TODO, FORMAT_UNDERLINE,
};
use crate::text::{self, Frame, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

macro_rules! sel {
    ($extra:literal) => {
        concat!(
            r##"{"outline": guid, "cell"?: {"block","row","col"}, "cells"?: {"block","rows":[r0,r1],"cols":[c0,c1]}, "start"?: {"para","char"}, "end"?: {"para","char"}, "page"?: guid"##,
            $extra,
            "}"
        )
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
        CommandSpec::new(FORMAT_BULLETS, "Bullets", bullets)
            .key("Mod+.")
            .when(has_page)
            .params(sel!(r##", "style"?: "disc"|"circle"|"square"|"dash"|"arrow" (default: by level), "value"?: bool"##)),
        CommandSpec::new(FORMAT_NUMBERING, "Numbering", numbering)
            .key("Mod+/")
            .when(has_page)
            .params(sel!(r##", "style"?: "decimal"|"lowerAlpha"|"upperAlpha"|"lowerRoman"|"upperRoman" (or "1.", "a.", "A.", "i.", "I."; default: by level), "value"?: bool"##)),
        CommandSpec::new(FORMAT_INDENT, "Increase Indent", indent).key("Alt+Shift+Right").when(has_page).params(sel!("")),
        CommandSpec::new(FORMAT_OUTDENT, "Decrease Indent", outdent).key("Alt+Shift+Left").when(has_page).params(sel!("")),
        CommandSpec::new(FORMAT_ALIGN, "Paragraph Alignment", align)
            .when(has_page)
            .params(sel!(r##", "align": "left"|"center"|"right"|"justify""##)),
        CommandSpec::new(FORMAT_TAG, "Tag", tag).when(has_page).params(sel!(
            r##", "tag": "toDo"|"important"|"question"|"rememberForLater"|"definition"|"highlight"|"contact"|"address"|"phone"|"idea"|"critical" | null (remove every tag), "value"?: bool (default: toggle)"##
        )),
        CommandSpec::new(FORMAT_TAG_TODO, "To Do Tag", tag_todo).key("Mod+1").when(has_page).params(sel!(r##", "value"?: bool"##)),
        CommandSpec::new(FORMAT_TOGGLE_TODO, "Check To Do", toggle_todo).when(has_page).params(sel!(r##", "checked"?: bool"##)),
    ]
}

// --- character formatting ------------------------------------------------------------------

/// The characters a character command covers: the range (and the tables it spans), the word
/// at a collapsed caret, or the whole outline. `None`: a caret outside any word.
fn char_targets(params: &Value, o: &Outline) -> Result<Option<Vec<(Frame, TextPos, TextPos)>>, CmdError> {
    if params.get("cells").is_none_or(Value::is_null) {
        let frame = frame_param(params)?;
        let fl = text::flow(o, frame).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
        if let Some((a, b)) = range(params, fl)
            && a == b
        {
            return Ok(text::word_at(fl, a).map(|(a, b)| vec![(frame, a, b)]));
        }
    }
    targets(params, o).map(Some)
}

/// Every block's paragraphs, for "did anything change".
fn snapshot(o: &Outline) -> Vec<Block> {
    o.blocks.clone()
}

fn collapsed() -> CmdResult {
    Ok(CmdOutput::query(json!({"collapsed": true})))
}

/// Applies `f` (made from the covered runs) to the covered characters; unchanged when it
/// changed nothing.
fn apply_chars(s: &mut Session, params: &Value, f: &dyn Fn(&[Run]) -> Box<dyn FnMut(&mut Run)>) -> CmdResult {
    with_outline(s, params, |o| {
        let Some(targets) = char_targets(params, o)? else { return collapsed() };
        let runs: Vec<Run> = targets.iter().filter_map(|(fr, a, b)| text::flow(o, *fr).map(|fl| text::runs_in(fl, *a, *b))).flatten().collect();
        let before = snapshot(o);
        let mut g = f(&runs);
        for (fr, a, b) in targets {
            if let Some(fl) = text::flow_mut(o, fr) {
                text::apply_range(fl, a, b, &mut *g);
            }
        }
        Ok(if o.blocks == before { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

/// A toggle: on unless everything covered already has it (or `value` says).
fn toggle(s: &mut Session, params: &Value, get: fn(&Run) -> bool, set: fn(&mut Run, bool)) -> CmdResult {
    let value = p::bool(params, "value");
    apply_chars(s, params, &|runs| {
        let on = value.unwrap_or_else(|| runs.is_empty() || !runs.iter().all(get));
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
    apply_chars(s, params, &|runs| {
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
    apply_chars(s, params, &|_| Box::new(move |r: &mut Run| r.color = c))
}

fn size(s: &mut Session, params: &Value) -> CmdResult {
    let v = match params.get("size") {
        Some(Value::Null) => None,
        Some(_) => Some(clamp_size(p::req_f32(params, "size")?)),
        None => return Err(CmdError::Params("`size` (number or null) is required".into())),
    };
    apply_chars(s, params, &|_| Box::new(move |r: &mut Run| r.size = v))
}

fn font(s: &mut Session, params: &Value) -> CmdResult {
    let v = match params.get("font") {
        Some(Value::Null) => None,
        Some(Value::String(f)) => Some(clean_font(f).ok_or_else(|| CmdError::Params("`font` must not be empty".into()))?),
        _ => return Err(CmdError::Params("`font` (string or null) is required".into())),
    };
    apply_chars(s, params, &|_| {
        let v = v.clone();
        Box::new(move |r: &mut Run| r.font = v.clone())
    })
}

/// Clear All Formatting: plain runs (links kept), normal paragraphs (lists and tags kept).
fn clear(s: &mut Session, params: &Value) -> CmdResult {
    with_outline(s, params, |o| {
        let targets = match char_targets(params, o)? {
            Some(t) => t,
            // A caret outside a word: its paragraph's style still goes back to normal.
            None => {
                let frame = frame_param(params)?;
                let at = text::flow(o, frame).and_then(|fl| range(params, fl)).map_or(TextPos::default(), |(a, _)| a);
                vec![(frame, at, at)]
            }
        };
        let before = snapshot(o);
        for (fr, a, b) in targets {
            let Some(fl) = text::flow_mut(o, fr) else { continue };
            text::apply_range(fl, a, b, &mut |r| *r = Run { text: std::mem::take(&mut r.text), link: r.link.take(), ..Run::default() });
            text::for_paras_mut(fl, a.para, b.para, &mut |pg| pg.style = ParaStyle::Normal);
        }
        Ok(if o.blocks == before { CmdOutput::unchanged() } else { CmdOutput::ok() })
    })
}

// --- paragraph formatting ------------------------------------------------------------------

/// Applies `f` to every paragraph the range touches (all of them without one), table cells
/// included (see [`targets`]).
fn apply_paras(s: &mut Session, params: &Value, f: &dyn Fn(&mut [Paragraph])) -> CmdResult {
    with_outline(s, params, |o| {
        let targets = targets(params, o)?;
        let mut paras: Vec<Paragraph> = Vec::new();
        for (fr, a, b) in &targets {
            if let Some(fl) = text::flow(o, *fr) {
                paras.extend(text::paras_in(fl, a.para, b.para).into_iter().cloned());
            }
        }
        if paras.is_empty() {
            return Ok(CmdOutput::unchanged());
        }
        let before = paras.clone();
        f(&mut paras);
        if paras == before {
            return Ok(CmdOutput::unchanged());
        }
        let mut it = paras.into_iter();
        for (fr, a, b) in targets {
            let Some(fl) = text::flow_mut(o, fr) else { continue };
            text::for_paras_mut(fl, a.para, b.para, &mut |pg| {
                if let Some(np) = it.next() {
                    *pg = np;
                }
            });
        }
        Ok(CmdOutput::ok())
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

/// Bullets or numbering, optionally in a `style`: on unless every paragraph already has it (in
/// that style, when one is given), or as `value` says. Off removes the list and its style.
fn list(s: &mut Session, params: &Value, kind: ListKind) -> CmdResult {
    let value = p::bool(params, "value");
    let style = match params.get("style") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => {
            let st = ListStyle::parse(name).filter(|st| st.kind() == kind).ok_or_else(|| {
                let what = if kind == ListKind::Bullet { "bullet" } else { "numbering" };
                CmdError::Params(format!("unknown {what} style `{}`", name.chars().take(20).collect::<String>()))
            })?;
            Some(st)
        }
        Some(_) => return Err(CmdError::Params("`style` must be a string".into())),
    };
    apply_paras(s, params, &|ps| {
        let has = |pg: &Paragraph| pg.list == Some(kind) && style.is_none_or(|st| pg.marker_style() == Some(st));
        let on = value.unwrap_or_else(|| !ps.iter().all(has));
        for pg in ps {
            if on {
                pg.list = Some(kind);
                if style.is_some() {
                    pg.list_style = style;
                }
            } else if pg.list == Some(kind) || value == Some(false) {
                pg.list = None;
                pg.list_style = None;
            }
        }
    })
}

fn align(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::req_str(params, "align")?;
    let a = Align::parse(name).ok_or_else(|| CmdError::Params(format!("unknown alignment `{}`", name.chars().take(20).collect::<String>())))?;
    apply_paras(s, params, &|ps| {
        for pg in ps {
            pg.align = a;
        }
    })
}

/// The tags Mod+1 … Mod+9 put on (as in OneNote's default tag list); Mod+0 removes them all.
pub const TAG_KEYS: [(&str, Tag); 9] = [
    ("Mod+1", Tag::ToDo { checked: false }),
    ("Mod+2", Tag::Important),
    ("Mod+3", Tag::Question),
    ("Mod+4", Tag::RememberForLater),
    ("Mod+5", Tag::Definition),
    ("Mod+6", Tag::Highlight),
    ("Mod+7", Tag::Contact),
    ("Mod+8", Tag::Address),
    ("Mod+9", Tag::Phone),
];
/// Removes every tag from the paragraphs (`format.tag` with `"tag": null`).
pub const TAG_REMOVE_KEY: &str = "Mod+0";

/// The shortcut that puts `tag` on, if it has one.
pub fn tag_key(tag: Tag) -> Option<&'static str> {
    TAG_KEYS.iter().find(|(_, t)| t.name() == tag.name()).map(|(k, _)| *k)
}

/// A tag: on for every paragraph the range touches unless all already have it (or `value`
/// says). A paragraph holds one tag, so putting one on replaces another; a To Do keeps its tick.
/// `"tag": null` removes every tag.
fn tag(s: &mut Session, params: &Value) -> CmdResult {
    let wanted = match params.get("tag") {
        Some(Value::Null) => None,
        Some(Value::String(name)) => {
            Some(Tag::parse(name).ok_or_else(|| CmdError::Params(format!("unknown tag `{}`", name.chars().take(24).collect::<String>())))?)
        }
        _ => return Err(CmdError::Params("`tag` (a tag name or null) is required".into())),
    };
    let value = p::bool(params, "value");
    apply_paras(s, params, &|ps| {
        let Some(t) = wanted else {
            for pg in ps.iter_mut() {
                pg.tag = None;
            }
            return;
        };
        let has = |pg: &Paragraph| pg.tag.is_some_and(|x| x.name() == t.name());
        let on = value.unwrap_or_else(|| !ps.iter().all(has));
        for pg in ps {
            if on {
                if !has(pg) {
                    pg.tag = Some(t);
                }
            } else if has(pg) {
                pg.tag = None;
            }
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
