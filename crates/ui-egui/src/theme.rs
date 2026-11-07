//! Colours and shapes: flat, opaque surfaces, barely rounded corners, bold colour, NoteCraft
//! purple (`#c44df0`) as the accent. Every colour the UI paints comes from [`Tokens`].

use egui::{Color32, CornerRadius, Stroke, Visuals};

/// NoteCraft purple: title bar, ribbon tab underline, selection, the File page's side bar.
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

/// The palette for one theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    /// The title bar (accent coloured, like OneNote's).
    pub title_bar: Color32,
    /// Text and icons on the title bar.
    pub on_title_bar: Color32,
    pub ribbon: Color32,
    /// The ribbon's tab row.
    pub tab_strip: Color32,
    /// The selected ribbon tab's text and underline.
    pub tab_selected: Color32,
    /// Behind the page (the canvas's surround).
    pub canvas: Color32,
    /// The page itself (stays paper-white in the dark theme unless the page has a colour).
    pub page: Color32,
    /// Text on the page.
    pub page_text: Color32,
    pub panel: Color32,
    /// The notebook pane on the left.
    pub notebook_pane: Color32,
    /// The page list on the right.
    pub page_list: Color32,
    pub page_list_selected: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub checked: Color32,
    pub accent: Color32,
    /// Accent-coloured text on `panel`/`ribbon`.
    pub accent_text: Color32,
    /// Text and icons on `accent`, chosen for contrast (≥ 4.5:1).
    pub on_accent: Color32,
    /// The File page's side bar.
    pub nav: Color32,
    pub input: Color32,
    pub input_border: Color32,
    pub status_bar: Color32,
    /// Text selection on the page.
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
}

impl Tokens {
    pub fn light() -> Tokens {
        Tokens {
            dark: false,
            title_bar: APP_COLOR,
            on_title_bar: on(APP_COLOR),
            ribbon: Color32::from_rgb(0xF7, 0xF7, 0xF7),
            tab_strip: Color32::from_rgb(0xF7, 0xF7, 0xF7),
            tab_selected: APP_INK,
            canvas: Color32::from_rgb(0xE9, 0xE9, 0xEB),
            page: Color32::WHITE,
            page_text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            panel: Color32::from_rgb(0xFA, 0xFA, 0xFA),
            notebook_pane: Color32::from_rgb(0xF2, 0xF2, 0xF2),
            page_list: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            page_list_selected: Color32::from_rgb(0xE6, 0xD3, 0xF7),
            border: Color32::from_rgb(0xD6, 0xD6, 0xD6),
            border_strong: Color32::from_rgb(0xA8, 0xA8, 0xA8),
            text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            text_dim: Color32::from_rgb(0x5C, 0x5C, 0x5C),
            text_disabled: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            icon: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            hover: Color32::from_rgb(0xEC, 0xE0, 0xF8),
            pressed: Color32::from_rgb(0xDC, 0xC4, 0xF2),
            checked: Color32::from_rgb(0xE2, 0xCC, 0xF5),
            accent: APP_COLOR,
            accent_text: APP_INK,
            on_accent: on(APP_COLOR),
            nav: Color32::from_rgb(0x9B, 0x2F, 0xCB),
            input: Color32::WHITE,
            input_border: Color32::from_rgb(0xBD, 0xBD, 0xBD),
            status_bar: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            selection: Color32::from_rgba_unmultiplied(0xC4, 0x4D, 0xF0, 0x55),
            handle: APP_INK,
            container_border: Color32::from_rgb(0xBF, 0xBF, 0xBF),
            rule_line: Color32::from_rgb(0xC9, 0xDD, 0xF0),
            rule_margin: Color32::from_rgb(0xF0, 0xB8, 0xB8),
            title_rule: Color32::from_rgb(0xC8, 0xC8, 0xC8),
            red: Color32::from_rgb(0xC4, 0x2B, 0x1C),
            green: Color32::from_rgb(0x10, 0x7C, 0x10),
            blue: Color32::from_rgb(0x1F, 0x5F, 0xBF),
        }
    }

