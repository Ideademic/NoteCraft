//! Copy, cut and paste with rich content: text with its formatting, tables, whole note
//! containers, pictures, printouts, attached files and ink.
//!
//! A copy is a [`Clip`] (NoteCraft's own JSON, which the front end keeps as its internal
//! clipboard) plus plain text and an HTML fragment for other apps. Paste takes a clip, HTML
//! from another app (`formats::clip::from_html_blocks`: basic formatting and simple tables kept)
//! or plain text, and puts it at the caret or, without one, as a new note container (or, for
//! pictures and ink, where the pointer is). Each paste is one undo step.
//!
//! In a table cell (`"cell"`, as the text commands take it) copy and paste work on the cell's
//! text; a table pasted into a cell comes in as text (tables don't nest). Selected cells
//! (`"cells"`) copy as a table of those cells, and cut empties them.

use base64::Engine as _;
use notecraft_doc::ink::MAX_INK_PER_PAGE;
use notecraft_doc::page::{MAX_ELEMENTS_PER_PAGE, MAX_PARAGRAPHS};
use notecraft_doc::{Block, ElementKind, Guid, InkStroke, Outline, PageElement, Paragraph, Rect, Row, Run, Table};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::draw::guid_list;
use super::text::{frame_param, outline_id, range};
use super::{has_page, insert_pos, target_page};
use crate::ids::{EDIT_COPY, EDIT_CUT, EDIT_PASTE, EDIT_PASTE_PICTURE};
use crate::text::{self, CellRange, Flow, Frame, MAX_PARA_CHARS, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Largest clip pasted, bytes of JSON.
pub const MAX_CLIP_BYTES: usize = 64 * 1024 * 1024;
/// How far a pasted copy of items sits from the originals without a position, points.
pub const PASTE_OFFSET: f32 = 18.0;

/// What was copied: blocks of text (paragraphs and tables: a text selection) or whole items and
/// ink.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Clip {
    /// Clips written before tables call it `paragraphs`.
    #[serde(alias = "paragraphs")]
    pub blocks: Vec<Block>,
    pub elements: Vec<PageElement>,
    pub ink: Vec<InkStroke>,
}

impl Clip {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.elements.is_empty() && self.ink.is_empty()
    }

    /// The text: a line per paragraph and per table row (cells tab-separated); note
    /// containers' text, a blank line between them.
    pub fn plain_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.blocks.is_empty() {
            parts.push(self.blocks.iter().map(Block::text).collect::<Vec<_>>().join("\n"));
        }
        for el in &self.elements {
            match &el.kind {
                ElementKind::Outline(o) => parts.push(o.text()),
                ElementKind::EmbeddedFile { name, .. } => parts.push(name.clone()),
                ElementKind::Image(i) if !i.alt.is_empty() => parts.push(i.alt.clone()),
                _ => {}
            }
        }
        parts.join("\n\n")
    }

    /// An HTML fragment for other apps: text with its formatting, tables, pictures inline.
    pub fn html(&self) -> String {
        let mut out = notecraft_formats::clip::blocks_to_html(&self.blocks);
        for el in &self.elements {
            match &el.kind {
                ElementKind::Outline(o) => out.push_str(&notecraft_formats::clip::blocks_to_html(&o.blocks)),
                ElementKind::Image(img) if img.format != notecraft_doc::ImageFormat::Other => {
                    let (mime, data) = match notecraft_formats::picture::bake(img) {
                        Some(png) => ("image/png", png),
                        None => (img.format.mime(), img.data.to_vec()),
                    };
                    out.push_str(&format!(
                        "<img alt=\"{}\" width=\"{}\" height=\"{}\" src=\"data:{mime};base64,{}\">",
                        notecraft_formats::clip::escape(&img.alt),
                        (img.size.w * 4.0 / 3.0).round(),
                        (img.size.h * 4.0 / 3.0).round(),
                        base64::engine::general_purpose::STANDARD.encode(data)
                    ));
                }
                _ => {}
            }
        }
        out
    }
}

