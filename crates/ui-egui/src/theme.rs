//! Colours and shapes: flat, opaque surfaces, barely rounded corners, bold colour, NoteCraft
//! purple (`#c44df0`) as the accent. Every colour the UI paints comes from [`Tokens`].
//!
//! The look follows the family (WordCraft, GridCraft, DeckCraft) and OneNote 2016's desktop
//! arrangement: an accent-coloured title bar and ribbon tab strip with white text, a light ribbon
//! body, coloured section tabs and a page list tinted with the section's colour. Nothing is
//! see-through: every token is opaque (a test checks), and there are no shadows.

use egui::{Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Shadow, Stroke, Visuals};
use serde::{Deserialize, Serialize};

/// NoteCraft purple: title bar, ribbon tab strip, selection, the File page's side bar.
pub const APP_COLOR: Color32 = Color32::from_rgb(0xC4, 0x4D, 0xF0);
/// A deeper purple for text and icons drawn in the accent colour on light surfaces (WCAG AA on
/// white).
pub const APP_INK: Color32 = Color32::from_rgb(0x8A, 0x22, 0xB8);

/// Corner radius of buttons and inputs.
pub const RADIUS_SMALL: u8 = 2;
/// Corner radius of menus, popups and note containers.
pub const RADIUS_MEDIUM: u8 = 2;
/// Panels and the ribbon are square.
pub const RADIUS_PANEL: u8 = 0;

/// Light, dark, or whatever the system prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => crate::tl!("Use system setting"),
            Appearance::Light => crate::tl!("Light"),
            Appearance::Dark => crate::tl!("Dark"),
        }
    }

    /// Whether this appearance draws dark, given what the system prefers (`None`: unknown, light).
    pub fn is_dark(self, system: Option<bool>) -> bool {
        match self {
            Appearance::System => system.unwrap_or(false),
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }
}

/// The palette for one theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// The title bar (accent coloured, like OneNote's).
    pub title_bar: Color32,
    /// Text and icons on the title bar and the tab strip: white, as on OneNote's coloured title
    /// bar. On the brand purple that is 3.7:1, which WCAG accepts for large text and interface
    /// marks (1.4.3 / 1.4.11); body text never sits on it.
    pub on_title_bar: Color32,
    /// Title-bar and tab-strip buttons under the pointer, and pressed.
    pub title_hover: Color32,
    pub title_pressed: Color32,
    /// The close button under the pointer.
    pub close_hover: Color32,
    pub ribbon: Color32,
    /// The ribbon's tab row (the accent band under the title bar).
    pub tab_strip: Color32,
    /// Unselected tab text on the tab strip.
    pub tab_text: Color32,
    /// The selected ribbon tab's text (on [`Tokens::ribbon`]).
    pub tab_selected: Color32,
    /// The File tab's own fill.
    pub file_tab: Color32,
    /// Group titles under the ribbon's controls.
    pub group_label: Color32,
    /// Behind the page (the canvas's surround).
    pub canvas: Color32,
    /// The page itself (stays paper-white in the dark theme unless the page has a colour).
    pub page: Color32,
    /// Text on the page.
    pub page_text: Color32,
    pub panel: Color32,
    /// The notebook pane on the left.
    pub notebook_pane: Color32,
    /// The page list on the right (behind its section tint).
    pub page_list: Color32,
    pub page_list_selected: Color32,
    /// Behind the section tabs.
    pub section_strip: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub checked: Color32,
    /// The outline of a checked (toggled on) button.
    pub checked_border: Color32,
    pub accent: Color32,
    /// Accent-coloured text on `panel`/`ribbon`.
    pub accent_text: Color32,
    /// Text and icons on `accent`, chosen for contrast (≥ 4.5:1).
    pub on_accent: Color32,
    /// The File page's side bar.
    pub nav: Color32,
    /// Text on `nav`, `nav_hover`, `nav_selected` and on `accent_text` fills (primary buttons).
    pub on_nav: Color32,
    pub nav_hover: Color32,
    pub nav_selected: Color32,
    pub input: Color32,
    pub input_border: Color32,
    pub menu: Color32,
    pub status_bar: Color32,
    /// Text selection on the page (opaque: painted behind the text).
    pub selection: Color32,
    /// Selection handles and the lasso.
    pub handle: Color32,
    /// A note container's frame while hovered or edited.
    pub container_border: Color32,
    /// Rule lines and grid on the page.
    pub rule_line: Color32,
    /// The red margin line beside rule lines.
    pub rule_margin: Color32,
    /// The line under the page title.
    pub title_rule: Color32,
    pub red: Color32,
    pub green: Color32,
    pub blue: Color32,
    pub yellow: Color32,
    pub orange: Color32,
}

