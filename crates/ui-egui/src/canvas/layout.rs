//! Laying out note containers: each paragraph becomes an egui galley (rich text, wrapped at the
//! container's width less its indent, list marker and tag), stacked down the container. A table
//! is a grid whose cells are laid out like small containers ([`TableLayout`]); its block keeps a
//! placeholder row in [`OutlineLayout::paras`] as tall as the table, so block indexes line up.
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
use notecraft_doc::table::{CELL_PAD_X, CELL_PAD_Y, TABLE_GAP};
use notecraft_doc::{Block, Color, ElementKind, Guid, Outline, Paragraph, Point, Tag};
use notecraft_engine::text::{CellRef, Frame};
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
    /// A table's placeholder (its text is in [`OutlineLayout::tables`]).
    pub table: bool,
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

/// One laid-out table. Offsets are screen pixels from its container's top-left corner.
#[derive(Clone)]
pub struct TableLayout {
    /// The table's block in its container.
    pub block: usize,
    /// Column edges, left to right (one more than the columns).
    pub col_x: Vec<f32>,
    /// Row edges, top to bottom (one more than the rows).
    pub row_y: Vec<f32>,
    pub borders: bool,
    /// Each cell's text, laid out as a small container (rows, then columns).
    pub cells: Vec<Vec<OutlineLayout>>,
    pub shading: Vec<Vec<Option<Color>>>,
}

impl TableLayout {
    pub fn rows(&self) -> usize {
        self.row_y.len().saturating_sub(1)
    }

    pub fn cols(&self) -> usize {
        self.col_x.len().saturating_sub(1)
    }

    pub fn cell(&self, row: usize, col: usize) -> Option<&OutlineLayout> {
        self.cells.get(row)?.get(col)
    }

    /// The table on screen (its container `o`).
    pub fn rect(&self, o: &OutlineLayout, v: &View) -> Rect {
        let top = v.to_screen(o.pos);
        let (x0, x1) = (self.col_x.first().copied().unwrap_or(0.0), self.col_x.last().copied().unwrap_or(0.0));
        let (y0, y1) = (self.row_y.first().copied().unwrap_or(0.0), self.row_y.last().copied().unwrap_or(0.0));
        Rect::from_min_max(top + vec2(x0, y0), top + vec2(x1, y1))
    }

    /// A cell's box on screen.
    pub fn cell_rect(&self, o: &OutlineLayout, v: &View, row: usize, col: usize) -> Option<Rect> {
        let top = v.to_screen(o.pos);
        let (x0, x1) = (*self.col_x.get(col)?, *self.col_x.get(col + 1)?);
        let (y0, y1) = (*self.row_y.get(row)?, *self.row_y.get(row + 1)?);
        Some(Rect::from_min_max(top + vec2(x0, y0), top + vec2(x1, y1)))
    }

    /// The cell nearest to screen point `at`.
    pub fn cell_at(&self, o: &OutlineLayout, v: &View, at: Pos2) -> (usize, usize) {
        let top = v.to_screen(o.pos);
        let (x, y) = (at.x - top.x, at.y - top.y);
        let pick = |edges: &[f32], p: f32| {
            let n = edges.len().saturating_sub(1);
            (0..n).find(|&i| edges.get(i + 1).is_some_and(|e| p < *e)).unwrap_or(n.saturating_sub(1))
        };
        (pick(&self.row_y, y), pick(&self.col_x, x))
    }

    /// The column border (1-based: the right edge of column `i - 1`) within `slack` pixels of
    /// screen point `at`, if `at` is beside the table's rows.
    pub fn border_at(&self, o: &OutlineLayout, v: &View, at: Pos2, slack: f32) -> Option<usize> {
        let r = self.rect(o, v);
        if at.y < r.top() || at.y > r.bottom() {
            return None;
        }
        let left = v.to_screen(o.pos).x;
        (1..self.col_x.len()).find(|&i| self.col_x.get(i).is_some_and(|x| (left + x - at.x).abs() <= slack))
    }
}

/// One laid-out note container (or table cell, whose `pos` is its text area's corner).
#[derive(Clone)]
pub struct OutlineLayout {
    pub id: Guid,
    /// Top-left, points (a container being dragged has its preview position here).
    pub pos: Point,
    /// Width, points (a table wider than the container widens it).
    pub width: f32,
    pub scale: f32,
    pub height_px: f32,
    /// One per block.
    pub paras: Vec<ParaLayout>,
    pub tables: Vec<TableLayout>,
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

