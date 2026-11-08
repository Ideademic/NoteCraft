//! How runs, paragraphs and the paper look in print: the same rules as the canvas
//! (`crates/ui-egui/src/canvas/style.rs` and `canvas/page.rs`), in the light theme, written
//! again here because the canvas's builders live in the egui crate. Keep the two in step: sizes,
//! the style colours, list markers and numbering, indents, and the dark-paper colour mapping.

use notecraft_doc::{Color, ListKind, ParaStyle, Paragraph, Run};

/// Indent per list level, points.
pub const INDENT_STEP: f32 = 22.0;
/// Room for a bullet or number before the text, points.
pub const MARKER_W: f32 = 18.0;
/// Room for a tag (To Do box, star, question mark) before the text, points.
pub const TAG_W: f32 = 20.0;
/// Space above a heading that follows other text, points.
pub const HEADING_SPACE: f32 = 6.0;
/// Title font size, points.
pub const TITLE_PT: f32 = 20.0;
/// Shortest title rule, points.
pub const TITLE_RULE_MIN: f32 = 330.0;
/// Date line font size, points.
pub const DATE_PT: f32 = 9.5;
/// Where the red margin line of rule lines sits, points from the left.
pub const MARGIN_X: f32 = 24.0;

pub const WHITE: Color = Color::rgb(0xFF, 0xFF, 0xFF);
const TEXT: Color = Color::rgb(0x1F, 0x1F, 0x1F);
pub const ACCENT: Color = Color::NOTECRAFT;
pub const RULE_LINE: Color = Color::rgb(0xC9, 0xDD, 0xF0);
pub const RULE_MARGIN: Color = Color::rgb(0xF0, 0xB8, 0xB8);
pub const TITLE_RULE: Color = Color::rgb(0xC8, 0xC8, 0xC8);
pub const BLUE: Color = Color::rgb(0x1F, 0x5F, 0xBF);
pub const RED: Color = Color::rgb(0xC4, 0x2B, 0x1C);

/// `a` moved `t` (0–1) of the way to `b`, opaque.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color::rgb(ch(a.r, b.r), ch(a.g, b.g), ch(a.b, b.b))
}

/// WCAG relative luminance.
pub fn luminance(c: Color) -> f32 {
    fn lin(v: u8) -> f32 {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    }
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, 1–21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// The colours a page prints with: its paper and automatic text colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub text: Color,
    pub paper: Color,
    /// The page colour is dark: very dark text and ink turn light, as on the canvas.
    pub dark: bool,
    pub link: Color,
}

impl Ink {
    pub fn new(page_color: Option<Color>) -> Ink {
        let paper = page_color.map_or(WHITE, |c| c.with_alpha(255));
        let dark = luminance(paper) < 0.25;
        let text = if page_color.is_some() && dark { Color::rgb(0xF0, 0xF0, 0xF0) } else { TEXT };
        let link = if dark { Color::rgb(0x7C, 0xB4, 0xF5) } else { Color::rgb(0x05, 0x63, 0xC1) };
        Ink { text, paper, dark, link }
    }

    /// A document colour as printed on this paper (the canvas's `Ink::adapt`).
    pub fn adapt(&self, c: Color) -> Color {
        if !self.dark {
            return c;
        }
        let opaque = c.with_alpha(255);
        let light = if luminance(opaque) < 0.06 {
            mix(opaque, self.text, 0.9)
        } else {
            let mut t = 0.0;
            let mut out = opaque;
            while contrast(out, self.paper) < 3.0 && t < 0.9 {
                t += 0.15;
                out = mix(opaque, WHITE, t);
            }
            out
        };
        light.with_alpha(c.a)
    }

    /// Rule lines and the margin line.
    pub fn rules(&self, page_color: Option<Color>) -> (Color, Color) {
        if page_color.is_some() { (mix(self.paper, BLUE, 0.22), mix(self.paper, RED, 0.35)) } else { (RULE_LINE, RULE_MARGIN) }
    }

    pub fn title_rule(&self) -> Color {
        if self.dark { mix(self.paper, self.text, 0.35) } else { TITLE_RULE }
    }

    pub fn date(&self) -> Color {
        if self.dark { mix(self.paper, self.text, 0.6) } else { mix(self.paper, Color::rgb(0x40, 0x40, 0x40), 0.8) }
    }
}

/// The style's own colour (headings, quotes).
pub fn style_color(style: ParaStyle, ink: &Ink) -> Option<Color> {
    let (light, dark) = match style {
        ParaStyle::Heading1 => (Color::rgb(0x1E, 0x4E, 0x79), Color::rgb(0x8F, 0xB8, 0xE8)),
        ParaStyle::Heading2 | ParaStyle::Heading4 | ParaStyle::Heading5 => (Color::rgb(0x2E, 0x75, 0xB5), Color::rgb(0x8C, 0xC0, 0xF0)),
        ParaStyle::Heading3 | ParaStyle::Heading6 => (Color::rgb(0x1F, 0x37, 0x63), Color::rgb(0xA8, 0xBE, 0xE6)),
        ParaStyle::Quote => (Color::rgb(0x59, 0x59, 0x59), Color::rgb(0xB4, 0xB4, 0xB8)),
        ParaStyle::Normal | ParaStyle::Title | ParaStyle::Code => return None,
    };
    Some(if ink.dark { dark } else { light })
}

/// Italic by style (Heading 4 and 6, Quote).
pub fn style_italic(style: ParaStyle) -> bool {
    matches!(style, ParaStyle::Heading4 | ParaStyle::Heading6 | ParaStyle::Quote)
}

