//! Text: laid out by epaint exactly as the canvas lays it out (same wrap widths, sizes and
//! faces, so the same line breaks), then written glyph by glyph at epaint's positions with the
//! embedded faces, so it reads, selects and copies as text. Highlights, underlines and
//! strikethrough follow epaint's geometry; links become link areas.

use std::sync::Arc;

use craft_pdf::write::backend as k;
use epaint::text::{FontId, Galley, LayoutJob, TextFormat, TextWrapping};
use k::geom::{Point, Transform};
use k::surface::Surface;
use k::text::{GlyphId, KrillaGlyph};
use notecraft_doc::{Align, Color, Outline, Paragraph, Run, Tag};
use notecraft_geom::Rect;

use crate::draw;
use crate::fonts::{FontSet, LAYOUT_PPP, StyleKey};
use crate::style::{self, HEADING_SPACE, INDENT_STEP, Ink, MARKER_W, Marker, RunLook, TAG_W};

/// Most characters of one paragraph laid out (hostile files): the rest isn't printed.
pub const MAX_PARA_CHARS: usize = 200_000;
/// Slant of synthetic italics: the canvas shifts a glyph's top by a quarter of its height.
const FAKE_ITALIC: f32 = 0.25;

/// One format section of a text block.
#[derive(Clone, Debug)]
pub struct Section {
    pub style: usize,
    pub look: RunLook,
    pub link: Option<String>,
}

/// Laid-out text with what each character looks like.
#[derive(Clone)]
pub struct Block {
    pub galley: Arc<Galley>,
    /// The section of each character of the job's text.
    pub char_section: Vec<u32>,
    pub sections: Vec<Section>,
}

/// A link area on the page (points) and its target.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkArea {
    pub rect: Rect,
    pub url: String,
}

impl Block {
    pub fn width(&self) -> f32 {
        self.galley.size().x
    }

    pub fn height(&self) -> f32 {
        self.galley.size().y
    }

    /// The first row's span, from the block's top.
    pub fn first_row(&self) -> (f32, f32) {
        self.galley.rows.first().map_or((0.0, self.height()), |r| (r.rect().top(), r.rect().bottom()))
    }
}

/// The style key a look asks for.
pub fn key_of(look: &RunLook) -> StyleKey {
    StyleKey::new(&look.family, look.bold, look.italic)
}

fn format(fs: &FontSet, sec: &Section) -> TextFormat {
    let family = fs.style_info(sec.style).map_or(epaint::text::FontFamily::Proportional, |s| s.family.clone());
    TextFormat { font_id: FontId::new(sec.look.size, family), ..TextFormat::default() }
}

/// Lays out `runs` (a paragraph's, or a label's) wrapped at `wrap` points (`None`: no wrap), left
/// aligned.
pub fn layout_runs(fs: &mut FontSet, runs: &[(String, Section)], wrap: Option<f32>, empty: Section) -> Block {
    layout_aligned(fs, runs, wrap, empty, Align::Left)
}

/// [`layout_runs`] with the paragraph's alignment, as the canvas lays it out (centred and right
/// rows around x = 0, see [`style::align_dx`]; justified rows fill the wrap width).
pub fn layout_aligned(fs: &mut FontSet, runs: &[(String, Section)], wrap: Option<f32>, empty: Section, align: Align) -> Block {
    let halign = match align {
        Align::Center => epaint::emath::Align::Center,
        Align::Right => epaint::emath::Align::RIGHT,
        Align::Left | Align::Justify => epaint::emath::Align::LEFT,
    };
    let mut job = LayoutJob {
        wrap: wrap.map_or_else(TextWrapping::no_max_width, |w| TextWrapping::wrap_at_width(w.max(8.0))),
        halign,
        justify: align == Align::Justify,
        ..LayoutJob::default()
    };
    let mut char_section = Vec::new();
    let mut sections = Vec::new();
    for (text, sec) in runs {
        if text.is_empty() || char_section.len() >= MAX_PARA_CHARS {
            continue;
        }
        let room = MAX_PARA_CHARS - char_section.len();
        let text: String = text.chars().take(room).map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        let si = u32::try_from(sections.len()).unwrap_or(u32::MAX);
        char_section.extend(std::iter::repeat_n(si, text.chars().count()));
        job.append(&text, 0.0, format(fs, sec));
        sections.push(sec.clone());
    }
    if job.sections.is_empty() {
        // An empty paragraph still has a line of its style's height.
        job.append("", 0.0, format(fs, &empty));
        sections.push(empty);
    }
    let galley = fs.fonts.with_pixels_per_point(LAYOUT_PPP).layout_job(job);
    Block { galley, char_section, sections }
}

