//! How runs and paragraphs look: fonts and sizes, the paragraph styles' sizes and colours
//! (headings in blues, Quote in grey italics, Code in Consolas), alignment, list markers and
//! numbering, highlight tags, and the colour mapping for dark pages.
//!
//! A run is drawn in its own font ([`crate::fonts`]): the installed family's real Regular, Bold,
//! Italic or Bold Italic face, its metric-compatible substitute, or a face of the same kind. Only
//! a family without the style draws it itself: italic slanted by egui, bold painted a second time
//! nudged sideways by [`super::text`] (the overlay job of [`jobs`]).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};
use notecraft_doc::{Align, ListKind, ListStyle, ParaStyle, Paragraph, Run};

use crate::fonts;
use crate::theme::{Tokens, color32, contrast, luminance, mix};

/// Indent per list level, points.
pub const INDENT_STEP: f32 = 22.0;
/// Room for a bullet or number before the text, points.
pub const MARKER_W: f32 = 18.0;
/// Room for a tag (To Do box, star, question mark) before the text, points.
pub const TAG_W: f32 = 20.0;
/// Space above a heading that follows other text, points.
pub const HEADING_SPACE: f32 = 6.0;

/// Colours the page's text is drawn with this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// Automatic text colour.
    pub text: Color32,
    /// The paper.
    pub paper: Color32,
    /// The paper is dark (dark theme without a page colour, or a dark page colour).
    pub dark: bool,
    pub link: Color32,
}

impl Ink {
    pub fn new(tokens: &Tokens, page_color: Option<notecraft_doc::Color>) -> Ink {
        let paper = page_color.map_or(tokens.page, |c| crate::theme::opaque(color32(c)));
        let dark = luminance(paper) < 0.25;
        let text = if page_color.is_some() {
            if dark { Color32::from_rgb(0xF0, 0xF0, 0xF0) } else { Color32::from_rgb(0x1F, 0x1F, 0x1F) }
        } else {
            tokens.page_text
        };
        let link = if dark { Color32::from_rgb(0x7C, 0xB4, 0xF5) } else { Color32::from_rgb(0x05, 0x63, 0xC1) };
        Ink { text, paper, dark, link }
    }

    /// A document colour as drawn: on dark paper very dark colours (black text and ink) turn
    /// light so they stay readable, like OneNote's dark mode; other colours are kept.
    pub fn adapt(&self, c: Color32) -> Color32 {
        if !self.dark {
            return c;
        }
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        let opaque = Color32::from_rgb(r, g, b);
        let light = if luminance(opaque) < 0.06 {
            mix(opaque, self.text, 0.9)
        } else {
            // Lighten until it reads on the dark paper (3:1, like large text and ink).
            let mut t = 0.0;
            let mut out = opaque;
            while contrast(out, self.paper) < 3.0 && t < 0.9 {
                t += 0.15;
                out = mix(opaque, Color32::WHITE, t);
            }
            out
        };
        Color32::from_rgba_unmultiplied(light.r(), light.g(), light.b(), a)
    }
}

/// The style's own colour (headings), adapted to the paper.
pub fn style_color(style: ParaStyle, ink: &Ink) -> Option<Color32> {
    let (light, dark) = match style {
        ParaStyle::Heading1 => (Color32::from_rgb(0x1E, 0x4E, 0x79), Color32::from_rgb(0x8F, 0xB8, 0xE8)),
        ParaStyle::Heading2 | ParaStyle::Heading4 | ParaStyle::Heading5 => (Color32::from_rgb(0x2E, 0x75, 0xB5), Color32::from_rgb(0x8C, 0xC0, 0xF0)),
        ParaStyle::Heading3 | ParaStyle::Heading6 => (Color32::from_rgb(0x1F, 0x37, 0x63), Color32::from_rgb(0xA8, 0xBE, 0xE6)),
        ParaStyle::Quote => (Color32::from_rgb(0x59, 0x59, 0x59), Color32::from_rgb(0xB4, 0xB4, 0xB8)),
        ParaStyle::Normal | ParaStyle::Title | ParaStyle::Code => return None,
    };
    Some(if ink.dark { dark } else { light })
}

/// Italic by style (Heading 4 and 6, Quote).
fn style_italic(style: ParaStyle) -> bool {
    matches!(style, ParaStyle::Heading4 | ParaStyle::Heading6 | ParaStyle::Quote)
}

/// Bold by style (Heading 3 and 5).
fn style_bold(style: ParaStyle) -> bool {
    matches!(style, ParaStyle::Heading3 | ParaStyle::Heading5)
}

/// The font a paragraph style draws runs without a font of their own in.
pub fn default_font(style: ParaStyle) -> &'static str {
    if style == ParaStyle::Code { fonts::DEFAULT_MONO_FONT } else { fonts::DEFAULT_FONT }
}

