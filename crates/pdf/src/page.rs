//! One page in print: laid out once, then drawn onto each sheet it covers, in the canvas's
//! order: paper and rule lines, the title block, pictures, printouts and attached files, the
//! highlighter, the text, then the other ink.

use std::collections::HashMap;
use std::sync::Arc;

use craft_pdf::write::backend as k;
use k::image::Image;
use k::surface::Surface;
use notecraft_doc::page::TITLE_ORIGIN;
use notecraft_doc::{Color, ElementKind, ImageElement, ImageFormat, InkTool, ListStyle, Page, Tag};
use notecraft_geom::Rect;

use crate::draw;
use crate::fonts::FontSet;
use crate::ink;
use crate::style::{self, DATE_PT, Ink, MARGIN_X, MARKER_W, Marker, TAG_W, TITLE_PT, TITLE_RULE_MIN, mix};
use crate::text::{self, Block, LinkArea, OutlineLay};

/// Most pixels a picture may decode to when it has to be re-encoded (BMP and the like).
const MAX_DECODE_PIXELS: u64 = 64_000_000;

/// The title block: the title, the rule under it, the date and time.
pub struct TitleLay {
    pub rect: Rect,
    pub title: Block,
    pub date: Block,
    pub time: Block,
}

/// A page laid out for print.
pub struct PageLay<'a> {
    pub page: &'a Page,
    pub ink: Ink,
    pub title: Option<TitleLay>,
    pub outlines: Vec<OutlineLay>,
    /// Everything drawn, page points.
    pub bounds: Rect,
    /// Text rows' vertical spans (sheet breaks avoid cutting them).
    pub rows: Vec<(f32, f32)>,
    /// Every drawn item's box (empty sheets are skipped).
    pub boxes: Vec<Rect>,
}

pub fn title_block(fs: &mut FontSet, page: &Page, ink: &Ink, utc_offset_minutes: i32) -> TitleLay {
    let title = text::simple(fs, &page.title, text::plain_look(TITLE_PT, ink.text));
    let (d, t) = style::date_text(page.created, utc_offset_minutes);
    let date = text::simple(fs, &d, text::plain_look(DATE_PT, ink.date()));
    let time = text::simple(fs, &t, text::plain_look(DATE_PT, ink.date()));
    let w = (title.width() + 12.0).max(TITLE_RULE_MIN);
    let h = title.height() + 4.0;
    TitleLay { rect: Rect::new(TITLE_ORIGIN.x, TITLE_ORIGIN.y, w, h), title, date, time }
}

/// A printout's label (its file name over the first page) and an attached file's name.
fn label(fs: &mut FontSet, name: &str, size: f32, color: Color) -> Block {
    text::simple(fs, name, text::plain_look(size, color))
}

pub fn lay_out<'a>(fs: &mut FontSet, page: &'a Page, header: bool, utc_offset_minutes: i32) -> PageLay<'a> {
    let ink = Ink::new(page.background.color);
    let title = header.then(|| title_block(fs, page, &ink, utc_offset_minutes));
    let mut boxes = Vec::new();
    let mut rows = Vec::new();
    if let Some(t) = &title {
        boxes.push(Rect::new(t.rect.x, t.rect.y, t.rect.w, t.rect.h + 5.0 + t.date.height()));
        rows.push((t.rect.y, t.rect.y + t.rect.h + 5.0 + t.date.height()));
    }
    let mut outlines = Vec::new();
    for el in &page.elements {
        match &el.kind {
            ElementKind::Outline(o) => {
                let lay = text::layout_outline(fs, o, &ink);
                text::row_spans(&lay, &mut rows);
                boxes.push(lay.rect);
                outlines.push(lay);
            }
            ElementKind::Printout(p) => {
                let r = p.bounds();
                boxes.push(Rect::new(r.x, r.y - 16.0, r.w, r.h + 16.0));
            }
            ElementKind::Opaque { .. } => {}
            _ => boxes.extend(el.bounds()),
        }
    }
    boxes.extend(page.ink.iter().filter_map(notecraft_doc::InkStroke::bounds));
    let boxes: Vec<Rect> = boxes.into_iter().filter(|r| [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite())).collect();
    let bounds = boxes.iter().copied().reduce(Rect::union).unwrap_or(Rect::new(TITLE_ORIGIN.x, TITLE_ORIGIN.y, 1.0, 1.0));
    PageLay { page, ink, title, outlines, bounds, rows, boxes }
}

