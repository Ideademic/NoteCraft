//! Red wavy lines under misspelled words, for the paragraphs on screen only.
//!
//! The canvas never waits for the spelling check: `notecraft_engine::proof::lookup` answers from
//! the cache or queues the paragraph, and the canvas asks for another frame until the answers are
//! in. The desktop checks on a background thread; in the browser the canvas checks a few
//! milliseconds' worth per frame (`proof::work`). The word being typed (the caret at its end) isn't
//! marked until the caret leaves it, as in word processors.

use egui::text::CharIndex;
use egui::{Color32, Painter, Pos2, Stroke, pos2};
use notecraft_engine::Session;
use notecraft_engine::proof;
use notecraft_engine::text::{CellRef, Frame};

use super::CanvasState;
use super::layout::{OutlineLayout, PageLayout};
use super::view::View;

/// The browser's checking budget per frame, milliseconds.
const WEB_BUDGET_MS: f64 = 6.0;

/// The colour of the wavy line.
pub fn line_color(dark: bool) -> Color32 {
    if dark { Color32::from_rgb(0xFF, 0x6B, 0x6B) } else { Color32::from_rgb(0xD8, 0x1E, 0x28) }
}

/// Paints the misspellings of the visible paragraphs.
pub fn paint(ctx: &egui::Context, painter: &Painter, state: &mut CanvasState, session: &Session, v: &View, lay: &PageLayout, dark: bool) {
    state.review.marks.clear();
    if !state.review.spelling {
        return;
    }
    let Some(page) = session.current_page() else { return };
    let mut waiting = false;
    let color = line_color(dark);
    for o in &lay.outlines {
        if !o.rect(v).expand(40.0).intersects(v.clip) {
            continue;
        }
        let Some(model) = page.outline(o.id) else { continue };
        // The container's own paragraphs, then each table's cells.
        let mut flows: Vec<(Frame, &OutlineLayout)> = vec![(Frame::Top, o)];
        for t in &o.tables {
            for (r, row) in t.cells.iter().enumerate() {
                flows.extend(row.iter().enumerate().map(|(c, cl)| (Frame::Cell(CellRef::new(t.block, r, c)), cl)));
            }
        }
        for (frame, fl) in flows {
            let Some(flow) = notecraft_engine::text::flow(model, frame) else { continue };
            for (i, pl) in fl.paras.iter().enumerate() {
                let Some(at) = fl.galley_origin(v, i) else { continue };
                if pl.table || at.y > v.clip.bottom() || at.y + pl.height < v.clip.top() {
                    continue;
                }
                let Some(para) = flow.para(i) else { continue };
                let text = para.text();
                if text.trim().is_empty() {
                    continue;
                }
                let Some(found) = proof::lookup(&text, session.proof.lang_of(para)) else {
                    waiting = true;
                    continue;
                };
                let found = if session.proof.has_ignored() { session.proof.visible(&text, &found) } else { found.to_vec() };
                for m in &found {
                    if typing_here(state, o.id, frame, i, m.end) {
                        continue;
                    }
                    underline(painter, fl, v, i, m.start, m.end, color);
                    if state.review.marks.len() < 2000 {
                        state.review.marks.push((o.id, frame, i, m.start, m.end));
                    }
                }
            }
        }
    }
    if waiting {
        if !proof::BACKGROUND {
            proof::work(WEB_BUDGET_MS);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}

/// Whether the caret sits at the end of the word ending at `end` in paragraph `para` (still being
/// typed).
fn typing_here(state: &CanvasState, outline: notecraft_doc::Guid, frame: Frame, para: usize, end: usize) -> bool {
    state.caret.as_ref().is_some_and(|c| c.outline == outline && c.frame() == frame && c.is_collapsed() && c.head.para == para && c.head.char == end)
}

/// A wavy line under characters `a..b` of paragraph `i`, row by row.
fn underline(painter: &Painter, o: &OutlineLayout, v: &View, i: usize, a: usize, b: usize, color: Color32) {
    let Some(p) = o.paras.get(i) else { return };
    let Some(origin) = o.galley_origin(v, i) else { return };
    let mut base = 0usize;
    let amp = (v.scale * 1.1).clamp(1.0, 2.5);
    for row in &p.galley.rows {
        let n = row.char_count_including_newline().0;
        let (rs, re) = (a.max(base), b.min(base + row.char_count_excluding_newline().0));
        if rs < re {
            for xr in row.x_ranges(CharIndex(rs - base)..CharIndex(re - base)) {
                let y = origin.y + row.max_y() - amp * 0.6;
                let (x0, x1) = (origin.x + row.pos.x + xr.min, origin.x + row.pos.x + xr.max);
                if x1 - x0 >= 1.0 {
                    painter.add(egui::Shape::line(wave(x0, x1, y, amp), Stroke::new((v.scale * 0.75).clamp(0.8, 1.6), color)));
                }
            }
        }
        base += n;
        if base >= b {
            break;
        }
    }
}

/// A zig-zag from `x0` to `x1` around `y`.
fn wave(x0: f32, x1: f32, y: f32, amp: f32) -> Vec<Pos2> {
    let step = amp * 1.6;
    let n = (((x1 - x0) / step).ceil() as usize).clamp(1, 2000);
    (0..=n).map(|k| pos2((x0 + k as f32 * step).min(x1), if k % 2 == 0 { y + amp } else { y - amp })).collect()
}

#[cfg(test)]
mod tests {
    use super::wave;

    #[test]
    fn waves_stay_in_their_span() {
        let w = wave(10.0, 50.0, 5.0, 1.0);
        assert!(w.len() > 5 && w.iter().all(|p| (10.0..=50.0).contains(&p.x) && (p.y - 5.0).abs() <= 1.0));
        assert_eq!(wave(0.0, 1e9, 0.0, 1.0).len(), 2001, "bounded");
    }
}
