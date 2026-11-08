//! The History tab's views:
//!
//! - **Notebook Recycle Bin** ([`BinView`]): the page area shows the notebook's recycle bin like a
//!   notebook of its own. The section tabs become the deleted sections plus "Deleted Pages", an
//!   info bar explains the 60 days and offers Empty Recycle Bin, the page list shows that tab's
//!   pages, and the page shows read-only. Right-click a tab or page for Restore, Restore To and
//!   Delete Forever.
//! - **Page versions**: under each page in the page list (History ▸ Page Versions), dated and
//!   in a lighter style ([`version_rows`]); one opens read-only with a banner offering Restore
//!   and Delete ([`banner`]).
//! - **Recent Edits**: a results pane on the right listing the pages changed in a period
//!   ([`recent_edits_pane`]).
//!
//! Read-only pages are drawn by the ordinary canvas on a throwaway session holding just that
//! page ([`preview`]), so nothing done there reaches the notebooks.

use chrono::{DateTime, FixedOffset, Offset as _, Utc};
use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::{Guid, Notebook, Page, Section, SectionEntry};
use notecraft_engine::{Origin, Session, ids};
use serde_json::json;

use crate::app::Prompt;
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, Tokens, fit_text, mix, paint_strong, regular, text_width};
use crate::{NoteApp, tl, widgets};

/// Height of the recycle bin's and a version's info bar.
pub const BAR_H: f32 = 34.0;
const ROW_H: f32 = 30.0;
const VERSION_ROW_H: f32 = 24.0;

/// Which tab of the recycle bin is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinTab {
    /// `OneNote_DeletedPages`: pages deleted on their own.
    Pages,
    /// A deleted section.
    Section(Guid),
}

/// The open recycle bin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinView {
    pub notebook: Guid,
    pub tab: BinTab,
    /// The page showing (read-only).
    pub page: Option<Guid>,
}

/// The Recent Edits periods, as `history.recentEdits` names them, with their menu labels.
pub const PERIODS: [(&str, &str); 7] = [
    ("today", "Today"),
    ("yesterday", "Since Yesterday"),
    ("7d", "Last 7 Days"),
    ("14d", "Last 14 Days"),
    ("30d", "Last 30 Days"),
    ("6m", "Last 6 Months"),
    ("all", "All Pages"),
];

/// The label of a Recent Edits period.
pub fn period_label(key: &str) -> &'static str {
    match key {
        "today" => tl!("Today"),
        "yesterday" => tl!("Since Yesterday"),
        "7d" => tl!("Last 7 Days"),
        "14d" => tl!("Last 14 Days"),
        "30d" => tl!("Last 30 Days"),
        "6m" => tl!("Last 6 Months"),
        _ => tl!("All Pages"),
    }
}

/// A time as the page list shows it, in the user's time zone.
pub fn when(t: DateTime<Utc>, offset_minutes: i32) -> String {
    let offset = FixedOffset::east_opt(offset_minutes.clamp(-14 * 60, 14 * 60) * 60).unwrap_or_else(|| Utc.fix());
    t.with_timezone(&offset).format("%d/%m/%Y %-I:%M %p").to_string()
}

// --- the recycle bin ------------------------------------------------------------------------

/// Opens the current notebook's recycle bin (History ▸ Notebook Recycle Bin).
pub fn open_bin(app: &mut NoteApp) {
    let Some(nb) = app.session.current_notebook() else { return };
    let tab = if !nb.recycle_bin.pages.is_empty() || nb.recycle_bin.sections.is_empty() {
        BinTab::Pages
    } else {
        nb.recycle_bin.sections.last().map_or(BinTab::Pages, |d| BinTab::Section(d.section.id))
    };
    let notebook = nb.id;
    app.session.viewing_version = None;
    app.ui.bin_view = Some(BinView { notebook, tab, page: None });
    app.ui.bin_opened_at = app.session.current;
    fix_bin_view(app);
}

/// Closes the recycle bin view.
pub fn close_bin(app: &mut NoteApp) {
    app.ui.bin_view = None;
}

