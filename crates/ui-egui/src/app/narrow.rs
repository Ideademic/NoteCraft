//! Narrow windows: the page keeps room to work. When the side panes would leave the page less than
//! [`MIN_CANVAS_WIDTH`], the notebook pane folds to its bar, then the page list narrows and folds
//! to its bar too. This is temporary: the user's choices (`notebook_pane_open`, `page_list_open`)
//! and dragged widths stay as they are and come back when the window is wide enough. While a pane
//! is folded this way, its bar opens it as an overlay over the page, closed by a click outside.
//! Each side pane is also held to [`MAX_PANE_SHARE`] of the window.

use egui::{Id, IdSalt, Panel, PanelState, Rect, Ui, pos2, vec2};

use super::{NOTEBOOK_BAR_WIDTH, NOTEBOOK_PANE_WIDTH, NoteApp, PAGE_LIST_WIDTH};

/// The page keeps at least this much width before the side panes give way.
pub const MIN_CANVAS_WIDTH: f32 = 360.0;
/// A side pane takes at most this share of the window's width.
pub const MAX_PANE_SHARE: f32 = 0.35;
/// The folded page list's width.
pub const PAGE_BAR_WIDTH: f32 = 28.0;

/// A resizable side pane: egui's id for it, its default width, its narrowest and widest.
#[derive(Clone, Copy, Debug)]
pub struct PaneSpec {
    pub salt: &'static str,
    pub default: f32,
    pub range: (f32, f32),
}

pub const NOTEBOOKS: PaneSpec = PaneSpec { salt: "nc_notebooks", default: NOTEBOOK_PANE_WIDTH, range: (160.0, 480.0) };
pub const PAGES: PaneSpec = PaneSpec { salt: "nc_pages", default: PAGE_LIST_WIDTH, range: (150.0, 480.0) };
pub const BIN_PAGES: PaneSpec = PaneSpec { salt: "nc_bin_pages", ..PAGES };
pub const TEMPLATES: PaneSpec = PaneSpec { salt: "nc_templates_pane", default: crate::panes::templates::PANE_W, range: (220.0, 480.0) };
pub const TAGS: PaneSpec = PaneSpec { salt: "nc_tags_pane", default: crate::panes::tags::PANE_W, range: (220.0, 480.0) };
pub const RECENT_EDITS: PaneSpec = PaneSpec { salt: "nc_recent_edits", default: crate::panes::history::RECENT_W, range: (200.0, 480.0) };
pub const SPELLING: PaneSpec = PaneSpec { salt: "nc_spelling", default: crate::panes::spelling::PANE_W, range: (200.0, 420.0) };

/// A pane the window can fold for room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidePane {
    Notebooks,
    Pages,
}

/// What a narrow window has done to the panes this frame (not saved).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NarrowPanes {
    /// The notebook pane is open by choice but folded for room.
    pub notebooks_folded: bool,
    /// The page list is open by choice but folded for room.
    pub pages_folded: bool,
    /// The widest the notebook pane and the page list may be this frame: their share of the
    /// window, less what would leave the page under [`MIN_CANVAS_WIDTH`] (so a drag stops there;
    /// the page list also narrows this way before it folds).
    pub notebooks_cap: f32,
    pub pages_cap: f32,
    /// The widest any other side pane may be this frame ([`MAX_PANE_SHARE`] of the window).
    pub share: f32,
    /// A folded pane shown over the page.
    pub overlay: Option<SidePane>,
}

impl NarrowPanes {
    pub fn folded(&self, pane: SidePane) -> bool {
        match pane {
            SidePane::Notebooks => self.notebooks_folded,
            SidePane::Pages => self.pages_folded,
        }
    }
}

/// The id egui keeps a panel's width under (as [`Panel`] makes it).
fn panel_id(ui: &Ui, salt: &str) -> Id {
    ui.scope_id().with_salt(IdSalt::new(salt))
}