    /// The table at block `block`.
    pub fn table(&self, block: usize) -> Option<&TableLayout> {
        self.tables.iter().find(|t| t.block == block)
    }

    /// The layout of a flow: the container's own, or a cell's.
    pub fn frame(&self, frame: Frame) -> Option<&OutlineLayout> {
        match frame {
            Frame::Top => Some(self),
            Frame::Cell(c) => self.table(c.block)?.cell(c.row, c.col),
        }
    }

    /// The text position nearest to screen point `at` in the container's own blocks (a table's
    /// block gives its index).
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
        if p.table {
            return TextPos::new(best, 0);
        }
        let local = vec2(at.x - top.x - p.galley_x(), (y - p.top).clamp(0.0, p.height.max(0.0)));
        let c = p.galley.cursor_from_pos(local);
        TextPos::new(best, c.index.0.min(p.chars))
    }

    /// The flow and text position nearest to `at`: inside a table's cell when `at` is over it.
    pub fn hit_frame(&self, v: &View, at: Pos2) -> (Frame, TextPos) {
        let pos = self.hit(v, at);
        match self.table(pos.para) {
            Some(t) => {
                let (r, c) = t.cell_at(self, v, at);
                let cp = t.cell(r, c).map_or(TextPos::default(), |cl| cl.hit(v, at));
                (Frame::Cell(CellRef::new(pos.para, r, c)), cp)
            }
            None => (Frame::Top, pos),
        }
    }

    /// The caret's rectangle on screen (zero width) at `pos`.
    pub fn caret_rect(&self, v: &View, pos: TextPos) -> Option<Rect> {
        let p = self.paras.get(pos.para)?;
        let o = self.galley_origin(v, pos.para)?;
        let r = p.galley.pos_from_cursor(CCursor::new(pos.char.min(p.chars)));
        Some(r.translate(o.to_vec2()))
    }

    /// The caret's rectangle in flow `frame`.
    pub fn frame_caret_rect(&self, v: &View, frame: Frame, pos: TextPos) -> Option<Rect> {
        self.frame(frame)?.caret_rect(v, pos)
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

/// A table's cached specs.
#[derive(Clone)]
struct TableSpec {
    /// Indent, pixels.
    x: f32,
    /// Column widths, pixels.
    col_w: Vec<f32>,
    /// Column widths, points.
    col_pt: Vec<f32>,
    borders: bool,
    rows: Vec<Vec<(Option<Color>, Vec<ParaSpec>)>>,
}

#[derive(Clone)]
enum BlockSpec {
    Para(ParaSpec),
    Table(TableSpec),
}

#[derive(Default)]
struct Cache {
    /// Per paragraph content hash.
    paras: HashMap<u64, ParaSpec>,
    used: HashMap<u64, ()>,
    /// Per container: (revision, width bits, scale bits, ink hash) and its blocks' specs.
    outlines: HashMap<Guid, (u64, u32, u32, u64, Arc<Vec<BlockSpec>>)>,
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

/// What a container needs laid out besides its blocks.
struct Env<'a> {
    scale: f32,
    ink: &'a Ink,
    fonts: Option<u64>,
    ink_h: u64,
}

/// Specs for `paras` (a run of consecutive paragraphs) wrapped in `width` points. `first`: the
/// first text of its container or cell (no space above a heading there).
fn para_specs(c: &mut Cache, paras: &[&Paragraph], width: f32, env: &Env<'_>, first: bool) -> Vec<ParaSpec> {
    let scale = env.scale;
    let shapes: Vec<Paragraph> = paras.iter().map(|p| Paragraph { runs: Vec::new(), ..(*p).clone() }).collect();
    let markers = style::markers(&shapes);
    let mut out = Vec::with_capacity(paras.len());
    for (i, p) in paras.iter().enumerate() {
        let space_before = if !(first && i == 0) && p.style.heading_level().is_some() { HEADING_SPACE * scale } else { 0.0 };
        let marker = markers.get(i).cloned().flatten();
        let indent = f32::from(p.indent.min(notecraft_doc::page::MAX_INDENT)) * INDENT_STEP;
        let tag_w = if p.tag.is_some() { TAG_W } else { 0.0 };
        let marker_w = if p.list.is_some() { MARKER_W } else { 0.0 };
        let text_x_pt = indent + tag_w + marker_w;
        let wrap_px = ((width - text_x_pt).max(24.0) * scale).round();
        let key = para_hash(p, &marker, wrap_px.to_bits(), scale.to_bits(), env.ink_h);
        c.used.insert(key, ());
        if let Some(s) = c.paras.get(&key) {
            let mut s = s.clone();
            s.space_before = space_before;
            out.push(s);
            continue;
        }
        let (job, bold) = style::jobs(p, scale, wrap_px, env.ink, env.fonts.is_some());
        let first_size = p.runs.iter().find(|r| !r.text.is_empty()).or(p.runs.first()).map_or(p.style.font_size(), |r| style::size_pt(r, p.style));
        let spec = ParaSpec {
            job: Arc::new(job),
            bold: bold.map(Arc::new),
            text_x: text_x_pt * scale,
            align_dx: style::align_dx(p.align, wrap_px),
            marker_x: (indent + tag_w) * scale,
            tag_x: indent * scale,
            marker,
            tag: p.tag,
            space_before,
            size_px: style::px(first_size, scale),
            chars: notecraft_engine::text::para_len(p),
        };
        c.paras.insert(key, spec.clone());
        out.push(spec);
    }
    out
}

fn specs_for(c: &mut Cache, o: &Outline, width: f32, env: &Env<'_>) -> Vec<BlockSpec> {
    let mut out = Vec::with_capacity(o.blocks.len());
    let mut run: Vec<&Paragraph> = Vec::new();
    let mut first = true;
    let flush = |c: &mut Cache, run: &mut Vec<&Paragraph>, first: &mut bool, out: &mut Vec<BlockSpec>| {
        if run.is_empty() {
            return;
        }
        out.extend(para_specs(c, run, width, env, *first).into_iter().map(BlockSpec::Para));
        run.clear();
        *first = false;
    };
    for b in &o.blocks {
        match b {
            Block::Paragraph(p) => run.push(p),
            Block::Table { table } => {
                flush(c, &mut run, &mut first, &mut out);
                first = false;
                let cols = table.col_count();
                let col_pt: Vec<f32> = (0..cols).map(|i| table.col_width(i)).collect();
                let rows = table
                    .rows
                    .iter()
                    .map(|row| {
                        (0..cols)
                            .map(|ci| {
                                let cell = row.cells.get(ci);
                                let paras: Vec<&Paragraph> = cell.map(|cl| cl.paragraphs.iter().collect()).unwrap_or_default();
                                let w = col_pt.get(ci).copied().unwrap_or(72.0) - 2.0 * CELL_PAD_X;
                                (cell.and_then(|cl| cl.shading), para_specs(c, &paras, w.max(8.0), env, true))
                            })
                            .collect()
                    })
                    .collect();
                out.push(BlockSpec::Table(TableSpec {
                    x: f32::from(table.indent.min(notecraft_doc::page::MAX_INDENT)) * INDENT_STEP * env.scale,
                    col_w: col_pt.iter().map(|w| w * env.scale).collect(),
                    col_pt,
                    borders: table.borders,
                    rows,
                }));
            }
        }
    }
    flush(c, &mut run, &mut first, &mut out);
    out
}

/// Galleys for `specs`, stacked down from `y` (pixels); returns them and the bottom.
fn stack(ctx: &egui::Context, specs: &[ParaSpec], mut y: f32) -> (Vec<ParaLayout>, f32) {
    let mut paras = Vec::with_capacity(specs.len());
    for s in specs {
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
            table: false,
        });
        y += h;
    }
    (paras, y)
}