/// Keeps the recycle bin view pointing at things that are there (a restored page goes, the
/// tab's first page shows); closes it when the user went elsewhere.
pub fn fix_bin_view(app: &mut NoteApp) {
    let Some(mut v) = app.ui.bin_view else { return };
    if app.session.current != app.ui.bin_opened_at {
        // Navigation (notebook pane, a restore) leaves the bin.
        app.ui.bin_view = None;
        return;
    }
    let Some(nb) = app.session.notebook(v.notebook) else {
        app.ui.bin_view = None;
        return;
    };
    let bin = &nb.recycle_bin;
    if let BinTab::Section(id) = v.tab
        && bin.section(id).is_none()
    {
        v.tab = BinTab::Pages;
        v.page = None;
    }
    let pages: Vec<Guid> = bin_pages(nb, v.tab).iter().map(|p| p.id).collect();
    if v.page.is_none_or(|p| !pages.contains(&p)) {
        v.page = pages.first().copied();
    }
    app.ui.bin_view = Some(v);
}

/// The pages a recycle bin tab holds.
fn bin_pages(nb: &Notebook, tab: BinTab) -> Vec<&Page> {
    match tab {
        BinTab::Pages => nb.recycle_bin.pages.iter().rev().map(|d| &d.page).collect(),
        BinTab::Section(id) => nb.recycle_bin.section(id).map(|d| d.section.pages.iter().collect()).unwrap_or_default(),
    }
}

/// The recycle bin's tabs (in place of the section tabs) and its info bar.
pub fn bin_tabs(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let Some(view) = app.ui.bin_view else { return };
    let Some(nb) = app.session.notebook(view.notebook) else { return };
    let strip_h = super::sections::STRIP_H;
    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), strip_h), Sense::hover());
    ui.painter().rect_filled(strip, 0.0, t.section_strip);
    let band = Rect::from_min_max(pos2(strip.min.x, strip.max.y - super::sections::BAND_H), strip.max);
    let mut tabs: Vec<(BinTab, String, egui::Color32)> = vec![(BinTab::Pages, tl!("Deleted Pages").to_owned(), t.border_strong)];
    for d in nb.recycle_bin.sections.iter().rev() {
        tabs.push((BinTab::Section(d.section.id), d.section.name.clone(), t.section_fill(d.section.color)));
    }
    let current_color = tabs.iter().find(|(tb, ..)| *tb == view.tab).map_or(t.border, |(_, _, c)| *c);
    ui.painter().rect_filled(band, 0.0, current_color);
    let mut x = strip.min.x + 6.0;
    let bottom = band.min.y;
    // The bin's own mark first: what this strip is.
    let label = crate::i18n::fmt_plain(tl!("{notebook} Recycle Bin"), &[("notebook", &nb.name)]);
    let lw = text_width(ui.ctx(), &label, regular(12.5)) + 30.0;
    let lr = Rect::from_min_max(pos2(x, bottom - 24.0), pos2(x + lw, bottom));
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(lr.min.x + 10.0, lr.center().y), vec2(16.0, 16.0)),
        Icon::RecycleBin,
        icons::Style::new(t.icon, t.accent),
    );
    paint_strong(ui.painter(), pos2(lr.min.x + 22.0, lr.center().y), Align2::LEFT_CENTER, &label, regular(12.5), t.text);
    x += lw + 8.0;
    let right = strip.max.x - 40.0;
    for (tab, name, fill) in tabs {
        let w = (text_width(ui.ctx(), &name, regular(12.5)) + 24.0).clamp(60.0, 200.0);
        if x + w > right {
            break;
        }
        let current = tab == view.tab;
        let r = if current {
            Rect::from_min_max(pos2(x, bottom - 27.0), pos2(x + w, band.max.y))
        } else {
            Rect::from_min_max(pos2(x, bottom - 24.0), pos2(x + w, bottom))
        };
        x += w + 2.0;
        let resp = ui.interact(r, ui.scope_id().with(("bin_tab", format!("{tab:?}"))), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, current, &name));
        let fill = if resp.hovered() && !current { mix(fill, egui::Color32::WHITE, 0.25) } else { fill };
        ui.painter().rect_filled(r, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, fill);
        let ink = crate::theme::on(fill);
        let shown = fit_text(ui.ctx(), &name, &regular(12.5), w - 12.0);
        if current {
            paint_strong(ui.painter(), r.center() - vec2(0.0, 1.5), Align2::CENTER_CENTER, &shown, regular(12.5), ink);
        } else {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, &shown, regular(12.5), ink);
        }
        if resp.clicked() {
            app.ui.bin_view = Some(BinView { tab, page: None, ..view });
            fix_bin_view(app);
        }
        if let BinTab::Section(id) = tab {
            resp.context_menu(|ui| section_menu(ui, app, view.notebook, id));
        }
    }
    let close = Rect::from_center_size(pos2(strip.max.x - 20.0, bottom - 12.0), vec2(24.0, 22.0));
    if super::icon_button(ui, app, close, Icon::Close, tl!("Close the Recycle Bin")) {
        close_bin(app);
    }
    // The info bar.
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::hover());
    paint_bar(ui, &t, bar);
    let text = tl!("Pages and sections in the Notebook Recycle Bin are deleted automatically after 60 days.");
    let mut buttons_x = bar.max.x - 8.0;
    let can_empty = app.session.notebook(view.notebook).is_some_and(|n| !n.recycle_bin.is_empty());
    if bar_button(ui, &t, &mut buttons_x, bar, tl!("Empty Recycle Bin"), can_empty) {
        ask_empty(app, view.notebook);
    }
    let room = (buttons_x - bar.min.x - 40.0).max(0.0);
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(bar.min.x + 18.0, bar.center().y), vec2(16.0, 16.0)),
        Icon::Info,
        icons::Style::new(t.icon, t.accent),
    );
    ui.painter().text(
        pos2(bar.min.x + 34.0, bar.center().y),
        Align2::LEFT_CENTER,
        fit_text(ui.ctx(), text, &regular(12.5), room),
        regular(12.5),
        t.text,
    );
}

