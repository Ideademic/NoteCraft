//! Home › Find Tags: the Tags Summary pane on the right. It lists the tagged notes in the chosen
//! scope (this page, section, section group, notebook or all notebooks; `tags.summary`), grouped
//! by tag name, section, page title, date or note text, optionally only the To Do items not yet
//! ticked. Clicking a note goes to it with the caret at the start of its paragraph. Like
//! OneNote's, the list is a snapshot: Refresh Results reads the notebooks again.

use chrono::{DateTime, Utc};
use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::{Guid, Tag};
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, fit_text, paint_strong, regular};
use crate::{NoteApp, tl, widgets};

/// Default width of the pane.
pub const PANE_W: f32 = 290.0;

/// How the notes are grouped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GroupBy {
    #[default]
    Tag,
    Section,
    Title,
    Date,
    Text,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [GroupBy::Tag, GroupBy::Section, GroupBy::Title, GroupBy::Date, GroupBy::Text];

    pub fn label(self) -> &'static str {
        match self {
            GroupBy::Tag => tl!("Tag name"),
            GroupBy::Section => tl!("Section"),
            GroupBy::Title => tl!("Page title"),
            GroupBy::Date => tl!("Date"),
            GroupBy::Text => tl!("Note text"),
        }
    }
}

/// The scopes the pane can search, as `tags.summary` names them.
pub const SCOPES: [&str; 5] = ["page", "section", "sectionGroup", "notebook", "all"];

/// A scope's label.
pub fn scope_label(scope: &str) -> &'static str {
    match scope {
        "page" => tl!("This Page"),
        "section" => tl!("This Section"),
        "sectionGroup" => tl!("This Section Group"),
        "notebook" => tl!("This Notebook"),
        _ => tl!("All Notebooks"),
    }
}

/// One tagged note.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub page: Guid,
    pub page_title: String,
    pub section_name: String,
    pub outline: Guid,
    pub para: usize,
    pub tag: Tag,
    pub label: String,
    pub text: String,
    pub modified: Option<DateTime<Utc>>,
}

/// The pane's state.
#[derive(Clone, Debug, PartialEq)]
pub struct TagsPane {
    pub group: GroupBy,
    pub scope: String,
    pub unchecked_only: bool,
    pub items: Vec<Item>,
    pub truncated: bool,
}

impl Default for TagsPane {
    fn default() -> Self {
        TagsPane { group: GroupBy::Tag, scope: "notebook".to_owned(), unchecked_only: false, items: Vec::new(), truncated: false }
    }
}

/// Opens the pane (Home › Find Tags) and reads the tags; again, it closes.
pub fn toggle(app: &mut NoteApp) {
    if app.ui.tags_pane.take().is_some() {
        return;
    }
    let mut pane = TagsPane::default();
    refresh(app, &mut pane);
    app.ui.tags_pane = Some(pane);
}

/// Reads the tagged notes again.
pub fn refresh(app: &mut NoteApp, pane: &mut TagsPane) {
    let params = json!({"scope": pane.scope, "uncheckedOnly": pane.unchecked_only});
    let out = match app.session.run(ids::TAGS_SUMMARY, &params) {
        Ok(o) => o.value,
        Err(e) => {
            app.set_status(e.to_string(), true);
            Value::Null
        }
    };
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    pane.items = out
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let mut tag = Tag::parse(&s(v, "tag"))?;
            if let (Tag::ToDo { checked }, Some(c)) = (&mut tag, v.get("checked").and_then(Value::as_bool)) {
                *checked = c;
            }
            Some(Item {
                page: Guid::parse(&s(v, "page"))?,
                page_title: s(v, "pageTitle"),
                section_name: s(v, "sectionName"),
                outline: Guid::parse(&s(v, "outline"))?,
                para: v.get("para").and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok()).unwrap_or(0),
                tag,
                label: s(v, "label"),
                text: s(v, "text"),
                modified: DateTime::parse_from_rfc3339(&s(v, "modified")).ok().map(|d| d.with_timezone(&Utc)),
            })
        })
        .collect();
    pane.truncated = out.get("truncated").and_then(Value::as_bool).unwrap_or(false);
}

/// The items in groups: (heading, items), in display order.
pub fn grouped(items: &[Item], by: GroupBy, utc_offset_minutes: i32) -> Vec<(String, Vec<Item>)> {
    let key = |it: &Item| -> (String, String) {
        match by {
            GroupBy::Tag => {
                let order = Tag::ALL.iter().position(|t| t.name() == it.tag.name()).unwrap_or(99);
                (format!("{order:02}"), it.label.clone())
            }
            GroupBy::Section => (it.section_name.to_lowercase(), it.section_name.clone()),
            GroupBy::Title => {
                let t = if it.page_title.trim().is_empty() { tl!("Untitled page").to_owned() } else { it.page_title.clone() };
                (t.to_lowercase(), t)
            }
            GroupBy::Date => match it.modified {
                // Newest first: sort on the negated time.
                Some(d) => {
                    let local = crate::panes::history::when(d, utc_offset_minutes);
                    let day = local.split(' ').next().unwrap_or("").to_owned();
                    (format!("{:020}", i64::MAX - d.date_naive().and_hms_opt(0, 0, 0).map_or(0, |n| n.and_utc().timestamp())), day)
                }
                None => ("~".into(), tl!("No date").to_owned()),
            },
            GroupBy::Text => {
                let first = it.text.trim().chars().next().map_or('#', |c| c.to_uppercase().next().unwrap_or(c));
                let first = if first.is_alphanumeric() { first } else { '#' };
                (first.to_string(), first.to_string())
            }
        }
    };
    let mut keyed: Vec<((String, String), Item)> = items.iter().map(|it| (key(it), it.clone())).collect();
    keyed.sort_by(|a, b| a.0.0.cmp(&b.0.0).then_with(|| a.1.text.to_lowercase().cmp(&b.1.text.to_lowercase())));
    let mut out: Vec<(String, Vec<Item>)> = Vec::new();
    for ((_, heading), it) in keyed {
        match out.last_mut() {
            Some((h, list)) if *h == heading => list.push(it),
            _ => out.push((heading, vec![it])),
        }
    }
    out
}