/// Pictures embedded once per export, by their data.
#[derive(Default)]
pub struct Images {
    /// By the picture data and how it is drawn (rotation, flips, crop: see
    /// [`notecraft_doc::PictureTransform`]).
    map: HashMap<(usize, [u32; 6]), Option<Image>>,
}

impl Images {
    fn get(&mut self, img: &ImageElement) -> Option<Image> {
        let t = img.transform.sanitized();
        let c = t.crop.map_or([0.0, 0.0, 1.0, 1.0], |c| [c.left, c.top, c.right, c.bottom]);
        let shape = [
            u32::from(t.rotation) | (u32::from(t.flip_h) << 2) | (u32::from(t.flip_v) << 3),
            0,
            c[0].to_bits(),
            c[1].to_bits(),
            c[2].to_bits(),
            c[3].to_bits(),
        ];
        let key = (Arc::as_ptr(&img.data) as usize, shape);
        self.map.entry(key).or_insert_with(|| decode(img)).clone()
    }
}

fn decode(img: &ImageElement) -> Option<Image> {
    // A rotated, flipped or cropped picture is embedded as it is drawn.
    let data: Arc<Vec<u8>> = match notecraft_formats::picture::bake(img) {
        Some(png) => Arc::new(png),
        None if !img.transform.is_identity() => return None,
        None => img.data.clone(),
    };
    let direct = match ImageFormat::sniff(&data) {
        ImageFormat::Png => Image::from_png(data.clone().into(), true).ok(),
        ImageFormat::Jpeg => Image::from_jpeg(data.clone().into(), true).ok(),
        ImageFormat::Gif => Image::from_gif(data.clone().into(), true).ok(),
        _ => None,
    };
    if direct.is_some() {
        return direct;
    }
    // Anything else (BMP, WebP…), or a file Krilla can't read: decode and embed the pixels.
    let reader = image::ImageReader::new(std::io::Cursor::new(data.as_slice())).with_guessed_format().ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    if u64::from(w) * u64::from(h) > MAX_DECODE_PIXELS || w == 0 || h == 0 {
        return None;
    }
    let rgba = image::load_from_memory(&data).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(Image::from_rgba8(rgba.into_raw(), w, h))
}

fn visible(r: Rect, clip: Rect) -> bool {
    r.x <= clip.x + clip.w && r.x + r.w >= clip.x && r.y <= clip.y + clip.h && r.y + r.h >= clip.y
}

