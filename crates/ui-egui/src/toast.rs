//! Toasts: a short message in a pill near the bottom centre of the window that fades away by
//! itself (a "Coming soon" for a control that isn't built yet). One at a time; a new one replaces
//! it. Status-bar notices stay for what the user should be able to read again.

use egui::{Align2, Context, CornerRadius, Id, Order, Rect, pos2, vec2};

use crate::theme::{RADIUS_SMALL, Tokens, regular};

/// How long a toast shows, seconds (the last [`FADE`] of it fading out).
pub const DURATION: f64 = 2.5;
/// The fade at the end, seconds.
pub const FADE: f64 = 0.4;
/// Room between the toast and the status bar.
const GAP: f32 = 16.0;

/// The toast showing, if any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toast {
    pub text: String,
    /// When it first showed (egui time); set at its first paint.
    shown_at: Option<f64>,
}

impl Toast {
    pub fn new(text: impl Into<String>) -> Toast {
        Toast { text: text.into(), shown_at: None }
    }
}

/// Paints the toast, and drops it once its time is up.
pub fn show(ctx: &Context, toast: &mut Option<Toast>, t: &Tokens) {
    let Some(current) = toast.as_mut() else { return };
    let now = ctx.input(|i| i.time);
    let age = now - *current.shown_at.get_or_insert(now);
    if !(0.0..DURATION).contains(&age) {
        *toast = None;
        return;
    }
    // Fully opaque until the fade.
    let alpha = (((DURATION - age) / FADE).clamp(0.0, 1.0)) as f32;
    let font = regular(13.0);
    let galley = ctx.fonts_mut(|f| f.layout_no_wrap(current.text.clone(), font, t.panel));
    let size = galley.size() + vec2(28.0, 14.0);
    let screen = ctx.content_rect();
    let bottom = screen.max.y - crate::chrome::STATUS_BAR_H - GAP;
    let rect = Rect::from_min_size(pos2(screen.center().x - size.x / 2.0, bottom - size.y), size);
    egui::Area::new(Id::unique("nc_toast")).order(Order::Tooltip).fixed_pos(rect.min).interactable(false).constrain(false).show(ctx, |ui| {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(RADIUS_SMALL), t.text.gamma_multiply(alpha));
        p.galley_with_override_text_color(Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size()).min, galley, t.panel.gamma_multiply(alpha));
    });
    // Wake for the fade, then every frame until it's gone.
    let still = DURATION - FADE - age;
    if still > 0.0 {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(still));
    } else {
        ctx.request_repaint();
    }
}
