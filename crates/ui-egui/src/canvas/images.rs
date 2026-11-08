//! Pictures, file printouts and attached files: decoded into textures (cached per element and
//! picture data, decoded only once visible, scaled down past 4096 px), drawn at their place and
//! size. A printout is a stack of page pictures with a thin border and the file's name above,
//! meant to be inked over. A picture's rotation, flips and crop are texture coordinates on its
//! corners (the file is never changed). The selected item shows a frame, and a picture its corner
//! handles.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Align2, Color32, ColorImage, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, TextureHandle, TextureOptions, pos2, vec2};
use notecraft_doc::{ElementKind, Guid, ImageElement, Page, Point};
use notecraft_engine::Session;

use super::style::Ink;
use super::view::View;
use super::{CanvasState, Tool, pointer};
use crate::theme::{Tokens, mix};

/// Largest texture side; bigger pictures are scaled down for display (the file is kept).
const MAX_TEXTURE: u32 = 4096;
/// Handle size, pixels.
const HANDLE: f32 = 8.0;
/// Frames an unused texture is kept for.
const KEEP_FRAMES: u64 = 600;

#[derive(Default)]
struct Textures {
    frame: u64,
    /// (element, page index) → (data pointer, texture or a failed decode, last frame used).
    map: HashMap<(Guid, usize), (usize, Option<TextureHandle>, u64)>,
}

fn textures(ctx: &egui::Context) -> Arc<Mutex<Textures>> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Textures>>>(egui::Id::unique("nc_canvas_textures")).clone())
}

/// Decodes a picture for display; `None` when it can't be read.
pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > MAX_TEXTURE || img.height() > MAX_TEXTURE { img.thumbnail(MAX_TEXTURE, MAX_TEXTURE) } else { img };
    let rgba = img.to_rgba8();
    let size = [usize::try_from(rgba.width()).ok()?, usize::try_from(rgba.height()).ok()?];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

fn texture(ctx: &egui::Context, tex: &mut Textures, id: Guid, index: usize, img: &ImageElement) -> Option<TextureHandle> {
    let ptr = Arc::as_ptr(&img.data) as usize;
    let frame = tex.frame;
    if let Some((p, t, used)) = tex.map.get_mut(&(id, index))
        && *p == ptr
    {
        *used = frame;
        return t.clone();
    }
    let handle = decode(&img.data).map(|c| ctx.load_texture(format!("nc-img-{id}-{index}"), c, TextureOptions::LINEAR));
    tex.map.insert((id, index), (ptr, handle.clone(), frame));
    handle
}

/// An element's rect on the page (points), as the drag in progress would leave it.
pub fn element_rect(state: &CanvasState, el: &notecraft_doc::PageElement) -> Option<notecraft_doc::Rect> {
    let r = match &el.kind {
        ElementKind::Image(i) => notecraft_doc::Rect::from_min_size(i.pos, i.size),
        // The pages are drawn where they are: `outline.move` moves them with the printout.
        ElementKind::Printout(p) => p.bounds(),
        ElementKind::EmbeddedFile { .. } => el.bounds()?,
        _ => return None,
    };
    Some(pointer::preview_rect(state, el.id, r))
}

