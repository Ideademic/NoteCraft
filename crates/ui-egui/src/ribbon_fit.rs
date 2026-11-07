//! Ribbon groups that shrink to fit the window, the way Office's ribbon does: when a tab is
//! wider than the window, groups give up their small buttons' labels and galleries show fewer
//! tiles (compact), then fold into a single drop-down button that opens the whole group
//! (collapsed), starting from the right. Ported from WordCraft's `ribbon_fit` (same licence).
//!
//! Each frame [`begin`] plans every group's level from the widths the groups measured on earlier
//! frames ([`record`]) and the width there is; [`crate::widgets::group`] reads its level
//! with [`level`]. Groups are known by their position on the tab and their title, so two custom
//! groups with the same name keep their own widths. A group drawn as designed also estimates its
//! compact width from what its controls would give up ([`add_saving`]), so the planner can choose
//! a compact group over a collapsed one before it has ever been drawn compact. A width not measured
//! yet is estimated, and the plan settles once it is. Widths are measured again after the interface
//! language or scale changes. The ribbon still scrolls if even fully collapsed groups don't fit.
//!
//! The simplified ribbon fits differently: whole segments move into a More commands menu from the
//! right ([`overflow_fit`]).

use std::collections::HashMap;

use egui::{Context, Id};

/// The group as designed.
pub const FULL: u8 = 0;
/// Small buttons without their labels (tooltips keep the names), a shorter Styles gallery.
pub const COMPACT: u8 = 1;
/// One drop-down button holding the whole group.
pub const COLLAPSED: u8 = 2;

/// Estimated width of a collapsed group (its button and divider) before it is measured.
const COLLAPSED_W: f32 = 64.0;

/// One group of a tab in drawing order: its title and measured widths at each level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupWidths {
    pub title: String,
    pub widths: [Option<f32>; 3],
}

/// Each group's level (by position) so that the row fits `avail`: groups shrink from the right,
/// first all to compact, then to collapsed, stopping as soon as the row fits. Then whatever room is
/// left is given back, from the left.
pub fn plan(groups: &[GroupWidths], avail: f32) -> Vec<u8> {
    let full = |g: &GroupWidths| g.widths[0].unwrap_or(0.0);
    let width = |g: &GroupWidths, level: u8| -> f32 {
        match level {
            FULL => full(g),
            COMPACT => g.widths[1].unwrap_or(full(g)),
            _ => g.widths[2].unwrap_or(COLLAPSED_W.min(full(g))),
        }
    };
    let mut levels = vec![FULL; groups.len()];
    let mut total: f32 = groups.iter().map(full).sum();
    for target in [COMPACT, COLLAPSED] {
        for i in (0..groups.len()).rev() {
            if total <= avail {
                break;
            }
            let (Some(g), Some(level)) = (groups.get(i), levels.get_mut(i)) else { continue };
            total += width(g, target) - width(g, *level);
            *level = target;
        }
    }
    // Shrinking a wide group (Styles) can make room the groups shrunk before it no longer need,
    // and a group of large buttons gains nothing collapsed: give back whatever still fits.
    for (g, level) in groups.iter().zip(levels.iter_mut()) {
        while *level > FULL {
            let wider = *level - 1;
            let next = total + width(g, wider) - width(g, *level);
            if !next.is_finite() || next > avail {
                break;
            }
            total = next;
            *level = wider;
        }
    }
    levels
}

/// The simplified ribbon's row: how many of its segments (widths in drawing order, `None` when not
/// measured yet, counted as `guess`) stay in a row `avail` points wide. All of them when they fit;
/// else as many from the left as leave room for the More commands button (`more_w`); the rest
/// move into its menu.
pub fn overflow_fit(widths: &[Option<f32>], avail: f32, more_w: f32, guess: f32) -> usize {
    if avail.is_nan() {
        return 0;
    }
    let w = |x: &Option<f32>| x.filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(guess);
    let total: f32 = widths.iter().map(w).sum();
    if total <= avail {
        return widths.len();
    }
    let room = avail - more_w;
    let mut used = 0.0;
    let mut n = 0;
    for x in widths {
        let next = used + w(x);
        if next > room || !next.is_finite() {
            break;
        }
        used = next;
        n += 1;
    }
    n
}