/// Asks before emptying a notebook's recycle bin.
pub fn ask_empty(app: &mut NoteApp, notebook: Guid) {
    app.ui.prompt = Some(Prompt::Confirm {
        title: tl!("Empty the Notebook Recycle Bin?").to_owned(),
        body: tl!("Every page and section in it is deleted for good. This can be undone until you close NoteCraft.").to_owned(),
        yes: tl!("Delete").to_owned(),
        command: ids::HISTORY_EMPTY_RECYCLE_BIN.to_owned(),
        params: json!({ "notebook": notebook }),
    });
}

/// The yellow-tinted bar over the page (recycle bin, page version).
fn paint_bar(ui: &Ui, t: &Tokens, bar: Rect) {
    ui.painter().rect_filled(bar, 0.0, mix(t.page, t.yellow, 0.28));
    ui.painter().line_segment([pos2(bar.min.x, bar.max.y - 0.5), pos2(bar.max.x, bar.max.y - 0.5)], Stroke::new(1.0, mix(t.border, t.yellow, 0.4)));
}

/// A text button at the right end of a bar, laid out right to left from `x`.
fn bar_button(ui: &mut Ui, t: &Tokens, x: &mut f32, bar: Rect, text: &str, enabled: bool) -> bool {
    let w = text_width(ui.ctx(), text, regular(12.5)) + 20.0;
    let r = Rect::from_min_max(pos2(*x - w, bar.min.y + 5.0), pos2(*x, bar.max.y - 5.0));
    *x -= w + 6.0;
    let resp = ui.interact(r, ui.scope_id().with(("bar_button", text)), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    let fill = if enabled && resp.hovered() { t.hover } else { t.input };
    ui.painter().rect(r, RADIUS_SMALL, fill, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, regular(12.5), if enabled { t.text } else { t.text_disabled });
    enabled && resp.clicked()
}

/// Right-click on a deleted section's tab.
fn section_menu(ui: &mut Ui, app: &mut NoteApp, notebook: Guid, id: Guid) {
    if widgets::menu_item(ui, Some(Icon::Undo), tl!("Restore"), true) {
        restore(app, id, None);
    }
    let groups: Vec<(Guid, String)> = app.session.notebook(notebook).map(groups_of).unwrap_or_default();
    ui.menu_button(tl!("Restore To"), |ui| {
        let name = app.session.notebook(notebook).map(|n| n.name.clone()).unwrap_or_default();
        if widgets::menu_item(ui, Some(Icon::Notebook), &name, true) {
            restore(app, id, Some(notebook));
        }
        for (g, name) in groups {
            if widgets::menu_item(ui, Some(Icon::SectionGroup), &name, true) {
                restore(app, id, Some(g));
            }
        }
    });
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete Forever"), true) {
        app.act(ids::HISTORY_DELETE_FOREVER, json!({ "id": id }));
        fix_bin_view(app);
    }
}