/// Pictures, printouts and files, back to front.
pub fn paint(ctx: &egui::Context, painter: &Painter, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View) {
    let ink = Ink::new(tokens, page.background.color);
    let tex = textures(ctx);
    let mut tex = match tex.lock() {
        Ok(t) => t,
        Err(p) => p.into_inner(),
    };
    tex.frame += 1;
    for el in &page.elements {
        match &el.kind {
            ElementKind::Image(img) => {
                let Some(r) = element_rect(state, el) else { continue };
                // While it is being cropped the whole picture shows (the crop dims the rest).
                let (r, uv) = match &state.crop {
                    Some(c) if c.id == el.id => (c.full, full_uvs(&img.transform)),
                    _ => (r, uvs(&img.transform)),
                };
                let sr = v.rect(r);
                if !sr.intersects(v.clip) {
                    continue;
                }
                draw_transformed(painter, texture(ctx, &mut tex, el.id, 0, img), sr, uv, &ink, tokens);
            }
            ElementKind::Printout(p) => {
                // Pages sit at their own positions (`outline.move` moves them); a drag in
                // progress shifts them all.
                let base = notecraft_doc::Rect::from_min_size(p.pos, notecraft_doc::Size::ZERO);
                let moved = pointer::preview_rect(state, el.id, base);
                let (dx, dy) = (moved.x - base.x, moved.y - base.y);
                if let Some(first) = p.pages.first() {
                    // The file's name over the first page.
                    let at = v.pt(first.pos.x + dx, first.pos.y + dy);
                    let font = FontId::proportional(super::style::px(9.0, v.scale));
                    let grey = mix(ink.paper, ink.text, 0.6);
                    let label_y = at.y - v.len(4.0);
                    let icon = Rect::from_min_size(pos2(at.x, label_y - v.len(10.0)), vec2(v.len(8.0), v.len(10.0)));
                    painter.rect_stroke(icon, 0.0, Stroke::new(1.0, grey), StrokeKind::Inside);
                    painter.text(pos2(icon.right() + v.len(4.0), label_y), Align2::LEFT_BOTTOM, &p.file_name, font, grey);
                }
                for (i, pg) in p.pages.iter().enumerate() {
                    let r = notecraft_doc::Rect::from_min_size(pg.pos, pg.size).translate(dx, dy);
                    let sr = v.rect(r);
                    if !sr.intersects(v.clip) {
                        continue;
                    }
                    draw_transformed(painter, texture(ctx, &mut tex, el.id, i, pg), sr, uvs(&pg.transform), &ink, tokens);
                    painter.rect_stroke(sr, 0.0, Stroke::new(1.0, mix(ink.paper, ink.text, 0.25)), StrokeKind::Outside);
                }
            }
            ElementKind::EmbeddedFile { name, .. } => {
                let Some(r) = element_rect(state, el) else { continue };
                let sr = v.rect(r);
                if !sr.intersects(v.clip) {
                    continue;
                }
                paint_file(painter, sr, name, &ink, tokens, v);
            }
            _ => {}
        }
    }
    let frame = tex.frame;
    let live: Vec<Guid> = page.elements.iter().map(|e| e.id).collect();
    tex.map.retain(|(id, _), (_, _, used)| live.contains(id) || frame.saturating_sub(*used) < KEEP_FRAMES);
}

/// Texture coordinates of a picture's displayed corners (top-left, clockwise): its rotation,
/// flips and crop.
pub fn uvs(t: &notecraft_doc::PictureTransform) -> [Pos2; 4] {
    t.sanitized().uv_corners().map(|(u, v)| pos2(u, v))
}

/// The same for the whole, uncropped picture.
fn full_uvs(t: &notecraft_doc::PictureTransform) -> [Pos2; 4] {
    uvs(&notecraft_doc::PictureTransform { crop: None, ..*t })
}

/// Draws a picture into `sr` with its corners taking texture coordinates `uv` (top-left,
/// clockwise): rotation and flips without touching the file.
fn draw_transformed(painter: &Painter, t: Option<TextureHandle>, sr: Rect, uv: [Pos2; 4], ink: &Ink, tokens: &Tokens) {
    match t {
        Some(t) => {
            let mut mesh = egui::Mesh::with_texture(t.id());
            for (p, uv) in corners(sr).into_iter().zip(uv) {
                mesh.vertices.push(egui::epaint::Vertex { pos: p, uv, color: Color32::WHITE });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        None => {
            // A picture NoteCraft can't read: a grey box with a cross.
            let fill = mix(ink.paper, ink.text, 0.08);
            painter.rect(sr, 0.0, fill, Stroke::new(1.0, tokens.border_strong), StrokeKind::Inside);
            let s = Stroke::new(1.0, tokens.border_strong);
            painter.line_segment([sr.left_top(), sr.right_bottom()], s);
            painter.line_segment([sr.right_top(), sr.left_bottom()], s);
        }
    }
}

/// An attached file: a page-with-folded-corner icon and its name.
fn paint_file(painter: &Painter, sr: Rect, name: &str, ink: &Ink, tokens: &Tokens, v: &View) {
    let icon_h = (sr.height() - v.len(4.0)).max(8.0);
    let icon = Rect::from_min_size(sr.min + vec2(v.len(2.0), v.len(2.0)), vec2(icon_h * 0.78, icon_h));
    let fold = icon.width() * 0.32;
    let line = mix(ink.paper, ink.text, 0.55);
    let pts = vec![
        icon.left_top(),
        pos2(icon.right() - fold, icon.top()),
        pos2(icon.right(), icon.top() + fold),
        icon.right_bottom(),
        icon.left_bottom(),
        icon.left_top(),
    ];
    painter.add(egui::Shape::convex_polygon(pts[..5].to_vec(), mix(ink.paper, Color32::WHITE, 0.6), Stroke::NONE));
    painter.add(egui::Shape::line(pts, Stroke::new(1.0, line)));
    painter.line_segment([pos2(icon.right() - fold, icon.top()), pos2(icon.right() - fold, icon.top() + fold)], Stroke::new(1.0, line));
    painter.line_segment([pos2(icon.right() - fold, icon.top() + fold), pos2(icon.right(), icon.top() + fold)], Stroke::new(1.0, line));
    painter.rect_filled(
        Rect::from_min_size(pos2(icon.left() + 2.0, icon.center().y), vec2(icon.width() - 4.0, icon.height() * 0.18)),
        0.0,
        tokens.accent,
    );
    let font = FontId::proportional(super::style::px(10.0, v.scale));
    painter.text(pos2(icon.right() + v.len(6.0), icon.center().y), Align2::LEFT_CENTER, name, font, ink.text);
}

/// The frame of the selected item and a picture's corner handles.
pub fn paint_selection(painter: &Painter, state: &CanvasState, page: &Page, tokens: &Tokens, v: &View) {
    for id in &state.selection.elements {
        if state.crop.as_ref().is_some_and(|c| c.id == *id) {
            continue;
        }
        let Some(el) = page.element(*id) else { continue };
        let Some(r) = element_rect(state, el) else { continue };
        let sr = v.rect(r);
        painter.rect_stroke(sr.expand(1.0), 0.0, Stroke::new(1.0, tokens.handle), StrokeKind::Outside);
        if matches!(el.kind, ElementKind::Image(_)) {
            for c in corners(sr) {
                let h = Rect::from_center_size(c, vec2(HANDLE, HANDLE));
                painter.rect(h, 0.0, tokens.page, Stroke::new(1.0, tokens.handle), StrokeKind::Inside);
            }
        }
    }
}

fn corners(r: Rect) -> [Pos2; 4] {
    [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()]
}

fn selected_picture<'a>(state: &CanvasState, page: &'a Page) -> Option<&'a notecraft_doc::PageElement> {
    let [id] = state.selection.elements.as_slice() else { return None };
    page.element(*id).filter(|e| matches!(e.kind, ElementKind::Image(_)))
}