impl Tokens {
    pub fn light() -> Tokens {
        let title_bar = APP_COLOR;
        Tokens {
            dark: false,
            title_bar,
            on_title_bar: Color32::WHITE,
            title_hover: Color32::from_rgb(0xAE, 0x3A, 0xDA),
            title_pressed: Color32::from_rgb(0x95, 0x2B, 0xC0),
            close_hover: Color32::from_rgb(0xD7, 0x15, 0x26),
            ribbon: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            tab_strip: title_bar,
            tab_text: Color32::WHITE,
            tab_selected: APP_INK,
            file_tab: Color32::from_rgb(0x9B, 0x2F, 0xCB),
            group_label: Color32::from_rgb(0x5E, 0x5E, 0x5E),
            canvas: Color32::from_rgb(0xE9, 0xE9, 0xEB),
            page: Color32::WHITE,
            page_text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            panel: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            notebook_pane: Color32::from_rgb(0xF3, 0xF3, 0xF3),
            page_list: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            page_list_selected: Color32::from_rgb(0xE6, 0xD3, 0xF7),
            section_strip: Color32::WHITE,
            border: Color32::from_rgb(0xD6, 0xD6, 0xD6),
            border_strong: Color32::from_rgb(0xA8, 0xA8, 0xA8),
            text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            text_dim: Color32::from_rgb(0x5C, 0x5C, 0x5C),
            text_disabled: Color32::from_rgb(0xA6, 0xA6, 0xA6),
            icon: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            hover: Color32::from_rgb(0xEC, 0xDD, 0xF9),
            pressed: Color32::from_rgb(0xDA, 0xBF, 0xF3),
            checked: Color32::from_rgb(0xE2, 0xCC, 0xF5),
            checked_border: Color32::from_rgb(0xC9, 0x9C, 0xEC),
            accent: APP_COLOR,
            accent_text: APP_INK,
            on_accent: on(APP_COLOR),
            nav: Color32::from_rgb(0x9B, 0x2F, 0xCB),
            on_nav: Color32::WHITE,
            nav_hover: Color32::from_rgb(0xA6, 0x3C, 0xD6),
            nav_selected: Color32::from_rgb(0x7A, 0x1F, 0xA6),
            input: Color32::WHITE,
            input_border: Color32::from_rgb(0xBD, 0xBD, 0xBD),
            menu: Color32::WHITE,
            status_bar: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            selection: Color32::from_rgb(0xE9, 0xCC, 0xFA),
            handle: APP_INK,
            container_border: Color32::from_rgb(0xBF, 0xBF, 0xBF),
            rule_line: Color32::from_rgb(0xC9, 0xDD, 0xF0),
            rule_margin: Color32::from_rgb(0xF0, 0xB8, 0xB8),
            title_rule: Color32::from_rgb(0xC8, 0xC8, 0xC8),
            red: Color32::from_rgb(0xC4, 0x2B, 0x1C),
            green: Color32::from_rgb(0x10, 0x7C, 0x10),
            blue: Color32::from_rgb(0x1F, 0x5F, 0xBF),
            yellow: Color32::from_rgb(0xF2, 0xB7, 0x05),
            orange: Color32::from_rgb(0xE0, 0x6C, 0x00),
        }
    }

