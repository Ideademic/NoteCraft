//! Unit conversions. Everything converts to and from points (1/72 inch).

/// Points per inch.
pub const PT_PER_INCH: f32 = 72.0;
/// Points per half-inch, OneNote's layout unit (MS-ONE offsets and widths).
pub const PT_PER_HALF_INCH: f32 = 36.0;
/// Points per millimetre.
pub const PT_PER_MM: f32 = 72.0 / 25.4;
/// Points per ink unit (0.01 mm, HIMETRIC): 72 / 2540.
pub const PT_PER_INK_UNIT: f32 = 72.0 / 2540.0;

pub fn inch_to_pt(v: f32) -> f32 {
    v * PT_PER_INCH
}
pub fn pt_to_inch(v: f32) -> f32 {
    v / PT_PER_INCH
}
pub fn half_inch_to_pt(v: f32) -> f32 {
    v * PT_PER_HALF_INCH
}
pub fn pt_to_half_inch(v: f32) -> f32 {
    v / PT_PER_HALF_INCH
}
pub fn mm_to_pt(v: f32) -> f32 {
    v * PT_PER_MM
}
pub fn pt_to_mm(v: f32) -> f32 {
    v / PT_PER_MM
}
/// Ink units (0.01 mm) to points.
pub fn ink_to_pt(v: f32) -> f32 {
    v * PT_PER_INK_UNIT
}
/// Points to ink units (0.01 mm).
pub fn pt_to_ink(v: f32) -> f32 {
    v / PT_PER_INK_UNIT
}
/// Pixels at `dpi` to points. A non-positive or non-finite `dpi` counts as 96.
pub fn px_to_pt(px: f32, dpi: f32) -> f32 {
    px * PT_PER_INCH / sane_dpi(dpi)
}
/// Points to pixels at `dpi`. A non-positive or non-finite `dpi` counts as 96.
pub fn pt_to_px(pt: f32, dpi: f32) -> f32 {
    pt * sane_dpi(dpi) / PT_PER_INCH
}

fn sane_dpi(dpi: f32) -> f32 {
    if dpi.is_finite() && dpi > 0.0 { dpi } else { 96.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn conversions_round_trip() {
        assert_eq!(half_inch_to_pt(1.0), 36.0);
        assert_eq!(pt_to_half_inch(72.0), 2.0);
        assert_eq!(inch_to_pt(0.5), half_inch_to_pt(1.0));
        assert!(close(ink_to_pt(2540.0), 72.0));
        assert!(close(pt_to_ink(72.0), 2540.0));
        assert!(close(mm_to_pt(25.4), 72.0));
        assert!(close(pt_to_mm(mm_to_pt(12.0)), 12.0));
        assert_eq!(px_to_pt(96.0, 96.0), 72.0);
        assert_eq!(pt_to_px(72.0, 144.0), 144.0);
        assert_eq!(pt_to_px(72.0, f32::NAN), 96.0);
        assert_eq!(px_to_pt(96.0, -1.0), 72.0);
    }
}
