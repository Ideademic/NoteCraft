//! Laying out note containers: each paragraph becomes an egui galley (rich text, wrapped at the
//! container's width less its indent, list marker and tag), stacked down the container.
//!
//! Caching: building a paragraph's [`LayoutJob`] (runs → formats) is cached per paragraph, keyed
//! by a hash of its content, the wrap width, the zoom and the text colours; a whole container's
//! jobs are reused without hashing while `Session::revision` hasn't changed. Galleys come from
//! egui's own memoised galley cache every frame (cheap: a hash lookup), because a galley kept
//! across frames could point into a font atlas egui has since rebuilt.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use egui::text::{CCursor, LayoutJob};
use egui::{Galley, Pos2, Rect, pos2, vec2};
use notecraft_doc::{ElementKind, Guid, Outline, Paragraph, Point, Tag};
use notecraft_engine::{Session, TextPos};

use super::style::{self, HEADING_SPACE, INDENT_STEP, Ink, MARKER_W, Marker, TAG_W};
use super::view::View;
use super::{CanvasState, pointer};
use crate::theme::Tokens;

/// One laid-out paragraph. Offsets are screen pixels from the container's top-left corner.
#[derive(Clone)]
pub struct ParaLayout {
    pub top: f32,
    pub height: f32,
    /// Where the text area starts (after the indent, tag and marker).
    pub text_x: f32,
    /// Where the galley's origin is from `text_x`: centred and right-aligned rows sit around it.
    pub align_dx: f32,
    pub galley: Arc<Galley>,
    /// Only the bold runs whose face has no bold, painted again `bold_dx` to the right.
    pub bold: Option<Arc<Galley>>,
    pub bold_dx: f32,
    pub marker: Option<Marker>,
    pub marker_x: f32,
    pub tag: Option<Tag>,
    pub tag_x: f32,
    /// Font size of the paragraph's first run, pixels (markers and tags are sized from it).
    pub size_px: f32,
    pub chars: usize,
}

impl ParaLayout {
    /// The galley's origin, pixels from the container's left.
    pub fn galley_x(&self) -> f32 {
        self.text_x + self.align_dx
    }

    /// The first line's vertical span, pixels from the container's top.
    pub fn first_row(&self) -> (f32, f32) {
        let r = self.galley.rows.first().map_or(Rect::from_min_size(Pos2::ZERO, vec2(0.0, self.height)), |r| r.rect());
        (self.top + r.top(), self.top + r.bottom())
    }
}

/// One laid-out note container.
#[derive(Clone)]
pub struct OutlineLayout {
    pub id: Guid,
    /// Top-left, points (a container being dragged has its preview position here).
    pub pos: Point,
    /// Width, points.
    pub width: f32,
    pub scale: f32,
    pub height_px: f32,
    pub paras: Vec<ParaLayout>,
}

impl OutlineLayout {
    pub fn height_pt(&self) -> f32 {
        if self.scale > 0.0 { self.height_px / self.scale } else { 0.0 }
    }

    /// The text area on screen.
    pub fn rect(&self, v: &View) -> Rect {
        Rect::from_min_size(v.to_screen(self.pos), vec2(self.width * v.scale, self.height_px.max(4.0)))
    }

    /// Screen position of paragraph `i`'s galley origin.
    pub fn galley_origin(&self, v: &View, i: usize) -> Option<Pos2> {
        let p = self.paras.get(i)?;
        Some(v.to_screen(self.pos) + vec2(p.galley_x(), p.top))
    }

    /// The text position nearest to screen point `at`.
    pub fn hit(&self, v: &View, at: Pos2) -> TextPos {
        let top = v.to_screen(self.pos);
        let y = at.y - top.y;
        let mut best = 0;
        for (i, p) in self.paras.iter().enumerate() {
            best = i;
            if y < p.top + p.height {
                break;
            }
        }
        let Some(p) = self.paras.get(best) else { return TextPos::default() };
        let local = vec2(at.x - top.x - p.galley_x(), (y - p.top).clamp(0.0, p.height.max(0.0)));
        let c = p.galley.cursor_from_pos(local);
        TextPos::new(best, c.index.0.min(p.chars))
    }

    /// The caret's rectangle on screen (zero width) at `pos`.
    pub fn caret_rect(&self, v: &View, pos: TextPos) -> Option<Rect> {
        let p = self.paras.get(pos.para)?;
        let o = self.galley_origin(v, pos.para)?;
        let r = p.galley.pos_from_cursor(CCursor::new(pos.char.min(p.chars)));
        Some(r.translate(o.to_vec2()))
    }
}