    pub fn dark() -> Tokens {
        let title_bar = Color32::from_rgb(0x6E, 0x1F, 0x93);
        Tokens {
            dark: true,
            title_bar,
            on_title_bar: Color32::WHITE,
            title_hover: Color32::from_rgb(0x83, 0x2C, 0xAA),
            title_pressed: Color32::from_rgb(0x58, 0x16, 0x77),
            close_hover: Color32::from_rgb(0xC4, 0x2B, 0x1C),
            ribbon: Color32::from_rgb(0x2B, 0x2B, 0x2D),
            tab_strip: title_bar,
            tab_text: Color32::WHITE,
            tab_selected: Color32::from_rgb(0xE2, 0xB4, 0xF8),
            file_tab: Color32::from_rgb(0x58, 0x16, 0x77),
            group_label: Color32::from_rgb(0xB4, 0xB4, 0xB8),
            canvas: Color32::from_rgb(0x1E, 0x1E, 0x20),
            page: Color32::from_rgb(0x26, 0x26, 0x28),
            page_text: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            panel: Color32::from_rgb(0x25, 0x25, 0x27),
            notebook_pane: Color32::from_rgb(0x22, 0x22, 0x24),
            page_list: Color32::from_rgb(0x24, 0x24, 0x26),
            page_list_selected: Color32::from_rgb(0x4A, 0x2A, 0x5E),
            section_strip: Color32::from_rgb(0x1E, 0x1E, 0x20),
            border: Color32::from_rgb(0x3C, 0x3C, 0x40),
            border_strong: Color32::from_rgb(0x5A, 0x5A, 0x60),
            text: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            text_dim: Color32::from_rgb(0xB0, 0xB0, 0xB4),
            text_disabled: Color32::from_rgb(0x6E, 0x6E, 0x72),
            icon: Color32::from_rgb(0xD8, 0xD8, 0xDC),
            hover: Color32::from_rgb(0x3D, 0x31, 0x48),
            pressed: Color32::from_rgb(0x4E, 0x38, 0x5E),
            checked: Color32::from_rgb(0x46, 0x33, 0x56),
            checked_border: Color32::from_rgb(0x7A, 0x4B, 0x96),
            accent: APP_COLOR,
            accent_text: Color32::from_rgb(0xE2, 0xB4, 0xF8),
            on_accent: on(APP_COLOR),
            nav: Color32::from_rgb(0x6E, 0x1F, 0x93),
            on_nav: Color32::WHITE,
            nav_hover: Color32::from_rgb(0x83, 0x2C, 0xAA),
            nav_selected: Color32::from_rgb(0x4E, 0x12, 0x6B),
            input: Color32::from_rgb(0x1E, 0x1E, 0x20),
            input_border: Color32::from_rgb(0x55, 0x55, 0x5A),
            menu: Color32::from_rgb(0x2E, 0x2E, 0x31),
            status_bar: Color32::from_rgb(0x22, 0x22, 0x24),
            selection: Color32::from_rgb(0x5A, 0x34, 0x72),
            handle: Color32::from_rgb(0xE2, 0xB4, 0xF8),
            container_border: Color32::from_rgb(0x5A, 0x5A, 0x60),
            rule_line: Color32::from_rgb(0x34, 0x44, 0x58),
            rule_margin: Color32::from_rgb(0x6A, 0x3A, 0x3A),
            title_rule: Color32::from_rgb(0x4A, 0x4A, 0x4E),
            red: Color32::from_rgb(0xF1, 0x70, 0x70),
            green: Color32::from_rgb(0x6C, 0xCB, 0x5F),
            blue: Color32::from_rgb(0x6C, 0xA8, 0xF0),
            yellow: Color32::from_rgb(0xF7, 0xD0, 0x4A),
            orange: Color32::from_rgb(0xF5, 0x9E, 0x4B),
        }
    }

    pub fn for_dark(dark: bool) -> Tokens {
        if dark { Tokens::dark() } else { Tokens::light() }
    }

