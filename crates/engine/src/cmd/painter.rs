//! Home › Format Painter: pick up the formatting at the caret, then paint it onto other text.
//!
//! The canvas keeps what was picked (the brush stays on for one use, or until Esc after a
//! double-click); these commands only read and apply it. Text formatting (font, size, colours,
//! highlight, bold, italic, underline, strike) goes onto the painted characters; paragraph
//! formatting (style, alignment, list) onto the paragraphs the painted range covers whole. Tags
//! and links are not painted.

use notecraft_doc::{Align, ListKind, ListStyle, Outline, ParaStyle, Run};
use serde_json::{Value, json};

use super::text::{apply_style, outline_id, range, with_outline};
use super::{has_page, target_page};
use crate::ids::{FORMAT_PAINTER_APPLY, FORMAT_PAINTER_PICK};
use crate::text::{self, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(FORMAT_PAINTER_PICK, "Format Painter", pick).pure().when(has_page).params(
            r##"{"outline": guid, "start"?/"end"?/"at"?: pos, "page"?: guid} -> {"run": {"bold", "italic", "underline", "strike": bool, "highlight", "color": "#rrggbb"|null, "font": string|null, "size": number|null}, "para": {"style", "align", "list": "bullet"|"numbered"|null, "listStyle": string|null}}"##,
        ),
        CommandSpec::new(FORMAT_PAINTER_APPLY, "Paint Format", apply).when(has_page).params(
            r##"{"outline": guid, "start": pos, "end": pos (a caret: the word there), "format": what format.painterPick returned, "page"?: guid} -> {"caret": pos} | {"collapsed": true}"##,
        ),
    ]
}

/// The run formatting at the start of the range (what typing there would get for a caret).
fn run_at(o: &Outline, a: TextPos, b: TextPos) -> Run {
    if a != b
        && let Some(r) = text::runs_in(o, a, b).into_iter().next()
    {
        return Run { text: String::new(), ..r };
    }
    o.paragraphs.get(a.para).map(|p| text::format_at(p, a.char)).unwrap_or_default()
}

fn pick(s: &mut Session, params: &Value) -> CmdResult {
    let id = outline_id(params)?;
    let page = target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let (a, b) = range(params, o).unwrap_or_default();
    let r = run_at(o, a, b);
    let para = o.paragraphs.get(a.para).cloned().unwrap_or_default();
    let hex = |c: Option<notecraft_doc::Color>| c.map_or(Value::Null, |c| Value::String(c.to_hex()));
    Ok(CmdOutput::query(json!({
        "run": {
            "bold": r.bold, "italic": r.italic, "underline": r.underline, "strike": r.strike,
            "highlight": hex(r.highlight), "color": hex(r.color),
            "font": r.font, "size": r.size,
        },
        "para": {
            "style": para.style.name(), "align": para.align.name(),
            "list": para.list.map(|k| if k == ListKind::Bullet { "bullet" } else { "numbered" }),
            "listStyle": para.list_style.map(ListStyle::name),
        },
    })))
}

/// Paragraph formatting from a picked `para` object.
struct ParaFmt {
    style: ParaStyle,
    align: Align,
    list: Option<ListKind>,
    list_style: Option<ListStyle>,
}

fn para_fmt(v: &Value) -> Option<ParaFmt> {
    let v = v.as_object()?;
    let style = v.get("style").and_then(Value::as_str).and_then(ParaStyle::parse).unwrap_or_default();
    let align = v.get("align").and_then(Value::as_str).and_then(Align::parse).unwrap_or_default();
    let list = match v.get("list").and_then(Value::as_str) {
        Some("bullet") => Some(ListKind::Bullet),
        Some("numbered") => Some(ListKind::Numbered),
        _ => None,
    };
    let list_style = v.get("listStyle").and_then(Value::as_str).and_then(ListStyle::parse);
    Some(ParaFmt { style, align, list, list_style })
}

/// Paragraphs the range covers whole: it starts at or before their start and reaches their end.
fn covered(o: &Outline, a: TextPos, b: TextPos) -> Vec<usize> {
    (a.para..=b.para.min(o.paragraphs.len().saturating_sub(1)))
        .filter(|&i| {
            let Some(p) = o.paragraphs.get(i) else { return false };
            let starts = i > a.para || a.char == 0;
            let ends = if i < b.para {
                true
            } else if b.para > a.para && b.char == 0 {
                false
            } else {
                b.char >= text::para_len(p)
            };
            starts && ends && a != b
        })
        .collect()
}