/// Draws the page's content that falls in `clip` (page points), in page coordinates.
pub fn draw(s: &mut Surface, fs: &mut FontSet, images: &mut Images, lay: &PageLay<'_>, clip: Rect, links: &mut Vec<LinkArea>) {
    let page = lay.page;
    let ink = &lay.ink;
    rule_lines(s, page, ink, clip);
    if let Some(t) = &lay.title {
        draw_title(s, fs, t, ink, clip, links);
    }
    for el in &page.elements {
        match &el.kind {
            ElementKind::Image(img) => {
                let r = Rect::from_min_size(img.pos.sanitized(), img.size.sanitized());
                if visible(r, clip) {
                    picture(s, images, img, r, ink);
                }
            }
            ElementKind::Printout(p) => {
                if let Some(first) = p.pages.first() {
                    let at = first.pos.sanitized();
                    let grey = mix(ink.paper, ink.text, 0.6);
                    let label_y = at.y - 4.0;
                    draw::stroke_rect(s, at.x + 0.5, label_y - 10.0 + 0.5, 7.0, 9.0, 1.0, grey);
                    let name = label(fs, &p.file_name, 9.0, grey);
                    text::draw_block(s, fs, &name, at.x + 12.0, label_y - name.height(), clip, links);
                }
                for pg in &p.pages {
                    let r = Rect::from_min_size(pg.pos.sanitized(), pg.size.sanitized());
                    if visible(r, clip) {
                        picture(s, images, pg, r, ink);
                        draw::stroke_rect(s, r.x - 0.5, r.y - 0.5, r.w + 1.0, r.h + 1.0, 1.0, mix(ink.paper, ink.text, 0.25));
                    }
                }
            }
            ElementKind::EmbeddedFile { name, .. } => {
                if let Some(r) = el.bounds()
                    && visible(r, clip)
                {
                    file_icon(s, fs, r, name, ink, clip, links);
                }
            }
            ElementKind::Outline(_) | ElementKind::Opaque { .. } => {}
        }
    }
    ink_layer(s, page, ink, clip, true);
    for o in &lay.outlines {
        if !visible(Rect::new(o.rect.x - 40.0, o.rect.y, o.rect.w + 40.0, o.rect.h), clip) {
            continue;
        }
        paras(s, fs, &o.paras, clip, links);
        for t in &o.tables {
            if visible(t.rect, clip) {
                table(s, fs, t, ink, clip, links);
            }
        }
    }
    ink_layer(s, page, ink, clip, false);
}

fn paras(s: &mut Surface, fs: &mut FontSet, ps: &[text::ParaLay], clip: Rect, links: &mut Vec<LinkArea>) {
    for p in ps {
        if p.top > clip.y + clip.h || p.top + p.height < clip.y {
            continue;
        }
        marker(s, fs, p, clip, links);
        tag(s, fs, p, clip, links);
        text::draw_block(s, fs, &p.block, p.x + p.align_dx, p.top, clip, links);
    }
}

/// A table: cell shading, the cells' text, then the grid (hidden borders aren't printed, as in
/// OneNote).
fn table(s: &mut Surface, fs: &mut FontSet, t: &text::TableLay, ink: &Ink, clip: Rect, links: &mut Vec<LinkArea>) {
    for c in &t.cells {
        if let Some(sh) = c.shading
            && visible(c.rect, clip)
        {
            // On a dark page, darkened towards the paper so the light text stays readable.
            let sh = if ink.dark { mix(ink.paper, sh, 0.35) } else { sh };
            draw::fill_rect(s, c.rect.x, c.rect.y, c.rect.w, c.rect.h, sh);
        }
    }
    for c in &t.cells {
        if visible(c.rect, clip) {
            paras(s, fs, &c.paras, clip, links);
        }
    }
    if !t.borders {
        return;
    }
    let grey = mix(ink.paper, ink.text, 0.45);
    let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (t.col_x.first(), t.col_x.last(), t.row_y.first(), t.row_y.last()) else { return };
    for &y in &t.row_y {
        draw::line(s, (x0, y), (x1, y), 0.75, grey);
    }
    for &x in &t.col_x {
        draw::line(s, (x, y0), (x, y1), 0.75, grey);
    }
}

fn rule_lines(s: &mut Surface, page: &Page, ink: &Ink, clip: Rect) {
    let Some(step) = page.background.rule.spacing() else { return };
    let (line, margin) = ink.rules(page.background.color);
    let (x0, x1, y0, y1) = (clip.x, clip.x + clip.w, clip.y, clip.y + clip.h);
    let first = (y0 / step).floor().max(1.0) as i64;
    let last = (y1 / step).ceil() as i64;
    for k in first..=last.min(first.saturating_add(4000)) {
        let y = k as f32 * step;
        draw::line(s, (x0, y), (x1, y), 0.75, line);
    }
    if page.background.rule.is_grid() {
        let first = (x0 / step).floor().max(1.0) as i64;
        let last = (x1 / step).ceil() as i64;
        for k in first..=last.min(first.saturating_add(4000)) {
            let x = k as f32 * step;
            draw::line(s, (x, y0), (x, y1), 0.75, line);
        }
    } else if MARGIN_X >= x0 && MARGIN_X <= x1 {
        draw::line(s, (MARGIN_X, y0), (MARGIN_X, y1), 0.75, margin);
    }
}