/// Every section group of a notebook, by path name (`Work › Projects`).
fn groups_of(nb: &Notebook) -> Vec<(Guid, String)> {
    fn walk(entries: &[SectionEntry], prefix: &str, depth: usize, out: &mut Vec<(Guid, String)>) {
        if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
            return;
        }
        for e in entries {
            if let SectionEntry::Group(g) = e {
                let name = if prefix.is_empty() { g.name.clone() } else { format!("{prefix} › {}", g.name) };
                out.push((g.id, name.clone()));
                walk(&g.entries, &name, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&nb.sections, "", 0, &mut out);
    out
}

/// Restores `id` from the recycle bin (to `to`, if given) and goes to it.
fn restore(app: &mut NoteApp, id: Guid, to: Option<Guid>) {
    if app.run(ids::HISTORY_RESTORE, json!({ "id": id, "to": to })).is_ok() {
        app.ui.bin_view = None;
        app.set_status(tl!("Restored from the Notebook Recycle Bin."), false);
    }
}

/// The recycle bin's page list (in place of the section's).
pub fn bin_page_list(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    let Some(view) = app.ui.bin_view else { return };
    let Some(nb) = app.session.notebook(view.notebook) else { return };
    ui.painter().rect_filled(full, 0.0, t.page_list);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let header = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
    let heading = match view.tab {
        BinTab::Pages => tl!("Deleted Pages").to_owned(),
        BinTab::Section(id) => nb.recycle_bin.section(id).map(|d| d.section.name.clone()).unwrap_or_default(),
    };
    paint_strong(
        ui.painter(),
        pos2(header.min.x + 12.0, header.center().y),
        Align2::LEFT_CENTER,
        &fit_text(ui.ctx(), &heading, &regular(13.0), full.width() - 24.0),
        regular(13.0),
        t.text,
    );
    ui.painter().line_segment([pos2(full.min.x, header.max.y), pos2(full.max.x, header.max.y)], Stroke::new(1.0, t.border));
    let offset = app.canvas.utc_offset_minutes;
    let rows: Vec<(Guid, String, u8, String)> = match view.tab {
        BinTab::Pages => {
            nb.recycle_bin.pages.iter().rev().map(|d| (d.page.id, d.page.display_title().to_owned(), 0, when(d.deleted, offset))).collect()
        }
        BinTab::Section(id) => nb
            .recycle_bin
            .section(id)
            .map(|d| d.section.pages.iter().map(|p| (p.id, p.display_title().to_owned(), p.level, when(p.modified, offset))).collect())
            .unwrap_or_default(),
    };
    let notebook = view.notebook;
    let list = Rect::from_min_max(pos2(full.min.x + 1.0, header.max.y + 1.0), full.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_bin_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for (id, title, level, date) in &rows {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H + 4.0), Sense::click());
                let current = app.ui.bin_view.and_then(|v| v.page) == Some(*id);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, title));
                if current {
                    ui.painter().rect_filled(r, 0.0, t.page_list_selected);
                    ui.painter().rect_filled(Rect::from_min_max(r.min, pos2(r.min.x + 3.0, r.max.y)), 0.0, t.accent);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t.hover);
                }
                let x = r.min.x + 12.0 + f32::from(*level) * 16.0;
                let w = (r.max.x - x - 8.0).max(0.0);
                ui.painter().text(pos2(x, r.min.y + 4.0), Align2::LEFT_TOP, fit_text(ui.ctx(), title, &regular(13.0), w), regular(13.0), t.text);
                ui.painter().text(pos2(x, r.min.y + 20.0), Align2::LEFT_TOP, fit_text(ui.ctx(), date, &regular(11.0), w), regular(11.0), t.text_dim);
                if resp.clicked()
                    && let Some(v) = app.ui.bin_view.as_mut()
                {
                    v.page = Some(*id);
                }
                let id = *id;
                resp.context_menu(|ui| page_menu(ui, app, notebook, id));
            }
            if rows.is_empty() {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| ui.label(egui::RichText::new(tl!("Nothing here")).font(regular(12.0)).color(t.text_dim)));
            }
        });
    });
    ui.expand_to_include_rect(full);
}