/// What this frame's ribbon is drawing.
#[derive(Clone, Default)]
struct Frame {
    tab: String,
    /// The plan, by position, and the titles it was made for.
    titles: Vec<String>,
    levels: Vec<u8>,
    drawn: Vec<String>,
}

/// Widths measured so far: (tab, position, group, level) → width, and each tab's group order.
#[derive(Clone, Default)]
struct Measured {
    /// The interface language and scale the widths were measured at.
    signature: String,
    widths: HashMap<(String, usize, String, u8), f32>,
    order: HashMap<String, Vec<String>>,
}

/// Stable storage id for the ribbon fit plan of the current frame.
fn frame_id() -> Id {
    Id::unique("ribbon_fit_frame")
}
/// Stable storage id for widths measured from earlier ribbon frames.
fn measured_id() -> Id {
    Id::unique("ribbon_fit_measured")
}
/// Stable storage id for the ribbon's compact-layout choices.
fn compact_id() -> Id {
    Id::unique("ribbon_fit_compact")
}
/// Stable storage id for the points a group would save by drawing compact.
fn saving_id() -> Id {
    Id::unique("ribbon_fit_saving")
}
/// Stable storage id for whether this frame's ribbon fit plan is active.
fn active_id() -> Id {
    Id::unique("ribbon_fit_active")
}

/// What the measured widths depend on besides the groups themselves (the interface language
/// joins the scale once NoteCraft has translations).
fn signature(ctx: &Context) -> String {
    format!("en:{}", ctx.pixels_per_point())
}

/// Start drawing `tab` in a row `avail` points wide: plan the groups' levels.
pub fn begin(ctx: &Context, tab: &str, avail: f32) {
    let sig = signature(ctx);
    let (titles, groups) = ctx.data_mut(|d| {
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        if m.signature != sig {
            // Another language or scale: labels have new widths, so measure again.
            *m = Measured { signature: sig, ..Default::default() };
        }
        let titles = m.order.get(tab).cloned().unwrap_or_default();
        let groups: Vec<GroupWidths> = titles
            .iter()
            .enumerate()
            .map(|(i, t)| GroupWidths {
                title: t.clone(),
                widths: [FULL, COMPACT, COLLAPSED].map(|l| m.widths.get(&(tab.to_string(), i, t.clone(), l)).copied()),
            })
            .collect();
        (titles, groups)
    });
    let levels = if groups.iter().all(|g| g.widths[0].is_some()) { plan(&groups, avail) } else { Vec::new() };
    ctx.data_mut(|d| {
        d.insert_temp(frame_id(), Frame { tab: tab.to_string(), titles, levels, drawn: Vec::new() });
        d.insert_temp(active_id(), true);
    });
}

/// How many groups of this ribbon row are drawn so far: the next group's place, which tells apart
/// groups that share a title (a custom group named like a built-in one).
pub fn position(ctx: &Context) -> usize {
    ctx.data(|d| d.get_temp::<Frame>(frame_id()).map(|f| f.drawn.len())).unwrap_or(0)
}

/// The level the next group, `title`, draws at this frame.
pub fn level(ctx: &Context, title: &str) -> u8 {
    ctx.data(|d| {
        let f = d.get_temp::<Frame>(frame_id())?;
        let i = f.drawn.len();
        // Planned for another group here (one was added or removed): draw it as designed.
        if f.titles.get(i).map(String::as_str) != Some(title) {
            return None;
        }
        f.levels.get(i).copied()
    })
    .unwrap_or(FULL)
}