fn draw_title(s: &mut Surface, fs: &mut FontSet, t: &TitleLay, ink: &Ink, clip: Rect, links: &mut Vec<LinkArea>) {
    text::draw_block(s, fs, &t.title, t.rect.x, t.rect.y, clip, links);
    let rule_y = t.rect.y + t.rect.h;
    draw::line(s, (t.rect.x, rule_y), (t.rect.x + t.rect.w, rule_y), 0.75, ink.title_rule());
    let y = rule_y + 5.0;
    text::draw_block(s, fs, &t.date, t.rect.x, y, clip, links);
    text::draw_block(s, fs, &t.time, t.rect.x + t.date.width() + 22.0, y, clip, links);
}

fn picture(s: &mut Surface, images: &mut Images, img: &ImageElement, r: Rect, ink: &Ink) {
    let size = k::geom::Size::from_wh(r.w, r.h);
    match (images.get(img), size) {
        (Some(image), Some(size)) => {
            s.push_transform(&draw::translate(r.x, r.y));
            s.draw_image(image, size);
            s.pop();
        }
        _ if r.w > 0.0 && r.h > 0.0 => {
            // A picture NoteCraft can't read: a grey box with a cross, as on the canvas.
            let border = Color::rgb(0xA8, 0xA8, 0xA8);
            draw::fill_rect(s, r.x, r.y, r.w, r.h, mix(ink.paper, ink.text, 0.08));
            draw::stroke_rect(s, r.x + 0.5, r.y + 0.5, r.w - 1.0, r.h - 1.0, 1.0, border);
            draw::line(s, (r.x, r.y), (r.x + r.w, r.y + r.h), 1.0, border);
            draw::line(s, (r.x + r.w, r.y), (r.x, r.y + r.h), 1.0, border);
        }
        _ => {}
    }
}

/// An attached file: a page-with-folded-corner icon and its name (the canvas's `paint_file`).
fn file_icon(s: &mut Surface, fs: &mut FontSet, r: Rect, name: &str, ink: &Ink, clip: Rect, links: &mut Vec<LinkArea>) {
    let icon_h = (r.h - 4.0).max(8.0);
    let (ix, iy, iw) = (r.x + 2.0, r.y + 2.0, icon_h * 0.78);
    let fold = iw * 0.32;
    let line = mix(ink.paper, ink.text, 0.55);
    let outline = [(ix, iy), (ix + iw - fold, iy), (ix + iw, iy + fold), (ix + iw, iy + icon_h), (ix, iy + icon_h)];
    let mut pb = k::geom::PathBuilder::new();
    draw::polygon(&mut pb, &outline);
    if let Some(p) = pb.finish() {
        draw::fill_path(s, &p, draw::fill(mix(ink.paper, style::WHITE, 0.6), 1.0));
        draw::stroke_path(s, &p, draw::stroke(line, 1.0));
    }
    draw::polyline(s, &[(ix + iw - fold, iy), (ix + iw - fold, iy + fold), (ix + iw, iy + fold)], 1.0, line);
    draw::fill_rect(s, ix + 2.0, iy + icon_h / 2.0, iw - 4.0, icon_h * 0.18, style::ACCENT);
    let b = label(fs, name, 10.0, ink.text);
    text::draw_block(s, fs, &b, ix + iw + 6.0, iy + icon_h / 2.0 - b.height() / 2.0, clip, links);
}