    /// Every colour token by name (tests: all opaque, contrast).
    pub fn all(&self) -> Vec<(&'static str, Color32)> {
        vec![
            ("title_bar", self.title_bar),
            ("on_title_bar", self.on_title_bar),
            ("title_hover", self.title_hover),
            ("title_pressed", self.title_pressed),
            ("close_hover", self.close_hover),
            ("ribbon", self.ribbon),
            ("tab_strip", self.tab_strip),
            ("tab_text", self.tab_text),
            ("tab_selected", self.tab_selected),
            ("file_tab", self.file_tab),
            ("group_label", self.group_label),
            ("canvas", self.canvas),
            ("page", self.page),
            ("page_text", self.page_text),
            ("panel", self.panel),
            ("notebook_pane", self.notebook_pane),
            ("page_list", self.page_list),
            ("page_list_selected", self.page_list_selected),
            ("section_strip", self.section_strip),
            ("border", self.border),
            ("border_strong", self.border_strong),
            ("text", self.text),
            ("text_dim", self.text_dim),
            ("text_disabled", self.text_disabled),
            ("icon", self.icon),
            ("hover", self.hover),
            ("pressed", self.pressed),
            ("checked", self.checked),
            ("checked_border", self.checked_border),
            ("accent", self.accent),
            ("accent_text", self.accent_text),
            ("on_accent", self.on_accent),
            ("nav", self.nav),
            ("on_nav", self.on_nav),
            ("nav_hover", self.nav_hover),
            ("nav_selected", self.nav_selected),
            ("input", self.input),
            ("input_border", self.input_border),
            ("menu", self.menu),
            ("status_bar", self.status_bar),
            ("selection", self.selection),
            ("handle", self.handle),
            ("container_border", self.container_border),
            ("rule_line", self.rule_line),
            ("rule_margin", self.rule_margin),
            ("title_rule", self.title_rule),
            ("red", self.red),
            ("green", self.green),
            ("blue", self.blue),
            ("yellow", self.yellow),
            ("orange", self.orange),
        ]
    }