const COPY: &str = r##"{"outline": guid, "cell"?: cell, "cells"?: cell range, "start": pos, "end": pos} | {"elements": [guid], "strokes"?: [guid], "ink"?: true (the lasso selection)}, "page"?: guid -> {"clip": Clip, "text": string, "html": string}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(EDIT_COPY, "Copy", copy).pure().when(has_page).params(COPY),
        CommandSpec::new(EDIT_CUT, "Cut", cut).when(has_page).params(COPY),
        CommandSpec::new(EDIT_PASTE, "Paste", paste).when(has_page).params(
            r##"{"clip"?: Clip, "html"?: string, "text"?: string, "plain"?: bool (text only), "outline"?: guid, "cell"?: cell, "start"?/"end"?/"at"?: pos (paste at the caret), "x"?: number, "y"?: number (else a new note container, or items at the pointer), "page"?: guid} -> {"caret"?: pos, "cell"?: cell|null, "outline"?: guid, "ids"?: [guid]}"##,
        ),
        CommandSpec::new(EDIT_PASTE_PICTURE, "Paste Picture", super::insert::image)
            .when(has_page)
            .params(r##"{"bytesBase64": string, "name"?: string, "x"?: number, "y"?: number, "page"?: guid} -> {"id": guid}"##),
    ]
}

/// Paragraph `p`'s characters `s..e`, with their formatting.
fn slice(p: &Paragraph, s: usize, e: usize) -> Paragraph {
    let mut seen = 0usize;
    let mut runs = Vec::new();
    for r in &p.runs {
        let n = r.text.chars().count();
        let (lo, hi) = (s.max(seen), e.min(seen + n));
        if lo < hi {
            runs.push(Run { text: r.text.chars().skip(lo - seen).take(hi - lo).collect(), ..r.clone() });
        }
        seen += n;
    }
    let mut np = Paragraph { runs, ..p.clone() };
    text::normalize(&mut np);
    np
}

/// The blocks (with their formatting) between `a` and `b` of flow `frame` of `o`: a table the
/// range spans comes whole.
fn copy_text(o: &Outline, frame: Frame, a: TextPos, b: TextPos) -> Vec<Block> {
    let Some(fl) = text::flow(o, frame) else { return Vec::new() };
    let mut out = Vec::new();
    for i in a.para..=b.para.min(fl.count().saturating_sub(1)) {
        match fl.para(i) {
            Some(p) => {
                let (s, e) = text::para_span(fl, i, a, b);
                out.push(Block::Paragraph(slice(p, s, e)));
            }
            None => {
                if let Some(t) = o.table(i).filter(|_| frame == Frame::Top) {
                    out.push(Block::table(t.clone()));
                }
            }
        }
    }
    out
}

/// A table of the cells in `r` (their widths and shading kept).
fn copy_cells(t: &Table, r: CellRange) -> Table {
    let rows = (r.rows.0..=r.rows.1).map(|row| Row { cells: (r.cols.0..=r.cols.1).filter_map(|c| t.cell(row, c).cloned()).collect() }).collect();
    let mut out =
        Table { col_widths: (r.cols.0..=r.cols.1).map(|c| t.col_width(c)).collect(), col_locked: Vec::new(), rows, borders: t.borders, indent: 0 };
    out.normalize();
    out
}

/// What a copy names: a text range in a flow of an outline, cells, or items and strokes.
enum Target {
    Text { outline: Guid, frame: Frame, a: TextPos, b: TextPos },
    Cells { outline: Guid, range: CellRange },
    Items { elements: Vec<Guid>, strokes: Vec<Guid> },
}

fn target(s: &Session, params: &Value) -> Result<Target, CmdError> {
    if params.get("outline").is_some() && params.get("elements").is_none() {
        let outline = outline_id(params)?;
        if let Some(v) = params.get("cells").filter(|v| !v.is_null()) {
            let range = CellRange::from_json(v).ok_or_else(|| CmdError::Params("`cells` must be {\"block\", \"rows\", \"cols\"}".into()))?;
            return Ok(Target::Cells { outline, range });
        }
        return Ok(Target::Text { outline, frame: frame_param(params)?, a: TextPos::default(), b: TextPos::default() });
    }
    let elements = guid_list(params, "elements")?.unwrap_or_default();
    let mut strokes = guid_list(params, "strokes")?.unwrap_or_default();
    if p::bool(params, "ink") == Some(true) {
        strokes.extend(s.ink_selection.iter().copied());
    }
    if elements.is_empty() && strokes.is_empty() {
        return Err(CmdError::Params("`outline` with a range, or `elements`/`strokes`/`ink`, is required".into()));
    }
    Ok(Target::Items { elements, strokes })
}