/// The width the user left a pane at (egui keeps it), or its default.
fn wanted(ui: &Ui, pane: PaneSpec) -> f32 {
    PanelState::load(ui.ctx(), panel_id(ui, pane.salt)).map_or(pane.default, |s| s.size().x)
}

/// The width a pane would show at, no wider than `cap`.
fn shown(ui: &Ui, pane: PaneSpec, cap: f32) -> f32 {
    fit(wanted(ui, pane), pane.range.0, pane.range.1, cap)
}

/// The width a pane shows at, between `min` and `max` but no wider than `cap`.
fn fit(width: f32, min: f32, max: f32, cap: f32) -> f32 {
    width.clamp(min, max.min(cap).max(min))
}

/// Works out this frame's folds from the room the body has (`ui` is where the side panels go).
pub fn plan(app: &mut NoteApp, ui: &Ui) {
    let w = ui.available_rect_before_wrap().width().max(0.0);
    let share = (w * MAX_PANE_SHARE).floor();
    // The other right-hand panes take their share first; they don't fold.
    let mut extras = 0.0;
    let others = [
        (app.ui.templates_pane.is_some(), TEMPLATES),
        (app.ui.tags_pane.is_some(), TAGS),
        (app.ui.recent_edits.is_some(), RECENT_EDITS),
        (app.ui.review.spelling.is_some(), SPELLING),
        (app.ui.bin_view.is_some(), BIN_PAGES),
    ];
    for (open, pane) in others {
        if open {
            extras += shown(ui, pane, share);
        }
    }
    let nb_open = app.ui.notebook_pane_open;
    let pl_open = app.ui.page_list_open && app.ui.bin_view.is_none();
    let mut nb = if nb_open { shown(ui, NOTEBOOKS, share) } else { NOTEBOOK_BAR_WIDTH };
    let mut pl = match (pl_open, app.ui.bin_view.is_some()) {
        (true, _) => shown(ui, PAGES, share),
        (false, true) => 0.0,
        (false, false) => PAGE_BAR_WIDTH,
    };
    let room = |nb: f32, pl: f32| w - nb - pl - extras;
    let mut n = NarrowPanes { share, notebooks_cap: share, pages_cap: share, ..NarrowPanes::default() };
    if nb_open && room(nb, pl) < MIN_CANVAS_WIDTH {
        nb = NOTEBOOK_BAR_WIDTH;
        n.notebooks_folded = true;
    }
    if pl_open && room(nb, pl) < MIN_CANVAS_WIDTH {
        pl = (pl - (MIN_CANVAS_WIDTH - room(nb, pl))).max(PAGES.range.0);
        if room(nb, pl) < MIN_CANVAS_WIDTH {
            n.pages_folded = true;
            pl = PAGE_BAR_WIDTH;
        }
    }
    // Room each pane may still be dragged into (never under what it shows at now).
    n.notebooks_cap = share.min(nb + room(nb, pl) - MIN_CANVAS_WIDTH);
    n.pages_cap = share.min(pl + room(nb, pl) - MIN_CANVAS_WIDTH);
    // An overlay only stands in for a folded pane.
    n.overlay = app.ui.narrow.overlay.filter(|p| n.folded(*p));
    app.ui.narrow = n;
}

