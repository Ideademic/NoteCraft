//! Ink as SVG: one `<polyline>` per stroke, or one `<line>` per segment when pressure changes the
//! width along the stroke. Highlighters are drawn with their tool opacity.

use std::fmt::Write as _;

use notecraft_doc::{InkStroke, Rect};

/// Most strokes written into one SVG (the model caps a page's ink too).
pub const MAX_SVG_STROKES: usize = notecraft_doc::ink::MAX_INK_PER_PAGE;

/// The strokes as an `<svg>` element covering `view` (page points). `positioned` adds inline CSS
/// that places it absolutely at `view`'s top-left corner (inside a `position: relative` page).
pub fn ink_svg(strokes: &[InkStroke], view: Rect, positioned: bool) -> String {
    let (x, y) = (num(view.x), num(view.y));
    let (w, h) = (num(view.w.max(1.0)), num(view.h.max(1.0)));
    let mut out = String::new();
    let style = if positioned {
        format!(" style=\"position:absolute;left:{x}pt;top:{y}pt;width:{w}pt;height:{h}pt;pointer-events:none\"")
    } else {
        String::new()
    };
    let _ = write!(
        out,
        "<svg class=\"ink\" xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" aria-label=\"Ink\" viewBox=\"{x} {y} {w} {h}\" width=\"{w}pt\" height=\"{h}pt\"{style}>"
    );
    for st in strokes.iter().take(MAX_SVG_STROKES) {
        stroke_svg(st, &mut out);
    }
    out.push_str("</svg>");
    out
}

/// The strokes' bounds (with their widths), or `None` without ink.
pub fn ink_bounds(strokes: &[InkStroke]) -> Option<Rect> {
    strokes.iter().filter_map(InkStroke::bounds).reduce(Rect::union)
}

fn stroke_svg(st: &InkStroke, out: &mut String) {
    let Some(first) = st.points.first() else { return };
    let color = st.color;
    let rgb = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    let opacity = (f32::from(color.a) / 255.0 * st.tool.alpha()).clamp(0.0, 1.0);
    let op = if opacity < 1.0 { format!(" stroke-opacity=\"{}\"", num(opacity)) } else { String::new() };
    let tool = st.tool.name();
    if st.points.len() == 1 {
        let r = num(st.sample_width(first.pressure) / 2.0);
        let fop = if opacity < 1.0 { format!(" fill-opacity=\"{}\"", num(opacity)) } else { String::new() };
        let _ = write!(out, "<circle class=\"{tool}\" cx=\"{}\" cy=\"{}\" r=\"{r}\" fill=\"{rgb}\"{fop}/>", num(first.x), num(first.y));
        return;
    }
    let varies = st.tool.uses_pressure()
        && !st.pressure_is_flat()
        && st.points.windows(2).any(|w| w.first().map(|a| a.pressure) != w.get(1).map(|b| b.pressure));
    if varies && opacity >= 1.0 {
        // Segments of their own width; overlapping round caps make an even line.
        let _ = write!(out, "<g class=\"{tool}\" stroke=\"{rgb}\" stroke-linecap=\"round\" fill=\"none\">");
        for w in st.points.windows(2) {
            let (Some(a), Some(b)) = (w.first(), w.get(1)) else { continue };
            let width = (st.sample_width(a.pressure) + st.sample_width(b.pressure)) / 2.0;
            let _ = write!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"{}\"/>",
                num(a.x),
                num(a.y),
                num(b.x),
                num(b.y),
                num(width)
            );
        }
        out.push_str("</g>");
        return;
    }
    // One polyline at the average sample width (translucent ink must not overlap itself).
    let n = st.points.len() as f32;
    let width = st.points.iter().map(|p| st.sample_width(p.pressure)).sum::<f32>() / n;
    let pts: Vec<String> = st.points.iter().map(|p| format!("{},{}", num(p.x), num(p.y))).collect();
    let _ = write!(
        out,
        "<polyline class=\"{tool}\" points=\"{}\" fill=\"none\" stroke=\"{rgb}\"{op} stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
        pts.join(" "),
        num(width)
    );
}

/// A number for SVG/CSS: finite, at most two decimals, no trailing zeros.
pub(crate) fn num(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".to_owned() } else { s.to_owned() }
}

#[cfg(test)]
mod tests {
    use notecraft_doc::{InkPoint, InkTool};

    use super::*;

    #[test]
    fn strokes_become_svg() {
        let pen = InkStroke::new(InkTool::Pen, vec![InkPoint::new(0.0, 0.0, 1.0), InkPoint::new(10.0, 0.0, 0.5), InkPoint::new(20.0, 5.0, 0.2)]);
        let hl = InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(0.0, 10.0, 1.0), InkPoint::new(50.0, 10.0, 1.0)]);
        let dot = InkStroke::new(InkTool::Pencil, vec![InkPoint::new(3.0, 3.0, 1.0)]);
        let strokes = vec![pen, hl, dot];
        let b = ink_bounds(&strokes).unwrap();
        let svg = ink_svg(&strokes, b, true);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
        assert!(svg.contains("<line "), "pressure pen as segments: {svg}");
        assert!(svg.contains("<polyline class=\"highlighter\"") && svg.contains("stroke-opacity=\"0.5\""));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("position:absolute"));
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(f32::NAN), "0");
        assert_eq!(num(-0.001), "0");
        assert!(ink_bounds(&[]).is_none());
    }
}
