//! The search box at the right end of the section tabs ("Search (Ctrl+E)") and its results:
//! pages whose title or text holds the words, current notebook first, each with its section and
//! a snippet. Click one to go there; Esc clears the search. The arrow in the box picks where to
//! look: this section, this section group, this notebook or all notebooks (`search`'s scope);
//! "This Page" opens Find on the page (Ctrl+F) instead.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::Guid;
use notecraft_engine::ids;
use serde_json::json;

use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, fit_text, regular};
use crate::{NoteApp, tl};

/// Most results listed.
pub const MAX_RESULTS: usize = 50;

/// The scopes the search box offers (as `search` names them), after This Page.
pub const SCOPES: [&str; 4] = ["section", "sectionGroup", "notebook", "all"];

/// A scope's label.
pub fn scope_label(scope: &str) -> &'static str {
    crate::panes::tags::scope_label(scope)
}

/// One result.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub page: Guid,
    pub title: String,
    pub section: String,
    pub notebook: String,
    pub snippet: String,
}

/// Pages matching `query` in every open notebook (the engine's `search` command), the current
/// notebook's first.
pub fn find(app: &mut NoteApp, query: &str) -> Vec<Hit> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let scope = if SCOPES.contains(&app.ui.search_scope.as_str()) { app.ui.search_scope.clone() } else { "all".to_owned() };
    let Ok(out) = app.session.run(ids::SEARCH, &json!({ "query": query, "limit": MAX_RESULTS, "scope": scope })) else { return Vec::new() };
    let current = app.session.current.notebook.map(|g| g.to_string());
    let s = |h: &serde_json::Value, k: &str| h.get(k).and_then(serde_json::Value::as_str).unwrap_or("").to_owned();
    let mut hits: Vec<(bool, Hit)> = out
        .value
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let page = Guid::parse(&s(h, "page"))?;
            let here = current.as_deref().and_then(Guid::parse) == Guid::parse(&s(h, "notebook"));
            Some((
                !here,
                Hit { page, title: s(h, "pageTitle"), section: s(h, "sectionName"), notebook: s(h, "notebookName"), snippet: s(h, "snippet") },
            ))
        })
        .collect();
    hits.sort_by_key(|(later, _)| *later);
    hits.into_iter().map(|(_, h)| h).take(MAX_RESULTS).collect()
}

/// The id of the search box (Ctrl+E focuses it).
pub fn box_id() -> egui::Id {
    egui::Id::unique("nc_search_box")
}