/// Right-click on a page in the recycle bin.
fn page_menu(ui: &mut Ui, app: &mut NoteApp, notebook: Guid, id: Guid) {
    if widgets::menu_item(ui, Some(Icon::Undo), tl!("Restore"), true) {
        restore(app, id, None);
    }
    let sections: Vec<(Guid, String)> =
        app.session.notebook(notebook).map(|nb| nb.all_sections().iter().map(|s| (s.id, s.name.clone())).collect()).unwrap_or_default();
    ui.add_enabled_ui(!sections.is_empty(), |ui| {
        ui.menu_button(tl!("Restore To"), |ui| {
            for (s, name) in sections {
                if widgets::menu_item(ui, Some(Icon::NewSection), &name, true) {
                    restore(app, id, Some(s));
                }
            }
        });
    });
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete Forever"), true) {
        app.act(ids::HISTORY_DELETE_FOREVER, json!({ "id": id }));
        fix_bin_view(app);
    }
}

// --- read-only pages ------------------------------------------------------------------------

/// What the page area shows instead of the current page: a recycle bin page or a page version.
fn preview_page(app: &NoteApp) -> Option<(Page, u64)> {
    if let Some(v) = app.ui.bin_view {
        let nb = app.session.notebook(v.notebook)?;
        let page = bin_pages(nb, v.tab).into_iter().find(|p| Some(p.id) == v.page)?;
        return Some((page.clone(), key_of(page.id, 1)));
    }
    let (page, version) = app.session.viewing_version?;
    let p = app.session.current_page().filter(|p| p.id == page)?;
    let v = p.versions.iter().find(|v| v.id == version)?;
    let mut shown = (*v.page).clone();
    shown.id = page;
    shown.versions.clear();
    Some((shown, key_of(version, 2)))
}

/// A revision number for a read-only page's throwaway session: the canvas caches layouts by
/// element id and revision, and a version shares element ids with its page, so it must never
/// meet the live session's revision.
fn key_of(id: Guid, salt: u8) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ u64::from(salt);
    for b in id.0 {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    h | (1 << 63)
}

/// Whether the page area shows a read-only page this frame.
pub fn previewing(app: &NoteApp) -> bool {
    app.ui.bin_view.is_some() || app.session.viewing_version.is_some()
}