/// Group `title` drew at `level`, `width` points wide.
pub fn record(ctx: &Context, title: &str, level: u8, width: f32) {
    ctx.data_mut(|d| {
        let saving = d.remove_temp::<f32>(saving_id()).unwrap_or(0.0);
        // Only groups on the ribbon row between `begin` and `end` are planned.
        if !d.get_temp::<bool>(active_id()).unwrap_or(false) {
            return;
        }
        let f = d.get_temp_mut_or_default::<Frame>(frame_id());
        let i = f.drawn.len();
        f.drawn.push(title.to_string());
        let tab = f.tab.clone();
        if !(width.is_finite() && width > 0.0) {
            return;
        }
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        m.widths.insert((tab.clone(), i, title.to_string(), level), width);
        // Drawn as designed: until it is drawn compact, its compact width is what its labels and
        // gallery tiles would give back.
        if level == FULL && saving.is_finite() && saving > 0.0 {
            m.widths.entry((tab, i, title.to_string(), COMPACT)).or_insert((width - saving).max(COLLAPSED_W));
        }
    });
}

/// The row is drawn: remember its group order for the next frame's plan.
pub fn end(ctx: &Context) {
    let changed = ctx.data_mut(|d| {
        d.insert_temp(active_id(), false);
        let drawn = std::mem::take(&mut d.get_temp_mut_or_default::<Frame>(frame_id()).drawn);
        let tab = d.get_temp_mut_or_default::<Frame>(frame_id()).tab.clone();
        let m = d.get_temp_mut_or_default::<Measured>(measured_id());
        let changed = m.order.get(&tab) != Some(&drawn);
        if changed {
            m.order.insert(tab, drawn);
        }
        changed
    });
    if changed {
        // A new order (another tab, a group appeared): plan again with it next frame.
        ctx.request_repaint();
    }
}

/// Whether the group being drawn is compact (small buttons drop their labels).
pub fn compact(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(compact_id())).unwrap_or(false)
}

/// Mark the group being drawn compact or not, and start counting what it could give up.
pub fn set_compact(ctx: &Context, on: bool) {
    ctx.data_mut(|d| {
        d.insert_temp(compact_id(), on);
        d.remove_temp::<f32>(saving_id());
    });
}

/// The group's controls are drawn: what follows isn't compact (what they could give up is kept).
pub fn end_compact(ctx: &Context) {
    ctx.data_mut(|d| d.insert_temp(compact_id(), false));
}