/// A run's section.
pub fn section_of(fs: &FontSet, run: &Run, p: &Paragraph, ink: &Ink) -> Section {
    let look = style::para_run_look(run, p, ink);
    Section { style: fs.style(&key_of(&look)), link: run.link.clone().filter(|l| !l.trim().is_empty()), look }
}

/// Plain text in one look (labels, the title, numbers).
pub fn simple(fs: &mut FontSet, text: &str, look: RunLook) -> Block {
    let sec = Section { style: fs.style(&key_of(&look)), look, link: None };
    let clean: String = text.chars().filter(|c| !c.is_control()).take(2000).collect();
    layout_runs(fs, &[(clean, sec.clone())], None, sec)
}

/// A plain look in the default font.
pub fn plain_look(size: f32, color: Color) -> RunLook {
    RunLook {
        color,
        highlight: None,
        underline: false,
        strike: false,
        bold: false,
        italic: false,
        size,
        family: crate::fonts::DEFAULT_FONT.to_owned(),
    }
}

/// One laid-out paragraph, page points.
pub struct ParaLay {
    /// The text area's left edge (after the indent, tag and marker).
    pub x: f32,
    /// Where the block's origin is from `x` (centred and right-aligned rows sit around it).
    pub align_dx: f32,
    pub top: f32,
    pub height: f32,
    pub block: Block,
    pub marker: Option<(Marker, Block)>,
    pub marker_x: f32,
    pub tag: Option<Tag>,
    pub tag_x: f32,
    /// Font size of the paragraph's first run (markers and tags are sized from it).
    pub size: f32,
}

impl ParaLay {
    /// The first line's span, page points.
    pub fn first_row(&self) -> (f32, f32) {
        let (a, b) = self.block.first_row();
        (self.top + a, self.top + b)
    }
}

/// One laid-out note container, page points.
pub struct OutlineLay {
    pub rect: Rect,
    pub paras: Vec<ParaLay>,
}

/// Lays out a container as the canvas does (`canvas/layout.rs::specs_for` at 100 %).
pub fn layout_outline(fs: &mut FontSet, o: &Outline, ink: &Ink) -> OutlineLay {
    let width = if o.width.is_finite() { o.width.clamp(24.0, 4000.0) } else { notecraft_doc::page::DEFAULT_OUTLINE_WIDTH };
    let pos = o.pos.sanitized();
    let markers = style::markers(&o.paragraphs);
    let mut y = 0.0;
    let mut paras = Vec::with_capacity(o.paragraphs.len());
    for (i, p) in o.paragraphs.iter().enumerate() {
        let indent = f32::from(p.indent.min(notecraft_doc::page::MAX_INDENT)) * INDENT_STEP;
        let tag_w = if p.tag.is_some() { TAG_W } else { 0.0 };
        let marker_w = if p.list.is_some() { MARKER_W } else { 0.0 };
        let text_x = indent + tag_w + marker_w;
        let wrap = (width - text_x).max(24.0).round();
        let runs: Vec<(String, Section)> = p.runs.iter().map(|r| (r.text.clone(), section_of(fs, r, p, ink))).collect();
        let empty = section_of(fs, &Run::default(), p, ink);
        let block = layout_aligned(fs, &runs, Some(wrap), empty, p.align);
        let first = p.runs.iter().find(|r| !r.text.is_empty()).or(p.runs.first()).map_or(p.style.font_size(), |r| style::size_pt(r, p.style));
        if i > 0 && p.style.heading_level().is_some() {
            y += HEADING_SPACE;
        }
        let marker = markers.get(i).cloned().flatten().map(|m| {
            let label = match &m {
                Marker::Number(s) => s.clone(),
                Marker::Bullet(_) => String::new(),
            };
            let look = block.sections.first().map_or_else(|| plain_look(first, ink.text), |s| s.look.clone());
            let look = RunLook { highlight: None, underline: false, strike: false, ..look };
            let lb = simple(fs, &label, look);
            (m, lb)
        });
        let h = block.height().max(first);
        paras.push(ParaLay {
            x: pos.x + text_x,
            align_dx: style::align_dx(p.align, wrap),
            top: pos.y + y,

            height: h,
            block,
            marker,
            marker_x: pos.x + indent + tag_w,
            tag: p.tag,
            tag_x: pos.x + indent,
            size: first,
        });
        y += h;
    }
    OutlineLay { rect: Rect::new(pos.x, pos.y, width, y), paras }
}