/// Draws the read-only page (a recycle bin page or a version) in the page area. Returns false
/// when there is none (an empty recycle bin tab says so instead).
pub fn preview(ui: &mut Ui, app: &mut NoteApp, tokens: &Tokens) -> bool {
    let Some((page, rev)) = preview_page(app) else {
        if app.ui.bin_view.is_some() {
            let r = ui.available_rect_before_wrap();
            ui.painter().rect_filled(r, 0.0, tokens.canvas);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, tl!("The Notebook Recycle Bin is empty"), regular(14.0), tokens.text_dim);
            return true;
        }
        return false;
    };
    let mut nb = Notebook::new("preview");
    let mut sec = Section::new("preview", notecraft_doc::Color::WHITE);
    sec.pages.push(page);
    nb.sections.push(SectionEntry::Section(sec));
    let mut session = Session::new(nb, Origin::Scratch);
    session.revision = rev;
    let state = &mut app.preview_canvas;
    state.zoom = app.canvas.zoom;
    state.hide_title = app.canvas.hide_title;
    state.utc_offset_minutes = app.canvas.utc_offset_minutes;
    state.tool = crate::canvas::Tool::Select;
    crate::canvas::show(ui, state, &mut session, tokens, &app.services);
    // Nothing done to the throwaway page sticks: no caret, no selection, no messages.
    state.caret = None;
    state.editing = None;
    state.pending = None;
    state.selection = Default::default();
    state.messages.clear();
    true
}

/// The banner over a page version: its date, Restore Version, Delete Version, Close.
pub fn banner(ui: &mut Ui, app: &mut NoteApp) {
    let Some((page, version)) = app.session.viewing_version else { return };
    let Some(saved) = app.session.current_page().and_then(|p| p.versions.iter().find(|v| v.id == version)).map(|v| v.saved) else { return };
    let t = app.tokens.clone();
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H), Sense::hover());
    paint_bar(ui, &t, bar);
    let mut x = bar.max.x - 8.0;
    if bar_button(ui, &t, &mut x, bar, tl!("Close"), true) {
        app.act(ids::HISTORY_VIEW_VERSION, json!({ "page": page, "version": null }));
    }
    if bar_button(ui, &t, &mut x, bar, tl!("Delete Version"), true) {
        app.act(ids::HISTORY_DELETE_VERSION, json!({ "page": page, "version": version }));
    }
    if bar_button(ui, &t, &mut x, bar, tl!("Restore Version"), true)
        && app.run(ids::HISTORY_RESTORE_VERSION, json!({ "page": page, "version": version })).is_ok()
    {
        app.set_status(tl!("The page is back as it was; what it was before is kept as a version."), false);
    }
    let text = crate::i18n::fmt_plain(
        tl!("This is a page version from {when}. It can't be edited."),
        &[("when", &when(saved, app.canvas.utc_offset_minutes))],
    );
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(bar.min.x + 18.0, bar.center().y), vec2(16.0, 16.0)),
        Icon::PageVersions,
        icons::Style::new(t.icon, t.accent),
    );
    let room = (x - bar.min.x - 40.0).max(0.0);
    ui.painter().text(
        pos2(bar.min.x + 34.0, bar.center().y),
        Align2::LEFT_CENTER,
        fit_text(ui.ctx(), &text, &regular(12.5), room),
        regular(12.5),
        t.text,
    );
}

/// The banner over a recycle bin page: Restore, Delete Forever.
pub fn bin_page_banner(ui: &mut Ui, app: &mut NoteApp) {
    let Some(view) = app.ui.bin_view else { return };
    let Some(page) = view.page else { return };
    let t = app.tokens.clone();
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H - 4.0), Sense::hover());
    ui.painter().rect_filled(bar, 0.0, t.page);
    let mut x = bar.max.x - 8.0;
    if bar_button(ui, &t, &mut x, bar, tl!("Delete Forever"), true) {
        app.act(ids::HISTORY_DELETE_FOREVER, json!({ "id": page }));
        fix_bin_view(app);
    }
    if bar_button(ui, &t, &mut x, bar, tl!("Restore"), true) {
        restore(app, page, None);
    }
    ui.painter().text(pos2(bar.min.x + 12.0, bar.center().y), Align2::LEFT_CENTER, tl!("Deleted page (read-only)"), regular(12.0), t.text_dim);
}

// --- page versions in the page list ---------------------------------------------------------