/// A control of a group drawn as designed would be `w` points narrower in a compact group.
pub fn add_saving(ctx: &Context, w: f32) {
    if w.is_finite() && w > 0.0 && !compact(ctx) {
        ctx.data_mut(|d| *d.get_temp_mut_or_default::<f32>(saving_id()) += w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(title: &str, full: f32, compact: Option<f32>, collapsed: Option<f32>) -> GroupWidths {
        GroupWidths { title: title.into(), widths: [Some(full), compact, collapsed] }
    }

    #[test]
    fn everything_full_when_it_fits() {
        let gs = [g("A", 100.0, Some(80.0), Some(60.0)), g("B", 100.0, Some(80.0), Some(60.0))];
        assert_eq!(plan(&gs, 200.0), vec![FULL, FULL]);
    }

    #[test]
    fn rightmost_groups_compact_first_then_collapse() {
        let gs = [g("A", 100.0, Some(80.0), Some(60.0)), g("B", 100.0, Some(80.0), Some(60.0)), g("C", 100.0, Some(80.0), Some(60.0))];
        // 300 → C compact (280) fits 285.
        assert_eq!(plan(&gs, 285.0), vec![FULL, FULL, COMPACT]);
        // All compact is 240; then C collapses (220) to fit 230.
        assert_eq!(plan(&gs, 230.0), vec![COMPACT, COMPACT, COLLAPSED]);
        // Nothing fits: everything collapsed (the row scrolls).
        assert!(plan(&gs, 10.0).iter().all(|l| *l == COLLAPSED));
    }

    #[test]
    fn groups_that_still_fit_are_given_back() {
        // A wide gallery group in the middle and big-button groups that don't shrink.
        let gs = [
            g("Font", 300.0, Some(250.0), Some(60.0)),
            g("Styles", 500.0, Some(500.0), Some(60.0)),
            g("Voice", 60.0, Some(60.0), Some(60.0)),
            g("Editor", 60.0, Some(60.0), Some(60.0)),
        ];
        // 920 → 800: Styles collapses; Font, Voice and Editor stay as designed.
        assert_eq!(plan(&gs, 800.0), vec![FULL, COLLAPSED, FULL, FULL]);
    }

    /// the Home tab's shape at 1440 pt. The Styles gallery trimmed to fewer tiles (its
    /// compact width) fits, so it stays a gallery and no group folds.
    #[test]
    fn trimmed_gallery_keeps_styles_open() {
        let gs = [
            g("Clipboard", 130.0, Some(70.0), Some(60.0)),
            g("Font", 330.0, Some(290.0), Some(60.0)),
            g("Paragraph", 300.0, Some(280.0), Some(60.0)),
            g("Styles", 520.0, Some(290.0), Some(60.0)),
            g("Editing", 80.0, Some(40.0), Some(60.0)),
            g("Voice", 60.0, Some(60.0), Some(60.0)),
        ];
        let p = plan(&gs, 1250.0);
        assert_eq!(p, vec![FULL, FULL, FULL, COMPACT, FULL, FULL]);
        // Styles as designed (compact = full, the old behaviour) would have to fold.
        let mut wide = gs.clone();
        if let Some(s) = wide.get_mut(3) {
            s.widths[1] = Some(520.0);
        }
        assert_eq!(plan(&wide, 1250.0).get(3), Some(&COLLAPSED));
    }

    #[test]
    fn same_titles_keep_their_own_levels() {
        let gs = [g("New Group", 300.0, Some(300.0), Some(60.0)), g("New Group", 100.0, Some(100.0), Some(60.0))];
        assert_eq!(plan(&gs, 200.0), vec![COLLAPSED, FULL]);
    }

    #[test]
    fn simplified_row_moves_segments_into_more_from_the_right() {
        let w = [Some(100.0), Some(100.0), Some(100.0)];
        assert_eq!(overflow_fit(&w, 300.0, 30.0, 90.0), 3, "all fit without the More button");
        assert_eq!(overflow_fit(&w, 299.0, 30.0, 90.0), 2);
        assert_eq!(overflow_fit(&w, 229.0, 30.0, 90.0), 1);
        assert_eq!(overflow_fit(&w, 50.0, 30.0, 90.0), 0);
        assert_eq!(overflow_fit(&[None, Some(10.0)], 100.0, 30.0, 90.0), 2, "unknown widths are guessed");
        assert_eq!(overflow_fit(&[Some(f32::NAN), Some(f32::INFINITY)], 100.0, 30.0, 40.0), 2);
        assert_eq!(overflow_fit(&w, f32::NAN, 30.0, 90.0), 0);
        assert_eq!(overflow_fit(&[], 0.0, 30.0, 90.0), 0);
    }

    #[test]
    fn unmeasured_widths_are_estimated() {
        let gs = [g("A", 200.0, None, None), g("B", 200.0, None, None)];
        // Compact unknown counts as full, so the planner collapses B (estimated 64) and A, which
        // then fits as designed, is given back.
        assert_eq!(plan(&gs, 300.0), vec![FULL, COLLAPSED]);
        // Hostile numbers don't panic.
        let odd = [g("X", f32::NAN, None, None), g("Y", f32::INFINITY, Some(f32::NAN), None)];
        let _ = plan(&odd, 100.0);
        let _ = plan(&odd, f32::NAN);
    }
}
