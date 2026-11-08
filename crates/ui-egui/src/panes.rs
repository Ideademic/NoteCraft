//! The body's panes, in OneNote 2016's arrangement:
//!
//! - [`notebooks`]: the notebook pane on the left (notebooks › section groups › sections with
//!   colour chips), which collapses to a thin bar with the notebook's name on its side;
//! - [`sections`]: the coloured section tabs across the top of the page area (the current tab
//!   joins the band over the page), a "+" tab, section group tabs, and the search box at the
//!   right end ([`search`]);
//! - [`pages`]: the page list on the right, tinted with the section's colour (subpages indented,
//!   Add Page at the top), which collapses too;
//! - [`tags`] and [`templates`]: the Tags Summary and Templates task panes on the right.
//!
//! Everything changes the notebooks through commands; renames happen in place.

pub mod history;
pub mod notebooks;
pub mod pages;
pub mod search;
pub mod sections;
pub mod spelling;
pub mod tags;
pub mod templates;

use egui::{Rect, Sense, Ui, vec2};
use notecraft_doc::{Guid, SectionEntry};
use notecraft_engine::ids;
use serde_json::json;

use crate::NoteApp;
use crate::app::RenameTarget;

pub use notebooks::{notebook_bar, notebook_pane};
pub use pages::{page_bar, page_list};
pub use sections::section_tabs;

/// Remembered last page of each section, so going back to a section opens the page you left.
fn last_pages_id() -> egui::Id {
    egui::Id::unique("nc_last_page_of_section")
}

/// Notes the current page as its section's last.
pub fn remember_page(ctx: &egui::Context, app: &NoteApp) {
    if let (Some(s), Some(p)) = (app.session.current.section, app.session.current.page) {
        ctx.data_mut(|d| {
            let map = d.get_temp_mut_or_default::<std::collections::HashMap<Guid, Guid>>(last_pages_id());
            if map.len() > 2_000 {
                map.clear();
            }
            map.insert(s, p);
        });
    }
}

/// Goes to a section, at the page last open there.
pub fn go_section(ctx: &egui::Context, app: &mut NoteApp, id: Guid) {
    let last = ctx.data(|d| d.get_temp::<std::collections::HashMap<Guid, Guid>>(last_pages_id())).and_then(|m| m.get(&id).copied());
    if app.run(ids::NAV_SECTION, json!({ "id": id })).is_ok()
        && let Some(p) = last
        && app.session.current_section().is_some_and(|s| s.page(p).is_some())
    {
        app.act(ids::NAV_PAGE, json!({ "id": p }));
    }
}

/// Goes to a section group: its first section.
pub fn go_group(ctx: &egui::Context, app: &mut NoteApp, id: Guid) {
    let first = app.session.current_notebook().and_then(|nb| nb.group(id)).and_then(|g| first_section_in(&g.entries, 0));
    match first {
        Some(s) => go_section(ctx, app, s),
        None => app.set_status(crate::tl!("This section group is empty. Right-click it to add a section."), false),
    }
}

fn first_section_in(entries: &[SectionEntry], depth: usize) -> Option<Guid> {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return None;
    }
    entries.iter().find_map(|e| match e {
        SectionEntry::Section(s) => Some(s.id),
        SectionEntry::Group(g) => first_section_in(&g.entries, depth + 1),
    })
}

/// Starts renaming something in place.
pub fn start_rename(app: &mut NoteApp, target: RenameTarget) {
    let text = match target {
        RenameTarget::Notebook(id) => app.session.notebook(id).map(|n| n.name.clone()),
        RenameTarget::Section(id) => {
            app.session.notebooks.iter().find_map(|n| n.section(id).map(|s| s.name.clone()).or_else(|| n.group(id).map(|g| g.name.clone())))
        }
        RenameTarget::Page(id) => app.session.notebooks.iter().find_map(|n| n.find_page(id).map(|(_, p)| p.title.clone())),
    };
    if let Some(text) = text {
        app.ui.rename = Some((target, text));
    }
}

/// The in-place rename box for `target` at `rect`, if that's what is being renamed. Returns
/// whether it is showing. Enter (or clicking away) keeps the new name, Esc drops it.
pub fn rename_box(ui: &mut Ui, app: &mut NoteApp, target: RenameTarget, rect: Rect) -> bool {
    let Some((t, mut text)) = app.ui.rename.clone() else { return false };
    if t != target {
        return false;
    }
    let id = egui::Id::unique(("nc_rename", format!("{target:?}")));
    let resp = ui.put(rect, egui::TextEdit::singleline(&mut text).id(id).margin(vec2(4.0, 2.0)).font(crate::theme::regular(12.5)));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, crate::tl!("Name")));
    if !resp.has_focus() && !resp.lost_focus() && ui.data(|d| d.get_temp::<bool>(id.with("focused"))).is_none() {
        resp.request_focus();
        ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
    }
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if resp.lost_focus() || escape {
        ui.data_mut(|d| d.remove::<bool>(id.with("focused")));
        app.ui.rename = None;
        if !escape {
            commit_rename(app, target, &text);
        }
    } else {
        app.ui.rename = Some((target, text));
    }
    true
}

/// Renames through the right command (an empty name is ignored).
pub fn commit_rename(app: &mut NoteApp, target: RenameTarget, text: &str) {
    let text = text.trim();
    match target {
        RenameTarget::Notebook(id) if !text.is_empty() => app.act(ids::NOTEBOOK_RENAME, json!({ "id": id, "name": text })),
        RenameTarget::Section(id) if !text.is_empty() => app.act(ids::SECTION_RENAME, json!({ "id": id, "name": text })),
        RenameTarget::Page(id) => app.act(ids::PAGE_RENAME, json!({ "id": id, "title": text })),
        _ => {}
    }
}

/// A small square button with an icon (pane headers).
pub fn icon_button(ui: &mut Ui, app: &NoteApp, rect: Rect, icon: crate::icons::Icon, tip: &str) -> bool {
    let t = &app.tokens;
    let resp = ui.interact(rect, ui.scope_id().with(("pane_icon", tip)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    if resp.hovered() {
        ui.painter().rect_filled(rect, crate::theme::RADIUS_SMALL, t.hover);
    }
    crate::icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
        icon,
        crate::icons::Style::new(t.icon, t.accent),
    );
    resp.on_hover_text(tip).clicked()
}

/// The section colour menu: the palette; picks run `section.color`.
pub fn section_color_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid, current: Option<notecraft_doc::Color>) {
    let colors: Vec<egui::Color32> = notecraft_doc::Color::SECTION_PALETTE.iter().map(|c| crate::theme::color32(*c)).collect();
    if let Some(c) = crate::widgets::color::grid(ui, &colors, 8, current.map(crate::theme::color32)) {
        app.act(ids::SECTION_COLOR, json!({ "id": id, "color": crate::widgets::color::hex(c) }));
    }
}