/// A corner handle of the selected picture under `at`: (picture, corner, its rect).
pub fn handle_at(state: &CanvasState, session: &Session, v: &View, at: Pos2) -> Option<(Guid, u8, notecraft_doc::Rect)> {
    if state.crop.is_some() {
        return None;
    }
    let page = session.current_page()?;
    let el = selected_picture(state, page)?;
    let r = element_rect(state, el)?;
    let sr = v.rect(r);
    let c = corners(sr).iter().position(|c| Rect::from_center_size(*c, vec2(HANDLE + 4.0, HANDLE + 4.0)).contains(at))?;
    Some((el.id, u8::try_from(c).ok()?, r))
}

/// The handle under the pointer (for the cursor).
pub fn corner_hover(state: &CanvasState, session: &Session, v: &View, at: Pos2) -> Option<u8> {
    handle_at(state, session, v, at).map(|(_, c, _)| c)
}

/// The top-most picture or file under `at` (printouts too with the Select tool: with Type a
/// click on a printout starts a note on it).
pub fn element_at(state: &CanvasState, session: &Session, v: &View, at: Pos2) -> Option<Guid> {
    let page = session.current_page()?;
    page.elements.iter().rev().find_map(|el| {
        let pick = match el.kind {
            ElementKind::Image(_) | ElementKind::EmbeddedFile { .. } => true,
            ElementKind::Printout(_) => state.tool == Tool::Select,
            _ => false,
        };
        (pick && element_rect(state, el).is_some_and(|r| v.rect(r).contains(at))).then_some(el.id)
    })
}

/// Whether a selectable picture is under `at`.
pub fn over_picture(state: &CanvasState, session: &Session, v: &View, at: Pos2) -> bool {
    element_at(state, session, v, at).is_some()
}

/// `start` resized by dragging `corner` to `p`, keeping its shape (the opposite corner stays).
pub fn resized(start: notecraft_doc::Rect, corner: u8, p: Point) -> notecraft_doc::Rect {
    let (l, t, r, b) = (start.x, start.y, start.right(), start.bottom());
    let (ox, oy) = match corner % 4 {
        0 => (r, b),
        1 => (l, b),
        2 => (l, t),
        _ => (r, t),
    };
    let (w0, h0) = (start.w.max(1.0), start.h.max(1.0));
    let k = ((p.x - ox).abs() / w0).max((p.y - oy).abs() / h0).clamp(8.0 / w0.min(h0), 50.0);
    let (w, h) = (w0 * k, h0 * k);
    let x = if corner.is_multiple_of(4) || corner % 4 == 3 { ox - w } else { ox };
    let y = if corner % 4 < 2 { oy - h } else { oy };
    notecraft_doc::Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_resize_keeps_shape() {
        let s = notecraft_doc::Rect::new(10.0, 10.0, 100.0, 50.0);
        let r = resized(s, 2, Point::new(210.0, 30.0));
        assert_eq!((r.x, r.y, r.w, r.h), (10.0, 10.0, 200.0, 100.0));
        let r = resized(s, 0, Point::new(-90.0, 0.0));
        assert_eq!((r.right(), r.bottom()), (110.0, 60.0));
        assert!((r.w / r.h - 2.0).abs() < 1e-4);
        assert!(decode(b"not a picture").is_none());
    }
}