    pub fn dark() -> Tokens {
        Tokens {
            dark: true,
            title_bar: Color32::from_rgb(0x6E, 0x1F, 0x93),
            on_title_bar: Color32::WHITE,
            ribbon: Color32::from_rgb(0x2B, 0x2B, 0x2D),
            tab_strip: Color32::from_rgb(0x2B, 0x2B, 0x2D),
            tab_selected: Color32::from_rgb(0xDD, 0xA6, 0xF7),
            canvas: Color32::from_rgb(0x1E, 0x1E, 0x20),
            page: Color32::from_rgb(0x26, 0x26, 0x28),
            page_text: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            panel: Color32::from_rgb(0x25, 0x25, 0x27),
            notebook_pane: Color32::from_rgb(0x22, 0x22, 0x24),
            page_list: Color32::from_rgb(0x24, 0x24, 0x26),
            page_list_selected: Color32::from_rgb(0x4A, 0x2A, 0x5E),
            border: Color32::from_rgb(0x3C, 0x3C, 0x40),
            border_strong: Color32::from_rgb(0x5A, 0x5A, 0x60),
            text: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            text_dim: Color32::from_rgb(0xB0, 0xB0, 0xB4),
            text_disabled: Color32::from_rgb(0x6E, 0x6E, 0x72),
            icon: Color32::from_rgb(0xD8, 0xD8, 0xDC),
            hover: Color32::from_rgb(0x3A, 0x30, 0x44),
            pressed: Color32::from_rgb(0x4A, 0x36, 0x5A),
            checked: Color32::from_rgb(0x44, 0x32, 0x54),
            accent: APP_COLOR,
            accent_text: Color32::from_rgb(0xDD, 0xA6, 0xF7),
            on_accent: on(APP_COLOR),
            nav: Color32::from_rgb(0x6E, 0x1F, 0x93),
            input: Color32::from_rgb(0x1E, 0x1E, 0x20),
            input_border: Color32::from_rgb(0x55, 0x55, 0x5A),
            status_bar: Color32::from_rgb(0x22, 0x22, 0x24),
            selection: Color32::from_rgba_unmultiplied(0xC4, 0x4D, 0xF0, 0x66),
            handle: Color32::from_rgb(0xDD, 0xA6, 0xF7),
            container_border: Color32::from_rgb(0x5A, 0x5A, 0x60),
            rule_line: Color32::from_rgb(0x34, 0x44, 0x58),
            rule_margin: Color32::from_rgb(0x6A, 0x3A, 0x3A),
            title_rule: Color32::from_rgb(0x4A, 0x4A, 0x4E),
            red: Color32::from_rgb(0xF1, 0x70, 0x70),
            green: Color32::from_rgb(0x6C, 0xCB, 0x5F),
            blue: Color32::from_rgb(0x6C, 0xA8, 0xF0),
        }
    }

    pub fn for_dark(dark: bool) -> Tokens {
        if dark { Tokens::dark() } else { Tokens::light() }
    }

    /// egui visuals matching these tokens: flat, opaque, corners of [`RADIUS_SMALL`].
    pub fn visuals(&self) -> Visuals {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        let small = CornerRadius::same(RADIUS_SMALL);
        v.window_corner_radius = CornerRadius::same(RADIUS_MEDIUM);
        v.menu_corner_radius = CornerRadius::same(RADIUS_MEDIUM);
        v.window_fill = self.panel;
        v.panel_fill = self.panel;
        v.extreme_bg_color = self.input;
        v.faint_bg_color = self.ribbon;
        v.window_stroke = Stroke::new(1.0, self.border_strong);
        v.selection.bg_fill = self.selection;
        v.selection.stroke = Stroke::new(1.0, self.accent_text);
        v.hyperlink_color = self.accent_text;
        v.override_text_color = Some(self.text);
        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = small;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        v.widgets.inactive.bg_fill = self.ribbon;
        v.widgets.inactive.weak_bg_fill = self.ribbon;
        v.widgets.hovered.bg_fill = self.hover;
        v.widgets.hovered.weak_bg_fill = self.hover;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.border_strong);
        v.widgets.active.bg_fill = self.pressed;
        v.widgets.active.weak_bg_fill = self.pressed;
        v.widgets.open.bg_fill = self.checked;
        v.widgets.open.weak_bg_fill = self.checked;
        v
    }

    /// Applies [`Tokens::visuals`] to every egui theme.
    pub fn apply(&self, ctx: &egui::Context) {
        ctx.set_visuals(self.visuals());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_contrast_meets_wcag_aa() {
        for t in [Tokens::light(), Tokens::dark()] {
            assert!(contrast(t.on_accent, t.accent) >= 4.5, "on_accent {}", t.dark);
            assert!(contrast(t.on_title_bar, t.title_bar) >= 4.5, "title bar {}", t.dark);
            assert!(contrast(t.text, t.panel) >= 7.0, "text {}", t.dark);
            assert!(contrast(t.text_dim, t.panel) >= 4.5, "dim {}", t.dark);
            assert!(contrast(t.accent_text, t.ribbon) >= 4.5, "accent text {}", t.dark);
            assert!(contrast(t.page_text, t.page) >= 7.0, "page {}", t.dark);
        }
        assert_eq!(APP_COLOR, Color32::from_rgb(0xC4, 0x4D, 0xF0));
    }
}