/// The font a run is in: its own, else its paragraph style's.
pub fn run_font(run: &Run, style: ParaStyle) -> &str {
    run.font.as_deref().map(str::trim).filter(|f| !f.is_empty()).unwrap_or_else(|| default_font(style))
}

/// The face a run draws with. `named`: the font provider is active ([`fonts::active`]); until
/// then egui's own faces stand in.
pub fn face(run: &Run, style: ParaStyle, named: bool) -> fonts::Face {
    fonts::face(run_font(run, style), is_bold(run, style), is_italic(run, style), named)
}

/// Font size in points for a run (its own size, else the style's), clamped.
pub fn size_pt(run: &Run, style: ParaStyle) -> f32 {
    let s = run.size.unwrap_or_else(|| style.font_size());
    if s.is_finite() { s.clamp(1.0, 400.0) } else { style.font_size() }
}

/// A font size on screen, rounded to quarter pixels so zooming doesn't fill the glyph atlas
/// with a face per frame.
pub fn px(size_pt: f32, scale: f32) -> f32 {
    ((size_pt * scale) * 4.0).round().max(1.0) / 4.0
}

/// Is the run bold (its own flag or the style's)?
pub fn is_bold(run: &Run, style: ParaStyle) -> bool {
    run.bold || style_bold(style)
}

/// Is the run italic (its own flag or the style's)?
pub fn is_italic(run: &Run, style: ParaStyle) -> bool {
    run.italic || style_italic(style)
}

/// The egui format for `run` in a paragraph of `style`, and whether its bold must be painted
/// (the face has no bold). `highlight`: the paragraph's highlight tag colour, behind runs without
/// a highlight of their own.
pub fn run_format(run: &Run, style: ParaStyle, scale: f32, ink: &Ink, named: bool, highlight: Option<Color32>) -> (TextFormat, bool) {
    let size = px(size_pt(run, style), scale);
    let face = face(run, style, named);
    let font = FontId::new(size, face.family);
    let mut color = run.color.map(color32).map_or_else(|| style_color(style, ink).unwrap_or(ink.text), |c| ink.adapt(c));
    if run.link.is_some() && run.color.is_none() {
        color = ink.link;
    }
    let background = run.highlight.map(color32).or(highlight).unwrap_or(Color32::TRANSPARENT);
    if background != Color32::TRANSPARENT && ink.dark && run.color.is_none() {
        // Light text on a yellow highlight is unreadable: highlighted text stays dark.
        color = Color32::from_rgb(0x1F, 0x1F, 0x1F);
    }
    let line = Stroke::new((size / 14.0).clamp(1.0, 4.0), color);
    let format = TextFormat {
        font_id: font,
        color,
        background,
        expand_bg: 0.0,
        italics: face.fake_italic,
        underline: if run.underline || run.link.is_some() { line } else { Stroke::NONE },
        strikethrough: if run.strike { line } else { Stroke::NONE },
        ..TextFormat::default()
    };
    (format, face.fake_bold)
}

/// egui's alignment for a paragraph's rows.
fn halign(a: Align) -> egui::Align {
    match a {
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
        Align::Left | Align::Justify => egui::Align::LEFT,
    }
}

/// Where a galley laid out with [`jobs`] for `align` goes, from the left of its text area of
/// `wrap_px`: centred and right-aligned rows are placed around x = 0 by egui.
pub fn align_dx(align: Align, wrap_px: f32) -> f32 {
    match align {
        Align::Center => wrap_px.max(8.0) / 2.0,
        Align::Right => wrap_px.max(8.0),
        Align::Left | Align::Justify => 0.0,
    }
}

/// The paragraph's text as a layout job, and (when a run is bold in a face without a bold) the
/// overlay job that paints only those runs again, nudged sideways. Newlines in a run become
/// spaces so character indices stay the same. `named`: see [`face`].
pub fn jobs(p: &Paragraph, scale: f32, wrap_px: f32, ink: &Ink, named: bool) -> (LayoutJob, Option<LayoutJob>) {
    let wrap = egui::text::TextWrapping::wrap_at_width(wrap_px.max(8.0));
    let base = LayoutJob { wrap, halign: halign(p.align), justify: p.align == Align::Justify, ..LayoutJob::default() };
    let mut job = base.clone();
    let mut bold = base;
    let mut any_fake = false;
    let highlight = p.tag.and_then(notecraft_doc::Tag::highlight).map(color32);
    for r in &p.runs {
        if r.text.is_empty() {
            continue;
        }
        let text: String = r.text.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        let (f, fake_bold) = run_format(r, p.style, scale, ink, named, highlight);
        any_fake |= fake_bold;
        let overlay = TextFormat {
            color: if fake_bold { f.color } else { Color32::TRANSPARENT },
            background: Color32::TRANSPARENT,
            underline: Stroke::NONE,
            strikethrough: Stroke::NONE,
            ..f.clone()
        };
        job.append(&text, 0.0, f);
        bold.append(&text, 0.0, overlay);
    }
    if job.sections.is_empty() {
        // An empty paragraph still has a line of its style's height (in its font).
        let empty = p.runs.first().map(|r| Run { text: String::new(), ..r.clone() }).unwrap_or_default();
        job.append("", 0.0, run_format(&empty, p.style, scale, ink, named, None).0);
    }
    (job, any_fake.then_some(bold))
}