/// A table's grid and cells, its top at `y` pixels from the container's top.
fn table_layout(ctx: &egui::Context, id: Guid, pos: Point, scale: f32, block: usize, t: &TableSpec, y: f32) -> TableLayout {
    let (pad_x, pad_y) = (CELL_PAD_X * scale, CELL_PAD_Y * scale);
    let mut col_x = vec![t.x];
    for w in &t.col_w {
        let last = col_x.last().copied().unwrap_or(t.x);
        col_x.push(last + w);
    }
    let mut row_y = vec![y];
    let mut cells = Vec::with_capacity(t.rows.len());
    let mut shading = Vec::with_capacity(t.rows.len());
    for row in &t.rows {
        let top = row_y.last().copied().unwrap_or(y);
        let mut bottom = top + pad_y * 2.0;
        let mut out = Vec::with_capacity(row.len());
        for (ci, (_, specs)) in row.iter().enumerate() {
            let (paras, h) = stack(ctx, specs, 0.0);
            bottom = bottom.max(top + h + 2.0 * pad_y);
            let x = col_x.get(ci).copied().unwrap_or(t.x) + pad_x;
            let w = t.col_pt.get(ci).copied().unwrap_or(72.0) - 2.0 * CELL_PAD_X;
            let at = Point::new(pos.x + x / scale, pos.y + (top + pad_y) / scale);
            out.push(OutlineLayout { id, pos: at, width: w.max(8.0), scale, height_px: h, paras, tables: Vec::new() });
        }
        shading.push(row.iter().map(|(s, _)| *s).collect());
        cells.push(out);
        row_y.push(bottom);
    }
    TableLayout { block, col_x, row_y, borders: t.borders, cells, shading }
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
    let env = Env { scale, ink: &ink, fonts, ink_h };
    let mut outlines = Vec::new();
    for el in &page.elements {
        let ElementKind::Outline(o) = &el.kind else { continue };
        let (pos, width) = pointer::preview_geometry(state, el.id, o.pos, o.width);
        let width = if width.is_finite() { width.clamp(24.0, 4000.0) } else { notecraft_doc::page::DEFAULT_OUTLINE_WIDTH };
        let key = (session.revision, width.to_bits(), scale.to_bits(), ink_h);
        let specs = match c.outlines.get(&el.id) {
            Some((r, w, s, i, specs)) if (*r, *w, *s, *i) == key => specs.clone(),
            _ => {
                let specs = Arc::new(specs_for(&mut c, o, width, &env));
                c.outlines.insert(el.id, (key.0, key.1, key.2, key.3, specs.clone()));
                specs
            }
        };
        let mut y = 0.0;
        let mut paras = Vec::with_capacity(specs.len());
        let mut tables = Vec::new();
        let mut right = width * scale;
        for (block, s) in specs.iter().enumerate() {
            match s {
                BlockSpec::Para(p) => {
                    let (mut ps, bottom) = stack(ctx, std::slice::from_ref(p), y);
                    paras.append(&mut ps);
                    y = bottom;
                }
                BlockSpec::Table(t) => {
                    let gap = TABLE_GAP * scale;
                    let tl = table_layout(ctx, el.id, pos, scale, block, t, y + gap);
                    let bottom = tl.row_y.last().copied().unwrap_or(y) + gap;
                    right = right.max(tl.col_x.last().copied().unwrap_or(0.0));
                    let galley = ctx.fonts_mut(|f| f.layout_job(LayoutJob::default()));
                    paras.push(ParaLayout {
                        top: y,
                        height: bottom - y,
                        text_x: t.x,
                        align_dx: 0.0,
                        galley,
                        bold: None,
                        bold_dx: 0.0,
                        marker: None,
                        marker_x: 0.0,
                        tag: None,
                        tag_x: 0.0,
                        size_px: 0.0,
                        chars: 0,
                        table: true,
                    });
                    tables.push(tl);
                    y = bottom;
                }
            }
        }
        let width = if scale > 0.0 { right / scale } else { width };
        outlines.push(OutlineLayout { id: el.id, pos, width, scale, height_px: y, paras, tables });
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

/// Screen rectangle of a To Do box in paragraph `i` of `o` (a container or a cell).
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

/// Every To Do box in `o` and its cells: the flow, paragraph and box.
pub fn todo_boxes(o: &OutlineLayout, v: &View) -> Vec<(Frame, usize, Rect)> {
    let mut out: Vec<(Frame, usize, Rect)> = (0..o.paras.len()).filter_map(|i| todo_box(o, v, i).map(|r| (Frame::Top, i, r))).collect();
    for t in &o.tables {
        for (r, row) in t.cells.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                for i in 0..cell.paras.len() {
                    if let Some(b) = todo_box(cell, v, i) {
                        out.push((Frame::Cell(CellRef::new(t.block, r, c)), i, b));
                    }
                }
            }
        }
    }
    out
}