fn apply(s: &mut Session, params: &Value) -> CmdResult {
    let fmt =
        params.get("format").filter(|v| v.is_object()).ok_or_else(|| CmdError::Params("`format` (from format.painterPick) is required".into()))?;
    let run_v = fmt.get("run").filter(|v| v.is_object()).ok_or_else(|| CmdError::Params("`format.run` is required".into()))?;
    // Every text property is replaced: start from plain and apply the picked values.
    let mut picked = apply_style(run_v, Run::default())?;
    picked.link = None;
    let para = fmt.get("para").and_then(para_fmt);
    with_outline(s, params, |o| {
        let (a, b) = match range(params, o) {
            Some((a, b)) if a == b => match text::word_at(o, a) {
                Some(w) => w,
                None => return Ok(CmdOutput::query(json!({"collapsed": true}))),
            },
            Some(r) => r,
            None => (TextPos::default(), text::end_of(o)),
        };
        let before = o.paragraphs.clone();
        let p = picked.clone();
        text::apply_range(o, a, b, &mut |r: &mut Run| {
            let link = r.link.take();
            *r = Run { text: std::mem::take(&mut r.text), link, ..p.clone() };
        });
        if let Some(pf) = &para {
            for i in covered(o, a, b) {
                if let Some(pg) = o.paragraphs.get_mut(i) {
                    pg.style = pf.style;
                    pg.align = pf.align;
                    pg.list = pf.list;
                    pg.list_style = pf.list_style;
                }
            }
        }
        let out = json!({"caret": b.to_json()});
        Ok(if o.paragraphs == before { CmdOutput::query(out) } else { CmdOutput::value(out) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn painter_copies_text_formatting() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let v = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "Source text\nplain words here\nwhole line"})).unwrap().value;
        let o = v["id"].clone();
        s.run(ids::FORMAT_BOLD, &json!({"outline": o, "para": 0})).unwrap();
        s.run(ids::FORMAT_COLOR, &json!({"outline": o, "para": 0, "color": "#ff0000"})).unwrap();
        s.run(ids::FORMAT_STYLE, &json!({"outline": o, "para": 0, "style": "heading2"})).unwrap();
        s.run(ids::FORMAT_TAG, &json!({"outline": o, "para": 0, "tag": "idea"})).unwrap();
        let f = s.run(ids::FORMAT_PAINTER_PICK, &json!({"outline": o, "at": [0, 2]})).unwrap().value;
        assert_eq!(f["run"]["bold"], true);
        assert_eq!(f["run"]["color"], "#ff0000");
        assert_eq!(f["para"]["style"], "heading2");
        // A caret in a word paints that word, not its paragraph.
        s.run(ids::FORMAT_PAINTER_APPLY, &json!({"outline": o, "start": [1, 7], "end": [1, 7], "format": f})).unwrap();
        let ol = s.current_page().unwrap().outline(notecraft_doc::Guid::parse(o.as_str().unwrap()).unwrap()).unwrap().clone();
        assert!(ol.paragraphs[1].runs.iter().any(|r| r.bold && r.text == "words"));
        assert!(ol.paragraphs[1].runs.iter().any(|r| !r.bold && r.text.contains("plain")));
        assert_eq!(ol.paragraphs[1].style, ParaStyle::Normal);
        // A whole paragraph gets the paragraph format too; tags aren't painted.
        s.run(ids::FORMAT_PAINTER_APPLY, &json!({"outline": o, "start": [2, 0], "end": [2, 10], "format": f})).unwrap();
        let ol = s.current_page().unwrap().outline(notecraft_doc::Guid::parse(o.as_str().unwrap()).unwrap()).unwrap().clone();
        assert_eq!(ol.paragraphs[2].style, ParaStyle::Heading2);
        assert!(ol.paragraphs[2].tag.is_none());
        assert!(ol.paragraphs[2].runs.iter().all(|r| r.bold && r.color.is_some()));
        // Outside a word: nothing to paint.
        let r = s.run(ids::FORMAT_PAINTER_APPLY, &json!({"outline": o, "at": [1, 5], "end": [1, 5], "format": f})).unwrap();
        assert_eq!(r.value["collapsed"], true);
        assert!(s.run(ids::FORMAT_PAINTER_APPLY, &json!({"outline": o, "at": [1, 0], "format": 5})).is_err());
    }
}