/// Every text row's vertical span, page points (sheet breaks avoid cutting them).
pub fn row_spans(o: &OutlineLay, out: &mut Vec<(f32, f32)>) {
    for p in &o.paras {
        for r in &p.block.galley.rows {
            let rr = r.rect();
            out.push((p.top + rr.top(), p.top + rr.bottom()));
        }
    }
}

/// Draws `block` with its top-left corner at (`x`, `y`). Rows outside `clip` (page points)
/// are skipped. Link areas are added to `links`.
pub fn draw_block(s: &mut Surface, fs: &mut FontSet, b: &Block, x: f32, y: f32, clip: Rect, links: &mut Vec<LinkArea>) {
    let mut base = 0usize;
    for row in &b.galley.rows {
        let n = row.glyphs.len();
        let start = base;
        base += row.char_count_including_newline().0;
        let rr = row.rect();
        let (top, bottom) = (y + rr.top(), y + rr.bottom());
        if bottom < clip.y - 2.0 || top > clip.y + clip.h + 2.0 {
            continue;
        }
        let ox = x + row.pos.x;
        let oy = y + row.pos.y;
        let sec_of = |i: usize| b.char_section.get(start + i).and_then(|s| b.sections.get(*s as usize));
        // Highlights behind the text, one rectangle per same-colour stretch.
        spans(
            n,
            |i| sec_of(i).and_then(|s| s.look.highlight),
            |from, to, c| {
                let (Some(g0), Some(g1)) = (row.glyphs.get(from), row.glyphs.get(to)) else { return };
                let r = g0.logical_rect();
                draw::fill_rect(s, ox + g0.pos.x, oy + r.top(), g1.max_x() - g0.pos.x, r.height(), c);
            },
        );
        draw_glyphs(s, fs, row, b, start, ox, oy);
        // Underlines and strikethrough, as epaint draws them.
        for strike in [false, true] {
            spans(
                n,
                |i| sec_of(i).filter(|s| if strike { s.look.strike } else { s.look.underline }).map(|s| (s.look.color, s.look.size.to_bits())),
                |from, to, (c, size)| {
                    let (Some(g0), Some(g1)) = (row.glyphs.get(from), row.glyphs.get(to)) else { return };
                    let r = g0.logical_rect();
                    let ly = if strike { r.center().y } else { r.bottom() };
                    let w = (f32::from_bits(size) / 14.0).clamp(1.0, 4.0);
                    draw::line(s, (ox + g0.pos.x, oy + ly), (ox + g1.max_x(), oy + ly), w, c);
                },
            );
        }
        spans(
            n,
            |i| sec_of(i).and_then(|s| s.link.clone()),
            |from, to, url| {
                let (Some(g0), Some(g1)) = (row.glyphs.get(from), row.glyphs.get(to)) else { return };
                let r = g0.logical_rect();
                links.push(LinkArea { rect: Rect::new(ox + g0.pos.x, oy + r.top(), g1.max_x() - g0.pos.x, r.height()), url });
            },
        );
    }
}