/// The clip for `params` (and what it covered, for cut).
fn take_clip(s: &mut Session, params: &Value) -> Result<(Clip, Target), CmdError> {
    let t = target(s, params)?;
    let page = target_page(s, params)?;
    match t {
        Target::Text { outline, frame, .. } => {
            let o = page.outline(outline).ok_or_else(|| CmdError::Params(format!("no outline {outline}")))?;
            let fl = text::flow(o, frame).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
            let (a, b) = range(params, fl).unwrap_or((TextPos::default(), text::end_of(fl)));
            // A range on a table's block alone (`"para": n`) takes the table.
            if a == b && !(frame == Frame::Top && o.table(a.para).is_some()) {
                return Err(CmdError::Params("nothing is selected".into()));
            }
            Ok((Clip { blocks: copy_text(o, frame, a, b), ..Clip::default() }, Target::Text { outline, frame, a, b }))
        }
        Target::Cells { outline, range } => {
            let o = page.outline(outline).ok_or_else(|| CmdError::Params(format!("no outline {outline}")))?;
            let t = o.table(range.block).ok_or_else(|| CmdError::Params(format!("block {} is not a table", range.block)))?;
            let range = range.clamped(t.row_count(), t.col_count());
            Ok((Clip { blocks: vec![Block::table(copy_cells(t, range))], ..Clip::default() }, Target::Cells { outline, range }))
        }
        Target::Items { elements, strokes } => {
            let clip = Clip {
                blocks: Vec::new(),
                elements: page
                    .elements
                    .iter()
                    .filter(|e| elements.contains(&e.id) && !matches!(e.kind, ElementKind::Opaque { .. }))
                    .cloned()
                    .collect(),
                ink: page.ink.iter().filter(|st| strokes.contains(&st.id)).cloned().collect(),
            };
            if clip.is_empty() {
                return Err(CmdError::Params("nothing to copy".into()));
            }
            Ok((clip, Target::Items { elements, strokes }))
        }
    }
}

fn output(clip: &Clip) -> Value {
    json!({"clip": clip, "text": clip.plain_text(), "html": clip.html()})
}

fn copy(s: &mut Session, params: &Value) -> CmdResult {
    let (clip, _) = take_clip(s, params)?;
    Ok(CmdOutput::query(output(&clip)))
}

fn cut(s: &mut Session, params: &Value) -> CmdResult {
    let (clip, t) = take_clip(s, params)?;
    let page = target_page(s, params)?;
    let mut removed_ink = Vec::new();
    match t {
        Target::Text { outline, frame, a, b } => {
            if let Some(fl) = page.outline_mut(outline).and_then(|o| text::flow_mut(o, frame)) {
                let caret = text::delete(fl, a, b);
                page.touch();
                let mut v = output(&clip);
                if let Value::Object(m) = &mut v {
                    m.insert("caret".into(), caret.to_json());
                }
                return Ok(CmdOutput::value(v));
            }
        }
        Target::Cells { outline, range } => {
            if let Some(t) = page.outline_mut(outline).and_then(|o| o.table_mut(range.block)) {
                for c in range.cells() {
                    if let Some(cell) = t.cell_mut(c.row, c.col) {
                        cell.paragraphs = vec![Paragraph::plain("")];
                    }
                }
                page.touch();
            }
        }
        Target::Items { elements, strokes } => {
            page.elements.retain(|e| !elements.contains(&e.id) || matches!(e.kind, ElementKind::Opaque { .. }));
            page.ink.retain(|st| !strokes.contains(&st.id));
            removed_ink = strokes;
            page.touch();
        }
    }
    s.ink_selection.retain(|id| !removed_ink.contains(id));
    Ok(CmdOutput::value(output(&clip)))
}

/// Blocks as pasted: paragraphs sanitized, tables normalized, bounded.
fn clean(blocks: Vec<Block>) -> Vec<Block> {
    let mut total = 0usize;
    blocks
        .into_iter()
        .take(MAX_PARAGRAPHS)
        .map(|b| match b {
            Block::Paragraph(p) => {
                let mut p = p.sanitized();
                if p.runs.is_empty() {
                    p.runs.push(Run::default());
                }
                Block::Paragraph(p)
            }
            Block::Table { mut table } => {
                table.normalize();
                Block::table(table)
            }
        })
        .take_while(|b| {
            total = total.saturating_add(b.paragraphs().map(text::para_len).sum::<usize>());
            total <= MAX_PARA_CHARS
        })
        .collect()
}

