//! How runs and paragraphs look: font families and sizes, the paragraph styles' sizes and
//! colours (headings in blues, Quote in grey italics, Code in the monospace face), list markers
//! and numbering, and the colour mapping for dark pages.
//!
//! NoteCraft draws with egui's built-in faces (one proportional weight and a monospace one); a
//! run's font name is kept in the note and drawn with the closest of the two. Bold has no face of
//! its own, so [`super::layout`] paints bold runs a second time, nudged sideways.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Stroke};
use notecraft_doc::{ListKind, ParaStyle, Paragraph, Run};

use crate::theme::{Tokens, color32, contrast, luminance, mix};

/// Indent per list level, points.
pub const INDENT_STEP: f32 = 22.0;
/// Room for a bullet or number before the text, points.
pub const MARKER_W: f32 = 18.0;
/// Room for a tag (To Do box, star, question mark) before the text, points.
pub const TAG_W: f32 = 20.0;
/// Space above a heading that follows other text, points.
pub const HEADING_SPACE: f32 = 6.0;

/// Monospace font names: these runs draw in egui's monospace face.
const MONO: [&str; 9] = ["consolas", "courier new", "courier", "lucida console", "menlo", "monaco", "cascadia code", "cascadia mono", "monospace"];

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

/// The face a run draws with.
pub fn family(run_font: Option<&str>, style: ParaStyle) -> FontFamily {
    let mono = style == ParaStyle::Code || run_font.is_some_and(|f| MONO.contains(&f.trim().to_ascii_lowercase().as_str()));
    if mono { FontFamily::Monospace } else { FontFamily::Proportional }
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

/// The egui format for `run` in a paragraph of `style`.
pub fn run_format(run: &Run, style: ParaStyle, scale: f32, ink: &Ink) -> TextFormat {
    let size = px(size_pt(run, style), scale);
    let font = FontId::new(size, family(run.font.as_deref(), style));
    let mut color = run.color.map(color32).map_or_else(|| style_color(style, ink).unwrap_or(ink.text), |c| ink.adapt(c));
    if run.link.is_some() && run.color.is_none() {
        color = ink.link;
    }
    let background = run.highlight.map_or(Color32::TRANSPARENT, color32);
    if background != Color32::TRANSPARENT && ink.dark && run.color.is_none() {
        // Light text on a yellow highlight is unreadable: highlighted text stays dark.
        color = Color32::from_rgb(0x1F, 0x1F, 0x1F);
    }
    let line = Stroke::new((size / 14.0).clamp(1.0, 4.0), color);
    TextFormat {
        font_id: font,
        color,
        background,
        expand_bg: 0.0,
        italics: run.italic || style_italic(style),
        underline: if run.underline || run.link.is_some() { line } else { Stroke::NONE },
        strikethrough: if run.strike { line } else { Stroke::NONE },
        ..TextFormat::default()
    }
}

/// The paragraph's text as a layout job, and (when any of it is bold) the overlay job that
/// paints only the bold runs (see the module docs). Newlines in a run become spaces so character
/// indices stay the same.
pub fn jobs(p: &Paragraph, scale: f32, wrap_px: f32, ink: &Ink) -> (LayoutJob, Option<LayoutJob>) {
    let mut job = LayoutJob { wrap: egui::text::TextWrapping::wrap_at_width(wrap_px.max(8.0)), ..LayoutJob::default() };
    let mut bold = LayoutJob { wrap: egui::text::TextWrapping::wrap_at_width(wrap_px.max(8.0)), ..LayoutJob::default() };
    let mut any_bold = false;
    for r in &p.runs {
        if r.text.is_empty() {
            continue;
        }
        let text: String = r.text.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
        let f = run_format(r, p.style, scale, ink);
        let b = is_bold(r, p.style);
        any_bold |= b;
        let overlay = TextFormat {
            color: if b { f.color } else { Color32::TRANSPARENT },
            background: Color32::TRANSPARENT,
            underline: Stroke::NONE,
            strikethrough: Stroke::NONE,
            ..f.clone()
        };
        job.append(&text, 0.0, f);
        bold.append(&text, 0.0, overlay);
    }
    if job.sections.is_empty() {
        // An empty paragraph still has a line of its style's height.
        let empty = Run::default();
        job.append("", 0.0, run_format(&empty, p.style, scale, ink));
    }
    (job, any_bold.then_some(bold))
}

/// How far the bold overlay is nudged, pixels, for a font of `size_px`.
pub fn bold_offset(size_px: f32) -> f32 {
    (size_px * 0.055).clamp(0.45, 3.0)
}

/// The list marker of a paragraph.
#[derive(Clone, Debug, PartialEq)]
pub enum Marker {
    /// Drawn in code by level: disc, circle, square.
    Bullet(u8),
    /// "1.", "a.", "i." …
    Number(String),
}

/// Each paragraph's marker: bullets by indent, and numbers counted per level (a paragraph at a
/// lower level, or a non-numbered one at the same level, starts the count again).
pub fn markers(paras: &[Paragraph]) -> Vec<Option<Marker>> {
    let mut counters = [0u32; notecraft_doc::page::MAX_INDENT as usize + 1];
    let mut out = Vec::with_capacity(paras.len());
    for p in paras {
        let level = usize::from(p.indent.min(notecraft_doc::page::MAX_INDENT));
        // Deeper levels start again under this paragraph.
        for c in counters.iter_mut().skip(level + 1) {
            *c = 0;
        }
        match p.list {
            Some(ListKind::Numbered) => {
                let n = counters.get(level).copied().unwrap_or(0).saturating_add(1);
                if let Some(c) = counters.get_mut(level) {
                    *c = n;
                }
                out.push(Some(Marker::Number(number_label(n, p.indent))));
            }
            Some(ListKind::Bullet) => {
                if let Some(c) = counters.get_mut(level) {
                    *c = 0;
                }
                out.push(Some(Marker::Bullet(p.indent)));
            }
            None => {
                if let Some(c) = counters.get_mut(level) {
                    *c = 0;
                }
                out.push(None);
            }
        }
    }
    out
}

/// "3." at level 0, "c." at level 1, "iii." at level 2, then again.
pub fn number_label(n: u32, indent: u8) -> String {
    match indent % 3 {
        0 => format!("{n}."),
        1 => format!("{}.", letters(n)),
        _ => format!("{}.", roman(n)),
    }
}

fn letters(mut n: u32) -> String {
    let mut s = Vec::new();
    while n > 0 && s.len() < 8 {
        let d = ((n - 1) % 26) as u8;
        s.push(char::from(b'a' + d));
        n = (n - 1) / 26;
    }
    s.iter().rev().collect()
}

fn roman(n: u32) -> String {
    if n == 0 || n >= 4000 {
        return n.to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    let mut n = n;
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_by_level() {
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
    }

    #[test]
    fn mono_and_sizes() {
        assert_eq!(family(Some("Consolas"), ParaStyle::Normal), FontFamily::Monospace);
        assert_eq!(family(Some("Calibri"), ParaStyle::Code), FontFamily::Monospace);
        assert_eq!(family(None, ParaStyle::Normal), FontFamily::Proportional);
        let r = Run { size: Some(f32::NAN), ..Run::default() };
        assert_eq!(size_pt(&r, ParaStyle::Heading1), 16.0);
        assert_eq!(px(11.0, 1.0), 11.0);
    }
}