/// The rows of `page`'s versions under it in the page list (newest first), when History ▸ Page
/// Versions is on.
pub fn version_rows(ui: &mut Ui, app: &mut NoteApp, page: Guid, level: u8) {
    if !app.session.show_versions {
        return;
    }
    let Some(p) = app.session.notebooks.iter().find_map(|n| n.find_page(page)).map(|(_, p)| p) else { return };
    let versions: Vec<(Guid, DateTime<Utc>)> = p.versions.iter().rev().map(|v| (v.id, v.saved)).collect();
    let t = app.tokens.clone();
    let offset = app.canvas.utc_offset_minutes;
    for (id, saved) in versions {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), VERSION_ROW_H), Sense::click());
        let label = when(saved, offset);
        let current = app.session.viewing_version == Some((page, id));
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, &label));
        if current {
            let fill = app.session.current_section().map_or(t.page_list_selected, |s| t.section_selected(s.color));
            ui.painter().rect_filled(r, 0.0, fill);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover);
        }
        let x = r.min.x + 28.0 + f32::from(level) * 16.0;
        icons::paint_styled(
            ui.painter(),
            Rect::from_center_size(pos2(x - 9.0, r.center().y), vec2(12.0, 12.0)),
            Icon::PageVersions,
            icons::Style::disabled(t.icon),
        );
        let w = (r.max.x - x - 8.0).max(0.0);
        ui.painter().text(pos2(x, r.center().y), Align2::LEFT_CENTER, fit_text(ui.ctx(), &label, &regular(11.5), w), regular(11.5), t.text_dim);
        if resp.clicked() {
            app.act(ids::HISTORY_VIEW_VERSION, json!({ "page": page, "version": id }));
        }
        let resp = resp.on_hover_text(tl!("Page version (read-only)"));
        resp.context_menu(|ui| {
            if widgets::menu_item(ui, Some(Icon::Undo), tl!("Restore Version"), true) {
                app.act(ids::HISTORY_RESTORE_VERSION, json!({ "page": page, "version": id }));
            }
            if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete Version"), true) {
                app.act(ids::HISTORY_DELETE_VERSION, json!({ "page": page, "version": id }));
            }
            ui.separator();
            if widgets::menu_item(ui, Some(Icon::PageVersions), tl!("Hide Page Versions"), true) {
                app.act(ids::HISTORY_SHOW_VERSIONS, json!({ "value": false }));
            }
        });
    }
}

/// Asks before deleting every version in the section or notebook.
pub fn ask_delete_versions(app: &mut NoteApp, scope: &str) {
    let (title, id) = match scope {
        "notebook" => (tl!("Delete all page versions in this notebook?"), app.session.current.notebook),
        _ => (tl!("Delete all page versions in this section?"), app.session.current.section),
    };
    app.ui.prompt = Some(Prompt::Confirm {
        title: title.to_owned(),
        body: tl!("The pages stay as they are; only their earlier versions go. You can undo this with Ctrl+Z.").to_owned(),
        yes: tl!("Delete Versions").to_owned(),
        command: ids::HISTORY_DELETE_ALL_VERSIONS.to_owned(),
        params: json!({ "scope": scope, "id": id }),
    });
}

// --- recent edits ---------------------------------------------------------------------------

/// One row of Recent Edits.
#[derive(Clone, Debug, PartialEq)]
struct Edit {
    page: Guid,
    title: String,
    place: String,
    when: String,
    unread: bool,
}

/// Width of the Recent Edits pane.
pub const RECENT_W: f32 = 270.0;