/// Bold by style (Heading 3 and 5).
pub fn style_bold(style: ParaStyle) -> bool {
    matches!(style, ParaStyle::Heading3 | ParaStyle::Heading5)
}

/// Font size in points for a run (its own size, else the style's), clamped and rounded to
/// quarter points like the canvas at 100 %.
pub fn size_pt(run: &Run, style: ParaStyle) -> f32 {
    let s = run.size.unwrap_or_else(|| style.font_size());
    let s = if s.is_finite() { s.clamp(1.0, 400.0) } else { style.font_size() };
    ((s * 4.0).round() / 4.0).max(1.0)
}

/// The family a run asks for.
pub fn family(run: &Run, style: ParaStyle) -> String {
    match (&run.font, style) {
        (Some(f), _) if !f.trim().is_empty() => f.trim().to_owned(),
        (_, ParaStyle::Code) => crate::fonts::DEFAULT_MONO.to_owned(),
        _ => crate::fonts::DEFAULT_FONT.to_owned(),
    }
}

/// How a run prints.
#[derive(Clone, Debug, PartialEq)]
pub struct RunLook {
    pub color: Color,
    pub highlight: Option<Color>,
    pub underline: bool,
    pub strike: bool,
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub family: String,
}

pub fn run_look(run: &Run, style: ParaStyle, ink: &Ink) -> RunLook {
    let mut color = run.color.map_or_else(|| style_color(style, ink).unwrap_or(ink.text), |c| ink.adapt(c));
    if run.link.is_some() && run.color.is_none() {
        color = ink.link;
    }
    let highlight = run.highlight.filter(|h| h.a > 0);
    if highlight.is_some() && ink.dark && run.color.is_none() {
        color = TEXT;
    }
    RunLook {
        color,
        highlight,
        underline: run.underline || run.link.is_some(),
        strike: run.strike,
        bold: run.bold || style_bold(style),
        italic: run.italic || style_italic(style),
        size: size_pt(run, style),
        family: family(run, style),
    }
}

/// The list marker of a paragraph.
#[derive(Clone, Debug, PartialEq)]
pub enum Marker {
    /// Disc, circle or square by level.
    Bullet(u8),
    /// "1.", "a.", "i." …
    Number(String),
}

/// Each paragraph's marker: bullets by indent, numbers counted per level (as the canvas).
pub fn markers(paras: &[Paragraph]) -> Vec<Option<Marker>> {
    let mut counters = [0u32; notecraft_doc::page::MAX_INDENT as usize + 1];
    let mut out = Vec::with_capacity(paras.len());
    for p in paras {
        let level = usize::from(p.indent.min(notecraft_doc::page::MAX_INDENT));
        for c in counters.iter_mut().skip(level + 1) {
            *c = 0;
        }
        let slot = counters.get_mut(level);
        match p.list {
            Some(ListKind::Numbered) => {
                let n = slot.map_or(1, |c| {
                    *c = c.saturating_add(1);
                    *c
                });
                out.push(Some(Marker::Number(number_label(n, p.indent))));
            }
            Some(ListKind::Bullet) => {
                if let Some(c) = slot {
                    *c = 0;
                }
                out.push(Some(Marker::Bullet(p.indent)));
            }
            None => {
                if let Some(c) = slot {
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

/// The created date and time under the title, in the reader's time zone (`offset_minutes`).
pub fn date_text(created: chrono::DateTime<chrono::Utc>, offset_minutes: i32) -> (String, String) {
    let minutes = offset_minutes.clamp(-14 * 60, 14 * 60);
    match chrono::FixedOffset::east_opt(minutes * 60) {
        Some(offset) => {
            let t = created.with_timezone(&offset);
            (t.format("%A, %B %-d, %Y").to_string(), t.format("%-I:%M %p").to_string())
        }
        None => (created.format("%A, %B %-d, %Y").to_string(), created.format("%-I:%M %p").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_and_markers_match_the_canvas() {
        assert_eq!(number_label(3, 0), "3.");
        assert_eq!(number_label(27, 1), "aa.");
        assert_eq!(number_label(4, 2), "iv.");
        let mut ps = vec![Paragraph::plain("a"), Paragraph::plain("b"), Paragraph::plain("c")];
        for p in &mut ps {
            p.list = Some(ListKind::Numbered);
        }
        ps[1].indent = 1;
        let m = markers(&ps);
        assert_eq!(m, vec![Some(Marker::Number("1.".into())), Some(Marker::Number("a.".into())), Some(Marker::Number("2.".into()))]);
    }

    #[test]
    fn dark_paper_lightens_black() {
        let ink = Ink::new(Some(Color::rgb(0x20, 0x20, 0x30)));
        assert!(ink.dark);
        assert!(luminance(ink.adapt(Color::BLACK)) > 0.5);
        let light = Ink::new(None);
        assert_eq!(light.adapt(Color::BLACK), Color::BLACK);
        let r = Run { size: Some(f32::NAN), ..Run::default() };
        assert_eq!(size_pt(&r, ParaStyle::Heading1), 16.0);
        assert_eq!(family(&Run::default(), ParaStyle::Code), crate::fonts::DEFAULT_MONO);
    }

    #[test]
    fn dates_never_overflow() {
        let (d, t) = date_text(chrono::DateTime::<chrono::Utc>::MAX_UTC, i32::MAX);
        assert!(!d.is_empty() && !t.is_empty());
    }
}