/// Tables as text, for a cell (tables don't nest): a paragraph per row, cells separated by tabs.
fn flatten(blocks: Vec<Block>) -> Vec<Paragraph> {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table { table } => out.extend(table.texts().iter().map(|row| Paragraph::plain(&row.join("\t")))),
        }
    }
    out
}

/// Puts `paras` at `start..end` of `o`: the first joins the caret's paragraph (keeping its
/// formatting, unless that line is empty), the middle ones go in whole, the last takes the
/// text after the caret. Returns the caret after the pasted text.
fn splice(o: &mut dyn Flow, start: TextPos, end: TextPos, paras: Vec<Paragraph>) -> Result<TextPos, CmdError> {
    let at = text::delete(o, start, end);
    if o.count().saturating_add(paras.len()) > MAX_PARAGRAPHS {
        return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
    }
    let n = paras.len();
    let mut it = paras.into_iter();
    let Some(first) = it.next() else { return Ok(at) };
    let Some(p) = o.para_mut(at.para) else { return Ok(at) };
    let empty_line = text::para_len(p) == 0;
    let i = text::split_at(p, at.char);
    let tail: Vec<Run> = p.runs.drain(i..).collect();
    if empty_line {
        // An empty line becomes the pasted paragraph (a list item stays a list item).
        let runs = std::mem::take(&mut p.runs);
        *p = Paragraph { runs, ..first.clone() };
    }
    p.runs.retain(|r| !r.text.is_empty());
    p.runs.extend(first.runs);
    if n == 1 {
        let caret = TextPos::new(at.para, text::para_len(p));
        p.runs.extend(tail);
        text::normalize(p);
        return Ok(caret);
    }
    text::normalize(p);
    let mut idx = at.para;
    let rest: Vec<Paragraph> = it.collect();
    let last_i = rest.len().saturating_sub(1);
    let mut caret = at;
    for (k, mut np) in rest.into_iter().enumerate() {
        idx += 1;
        if k == last_i {
            caret = TextPos::new(idx, text::para_len(&np));
            np.runs.retain(|r| !r.text.is_empty());
            np.runs.extend(tail.iter().cloned());
        }
        text::normalize(&mut np);
        o.insert_para(idx, np);
    }
    Ok(caret)
}

/// Puts `blocks` (with tables) at `start..end` of the outline's own blocks: the caret's
/// paragraph is split there (an empty one is replaced) and the blocks go between its halves.
/// Returns the caret: at the start of the second half.
fn splice_blocks(o: &mut Outline, start: TextPos, end: TextPos, blocks: Vec<Block>) -> Result<TextPos, CmdError> {
    if o.blocks.len().saturating_add(blocks.len() + 1) > MAX_PARAGRAPHS {
        return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
    }
    let at = text::delete(o, start, end);
    let n = blocks.len();
    let Some(p) = o.para_mut(at.para) else {
        let i = at.para.saturating_add(1).min(o.blocks.len());
        o.blocks.splice(i..i, blocks);
        return Ok(TextPos::new(i + n, 0));
    };
    if text::para_len(p) == 0 {
        o.blocks.splice(at.para..=at.para, blocks);
        let next = at.para + n;
        if next >= o.blocks.len() || o.para(next).is_none() {
            o.blocks.insert(next.min(o.blocks.len()), Paragraph::plain("").into());
        }
        return Ok(TextPos::new(next, 0));
    }
    let i = text::split_at(p, at.char);
    let tail_runs: Vec<Run> = p.runs.drain(i..).collect();
    let mut tail = Paragraph { runs: tail_runs, ..text::continuation(p) };
    text::normalize(p);
    text::normalize(&mut tail);
    let head_empty = text::para_len(p) == 0;
    let from = if head_empty { at.para } else { at.para + 1 };
    if head_empty {
        o.blocks.remove(at.para);
    }
    o.blocks.splice(from..from, blocks);
    o.blocks.insert((from + n).min(o.blocks.len()), Block::Paragraph(tail));
    Ok(TextPos::new(from + n, 0))
}