fn marker(s: &mut Surface, fs: &mut FontSet, p: &text::ParaLay, clip: Rect, links: &mut Vec<LinkArea>) {
    let Some((m, label)) = &p.marker else { return };
    let (a, b) = p.first_row();
    let cy = (a + b) / 2.0;
    let color = p.block.sections.first().map_or(Color::BLACK, |s| s.look.color);
    match m {
        Marker::Bullet(style) => {
            // The canvas's shapes (`icons::paint_bullet`).
            let r = (p.size * 0.15).clamp(1.5, 12.0);
            let cx = p.marker_x + MARKER_W * 0.4;
            match style {
                ListStyle::Circle => draw::stroke_disc(s, cx, cy, r - 0.5, (r * 0.35).max(1.0), color),
                ListStyle::Square => draw::fill_rect(s, cx - r * 0.85, cy - r * 0.85, r * 1.7, r * 1.7, color),
                ListStyle::Dash => draw::line(s, (cx - r * 1.4, cy), (cx + r * 1.4, cy), (r * 0.4).max(1.0), color),
                ListStyle::Arrow => {
                    let w = r * 1.5;
                    let pts = [(cx - w * 0.7, cy - w), (cx + w * 0.9, cy), (cx - w * 0.7, cy + w), (cx - w * 0.2, cy)];
                    let mut pb = k::geom::PathBuilder::new();
                    draw::polygon(&mut pb, &pts);
                    if let Some(path) = pb.finish() {
                        draw::fill_path(s, &path, draw::fill(color, 1.0));
                    }
                }
                _ => draw::fill_disc(s, cx, cy, r, color),
            }
        }
        Marker::Number(_) => {
            let x = p.x - 4.0 - label.width();
            text::draw_block(s, fs, label, x, cy - label.height() / 2.0, clip, links);
        }
    }
}