    /// egui visuals matching these tokens: flat, opaque, no shadows, corners of [`RADIUS_SMALL`].
    pub fn visuals(&self) -> Visuals {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        let small = CornerRadius::same(RADIUS_SMALL);
        v.window_corner_radius = CornerRadius::same(RADIUS_MEDIUM);
        v.menu_corner_radius = CornerRadius::same(RADIUS_MEDIUM);
        v.window_fill = self.menu;
        v.panel_fill = self.panel;
        v.extreme_bg_color = self.input;
        v.faint_bg_color = self.ribbon;
        v.code_bg_color = self.ribbon;
        v.window_stroke = Stroke::new(1.0, self.border_strong);
        v.window_shadow = Shadow::NONE;
        v.popup_shadow = Shadow::NONE;
        v.selection.bg_fill = self.selection;
        v.selection.stroke = Stroke::new(1.0, self.accent_text);
        v.hyperlink_color = self.accent_text;
        v.override_text_color = Some(self.text);
        v.warn_fg_color = self.orange;
        v.error_fg_color = self.red;
        v.text_cursor.stroke = Stroke::new(1.5, self.accent_text);
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = small;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        v.widgets.noninteractive.bg_fill = self.panel;
        v.widgets.noninteractive.weak_bg_fill = self.panel;
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.inactive.bg_fill = self.input;
        v.widgets.inactive.weak_bg_fill = self.ribbon;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, self.input_border);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.hovered.bg_fill = self.hover;
        v.widgets.hovered.weak_bg_fill = self.hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.checked_border);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.active.bg_fill = self.pressed;
        v.widgets.active.weak_bg_fill = self.pressed;
        v.widgets.active.bg_stroke = Stroke::new(1.0, self.checked_border);
        v.widgets.active.fg_stroke = Stroke::new(1.0, self.text);
        v.widgets.open.bg_fill = self.checked;
        v.widgets.open.weak_bg_fill = self.checked;
        v.widgets.open.fg_stroke = Stroke::new(1.0, self.text);
        v
    }

    /// Applies [`Tokens::visuals`] and remembers the tokens for [`Tokens::get`].
    pub fn apply(&self, ctx: &egui::Context) {
        let visuals = self.visuals();
        // Both of egui's themes: a system theme change must not bring egui's own colours back.
        ctx.all_styles_mut(|s| {
            s.visuals = visuals.clone();
            s.spacing.button_padding = egui::vec2(6.0, 3.0);
            s.spacing.menu_margin = egui::Margin::same(4);
            s.spacing.interact_size.y = 22.0;
        });
        ctx.data_mut(|d| d.insert_temp(tokens_id(), self.clone()));
    }

    /// The tokens last [applied](Tokens::apply) to `ctx` (light before any).
    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(tokens_id())).unwrap_or_else(Tokens::light)
    }

    /// The colour a section's tab is drawn in: its own colour, a little deeper in the dark theme
    /// so it doesn't glare. Always opaque.
    pub fn section_fill(&self, c: notecraft_doc::Color) -> Color32 {
        let c = opaque(color32(c));
        if self.dark { mix(c, Color32::BLACK, 0.18) } else { c }
    }

    /// A pale tint of a section's colour: the page list's background and the band around the page.
    pub fn section_tint(&self, c: notecraft_doc::Color) -> Color32 {
        let c = opaque(color32(c));
        if self.dark { mix(c, self.page_list, 0.80) } else { mix(c, Color32::WHITE, 0.72) }
    }

    /// The selected page's row in the page list: a stronger tint of the section's colour.
    pub fn section_selected(&self, c: notecraft_doc::Color) -> Color32 {
        let c = opaque(color32(c));
        if self.dark { mix(c, self.page_list, 0.68) } else { mix(c, Color32::WHITE, 0.35) }
    }
}

fn tokens_id() -> egui::Id {
    egui::Id::unique("notecraft_tokens")
}

/// `a` moved `t` (0–1) of the way to `b`, opaque.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// The colour with its alpha dropped (painted at full strength).
pub fn opaque(c: Color32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgb(r, g, b)
}

/// White or near-black, whichever reads better on `bg`.
pub fn on(bg: Color32) -> Color32 {
    let ink = Color32::from_rgb(0x0B, 0x0B, 0x0C);
    if contrast(Color32::WHITE, bg) >= contrast(ink, bg) { Color32::WHITE } else { ink }
}

/// WCAG relative luminance.
pub fn luminance(c: Color32) -> f32 {
    fn lin(v: u8) -> f32 {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    }
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG contrast ratio, 1–21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A model colour as an egui colour.
pub fn color32(c: notecraft_doc::Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// An egui colour as a model colour (opaque).
pub fn model_color(c: Color32) -> notecraft_doc::Color {
    notecraft_doc::Color::rgb(c.r(), c.g(), c.b())
}

/// The interface font at `size` points.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Text painted a touch heavier than the interface face (it has one weight): drawn twice, the
/// second copy nudged sideways by a fraction of a pixel.
pub fn paint_strong(painter: &Painter, pos: Pos2, anchor: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    let r = painter.text(pos, anchor, text, font.clone(), color);
    painter.text(pos + egui::vec2(0.45, 0.0), anchor, text, font, color);
    r
}

/// Width of `text` laid out on one line in `font`.
pub fn text_width(ctx: &egui::Context, text: &str, font: FontId) -> f32 {
    ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE).size().x)
}