/// The Recent Edits pane: pages changed in the chosen period, newest first. Click to go there.
pub fn recent_edits_pane(ui: &mut Ui, app: &mut NoteApp) {
    let Some(since) = app.ui.recent_edits.clone() else { return };
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.panel);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let header = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
    paint_strong(ui.painter(), pos2(header.min.x + 12.0, header.center().y), Align2::LEFT_CENTER, tl!("Recent Edits"), regular(14.0), t.text);
    let close = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, close, Icon::Close, tl!("Close")) {
        app.ui.recent_edits = None;
        return;
    }
    // The period.
    let pick = Rect::from_min_size(pos2(full.min.x + 10.0, header.max.y + 2.0), vec2(full.width() - 20.0, 24.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(pick).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        crate::ribbon::simplified::text_menu(ui, app, period_label(&since), tl!("Period"), true, |ui, app| {
            for (key, _) in PERIODS {
                if widgets::check_item(ui, since == key, period_label(key)) {
                    app.ui.recent_edits = Some(key.to_owned());
                }
            }
        });
    });
    let rows = recent(app, &since);
    let list = Rect::from_min_max(pos2(full.min.x + 1.0, pick.max.y + 8.0), full.max);
    ui.painter().text(pos2(list.min.x + 11.0, list.min.y), Align2::LEFT_TOP, count_label(rows.len()), regular(11.5), t.text_dim);
    let list = Rect::from_min_max(pos2(list.min.x, list.min.y + 18.0), list.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_recent_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for e in &rows {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &e.title));
                let current = app.session.current.page == Some(e.page);
                if current {
                    ui.painter().rect_filled(r, 0.0, t.page_list_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t.hover);
                }
                let w = r.width() - 20.0;
                let title = fit_text(ui.ctx(), &e.title, &regular(13.0), w);
                if e.unread {
                    paint_strong(ui.painter(), pos2(r.min.x + 10.0, r.min.y + 4.0), Align2::LEFT_TOP, &title, regular(13.0), t.text);
                } else {
                    ui.painter().text(pos2(r.min.x + 10.0, r.min.y + 4.0), Align2::LEFT_TOP, title, regular(13.0), t.text);
                }
                ui.painter().text(
                    pos2(r.min.x + 10.0, r.min.y + 20.0),
                    Align2::LEFT_TOP,
                    fit_text(ui.ctx(), &e.place, &regular(11.0), w),
                    regular(11.0),
                    t.accent_text,
                );
                ui.painter().text(pos2(r.min.x + 10.0, r.min.y + 32.0), Align2::LEFT_TOP, &e.when, regular(11.0), t.text_dim);
                if resp.clicked() {
                    app.ui.bin_view = None;
                    app.act(ids::NAV_PAGE, json!({ "id": e.page }));
                }
            }
        });
    });
    ui.expand_to_include_rect(full);
}

fn count_label(n: usize) -> String {
    match n {
        0 => tl!("No pages changed in this period").to_owned(),
        1 => tl!("1 page").to_owned(),
        n => crate::i18n::fmt_plain(tl!("{n} pages"), &[("n", &n.to_string())]),
    }
}

/// `history.recentEdits` for `since`, cached by revision.
fn recent(app: &mut NoteApp, since: &str) -> Vec<Edit> {
    let offset = app.canvas.utc_offset_minutes;
    let Ok(out) = app.session.run(ids::HISTORY_RECENT_EDITS, &json!({ "since": since, "utcOffsetMinutes": offset })) else { return Vec::new() };
    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(serde_json::Value::as_str).unwrap_or("").to_owned();
    out.value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let page = Guid::parse(&s(v, "page"))?;
            let modified = DateTime::parse_from_rfc3339(&s(v, "modified")).ok()?.with_timezone(&Utc);
            Some(Edit {
                page,
                title: s(v, "title"),
                place: format!("{} › {}", s(v, "notebookName"), s(v, "sectionName")),
                when: when(modified, offset),
                unread: v.get("unread").and_then(serde_json::Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_follow_the_offset_and_keys_never_meet_live_revisions() {
        let t = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        assert_ne!(when(t, 0), when(t, 120));
        assert!(when(t, 99_999).contains('/'));
        let k = key_of(Guid::new(), 1);
        assert!(k >= 1 << 63);
        assert_ne!(key_of(Guid::NIL, 1), key_of(Guid::NIL, 2));
        for (key, _) in PERIODS {
            assert!(!period_label(key).is_empty());
        }
    }
}