/// Everything laid out on the page this frame.
#[derive(Clone, Default)]
pub struct PageLayout {
    pub outlines: Vec<OutlineLayout>,
}

impl PageLayout {
    pub fn outline(&self, id: Guid) -> Option<&OutlineLayout> {
        self.outlines.iter().find(|o| o.id == id)
    }
}

/// A paragraph's cached jobs and the geometry that doesn't depend on the galley.
#[derive(Clone)]
struct ParaSpec {
    job: Arc<LayoutJob>,
    bold: Option<Arc<LayoutJob>>,
    text_x: f32,
    align_dx: f32,
    marker_x: f32,
    tag_x: f32,
    marker: Option<Marker>,
    tag: Option<Tag>,
    space_before: f32,
    size_px: f32,
    chars: usize,
}

#[derive(Default)]
struct Cache {
    /// Per paragraph content hash.
    paras: HashMap<u64, ParaSpec>,
    used: HashMap<u64, ()>,
    /// Per container: (revision, width bits, scale bits, ink hash) and its paragraphs' specs.
    outlines: HashMap<Guid, (u64, u32, u32, u64, Arc<Vec<ParaSpec>>)>,
}

fn cache(ctx: &egui::Context) -> Arc<Mutex<Cache>> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::unique("nc_canvas_layout_cache")).clone())
}

/// What the jobs depend on besides the text: the colours, and the fonts (which faces resolve, or
/// egui's own while the provider isn't active).
fn ink_hash(ink: &Ink, fonts: Option<u64>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    fonts.hash(&mut h);
    ink.text.hash(&mut h);
    ink.paper.hash(&mut h);
    ink.link.hash(&mut h);
    h.finish()
}

fn para_hash(p: &Paragraph, marker: &Option<Marker>, wrap_bits: u32, scale_bits: u32, ink: u64) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in &p.runs {
        r.text.hash(&mut h);
        (r.bold, r.italic, r.underline, r.strike).hash(&mut h);
        r.highlight.hash(&mut h);
        r.color.hash(&mut h);
        r.font.hash(&mut h);
        r.size.map(f32::to_bits).hash(&mut h);
        r.link.is_some().hash(&mut h);
    }
    (p.indent, p.list, p.list_style, p.tag, p.style, p.align).hash(&mut h);
    match marker {
        Some(Marker::Bullet(l)) => (1u8, *l).hash(&mut h),
        Some(Marker::Number(s)) => (2u8, s).hash(&mut h),
        None => 0u8.hash(&mut h),
    }
    (wrap_bits, scale_bits, ink).hash(&mut h);
    h.finish()
}

fn specs_for(c: &mut Cache, o: &Outline, width: f32, scale: f32, ink: &Ink, fonts: Option<u64>) -> Vec<ParaSpec> {
    let markers = style::markers(&o.paragraphs);
    let ink_h = ink_hash(ink, fonts);
    let mut out = Vec::with_capacity(o.paragraphs.len());
    for (i, p) in o.paragraphs.iter().enumerate() {
        let marker = markers.get(i).cloned().flatten();
        let indent = f32::from(p.indent.min(notecraft_doc::page::MAX_INDENT)) * INDENT_STEP;
        let tag_w = if p.tag.is_some() { TAG_W } else { 0.0 };
        let marker_w = if p.list.is_some() { MARKER_W } else { 0.0 };
        let text_x_pt = indent + tag_w + marker_w;
        let wrap_px = ((width - text_x_pt).max(24.0) * scale).round();
        let key = para_hash(p, &marker, wrap_px.to_bits(), scale.to_bits(), ink_h);
        c.used.insert(key, ());
        if let Some(s) = c.paras.get(&key) {
            let mut s = s.clone();
            s.space_before = if i > 0 && p.style.heading_level().is_some() { HEADING_SPACE * scale } else { 0.0 };
            out.push(s);
            continue;
        }
        let (job, bold) = style::jobs(p, scale, wrap_px, ink, fonts.is_some());
        let first_size = p.runs.iter().find(|r| !r.text.is_empty()).or(p.runs.first()).map_or(p.style.font_size(), |r| style::size_pt(r, p.style));
        let size_px = style::px(first_size, scale);
        let spec = ParaSpec {
            job: Arc::new(job),
            bold: bold.map(Arc::new),
            text_x: text_x_pt * scale,
            align_dx: style::align_dx(p.align, wrap_px),
            marker_x: (indent + tag_w) * scale,
            tag_x: indent * scale,
            marker,
            tag: p.tag,
            space_before: if i > 0 && p.style.heading_level().is_some() { HEADING_SPACE * scale } else { 0.0 },
            size_px,
            chars: notecraft_engine::text::para_len(p),
        };
        c.paras.insert(key, spec.clone());
        out.push(spec);
    }
    out
}

