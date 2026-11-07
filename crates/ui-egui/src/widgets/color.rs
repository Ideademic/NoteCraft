//! Colour grids for the colour menus: font colour, highlight, ink, page and section colours.
//! NoteCraft's own palettes (bold, distinct); nothing taken from another product.

use egui::{Color32, CornerRadius, Rect, Sense, Stroke, Ui, vec2};

use crate::theme::Tokens;

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Font colours: a bold row and a deep row.
pub const TEXT: [Color32; 20] = [
    c(0x00, 0x00, 0x00),
    c(0x5A, 0x5A, 0x5A),
    c(0xD6, 0x2D, 0x20),
    c(0xF2, 0x7A, 0x1A),
    c(0xE8, 0xB4, 0x00),
    c(0x2E, 0xA0, 0x4F),
    c(0x10, 0x9C, 0xA8),
    c(0x2F, 0x6F, 0xD6),
    c(0x8A, 0x22, 0xB8),
    c(0xD0, 0x3A, 0x8C),
    c(0xFF, 0xFF, 0xFF),
    c(0x9A, 0x9A, 0x9E),
    c(0x8E, 0x1B, 0x12),
    c(0xA4, 0x4E, 0x0C),
    c(0x8A, 0x6A, 0x00),
    c(0x1A, 0x63, 0x30),
    c(0x0A, 0x5E, 0x66),
    c(0x1B, 0x3F, 0x86),
    c(0x55, 0x14, 0x73),
    c(0x86, 0x1E, 0x58),
];

/// Text highlight colours.
pub const HIGHLIGHT: [Color32; 10] = [
    c(0xFF, 0xF1, 0x00),
    c(0x7F, 0xF2, 0x5A),
    c(0x5A, 0xE6, 0xF2),
    c(0xFF, 0x8A, 0xD8),
    c(0x8A, 0xB4, 0xFF),
    c(0xFF, 0x9C, 0x5A),
    c(0xD2, 0xA6, 0xFF),
    c(0xFF, 0x6B, 0x6B),
    c(0xC8, 0xC8, 0xC8),
    c(0xB9, 0xE8, 0x8F),
];

/// Ink colours (Draw › Colour & Thickness).
pub const INK: [Color32; 16] = [
    c(0x00, 0x00, 0x00),
    c(0x5A, 0x5A, 0x5A),
    c(0xFF, 0xFF, 0xFF),
    c(0xD6, 0x2D, 0x20),
    c(0xF2, 0x7A, 0x1A),
    c(0xFF, 0xD4, 0x00),
    c(0x2E, 0xA0, 0x4F),
    c(0x10, 0x9C, 0xA8),
    c(0x2F, 0x6F, 0xD6),
    c(0x1B, 0x3F, 0x86),
    c(0xC4, 0x4D, 0xF0),
    c(0x8A, 0x22, 0xB8),
    c(0xD0, 0x3A, 0x8C),
    c(0x8A, 0x5A, 0x2E),
    c(0x7F, 0xF2, 0x5A),
    c(0x5A, 0xE6, 0xF2),
];

/// Page colours: pale, so text stays readable.
pub const PAGE: [Color32; 16] = [
    c(0xFF, 0xF8, 0xD6),
    c(0xFF, 0xE9, 0xD2),
    c(0xFF, 0xDE, 0xDE),
    c(0xFC, 0xE1, 0xF1),
    c(0xF1, 0xE3, 0xFC),
    c(0xE4, 0xE6, 0xFC),
    c(0xDD, 0xEE, 0xFF),
    c(0xD9, 0xF5, 0xF7),
    c(0xDD, 0xF4, 0xE0),
    c(0xEE, 0xF6, 0xD5),
    c(0xF3, 0xEE, 0xE2),
    c(0xEE, 0xEE, 0xEE),
    c(0xFF, 0xFF, 0xFF),
    c(0xE8, 0xF0, 0xE8),
    c(0xF5, 0xEC, 0xFF),
    c(0xFF, 0xF2, 0xE6),
];

/// A grid of swatches, `per_row` across; returns the one clicked. `current` gets a ring.
pub fn grid(ui: &mut Ui, colors: &[Color32], per_row: usize, current: Option<Color32>) -> Option<Color32> {
    let t = Tokens::get(ui.ctx());
    let mut picked = None;
    let per_row = per_row.max(1);
    egui::Grid::new(ui.next_auto_id()).spacing(vec2(3.0, 3.0)).show(ui, |ui| {
        for (i, col) in colors.iter().enumerate() {
            if swatch(ui, *col, current == Some(*col), &t) {
                picked = Some(*col);
            }
            if (i + 1) % per_row == 0 {
                ui.end_row();
            }
        }
    });
    if picked.is_some() {
        ui.close();
    }
    picked
}

/// One 18 px swatch. Returns whether it was clicked.
pub fn swatch(ui: &mut Ui, col: Color32, selected: bool, t: &Tokens) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
    let hex = format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &hex));
    let radius = CornerRadius::same(2);
    ui.painter().rect_filled(r, radius, col);
    ui.painter().rect_stroke(r, radius, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    if selected || resp.hovered() {
        let ring = Rect::from_center_size(r.center(), r.size() + vec2(4.0, 4.0));
        ui.painter().rect_stroke(ring, radius, Stroke::new(1.5, t.accent_text), egui::StrokeKind::Inside);
    }
    resp.on_hover_text(hex).clicked()
}

/// `#rrggbb` for a colour.
pub fn hex(col: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_are_opaque_and_distinct() {
        for set in [&TEXT[..], &HIGHLIGHT[..], &INK[..], &PAGE[..]] {
            for (i, a) in set.iter().enumerate() {
                assert_eq!(a.a(), 255);
                assert!(set.iter().skip(i + 1).all(|b| b != a), "{}", hex(*a));
            }
        }
        assert_eq!(hex(c(0xC4, 0x4D, 0xF0)), "#c44df0");
        // Page colours keep page text readable.
        for p in PAGE {
            assert!(crate::theme::contrast(Tokens::light().page_text, p) >= 7.0);
        }
    }
}