/// The search box at `rect`, and its results under it while it has text.
pub fn search_box(ui: &mut Ui, app: &mut NoteApp, rect: Rect) {
    let t = app.tokens.clone();
    let focused = ui.memory(|m| m.has_focus(box_id()));
    let border = if focused { t.accent_text } else { t.input_border };
    ui.painter().rect(rect, RADIUS_SMALL, t.input, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let icon = Rect::from_center_size(pos2(rect.max.x - 13.0, rect.center().y), vec2(14.0, 14.0));
    icons::paint_styled(ui.painter(), icon, Icon::Search, icons::Style::mono(t.text_dim));
    scope_button(ui, app, Rect::from_min_max(pos2(rect.max.x - 42.0, rect.min.y + 2.0), pos2(rect.max.x - 26.0, rect.max.y - 2.0)));
    let edit_rect = Rect::from_min_max(rect.min + vec2(2.0, 1.0), pos2(rect.max.x - 44.0, rect.max.y - 1.0));
    let key = crate::widgets::shortcut_text("Mod+E");
    let hint = crate::i18n::fmt_plain(tl!("Search ({key})"), &[("key", &key)]);
    let mut text = app.ui.search.clone();
    let resp = ui.put(
        edit_rect,
        egui::TextEdit::singleline(&mut text)
            .id(box_id())
            .frame(egui::Frame::NONE)
            .hint_text(egui::RichText::new(hint).color(t.text_dim))
            .margin(vec2(6.0, 3.0))
            .font(regular(12.5))
            .align(egui::Align2::LEFT_CENTER),
    );
    if app.ui.search_focus {
        resp.request_focus();
        app.ui.search_focus = false;
    }
    if text != app.ui.search {
        app.ui.search = text;
    }
    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.search.clear();
        resp.surrender_focus();
    }
    let open_id = box_id().with("open");
    if resp.has_focus() || resp.changed() {
        ui.data_mut(|d| d.insert_temp(open_id, true));
    }
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false) && !app.ui.search.trim().is_empty();
    if !open {
        return;
    }
    // Searched again only when the words or the notebooks change.
    let query = app.ui.search.clone();
    let cache = box_id().with("cache");
    let key = (query.clone(), app.session.revision, app.ui.search_scope.clone());
    let hits = match ui.data(|d| d.get_temp::<((String, u64, String), Vec<Hit>)>(cache)) {
        Some((k, hits)) if k == key => hits,
        _ => {
            let hits = find(app, &query);
            ui.data_mut(|d| d.insert_temp(cache, (key, hits.clone())));
            hits
        }
    };
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    let width = rect.width().max(320.0);
    let pos = pos2(rect.max.x - width, rect.max.y + 2.0);
    let mut go = None;
    let area = egui::Area::new(box_id().with("results")).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
        egui::Frame::NONE.fill(t.menu).stroke(Stroke::new(1.0, t.border_strong)).corner_radius(RADIUS_SMALL).inner_margin(4.0).show(ui, |ui| {
            ui.set_width(width - 10.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let count = hits.len();
            let found = match count {
                0 => tl!("No pages found").to_owned(),
                1 => tl!("1 page found").to_owned(),
                n if n >= MAX_RESULTS => crate::i18n::fmt_plain(tl!("{n}+ pages found"), &[("n", &n.to_string())]),
                n => crate::i18n::fmt_plain(tl!("{n} pages found"), &[("n", &n.to_string())]),
            };
            let label = format!("{found} \u{00b7} {}", scope_label(&app.ui.search_scope));
            ui.label(egui::RichText::new(label).font(regular(11.5)).color(t.text_dim));
            ui.add_space(3.0);
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for h in &hits {
                    let (r, row) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                    let title = if h.title.trim().is_empty() { tl!("Untitled page") } else { h.title.as_str() };
                    row.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
                    if row.hovered() {
                        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
                    }
                    let w = r.width() - 16.0;
                    let font = regular(12.5);
                    ui.painter().text(pos2(r.min.x + 8.0, r.min.y + 4.0), Align2::LEFT_TOP, fit_text(ui.ctx(), title, &font, w), font, t.text);
                    let place = format!("{} › {}", h.notebook, h.section);
                    let small = regular(11.0);
                    ui.painter().text(
                        pos2(r.min.x + 8.0, r.min.y + 19.0),
                        Align2::LEFT_TOP,
                        fit_text(ui.ctx(), &place, &small, w),
                        small.clone(),
                        t.accent_text,
                    );
                    ui.painter().text(
                        pos2(r.min.x + 8.0, r.min.y + 31.0),
                        Align2::LEFT_TOP,
                        fit_text(ui.ctx(), &h.snippet, &small, w),
                        small,
                        t.text_dim,
                    );
                    if row.clicked() {
                        go = Some(h.page);
                    }
                }
            });
        });
    });
    if enter && go.is_none() {
        go = hits.first().map(|h| h.page);
    }
    // A click outside the box and the results closes them.
    let clicked_outside =
        ui.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p) && !area.response.rect.contains(p)));
    if let Some(page) = go {
        app.act(ids::NAV_PAGE, json!({ "id": page }));
        ui.data_mut(|d| d.insert_temp(open_id, false));
        resp.surrender_focus();
    } else if clicked_outside {
        ui.data_mut(|d| d.insert_temp(open_id, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_pages_in_every_notebook() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        let hits = find(&mut app, "binder");
        assert_eq!(hits.len(), 1, "the welcome page mentions a binder");
        assert_eq!(hits.first().map(|h| h.section.as_str()), Some("Quick Notes"));
        assert!(find(&mut app, "").is_empty());
        app.act(ids::NOTEBOOK_NEW, json!({"name": "Second"}));
        assert_eq!(find(&mut app, "binder").len(), 1);
    }
}

/// The arrow in the search box: where to search.
fn scope_button(ui: &mut Ui, app: &mut NoteApp, r: Rect) {
    let t = app.tokens.clone();
    let resp = ui.interact(r, box_id().with("scope"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Search scope")));
    if resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    }
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(10.0, 10.0)), Icon::Dropdown, icons::Style::mono(t.text_dim));
    let resp = resp.on_hover_text(crate::i18n::fmt_plain(tl!("Searching {scope}"), &[("scope", scope_label(&app.ui.search_scope))]));
    crate::widgets::popup_menu(&resp, app, |ui, app| {
        let page = format!("{}    {}", tl!("This Page"), crate::widgets::shortcut_text("Mod+F"));
        if crate::widgets::menu_item(ui, Some(Icon::Search), &page, app.session.current_page().is_some()) {
            let q = app.ui.search.clone();
            crate::dialogs::find::open(app);
            if let Some(f) = app.ui.find.as_mut()
                && !q.trim().is_empty()
            {
                f.query = q;
            }
        }
        ui.separator();
        for s in SCOPES {
            if crate::widgets::check_item(ui, app.ui.search_scope == s, scope_label(s)) {
                app.ui.search_scope = s.to_owned();
            }
        }
    });
}
