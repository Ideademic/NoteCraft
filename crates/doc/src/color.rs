//! Colours: section and notebook colours, text and highlight colours, ink, page colour.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An sRGB colour with straight (not premultiplied) alpha. JSON form: `"#rrggbb"`, or
/// `"#rrggbbaa"` when not opaque.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(0xFF, 0xFF, 0xFF);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    /// NoteCraft's brand colour (`#c44df0`).
    pub const NOTECRAFT: Color = Color::rgb(0xC4, 0x4D, 0xF0);
    /// The default text highlight (yellow).
    pub const HIGHLIGHT_YELLOW: Color = Color::rgb(0xFF, 0xFF, 0x00);

    /// Section and notebook colours offered in the colour menus, in order. NoteCraft's own
    /// palette: bold, distinct, readable with dark text on top.
    pub const SECTION_PALETTE: [Color; 16] = [
        Color::rgb(0x8A, 0xA8, 0xE4), // blue
        Color::rgb(0xF6, 0xD3, 0x6B), // yellow
        Color::rgb(0x9C, 0xD8, 0x7E), // green
        Color::rgb(0xF2, 0x8B, 0x8B), // red
        Color::rgb(0xC4, 0x9B, 0xEA), // purple
        Color::rgb(0x7F, 0xD3, 0xD8), // cyan
        Color::rgb(0xF7, 0xB2, 0x6E), // orange
        Color::rgb(0xEB, 0x94, 0xC8), // magenta
        Color::rgb(0xB5, 0xC6, 0xE8), // mist blue
        Color::rgb(0xD5, 0xC4, 0xEC), // mist purple
        Color::rgb(0xD9, 0xC2, 0x9C), // tan
        Color::rgb(0xD7, 0xE8, 0x7D), // lime
        Color::rgb(0xA7, 0xDB, 0xA0), // apple
        Color::rgb(0x6F, 0xBF, 0xB0), // teal
        Color::rgb(0xE4, 0xA1, 0x95), // chalk red
        Color::rgb(0xC8, 0xC8, 0xCC), // silver
    ];

    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 0xFF }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }

    /// The palette colour for the `n`th new section (wraps around).
    pub fn section_color(n: usize) -> Color {
        Color::SECTION_PALETTE.get(n % Color::SECTION_PALETTE.len()).copied().unwrap_or(Color::NOTECRAFT)
    }

    pub fn with_alpha(self, a: u8) -> Color {
        Color { a, ..self }
    }

    pub fn is_opaque(self) -> bool {
        self.a == 0xFF
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`, with or without `#`, any case.
    pub fn parse_hex(s: &str) -> Option<Color> {
        let h = s.trim();
        let h = h.strip_prefix('#').unwrap_or(h);
        if !h.is_ascii() {
            return None;
        }
        let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
        match h.len() {
            3 => {
                let n = |i: usize| h.get(i..=i).and_then(|x| u8::from_str_radix(x, 16).ok()).map(|v| v * 17);
                Some(Color::rgb(n(0)?, n(1)?, n(2)?))
            }
            6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque (lower case).
    pub fn to_hex(self) -> String {
        if self.is_opaque() {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Relative luminance (WCAG 2), 0–1.
    pub fn luminance(self) -> f32 {
        fn lin(c: u8) -> f32 {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG contrast ratio between two opaque colours, 1–21.
    pub fn contrast(self, other: Color) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Color::parse_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("not a colour: {}", s.chars().take(32).collect::<String>())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::parse_hex("#c44df0"), Some(Color::NOTECRAFT));
        assert_eq!(Color::parse_hex("C44DF0"), Some(Color::NOTECRAFT));
        assert_eq!(Color::parse_hex("#fff"), Some(Color::WHITE));
        assert_eq!(Color::parse_hex("#11223380"), Some(Color::rgba(0x11, 0x22, 0x33, 0x80)));
        assert_eq!(Color::NOTECRAFT.to_hex(), "#c44df0");
        assert_eq!(Color::rgba(1, 2, 3, 4).to_hex(), "#01020304");
        for bad in ["", "#12", "#gggggg", "#1234567", "ééé"] {
            assert_eq!(Color::parse_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn serde_and_contrast() {
        let j = serde_json::to_string(&Color::NOTECRAFT).unwrap();
        assert_eq!(j, "\"#c44df0\"");
        assert_eq!(serde_json::from_str::<Color>(&j).unwrap(), Color::NOTECRAFT);
        assert!((Color::BLACK.contrast(Color::WHITE) - 21.0).abs() < 0.01);
        assert_eq!(Color::section_color(16), Color::SECTION_PALETTE[0]);
    }
}