/// How far a painted bold is nudged, pixels, for a font of `size_px`.
pub fn bold_offset(size_px: f32) -> f32 {
    (size_px * 0.055).clamp(0.45, 3.0)
}

/// List markers are a model rule: the PDF export draws the same ones.
pub use notecraft_doc::page::{Marker, markers};

/// "3." at level 0, "c." at level 1, "iii." at level 2, then again (the default numbering).
pub fn number_label(n: u32, indent: u8) -> String {
    ListStyle::by_level(ListKind::Numbered, indent).label(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_by_level_and_style() {
        assert_eq!(number_label(3, 0), "3.");
        assert_eq!(number_label(3, 1), "c.");
        assert_eq!(number_label(27, 1), "aa.");
        assert_eq!(number_label(4, 2), "iv.");
        assert_eq!(number_label(1, 3), "1.");
        let mut ps = vec![Paragraph::plain("a"), Paragraph::plain("b"), Paragraph::plain("c"), Paragraph::plain("d")];
        for p in &mut ps {
            p.list = Some(ListKind::Numbered);
        }
        ps[1].indent = 1;
        ps[2].indent = 1;
        let m = markers(&ps);
        assert_eq!(m[0], Some(Marker::Number("1.".into())));
        assert_eq!(m[1], Some(Marker::Number("a.".into())));
        assert_eq!(m[2], Some(Marker::Number("b.".into())));
        assert_eq!(m[3], Some(Marker::Number("2.".into())));
        // A chosen style counts on its own; another style at the same level starts again.
        ps[3].list_style = Some(ListStyle::UpperRoman);
        ps.push(Paragraph { list: Some(ListKind::Numbered), list_style: Some(ListStyle::UpperRoman), ..Paragraph::plain("e") });
        ps.push(Paragraph { list: Some(ListKind::Bullet), list_style: Some(ListStyle::Arrow), ..Paragraph::plain("f") });
        let m = markers(&ps);
        assert_eq!(m[3], Some(Marker::Number("I.".into())));
        assert_eq!(m[4], Some(Marker::Number("II.".into())));
        assert_eq!(m[5], Some(Marker::Bullet(ListStyle::Arrow)));
    }

    #[test]
    fn fonts_and_sizes() {
        assert_eq!(run_font(&Run::default(), ParaStyle::Normal), "Calibri");
        assert_eq!(run_font(&Run::default(), ParaStyle::Code), "Consolas");
        assert_eq!(run_font(&Run { font: Some("  ".into()), ..Run::default() }, ParaStyle::Normal), "Calibri");
        assert_eq!(run_font(&Run { font: Some("Georgia".into()), ..Run::default() }, ParaStyle::Code), "Georgia");
        assert_eq!(face(&Run { font: Some("Consolas".into()), ..Run::default() }, ParaStyle::Normal, false).family, egui::FontFamily::Monospace);
        let named = face(&Run { bold: true, ..Run::default() }, ParaStyle::Normal, true);
        assert!(matches!(&named.family, egui::FontFamily::Name(n) if n.starts_with("nc:Calibri:bn@")));
        let r = Run { size: Some(f32::NAN), ..Run::default() };
        assert_eq!(size_pt(&r, ParaStyle::Heading1), 16.0);
        assert_eq!(px(11.0, 1.0), 11.0);
    }

    #[test]
    fn alignment_and_highlight_tags_shape_the_job() {
        let ink = Ink { text: Color32::BLACK, paper: Color32::WHITE, dark: false, link: Color32::BLUE };
        let mut p = Paragraph { align: Align::Center, tag: Some(notecraft_doc::Tag::Definition), ..Paragraph::plain("word") };
        let (job, _) = jobs(&p, 1.0, 200.0, &ink, false);
        assert_eq!(job.halign, egui::Align::Center);
        assert_ne!(job.sections[0].format.background, Color32::TRANSPARENT, "the tag's highlight");
        assert_eq!(align_dx(Align::Center, 200.0), 100.0);
        assert_eq!(align_dx(Align::Right, 200.0), 200.0);
        p.align = Align::Justify;
        let (job, _) = jobs(&p, 1.0, 200.0, &ink, false);
        assert!(job.justify && job.halign == egui::Align::LEFT);
        // Without the provider every bold run is painted; an empty paragraph keeps a line.
        let b = Paragraph { runs: vec![Run { text: "b".into(), bold: true, ..Run::default() }], ..Paragraph::default() };
        assert!(jobs(&b, 1.0, 100.0, &ink, false).1.is_some());
        assert_eq!(jobs(&Paragraph::default(), 1.0, 100.0, &ink, false).0.sections.len(), 1);
    }
}