/// Shows a resizable side panel no wider than `cap`, keeping the width the user dragged it to (or
/// `default`): egui keeps the width a panel was shown at, so a cap would otherwise overwrite it.
pub fn side_panel<R>(
    ui: &mut Ui,
    left: bool,
    pane: PaneSpec,
    fill: egui::Color32,
    cap: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let (salt, default) = (pane.salt, pane.default);
    let id = panel_id(ui, salt);
    let (min, max) = pane.range;
    let hi = max.min(cap).max(min);
    let want = PanelState::load(ui.ctx(), id).map_or(default, |s| s.size().x);
    // A drag on the splitter sets a new width; anything else that narrows it is the cap, and the
    // width wanted is put back below. (egui names the splitter `<panel id>.with("__resize")`.
    // Without this check a drag that narrows a capped pane would be undone.)
    let dragging = ui.ctx().read_response(id.with("__resize")).is_some_and(|r| r.dragged() || r.drag_stopped());
    let panel = if left { Panel::left(salt) } else { Panel::right(salt) };
    let panel = panel.default_size(default).size_range(min..=hi).resizable(true).show_separator_line(false).frame(egui::Frame::NONE.fill(fill));
    let out = panel.show(ui, add);
    if !dragging
        && want > hi
        && let Some(after) = PanelState::load(ui.ctx(), id)
        && after.size().x < want
    {
        let outer_rect = Rect::from_min_size(after.outer_rect.min, vec2(want, after.outer_rect.height()));
        ui.ctx().data_mut(|d| d.insert_persisted(id, PanelState { outer_rect }));
    }
    out
}

/// The pane's fold button: closes the overlay if that's what is showing, else folds the pane.
pub fn fold(app: &mut NoteApp, pane: SidePane) {
    if app.ui.narrow.overlay == Some(pane) {
        app.ui.narrow.overlay = None;
        return;
    }
    match pane {
        SidePane::Notebooks => app.ui.notebook_pane_open = false,
        SidePane::Pages => app.ui.page_list_open = false,
    }
}

/// The bar's open button: shows the pane over the page if the window folded it, else opens it.
pub fn unfold(app: &mut NoteApp, pane: SidePane) {
    if app.ui.narrow.folded(pane) {
        app.ui.narrow.overlay = Some(pane);
        return;
    }
    match pane {
        SidePane::Notebooks => app.ui.notebook_pane_open = true,
        SidePane::Pages => app.ui.page_list_open = true,
    }
}

/// The pane's keyboard toggle.
pub fn toggle(app: &mut NoteApp, pane: SidePane) {
    let open = match pane {
        SidePane::Notebooks => app.ui.notebook_pane_open,
        SidePane::Pages => app.ui.page_list_open,
    };
    if app.ui.narrow.overlay == Some(pane) || (open && !app.ui.narrow.folded(pane)) {
        fold(app, pane);
    } else {
        unfold(app, pane);
    }
}

/// The overlay, if one is open, next to its bar (`bar`: where the bar was drawn this frame).
pub fn overlay(app: &mut NoteApp, ui: &Ui, bar: Option<Rect>) {
    let (Some(pane), Some(bar)) = (app.ui.narrow.overlay, bar) else { return };
    let ctx = ui.ctx().clone();
    let window = ctx.content_rect();
    let spec = match pane {
        SidePane::Notebooks => NOTEBOOKS,
        SidePane::Pages => PAGES,
    };
    // As wide as the pane was pinned, leaving a strip of the page to click back to.
    let w = shown(ui, spec, (window.width() - 60.0).max(spec.range.0));
    let rect = match pane {
        SidePane::Notebooks => Rect::from_min_size(bar.min, vec2(w, bar.height())),
        SidePane::Pages => Rect::from_min_max(pos2(bar.max.x - w, bar.min.y), bar.max),
    };
    let t = app.tokens.clone();
    egui::Area::new(Id::unique("nc_pane_overlay")).order(egui::Order::Middle).fixed_pos(rect.min).constrain(false).show(&ctx, |ui| {
        ui.set_clip_rect(rect);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            match pane {
                SidePane::Notebooks => crate::panes::notebook_pane(ui, app),
                SidePane::Pages => crate::panes::page_list(ui, app),
            }
            ui.expand_to_include_rect(rect);
        });
        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    });
    // A click outside the pane (and its bar, and any menu it opened) closes it.
    let pressed_outside = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !bar.contains(p)));
    if pressed_outside && !egui::Popup::is_any_open(&ctx) {
        app.ui.narrow.overlay = None;
    }
}