/// The blocks to paste: clip > html > text (plain: the text only).
fn blocks_of(params: &Value, clip: Option<&Clip>, plain: bool) -> Vec<Block> {
    let text_paras =
        |t: &str| t.replace("\r\n", "\n").replace('\r', "\n").split('\n').map(|l| Block::Paragraph(Paragraph::plain(l))).collect::<Vec<_>>();
    if plain {
        if let Some(t) = p::str(params, "text") {
            return text_paras(t);
        }
        if let Some(c) = clip {
            let t = c.plain_text();
            return if t.is_empty() { Vec::new() } else { text_paras(&t) };
        }
        if let Some(h) = p::str(params, "html") {
            let t = notecraft_formats::clip::from_html(h).iter().map(Paragraph::text).collect::<Vec<_>>().join("\n");
            return text_paras(&t);
        }
        return Vec::new();
    }
    if let Some(c) = clip {
        return c.blocks.clone();
    }
    if let Some(h) = p::str(params, "html") {
        return notecraft_formats::clip::from_html_blocks(h);
    }
    p::str(params, "text").map(text_paras).unwrap_or_default()
}

fn paste(s: &mut Session, params: &Value) -> CmdResult {
    let plain = p::bool(params, "plain").unwrap_or(false);
    let clip: Option<Clip> = match params.get("clip").filter(|v| !v.is_null()) {
        Some(v) => {
            let size = serde_json::to_vec(v).map(|b| b.len()).unwrap_or(usize::MAX);
            if size > MAX_CLIP_BYTES {
                return Err(CmdError::Params("the clip is too large".into()));
            }
            Some(serde_json::from_value(v.clone()).map_err(|e| CmdError::Params(format!("`clip`: {e}")))?)
        }
        None => None,
    };
    if let Some(h) = p::str(params, "html")
        && h.len() > MAX_CLIP_BYTES
    {
        return Err(CmdError::Params("the HTML is too large".into()));
    }
    let blocks = clean(blocks_of(params, clip.as_ref(), plain));
    let (elements, ink) = match (&clip, plain) {
        (Some(c), false) => (c.elements.clone(), c.ink.clone()),
        _ => (Vec::new(), Vec::new()),
    };
    let has_text = blocks.iter().any(|b| b.is_table() || b.as_paragraph().is_some_and(|p| text::para_len(p) > 0)) || blocks.len() > 1;
    if !has_text && elements.is_empty() && ink.is_empty() {
        return Err(CmdError::Params("nothing to paste".into()));
    }
    let mut out = serde_json::Map::new();
    let into = p::guid(params, "outline")?;
    let frame = frame_param(params)?;
    let page = target_page(s, params)?;
    if has_text {
        match into {
            Some(id) => {
                let o = page.outline_mut(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
                if o.blocks.is_empty() {
                    o.blocks.push(Paragraph::plain("").into());
                }
                let has_tables = blocks.iter().any(Block::is_table);
                let (a, b) = {
                    let fl = text::flow(o, frame).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
                    range(params, fl).unwrap_or_else(|| {
                        let e = text::end_of(fl);
                        (e, e)
                    })
                };
                let caret = if frame == Frame::Top && has_tables && !plain {
                    splice_blocks(o, a, b, blocks)?
                } else {
                    let paras = flatten(blocks);
                    let fl = text::flow_mut(o, frame).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
                    if fl.count() == 0 {
                        fl.insert_para(0, Paragraph::plain(""));
                    }
                    if plain || (clip.is_none() && params.get("html").is_none()) {
                        // Plain text takes the formatting at the caret, as typing does.
                        let at = text::delete(fl, a, b);
                        let fmt = fl.para(at.para).map(|pg| text::format_at(pg, at.char)).unwrap_or_default();
                        let t = paras.iter().map(Paragraph::text).collect::<Vec<_>>().join("\n");
                        if fl.count().saturating_add(paras.len()) > MAX_PARAGRAPHS {
                            return Err(CmdError::Failed("this note container holds as many paragraphs as it can".into()));
                        }
                        text::insert(fl, at, &t, &fmt)
                    } else {
                        splice(fl, a, b, paras)?
                    }
                };
                out.insert("caret".into(), caret.to_json());
                out.insert("cell".into(), frame.to_json());
                out.insert("outline".into(), json!(id));
            }
            None => {
                if page.elements.len() >= MAX_ELEMENTS_PER_PAGE {
                    return Err(CmdError::Failed("this page holds as many items as it can".into()));
                }
                let pos = insert_pos(params, page);
                let mut o = Outline::with_text(pos, "");
                o.blocks = blocks;
                for pg in o.paragraphs_mut() {
                    text::normalize(pg);
                }
                o.width = o.display_width();
                let caret = text::end_of(&o);
                let el = PageElement::new(ElementKind::Outline(o));
                out.insert("outline".into(), json!(el.id));
                out.insert("caret".into(), caret.to_json());
                page.elements.push(el);
            }
        }
    }
    let mut new_ids = Vec::new();
    let mut new_strokes = Vec::new();
    if !elements.is_empty() || !ink.is_empty() {
        if page.elements.len().saturating_add(elements.len()) > MAX_ELEMENTS_PER_PAGE || page.ink.len().saturating_add(ink.len()) > MAX_INK_PER_PAGE {
            return Err(CmdError::Failed("this page holds as many items as it can".into()));
        }
        let bbox = elements.iter().filter_map(PageElement::bounds).chain(ink.iter().filter_map(InkStroke::bounds)).reduce(Rect::union);
        let (dx, dy) = match (p::f32(params, "x"), p::f32(params, "y"), bbox) {
            (Some(x), Some(y), Some(b)) => (x - b.x, y - b.y),
            _ => (PASTE_OFFSET, PASTE_OFFSET),
        };
        for mut el in elements {
            if matches!(el.kind, ElementKind::Opaque { .. }) {
                continue;
            }
            el.id = Guid::new();
            match &mut el.kind {
                ElementKind::Outline(o) => {
                    o.blocks = clean(std::mem::take(&mut o.blocks));
                    o.sanitize();
                    if o.blocks.is_empty() {
                        o.blocks.push(Paragraph::plain("").into());
                    }
                    o.width = o.width.clamp(super::outline::MIN_OUTLINE_WIDTH, super::outline::MAX_OUTLINE_WIDTH);
                    if !o.width.is_finite() {
                        o.width = notecraft_doc::page::DEFAULT_OUTLINE_WIDTH;
                    }
                }
                ElementKind::Image(img) => {
                    img.size = img.size.sanitized();
                    img.transform = img.transform.sanitized();
                }
                _ => {}
            }
            if let Some(pos) = el.pos() {
                el.set_pos(pos.sanitized().offset(dx, dy));
            }
            new_ids.push(el.id);
            page.elements.push(el);
        }
        for st in ink {
            let mut st = InkStroke { id: Guid::new(), ..st }.sanitized();
            for pt in &mut st.points {
                pt.x = notecraft_doc::geom::sane(pt.x + dx);
                pt.y = notecraft_doc::geom::sane(pt.y + dy);
            }
            if st.points.is_empty() {
                continue;
            }
            new_strokes.push(st.id);
            new_ids.push(st.id);
            page.ink.push(st);
        }
    }
    page.touch();
    if !new_strokes.is_empty() {
        s.ink_selection = new_strokes;
    }
    if !new_ids.is_empty() {
        out.insert("ids".into(), json!(new_ids));
    }

    Ok(CmdOutput::value(Value::Object(out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    fn outline(s: &Session, id: &Value) -> Outline {
        s.current_page().unwrap().outline(Guid::parse(id.as_str().unwrap()).unwrap()).unwrap().clone()
    }

    #[test]
    fn copy_and_paste_text_keeps_formatting_across_containers() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let a = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "Hello bold world\nsecond line"})).unwrap().value["id"].clone();
        s.run(ids::FORMAT_BOLD, &json!({"outline": a, "start": [0, 6], "end": [0, 10]})).unwrap();
        let c = s.run(ids::EDIT_COPY, &json!({"outline": a, "start": [0, 6], "end": [1, 6]})).unwrap();
        assert!(!c.changed);
        assert_eq!(c.value["text"], "bold world\nsecond");
        assert!(c.value["html"].as_str().unwrap().contains("<strong>bold</strong>"));
        let clip = c.value["clip"].clone();
        let b = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 300, "text": "[]"})).unwrap().value["id"].clone();
        let depth = s.undo_depth();
        let out = s.run(ids::EDIT_PASTE, &json!({"clip": clip, "outline": b, "at": [0, 1]})).unwrap();
        assert_eq!(s.undo_depth(), depth + 1, "one undo step");
        let ob = outline(&s, &b);
        assert_eq!(ob.text(), "[bold world\nsecond]");
        assert!(ob.para(0).unwrap().runs.iter().any(|r| r.bold && r.text == "bold"));
        assert_eq!(out.value["caret"], json!({"para": 1, "char": 6}));
        // Plain text takes the caret's formatting.
        s.run(ids::EDIT_PASTE, &json!({"clip": c.value["clip"], "plain": true, "outline": b, "at": [1, 0]})).unwrap();
        let ob = outline(&s, &b);
        assert!(ob.paragraphs().flat_map(|p| &p.runs).filter(|r| r.bold).count() == 1);
        // Without an outline: a new container.
        let out = s.run(ids::EDIT_PASTE, &json!({"html": "<p>one <i>two</i></p><ul><li>three</li></ul>", "x": 300, "y": 400})).unwrap();
        let n = outline(&s, &out.value["outline"]);
        assert_eq!(n.text(), "one two\nthree");
        assert_eq!((n.pos.x, n.pos.y), (300.0, 400.0));
        assert_eq!(n.para(1).unwrap().list, Some(notecraft_doc::ListKind::Bullet));
        // Cut removes the text in the same step.
        let cut = s.run(ids::EDIT_CUT, &json!({"outline": a, "start": [0, 0], "end": [0, 6]})).unwrap();
        assert_eq!(cut.value["text"], "Hello ");
        assert_eq!(outline(&s, &a).para(0).unwrap().text(), "bold world");
        assert!(s.run(ids::EDIT_PASTE, &json!({"text": ""})).is_err());
        assert!(s.run(ids::EDIT_COPY, &json!({"outline": a, "at": [0, 1]})).is_err(), "nothing selected");
    }

    #[test]
    fn copy_and_paste_items_and_ink() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let a = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "box"})).unwrap().value["id"].clone();
        let st = s.run(ids::DRAW_STROKE, &json!({"points": [[50, 300], [90, 340]]})).unwrap().value["id"].clone();
        s.run(ids::DRAW_LASSO_SELECT, &json!({"ids": [st]})).unwrap();
        let c = s.run(ids::EDIT_COPY, &json!({"elements": [a], "ink": true})).unwrap().value;
        assert_eq!(c["clip"]["elements"].as_array().unwrap().len(), 1);
        assert_eq!(c["clip"]["ink"].as_array().unwrap().len(), 1);
        let out = s.run(ids::EDIT_PASTE, &json!({"clip": c["clip"]})).unwrap();
        let ids_out = out.value["ids"].as_array().unwrap();
        assert_eq!(ids_out.len(), 2);
        let page = s.current_page().unwrap();
        assert_eq!(page.elements.len(), 2);
        let copy = page.elements.iter().find(|e| e.id.to_string() != a.as_str().unwrap()).unwrap();
        assert_eq!(copy.pos().map(|p| (p.x, p.y)), Some((58.0, 118.0)), "offset from the original");
        assert_eq!(s.ink_selection.len(), 1, "the pasted ink is selected");
        // At a position: the items' top-left corner goes there.
        s.run(ids::EDIT_PASTE, &json!({"clip": {"elements": c["clip"]["elements"]}, "x": 500, "y": 500})).unwrap();
        assert!(s.current_page().unwrap().elements.iter().any(|e| e.pos().map(|p| (p.x, p.y)) == Some((500.0, 500.0))));
        // Cut takes them away.
        s.run(ids::EDIT_CUT, &json!({"elements": [a], "strokes": [st]})).unwrap();
        let page = s.current_page().unwrap();
        assert!(page.element(Guid::parse(a.as_str().unwrap()).unwrap()).is_none());
        assert_eq!(page.ink.len(), 1);
        assert!(s.run(ids::EDIT_PASTE, &json!({"clip": {"elements": 5}})).is_err());
    }

    #[test]
    fn a_pasted_list_into_an_empty_line_stays_a_list() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        let b = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": ""})).unwrap().value["id"].clone();
        s.run(ids::EDIT_PASTE, &json!({"html": "<ol><li>a</li><li>b</li></ol>", "outline": b, "at": [0, 0]})).unwrap();
        let o = outline(&s, &b);
        assert_eq!(o.text(), "a\nb");
        assert!(o.paragraphs().all(|p| p.list == Some(notecraft_doc::ListKind::Numbered)));
    }
}
