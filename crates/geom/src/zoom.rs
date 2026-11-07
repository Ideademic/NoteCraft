//! Zoom: page points ↔ screen (logical pixels).
//!
//! At zoom `z`, a page point `p` is drawn at `origin + p * z`, where `origin` is where the page's
//! top-left corner sits on screen.

use crate::Point;

/// Smallest zoom (10 %).
pub const MIN: f32 = 0.1;
/// Largest zoom (400 %).
pub const MAX: f32 = 4.0;
/// The steps Zoom In / Zoom Out move between.
pub const PRESETS: [f32; 11] = [0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

/// `z` within [`MIN`]..=[`MAX`]; non-finite becomes 100 %.
pub fn clamp(z: f32) -> f32 {
    if z.is_finite() { z.clamp(MIN, MAX) } else { 1.0 }
}

/// The next preset above `z` (or [`MAX`]).
pub fn zoom_in(z: f32) -> f32 {
    let z = clamp(z);
    PRESETS.iter().copied().find(|p| *p > z + 1e-3).unwrap_or(MAX)
}

/// The next preset below `z` (or [`MIN`]).
pub fn zoom_out(z: f32) -> f32 {
    let z = clamp(z);
    PRESETS.iter().rev().copied().find(|p| *p < z - 1e-3).unwrap_or(MIN)
}

/// A page point on screen.
pub fn page_to_screen(p: Point, zoom: f32, origin: Point) -> Point {
    let z = clamp(zoom);
    Point { x: origin.x + p.x * z, y: origin.y + p.y * z }
}

/// A screen point on the page.
pub fn screen_to_page(s: Point, zoom: f32, origin: Point) -> Point {
    let z = clamp(zoom);
    Point { x: (s.x - origin.x) / z, y: (s.y - origin.y) / z }
}

/// "100%"-style label.
pub fn percent_label(z: f32) -> String {
    format!("{}%", (clamp(z) * 100.0).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_clamps() {
        assert_eq!(zoom_in(1.0), 1.25);
        assert_eq!(zoom_out(1.0), 0.75);
        assert_eq!(zoom_in(4.0), MAX);
        assert_eq!(zoom_out(0.25), MIN);
        assert_eq!(clamp(f32::NAN), 1.0);
        assert_eq!(clamp(100.0), MAX);
        assert_eq!(percent_label(1.25), "125%");
    }

    #[test]
    fn screen_round_trip() {
        let o = Point::new(100.0, 50.0);
        let p = Point::new(30.0, 40.0);
        let s = page_to_screen(p, 2.0, o);
        assert_eq!(s, Point::new(160.0, 130.0));
        assert_eq!(screen_to_page(s, 2.0, o), p);
    }
}