/// Lays out every note container on the current page (with the preview position or width of
/// one being dragged).
pub fn page_layout(ctx: &egui::Context, state: &CanvasState, session: &Session, tokens: &Tokens, v: &View) -> PageLayout {
    let Some(page) = session.current_page() else { return PageLayout::default() };
    let ink = Ink::new(tokens, page.background.color);
    let scale = v.scale;
    let cache = cache(ctx);
    let mut c = match cache.lock() {
        Ok(c) => c,
        Err(p) => p.into_inner(),
    };
    c.used.clear();
    let fonts = crate::fonts::state(ctx);
    let ink_h = ink_hash(&ink, fonts);
    let mut outlines = Vec::new();
    for el in &page.elements {
        let ElementKind::Outline(o) = &el.kind else { continue };
        let (pos, width) = pointer::preview_geometry(state, el.id, o.pos, o.width);
        let width = if width.is_finite() { width.clamp(24.0, 4000.0) } else { notecraft_doc::page::DEFAULT_OUTLINE_WIDTH };
        let key = (session.revision, width.to_bits(), scale.to_bits(), ink_h);
        let specs = match c.outlines.get(&el.id) {
            Some((r, w, s, i, specs)) if (*r, *w, *s, *i) == key => specs.clone(),
            _ => {
                let specs = Arc::new(specs_for(&mut c, o, width, scale, &ink, fonts));
                c.outlines.insert(el.id, (key.0, key.1, key.2, key.3, specs.clone()));
                specs
            }
        };
        let mut y = 0.0;
        let mut paras = Vec::with_capacity(specs.len());
        for s in specs.iter() {
            y += s.space_before;
            let galley = ctx.fonts_mut(|f| f.layout_job((*s.job).clone()));
            let bold = s.bold.as_ref().map(|b| ctx.fonts_mut(|f| f.layout_job((**b).clone())));
            let h = galley.size().y.max(s.size_px);
            paras.push(ParaLayout {
                top: y,
                height: h,
                text_x: s.text_x,
                align_dx: s.align_dx,
                galley,

                bold,
                bold_dx: style::bold_offset(s.size_px),
                marker: s.marker.clone(),
                marker_x: s.marker_x,
                tag: s.tag,
                tag_x: s.tag_x,
                size_px: s.size_px,
                chars: s.chars,
            });
            y += h;
        }
        outlines.push(OutlineLayout { id: el.id, pos, width, scale, height_px: y, paras });
    }
    // Forget containers that are gone, and paragraph jobs nobody used (bounded memory).
    let live: Vec<Guid> = outlines.iter().map(|o| o.id).collect();
    c.outlines.retain(|id, _| live.contains(id));
    if c.paras.len() > 4 * c.used.len() + 256 {
        // Reused containers hold their specs themselves, so only this frame's rebuilt ones count.
        let mut keep = HashMap::new();
        let used: Vec<u64> = c.used.keys().copied().collect();
        for k in used {
            if let Some(s) = c.paras.remove(&k) {
                keep.insert(k, s);
            }
        }
        c.paras = keep;
    }
    PageLayout { outlines }
}

/// The text position of the screen point in the container under it, if any (with a little
/// slack around the text).
pub fn outline_at<'a>(lay: &'a PageLayout, v: &View, at: Pos2, slack: f32) -> Option<&'a OutlineLayout> {
    lay.outlines.iter().rev().find(|o| o.rect(v).expand(slack).contains(at))
}

/// Screen rectangle of a To Do box in paragraph `i` of `o`.
pub fn todo_box(o: &OutlineLayout, v: &View, i: usize) -> Option<Rect> {
    let p = o.paras.get(i)?;
    if !matches!(p.tag, Some(Tag::ToDo { .. })) {
        return None;
    }
    let (a, b) = p.first_row();
    let side = (p.size_px * 0.85).clamp(6.0, 60.0);
    let top = v.to_screen(o.pos);
    let cx = top.x + p.tag_x + (TAG_W * v.scale - side) / 2.0;
    let cy = top.y + (a + b) / 2.0;
    Some(Rect::from_min_size(pos2(cx, cy - side / 2.0), vec2(side, side)))
}
