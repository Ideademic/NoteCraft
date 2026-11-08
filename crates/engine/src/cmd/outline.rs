//! Note containers (outlines) and other page elements: new, move, resize, delete; replacing an
//! outline's paragraphs (typing).

use notecraft_doc::page::{DEFAULT_OUTLINE_WIDTH, MAX_ELEMENTS_PER_PAGE, MAX_PARAGRAPHS};
use notecraft_doc::{Block, ElementKind, Outline, PageElement, Size};
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::{OUTLINE_DELETE, OUTLINE_MOVE, OUTLINE_NEW, OUTLINE_RESIZE, TEXT_SET_PARAGRAPHS};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Narrowest outline, points.
pub const MIN_OUTLINE_WIDTH: f32 = 36.0;
/// Widest outline, points.
pub const MAX_OUTLINE_WIDTH: f32 = 4000.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(OUTLINE_NEW, "New Note Container", new)
            .when(has_page)
            .params(r##"{"x": number, "y": number, "width"?: number, "text"?: string, "page"?: guid}"##),
        CommandSpec::new(OUTLINE_MOVE, "Move", move_element).when(has_page).params(r##"{"id": guid, "x": number, "y": number, "page"?: guid}"##),
        CommandSpec::new(OUTLINE_RESIZE, "Resize", resize)
            .when(has_page)
            .params(r##"{"id": guid, "width": number, "height"?: number, "page"?: guid}"##),
        CommandSpec::new(OUTLINE_DELETE, "Delete", delete).when(has_page).params(r##"{"id": guid, "page"?: guid}"##),
        CommandSpec::new(TEXT_SET_PARAGRAPHS, "Type", set_paragraphs)
            .when(has_page)
            .params(r##"{"id": guid, "paragraphs": [Paragraph | {"table": Table}], "mergeKey"?: string, "page"?: guid}"##),
    ]
}

fn new(s: &mut Session, params: &Value) -> CmdResult {
    let pos = p::req_point(params)?;
    let width = p::f32(params, "width").unwrap_or(DEFAULT_OUTLINE_WIDTH).clamp(MIN_OUTLINE_WIDTH, MAX_OUTLINE_WIDTH);
    let text = p::str(params, "text").unwrap_or("");
    let page = target_page(s, params)?;
    if page.elements.len() >= MAX_ELEMENTS_PER_PAGE {
        return Err(CmdError::Failed("this page holds as many items as it can".into()));
    }
    let mut outline = Outline::with_text(pos, text);
    outline.width = width;
    outline.blocks.truncate(MAX_PARAGRAPHS);
    let el = PageElement::new(ElementKind::Outline(outline));
    let id = el.id;
    page.elements.push(el);
    page.touch();
    Ok(CmdOutput::value(json!({"id": id})))
}

/// Moves any element (outline, picture, printout, file) so its top-left corner is at (x, y).
fn move_element(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let pos = p::req_point(params)?;
    let page = target_page(s, params)?;
    let el = page.element_mut(id).ok_or_else(|| CmdError::Params(format!("no element {id}")))?;
    if matches!(el.kind, ElementKind::Opaque { .. }) {
        return Err(CmdError::Disabled("this content can't be moved".into()));
    }
    if el.pos() == Some(pos) {
        return Ok(CmdOutput::unchanged());
    }
    el.set_pos(pos);
    page.touch();
    Ok(CmdOutput::ok())
}

/// An outline's width, or a picture's size (`height` too; aspect kept when absent).
fn resize(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let width = p::req_f32(params, "width")?.clamp(MIN_OUTLINE_WIDTH, MAX_OUTLINE_WIDTH);
    let height = p::f32(params, "height");
    let page = target_page(s, params)?;
    let el = page.element_mut(id).ok_or_else(|| CmdError::Params(format!("no element {id}")))?;
    match &mut el.kind {
        ElementKind::Outline(o) => o.width = width,
        ElementKind::Image(img) => {
            let h = height.unwrap_or_else(|| if img.size.w > 0.0 { img.size.h * width / img.size.w } else { width });
            img.size = Size::new(width, h).sanitized();
        }
        _ => return Err(CmdError::Disabled("this item can't be resized".into())),
    }
    page.touch();
    Ok(CmdOutput::ok())
}

fn delete(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let page = target_page(s, params)?;
    page.remove_element(id).ok_or_else(|| CmdError::Params(format!("no element {id}")))?;
    page.touch();
    Ok(CmdOutput::ok())
}

/// Replaces an outline's blocks (v0.1 typing: the canvas edited a copy and sent it whole):
/// paragraphs, and tables as `{"table": …}`. Pass the same `mergeKey` (the outline id) while
/// typing to make it one undo step.
fn set_paragraphs(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let raw = params.get("paragraphs").filter(|v| v.is_array()).ok_or_else(|| CmdError::Params("`paragraphs` (array) is required".into()))?;
    if raw.as_array().is_some_and(|a| a.len() > MAX_PARAGRAPHS) {
        return Err(CmdError::Params(format!("at most {MAX_PARAGRAPHS} paragraphs")));
    }
    let blocks: Vec<Block> = serde_json::from_value(raw.clone()).map_err(|e| CmdError::Params(format!("`paragraphs`: {e}")))?;
    let mut probe = Outline { pos: notecraft_doc::Point::ZERO, width: DEFAULT_OUTLINE_WIDTH, blocks };
    probe.sanitize();
    let blocks = probe.blocks;
    let page = target_page(s, params)?;
    let outline = page.outline_mut(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    if outline.blocks == blocks {
        return Ok(CmdOutput::unchanged());
    }
    outline.blocks = blocks;
    page.touch();
    Ok(CmdOutput::ok())
}