/// `text` cut with an ellipsis to fit `width` in `font`.
pub fn fit_text(ctx: &egui::Context, text: &str, font: &FontId, width: f32) -> String {
    if text_width(ctx, text, font.clone()) <= width {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().take(400).collect();
    while !chars.is_empty() {
        chars.pop();
        let s: String = chars.iter().collect::<String>() + "…";
        if text_width(ctx, &s, font.clone()) <= width {
            return s;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_contrast_meets_wcag_aa() {
        for t in [Tokens::light(), Tokens::dark()] {
            assert!(contrast(t.on_accent, t.accent) >= 4.5, "on_accent {}", t.dark);
            // The brand purple carries white chrome text at the large-text / UI-mark level.
            let band = if t.dark { 4.5 } else { 3.0 };
            assert!(contrast(t.on_title_bar, t.title_bar) >= band, "title bar {}", t.dark);
            assert!(contrast(t.tab_text, t.tab_strip) >= band, "tab text {}", t.dark);
            assert!(contrast(t.on_accent, t.file_tab) >= 1.0);
            assert!(contrast(t.on_nav, t.file_tab) >= 4.5, "File tab {}", t.dark);
            assert!(contrast(t.tab_selected, t.ribbon) >= 4.5, "selected tab {}", t.dark);
            for (name, bg) in [("nav", t.nav), ("nav_hover", t.nav_hover), ("nav_selected", t.nav_selected), ("primary", t.accent_text)] {
                if !(t.dark && name == "primary") {
                    assert!(contrast(t.on_nav, bg) >= 4.5, "{name} {}", t.dark);
                }
            }
            assert!(contrast(t.text, t.panel) >= 7.0, "text {}", t.dark);
            assert!(contrast(t.text, t.ribbon) >= 7.0, "ribbon text {}", t.dark);
            assert!(contrast(t.text_dim, t.panel) >= 4.5, "dim {}", t.dark);
            assert!(contrast(t.group_label, t.ribbon) >= 4.5, "group label {}", t.dark);
            assert!(contrast(t.accent_text, t.ribbon) >= 4.5, "accent text {}", t.dark);
            assert!(contrast(t.page_text, t.page) >= 7.0, "page {}", t.dark);
            assert!(contrast(t.text, t.hover) >= 4.5, "hovered text {}", t.dark);
        }
        assert_eq!(APP_COLOR, Color32::from_rgb(0xC4, 0x4D, 0xF0));
    }

    #[test]
    fn nothing_is_see_through() {
        for t in [Tokens::light(), Tokens::dark()] {
            for (name, c) in t.all() {
                assert_eq!(c.a(), 255, "{name} in {} is translucent", if t.dark { "dark" } else { "light" });
            }
            let v = t.visuals();
            assert_eq!(v.window_shadow, Shadow::NONE);
            assert_eq!(v.popup_shadow, Shadow::NONE);
            for c in notecraft_doc::Color::SECTION_PALETTE {
                for c in [t.section_fill(c), t.section_tint(c), t.section_selected(c)] {
                    assert_eq!(c.a(), 255);
                }
            }
        }
        // Even a translucent model colour paints opaque as a section colour.
        assert_eq!(Tokens::light().section_fill(notecraft_doc::Color::rgba(10, 20, 30, 40)).a(), 255);
    }

    #[test]
    fn section_tabs_read_with_dark_text() {
        for t in [Tokens::light(), Tokens::dark()] {
            for c in notecraft_doc::Color::SECTION_PALETTE {
                let fill = t.section_fill(c);
                assert!(contrast(on(fill), fill) >= 4.5, "{c:?}");
                let tint = t.section_tint(c);
                assert!(contrast(t.text, tint) >= 4.5, "page list text on {c:?} ({})", t.dark);
                let sel = t.section_selected(c);
                assert!(contrast(t.text, sel) >= 4.5, "selected page text on {c:?} ({})", t.dark);
            }
        }
    }

    #[test]
    fn appearance_resolves() {
        assert!(!Appearance::System.is_dark(None));
        assert!(Appearance::System.is_dark(Some(true)));
        assert!(Appearance::Dark.is_dark(Some(false)));
        assert!(!Appearance::Light.is_dark(Some(true)));
        assert_eq!(serde_json::to_string(&Appearance::Dark).unwrap(), "\"dark\"");
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
    }
}