/// Calls `each(first, last, value)` for every stretch of consecutive glyphs `0..n` whose `key`
/// is the same `Some`.
fn spans<T: PartialEq + Clone>(n: usize, key: impl Fn(usize) -> Option<T>, mut each: impl FnMut(usize, usize, T)) {
    let mut cur: Option<(usize, T)> = None;
    for i in 0..n {
        let k = key(i);
        match (&cur, k) {
            (Some((_, v)), Some(k)) if *v == k => {}
            (_, k) => {
                if let Some((from, v)) = cur.take() {
                    each(from, i - 1, v);
                }
                cur = k.map(|k| (i, k));
            }
        }
    }
    if let Some((from, v)) = cur {
        each(from, n.saturating_sub(1), v);
    }
}

/// One row's text, in stretches of one section and one face (the face epaint took each
/// character from). Each stretch is shaped again with that face, so ligatures, kerning and marks
/// come out as the canvas draws them, and starts where epaint placed its first character.
fn draw_glyphs(s: &mut Surface, fs: &mut FontSet, row: &epaint::text::PlacedRow, b: &Block, start: usize, ox: f32, oy: f32) {
    let sec_index = |i: usize| b.char_section.get(start + i).copied();
    // (section, face, rtl) per glyph; None for what isn't drawn (controls, missing glyphs).
    let keys: Vec<Option<(u32, usize, bool)>> = row
        .glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let si = sec_index(i)?;
            let sec = b.sections.get(si as usize)?;
            if g.chr.is_control() {
                return None;
            }
            let (face, _) = fs.glyph(sec.style, g.chr)?;
            Some((si, face, g.is_rtl()))
        })
        .collect();
    let mut stretches = Vec::new();
    spans(keys.len(), |i| keys.get(i).copied().flatten(), |from, to, k| stretches.push((from, to, k)));
    for (from, to, (si, face, rtl)) in stretches {
        let (Some(sec), Some(font), Some(glyphs)) = (b.sections.get(si as usize), fs.pdf_font(face), row.glyphs.get(from..=to)) else { continue };
        let size = sec.look.size;
        let Some(first) = glyphs.first() else { continue };
        if size <= 0.0 {
            continue;
        }
        let text: String = glyphs.iter().map(|g| g.chr).collect();
        let Some(shaped) = fs.shape(face, &text, rtl) else { continue };
        let left = glyphs.iter().map(|g| g.pos.x).fold(f32::INFINITY, f32::min);
        // Each glyph's text: its cluster up to the next cluster (ligatures map to all their characters).
        let mut heads: Vec<usize> = shaped.iter().map(|g| g.cluster.min(text.len())).collect();
        heads.sort_unstable();
        heads.dedup();
        let pdf_glyphs: Vec<KrillaGlyph> = shaped
            .iter()
            .map(|g| {
                let c = g.cluster.min(text.len());
                let end = heads.get(heads.partition_point(|h| *h <= c)).copied().unwrap_or(text.len());
                KrillaGlyph::new(GlyphId::new(g.gid), g.x_advance, g.x_offset, g.y_offset, 0.0, c..end, None)
            })
            .collect();
        let style = fs.style_info(sec.style);
        let (fake_bold, fake_italic) = style.map_or((false, false), |st| (st.fake_bold, st.fake_italic));
        let (bx, by) = (ox + left, oy + first.pos.y);
        let t = if fake_italic { Transform::from_row(1.0, 0.0, -FAKE_ITALIC, 1.0, bx, by) } else { Transform::from_translate(bx, by) };
        s.push_transform(&t);
        s.set_fill(Some(draw::fill(sec.look.color, 1.0)));
        s.set_stroke(fake_bold.then(|| draw::stroke(sec.look.color, (size * 0.045).clamp(0.2, 4.0))));
        s.draw_glyphs(Point::from_xy(0.0, 0.0), &pdf_glyphs, font, &text, size, false);
        s.set_fill(None);
        s.set_stroke(None);
        s.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_group_runs() {
        let keys = [None, Some(1), Some(1), Some(2), None, Some(2)];
        let mut got = Vec::new();
        spans(keys.len(), |i| keys.get(i).copied().flatten(), |a, b, v| got.push((a, b, v)));
        assert_eq!(got, vec![(1, 2, 1), (3, 3, 2), (5, 5, 2)]);
        let mut none = Vec::new();
        spans(0, |_| Some(1), |a, b, v| none.push((a, b, v)));
        assert!(none.is_empty());
    }
}