/// The To Do box (ticked in the accent colour), and the other tags' icons, simplified from the
/// canvas's drawings (`icons::deck::extra`). The highlight tags have no icon: their colour is
/// behind the text.
fn tag(s: &mut Surface, fs: &mut FontSet, p: &text::ParaLay, clip: Rect, links: &mut Vec<LinkArea>) {
    let Some(tag) = p.tag.filter(|t| t.has_icon()) else { return };
    let (a, b) = p.first_row();
    let side = (p.size * 0.85).clamp(6.0, 60.0);
    let (cx, cy) = (p.tag_x + TAG_W / 2.0, (a + b) / 2.0);
    match tag {
        Tag::ToDo { checked } => {
            let (x, y) = (cx - side / 2.0, cy - side / 2.0);
            draw::fill_rect(s, x, y, side, side, style::WHITE);
            draw::stroke_rect(s, x + 0.5, y + 0.5, side - 1.0, side - 1.0, 1.0, Color::rgb(0x6B, 0x7A, 0x96));
            if checked {
                let pts = [(x + side * 0.2, y + side * 0.52), (x + side * 0.42, y + side * 0.74), (x + side * 0.84, y + side * 0.2)];
                draw::polyline(s, &pts, (side * 0.15).max(1.5), style::ACCENT);
            }
        }
        Tag::Important => {
            let outer = side * 0.6;
            let inner = outer * 0.45;
            let pts: Vec<(f32, f32)> = (0..10)
                .map(|k| {
                    let ang = std::f32::consts::PI * (k as f32) / 5.0 - std::f32::consts::FRAC_PI_2;
                    let rad = if k % 2 == 0 { outer } else { inner };
                    (cx + rad * ang.cos(), cy + rad * ang.sin())
                })
                .collect();
            let gold = Color::rgb(0xF2, 0xB7, 0x05);
            let mut pb = k::geom::PathBuilder::new();
            draw::polygon(&mut pb, &pts);
            if let Some(path) = pb.finish() {
                draw::fill_path(s, &path, draw::fill(gold, 1.0));
                draw::stroke_path(s, &path, draw::stroke(mix(gold, Color::BLACK, 0.3), 1.0));
            }
        }
        Tag::Question => {
            let r = side * 0.55;
            draw::fill_disc(s, cx, cy, r, Color::rgb(0x7A, 0x4F, 0xC9));
            let q = text::simple(fs, "?", text::plain_look(r * 1.6, style::WHITE));
            text::draw_block(s, fs, &q, cx - q.width() / 2.0, cy + r * 0.05 - q.height() / 2.0, clip, links);
        }
        Tag::Critical => {
            let r = side * 0.55;
            draw::fill_disc(s, cx, cy, r, Color::rgb(0xD2, 0x3B, 0x2E));
            draw::line(s, (cx, cy - r * 0.55), (cx, cy + r * 0.15), (r * 0.22).max(1.0), style::WHITE);
            draw::fill_disc(s, cx, cy + r * 0.5, (r * 0.13).max(0.6), style::WHITE);
        }
        Tag::Idea => {
            let r = side * 0.36;
            let gold = Color::rgb(0xF6, 0xC3, 0x2C);
            draw::fill_disc(s, cx, cy - r * 0.35, r, mix(gold, style::WHITE, 0.55));
            draw::stroke_disc(s, cx, cy - r * 0.35, r, 1.0, ink_line());
            draw::stroke_rect(s, cx - r * 0.42, cy + r * 0.62, r * 0.84, r * 0.55, 1.0, ink_line());
        }
        Tag::Contact => {
            let (w, h) = (side * 0.95, side * 0.7);
            draw::stroke_rect(s, cx - w / 2.0, cy - h / 2.0, w, h, 1.0, ink_line());
            draw::fill_disc(s, cx - w * 0.22, cy - h * 0.12, h * 0.17, BLUE);
            draw::fill_rect(s, cx - w * 0.38, cy + h * 0.1, w * 0.32, h * 0.22, BLUE);
            draw::line(s, (cx + w * 0.05, cy - h * 0.12), (cx + w * 0.38, cy - h * 0.12), 1.0, ink_line());
            draw::line(s, (cx + w * 0.05, cy + h * 0.15), (cx + w * 0.3, cy + h * 0.15), 1.0, ink_line());
        }
        Tag::Address => {
            let w = side * 0.8;
            draw::fill_rect(s, cx - w * 0.36, cy - w * 0.05, w * 0.72, w * 0.5, mix(BLUE, style::WHITE, 0.75));
            draw::stroke_rect(s, cx - w * 0.36, cy - w * 0.05, w * 0.72, w * 0.5, 1.0, ink_line());
            draw::polyline(s, &[(cx - w * 0.5, cy + w * 0.02), (cx, cy - w * 0.42), (cx + w * 0.5, cy + w * 0.02)], 1.0, ink_line());
            draw::fill_rect(s, cx - w * 0.08, cy + w * 0.17, w * 0.16, w * 0.28, BLUE);
        }
        Tag::Phone => {
            let (w, h) = (side * 0.45, side * 0.85);
            draw::stroke_rect(s, cx - w / 2.0, cy - h / 2.0, w, h, 1.0, ink_line());
            draw::fill_rect(s, cx - w * 0.32, cy - h * 0.36, w * 0.64, h * 0.6, mix(BLUE, style::WHITE, 0.75));
            draw::fill_disc(s, cx, cy + h * 0.38, (w * 0.08).max(0.6), ink_line());
        }
        Tag::RememberForLater | Tag::Definition | Tag::Highlight => {}
    }
}

/// The tag icons' line colour (the canvas's light-theme icon ink).
fn ink_line() -> Color {
    Color::rgb(0x44, 0x44, 0x4A)
}

/// The tag icons' blue accent.
const BLUE: Color = Color::rgb(0x2F, 0x7B, 0xE0);

fn ink_layer(s: &mut Surface, page: &Page, ink: &Ink, clip: Rect, highlighter: bool) {
    for st in &page.ink {
        if (st.tool == InkTool::Highlighter) != highlighter {
            continue;
        }
        if st.bounds().is_some_and(|b| !visible(b, clip)) {
            continue;
        }
        let (c, a) = ink::stroke_paint(st, ink);
        draw::fill_pieces(s, &ink::outline(st), c, a);
    }
}