/// Goes to a tagged note: its page, the caret at the start of its paragraph.
pub fn jump(app: &mut NoteApp, it: &Item) {
    app.ui.bin_view = None;
    if app.session.current.page != Some(it.page) {
        app.act(ids::NAV_PAGE, json!({ "id": it.page }));
    }
    app.canvas.goto = Some((it.outline, it.para));
}

/// The pane.
pub fn pane(ui: &mut Ui, app: &mut NoteApp) {
    let Some(mut pane) = app.ui.tags_pane.clone() else { return };
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.panel);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let header = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
    paint_strong(ui.painter(), pos2(header.min.x + 12.0, header.center().y), Align2::LEFT_CENTER, tl!("Tags Summary"), regular(14.0), t.text);
    let close = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, close, Icon::Close, tl!("Close")) {
        app.ui.tags_pane = None;
        return;
    }
    let mut changed = false;
    // Group by, and the unchecked filter.
    let controls = Rect::from_min_size(pos2(full.min.x + 10.0, header.max.y), vec2(full.width() - 20.0, 52.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(controls).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Group tags by:")).font(regular(12.0)).color(t.text_dim));
            crate::ribbon::simplified::text_menu(ui, app, pane.group.label(), tl!("Group tags by"), true, |ui, _app| {
                for g in GroupBy::ALL {
                    if widgets::check_item(ui, pane.group == g, g.label()) {
                        pane.group = g;
                    }
                }
            });
        });
        let mut only = pane.unchecked_only;
        if ui.checkbox(&mut only, egui::RichText::new(tl!("Show only unchecked items")).font(regular(12.0)).color(t.text)).changed() {
            pane.unchecked_only = only;
            changed = true;
        }
    });
    // Search scope and Refresh at the bottom.
    let footer = Rect::from_min_max(pos2(full.min.x + 10.0, full.max.y - 70.0), pos2(full.max.x - 10.0, full.max.y - 6.0));
    ui.painter().line_segment([pos2(full.min.x, footer.min.y - 4.0), pos2(full.max.x, footer.min.y - 4.0)], Stroke::new(1.0, t.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(footer).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Search:")).font(regular(12.0)).color(t.text_dim));
            crate::ribbon::simplified::text_menu(ui, app, scope_label(&pane.scope), tl!("Where to look for tags"), true, |ui, _app| {
                for s in SCOPES {
                    if widgets::check_item(ui, pane.scope == s, scope_label(s)) {
                        pane.scope = s.to_owned();
                        changed = true;
                    }
                }
            });
        });
        ui.add_space(4.0);
        if widgets::flat_button(ui, tl!("Refresh Results"), false, true) {
            changed = true;
        }
    });
    if changed {
        refresh(app, &mut pane);
    }
    let list = Rect::from_min_max(pos2(full.min.x + 1.0, controls.max.y + 4.0), pos2(full.max.x, footer.min.y - 8.0));
    let groups = grouped(&pane.items, pane.group, app.canvas.utc_offset_minutes);
    let mut go: Option<Item> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_tags_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if groups.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(tl!("No tagged notes here. Tag a note from Home › Tags, then refresh."))
                        .font(regular(12.0))
                        .color(t.text_dim),
                );
            }
            for (heading, items) in &groups {
                let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                ui.painter().text(pos2(hr.min.x + 10.0, hr.center().y + 2.0), Align2::LEFT_CENTER, heading, regular(12.5), t.accent_text);
                for it in items {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &it.text));
                    if resp.hovered() {
                        ui.painter().rect_filled(r.shrink2(vec2(4.0, 1.0)), RADIUS_SMALL, t.hover);
                    }
                    let icon = Rect::from_center_size(pos2(r.min.x + 22.0, r.min.y + 12.0), vec2(14.0, 14.0));
                    crate::ribbon::paragraph::paint_tag_icon(ui.painter(), icon, it.tag, icons::Style::new(t.icon, t.accent));
                    let w = r.width() - 46.0;
                    let text = if it.text.trim().is_empty() { tl!("(no text)") } else { it.text.as_str() };
                    ui.painter().text(
                        pos2(r.min.x + 36.0, r.min.y + 4.0),
                        Align2::LEFT_TOP,
                        fit_text(ui.ctx(), text, &regular(12.5), w),
                        regular(12.5),
                        t.text,
                    );
                    let place = match pane.group {
                        GroupBy::Section => it.page_title.clone(),
                        _ => format!("{} › {}", it.section_name, it.page_title),
                    };
                    ui.painter().text(
                        pos2(r.min.x + 36.0, r.min.y + 20.0),
                        Align2::LEFT_TOP,
                        fit_text(ui.ctx(), &place, &regular(10.5), w),
                        regular(10.5),
                        t.text_dim,
                    );
                    if resp.clicked() {
                        go = Some(it.clone());
                    }
                }
            }
            if pane.truncated {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tl!("More tags than can be listed: narrow the search.")).font(regular(11.5)).color(t.text_dim));
            }
        });
    });
    app.ui.tags_pane = Some(pane);
    if let Some(it) = go {
        jump(app, &it);
    }
}
