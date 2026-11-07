//! The notebook pane: every open notebook with its section groups and sections, and the thin
//! bar it collapses to.

use std::collections::HashSet;

use egui::{Align2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use notecraft_doc::{Guid, Notebook, SectionEntry};
use notecraft_engine::ids;
use serde_json::json;

use crate::app::{Prompt, RenameTarget};
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, fit_text, paint_strong, regular};
use crate::widgets;
use crate::{NoteApp, tl};

const ROW_H: f32 = 26.0;
const HEADER_H: f32 = 32.0;

/// Notebooks and groups the user folded (by id).
fn folded_id() -> egui::Id {
    egui::Id::unique("nc_folded_entries")
}

fn folded(ui: &Ui) -> HashSet<Guid> {
    ui.data(|d| d.get_temp::<HashSet<Guid>>(folded_id())).unwrap_or_default()
}

fn toggle_fold(ui: &Ui, id: Guid) {
    ui.data_mut(|d| {
        let set = d.get_temp_mut_or_default::<HashSet<Guid>>(folded_id());
        if !set.remove(&id) {
            set.insert(id);
        }
    });
}

/// What a row of the pane is.
enum Row {
    Notebook { id: Guid, name: String, color: egui::Color32, open: bool },
    Group { id: Guid, name: String, depth: usize, open: bool },
    Section { id: Guid, name: String, color: notecraft_doc::Color, depth: usize },
}

fn rows(app: &NoteApp, folded: &HashSet<Guid>) -> Vec<Row> {
    let mut out = Vec::new();
    for nb in &app.session.notebooks {
        let open = !folded.contains(&nb.id);
        out.push(Row::Notebook { id: nb.id, name: nb.name.clone(), color: app.tokens.section_fill(nb.color), open });
        if open {
            entry_rows(&nb.sections, 1, folded, app.session.current.section, nb, &mut out);
        }
    }
    out
}

fn entry_rows(entries: &[SectionEntry], depth: usize, folded: &HashSet<Guid>, current: Option<Guid>, nb: &Notebook, out: &mut Vec<Row>) {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(s) => out.push(Row::Section { id: s.id, name: s.name.clone(), color: s.color, depth }),
            SectionEntry::Group(g) => {
                // A group is open unless folded; the current section's groups always are.
                let holds_current = current.is_some_and(|c| nb.path_of(c).is_some_and(|p| p.contains(&g.id)));
                let open = holds_current || !folded.contains(&g.id);
                out.push(Row::Group { id: g.id, name: g.name.clone(), depth, open });
                if open {
                    entry_rows(&g.entries, depth + 1, folded, current, nb, out);
                }
            }
        }
    }
}

/// The notebook pane (expanded).
pub fn notebook_pane(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.notebook_pane);
    ui.painter().line_segment([full.right_top(), full.right_bottom()], Stroke::new(1.0, t.border));
    // Header: "Notebooks" and the pin that folds the pane away.
    let header = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    ui.painter().text(pos2(header.min.x + 12.0, header.center().y), Align2::LEFT_CENTER, tl!("Notebooks"), regular(13.0), t.text_dim);
    let pin = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, pin, Icon::ChevronLeft, tl!("Collapse Notebook Pane")) {
        app.ui.notebook_pane_open = false;
    }
    let footer_h = 34.0;
    let list = Rect::from_min_max(pos2(full.min.x, header.max.y), pos2(full.max.x - 1.0, full.max.y - footer_h));
    let folded = folded(ui);
    let rows = rows(app, &folded);
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_notebook_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            for row in &rows {
                draw_row(ui, app, row);
            }
        });
    });
    // Footer: + Notebook.
    let footer = Rect::from_min_max(pos2(full.min.x, full.max.y - footer_h), pos2(full.max.x - 1.0, full.max.y));
    ui.painter().line_segment([footer.left_top(), footer.right_top()], Stroke::new(1.0, t.border));
    let b = Rect::from_min_max(footer.min + vec2(4.0, 4.0), footer.max - vec2(4.0, 4.0));
    let resp = ui.interact(b, ui.scope_id().with("add_notebook"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("+ Notebook")));
    if resp.hovered() {
        ui.painter().rect_filled(b, RADIUS_SMALL, t.hover);
    }
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(b.min.x + 14.0, b.center().y), vec2(12.0, 12.0)),
        Icon::Plus,
        icons::Style::mono(t.accent_text),
    );
    ui.painter().text(pos2(b.min.x + 28.0, b.center().y), Align2::LEFT_CENTER, tl!("Notebook"), regular(12.5), t.accent_text);
    if resp.on_hover_text(tl!("Create a new notebook")).clicked() {
        crate::backstage::open(app, crate::backstage::BackstagePage::New);
    }
    // egui saves a resizable panel's *used* rect as its width for the next frame. The list above
    // stops 1 px short of the edge, so without claiming the whole pane it lost a pixel on every
    // repaint and shrank back after being dragged wider.
    ui.expand_to_include_rect(full);
}

fn draw_row(ui: &mut Ui, app: &mut NoteApp, row: &Row) {
    let t = app.tokens.clone();
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    let (id, depth, label) = match row {
        Row::Notebook { id, name, .. } => (*id, 0usize, name.as_str()),
        Row::Group { id, name, depth, .. } => (*id, *depth, name.as_str()),
        Row::Section { id, name, depth, .. } => (*id, *depth, name.as_str()),
    };
    let current = match row {
        Row::Notebook { id, .. } => app.session.current.notebook == Some(*id) && app.session.current.section.is_none(),
        Row::Section { id, .. } => app.session.current.section == Some(*id),
        Row::Group { .. } => false,
    };
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, label));
    let fill = match row {
        Row::Section { color, .. } if current => Some(t.section_selected(*color)),
        _ if resp.hovered() => Some(t.hover),
        _ => None,
    };
    if let Some(f) = fill {
        ui.painter().rect_filled(r, 0.0, f);
    }
    let indent = 8.0 + depth as f32 * 14.0;
    let mut x = r.min.x + indent;
    let target = match row {
        Row::Notebook { .. } => RenameTarget::Notebook(id),
        _ => RenameTarget::Section(id),
    };
    match row {
        Row::Notebook { color, open, .. } => {
            chevron(ui, pos2(x + 5.0, r.center().y), *open, &t);
            x += 14.0;
            icons::paint_notebook(ui.painter(), Rect::from_center_size(pos2(x + 9.0, r.center().y), vec2(18.0, 18.0)), *color);
            x += 24.0;
        }
        Row::Group { open, .. } => {
            chevron(ui, pos2(x + 5.0, r.center().y), *open, &t);
            x += 14.0;
            icons::paint_styled(
                ui.painter(),
                Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(16.0, 16.0)),
                Icon::SectionGroup,
                icons::Style::new(t.icon, t.accent),
            );
            x += 22.0;
        }
        Row::Section { color, .. } => {
            x += 14.0;
            let chip = Rect::from_center_size(pos2(x + 5.0, r.center().y), vec2(9.0, 15.0));
            ui.painter().rect_filled(chip, 1.0, t.section_fill(*color));
            x += 16.0;
        }
    }
    let text_rect = Rect::from_min_max(pos2(x, r.min.y + 2.0), pos2(r.max.x - 4.0, r.max.y - 2.0));
    if !super::rename_box(ui, app, target, text_rect) {
        let font = regular(if matches!(row, Row::Notebook { .. }) { 13.0 } else { 12.5 });
        let shown = fit_text(ui.ctx(), label, &font, text_rect.width());
        let at = pos2(x, r.center().y);
        if current || matches!(row, Row::Notebook { .. }) && app.session.current.notebook == Some(id) {
            paint_strong(ui.painter(), at, Align2::LEFT_CENTER, &shown, font, t.text);
        } else {
            ui.painter().text(at, Align2::LEFT_CENTER, &shown, font, t.text);
        }
    }
    let ctx = ui.ctx().clone();
    if resp.clicked() {
        match row {
            Row::Notebook { id, .. } => {
                let chevron_hit = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < r.min.x + indent + 14.0);
                if chevron_hit {
                    toggle_fold(ui, *id);
                } else if app.session.current.notebook != Some(*id) {
                    app.act(ids::NAV_NOTEBOOK, json!({ "id": id }));
                } else {
                    toggle_fold(ui, *id);
                }
            }
            Row::Group { id, .. } => toggle_fold(ui, *id),
            Row::Section { id, .. } => super::go_section(&ctx, app, *id),
        }
    }
    if resp.double_clicked() && !matches!(row, Row::Group { .. }) {
        super::start_rename(app, target);
    }
    resp.context_menu(|ui| match row {
        Row::Notebook { id, name, .. } => notebook_menu(ui, app, *id, name),
        Row::Group { id, name, .. } => group_menu(ui, app, *id, name),
        Row::Section { id, name, color, .. } => section_menu(ui, app, *id, name, *color),
    });
}

fn chevron(ui: &Ui, c: egui::Pos2, open: bool, t: &crate::Tokens) {
    let pts = if open {
        vec![c + vec2(-3.5, -1.5), c + vec2(3.5, -1.5), c + vec2(0.0, 2.5)]
    } else {
        vec![c + vec2(-1.5, -3.5), c + vec2(2.5, 0.0), c + vec2(-1.5, 3.5)]
    };
    ui.painter().add(Shape::convex_polygon(pts, t.text_dim, Stroke::NONE));
}

/// Right-click on a notebook.
pub fn notebook_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid, name: &str) {
    if widgets::menu_item(ui, Some(Icon::Rename), tl!("Rename"), true) {
        super::start_rename(app, RenameTarget::Notebook(id));
    }
    ui.menu_button(tl!("Notebook Color"), |ui| {
        let colors: Vec<egui::Color32> = notecraft_doc::Color::SECTION_PALETTE.iter().map(|c| crate::theme::color32(*c)).collect();
        let current = app.session.notebook(id).map(|n| crate::theme::color32(n.color));
        if let Some(c) = widgets::color::grid(ui, &colors, 8, current) {
            app.act(ids::NOTEBOOK_COLOR, json!({ "id": id, "color": widgets::color::hex(c) }));
        }
    });
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::NewSection), tl!("New Section"), true) {
        app.act(ids::SECTION_NEW, json!({ "notebook": id }));
    }
    if widgets::menu_item(ui, Some(Icon::SectionGroup), tl!("New Section Group"), true) {
        if app.session.current.notebook != Some(id) {
            app.act(ids::NAV_NOTEBOOK, json!({ "id": id }));
        }
        app.act(ids::SECTION_GROUP_NEW, json!({}));
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Info), tl!("Properties…"), true) {
        app.ui.info_notebook = Some(id);
        crate::backstage::open(app, crate::backstage::BackstagePage::Info);
    }
    if widgets::menu_item(ui, Some(Icon::Close), tl!("Close This Notebook"), true) {
        app.ui.prompt = Some(Prompt::CloseNotebook { id, name: name.to_owned() });
    }
}

pub fn group_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid, name: &str) {
    if widgets::menu_item(ui, Some(Icon::Rename), tl!("Rename"), true) {
        super::start_rename(app, RenameTarget::Section(id));
    }
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete"), true) {
        app.ui.prompt = Some(Prompt::DeleteSection { id, name: name.to_owned() });
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::NewSection), tl!("New Section"), true) {
        app.act(ids::SECTION_NEW, json!({ "group": id }));
    }
    if widgets::menu_item(ui, Some(Icon::SectionGroup), tl!("New Section Group"), true) {
        app.act(ids::SECTION_GROUP_NEW, json!({ "parent": id }));
    }
}

/// Right-click on a section (pane rows and tabs share it).
pub fn section_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid, name: &str, color: notecraft_doc::Color) {
    if widgets::menu_item(ui, Some(Icon::Rename), tl!("Rename"), true) {
        super::start_rename(app, RenameTarget::Section(id));
    }
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete"), true) {
        app.ui.prompt = Some(Prompt::DeleteSection { id, name: name.to_owned() });
    }
    ui.menu_button(tl!("Section Color"), |ui| super::section_color_menu(ui, app, id, Some(color)));
    ui.menu_button(tl!("Move or Copy"), |ui| move_menu(ui, app, id));
    ui.separator();
    let group = app.session.notebook_of_section(id).and_then(|nb| nb.path_of(id)).and_then(|p| p.last().copied());
    if widgets::menu_item(ui, Some(Icon::NewSection), tl!("New Section"), true) {
        app.act(ids::SECTION_NEW, json!({ "group": group }));
    }
    if widgets::menu_item(ui, Some(Icon::SectionGroup), tl!("New Section Group"), true) {
        app.act(ids::SECTION_GROUP_NEW, json!({ "parent": group }));
    }
}

/// Move a section to the end of another notebook or section group.
fn move_menu(ui: &mut Ui, app: &mut NoteApp, id: Guid) {
    widgets::menu_heading(ui, tl!("Move to"));
    let mut targets: Vec<(String, Guid, Option<Guid>, usize)> = Vec::new();
    for nb in &app.session.notebooks {
        targets.push((nb.name.clone(), nb.id, None, nb.sections.len()));
        collect_groups(&nb.sections, nb.id, 1, &mut targets);
    }
    for (label, nb, group, len) in targets {
        if widgets::menu_item(ui, Some(if group.is_some() { Icon::SectionGroup } else { Icon::NewNotebook }), &label, true) {
            app.act(ids::SECTION_MOVE, json!({ "id": id, "index": len, "group": group, "notebook": nb }));
        }
    }
}

fn collect_groups(entries: &[SectionEntry], nb: Guid, depth: usize, out: &mut Vec<(String, Guid, Option<Guid>, usize)>) {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        if let SectionEntry::Group(g) = e {
            out.push((format!("{}{}", "    ".repeat(depth), g.name), nb, Some(g.id), g.entries.len()));
            collect_groups(&g.entries, nb, depth + 1, out);
        }
    }
}

/// The collapsed notebook pane: a thin bar with the current notebook's name on its side.
pub fn notebook_bar(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.notebook_pane);
    ui.painter().line_segment([full.right_top(), full.right_bottom()], Stroke::new(1.0, t.border));
    let pin = Rect::from_center_size(pos2(full.center().x, full.min.y + 16.0), vec2(24.0, 24.0));
    if super::icon_button(ui, app, pin, Icon::ChevronRight, tl!("Expand Notebook Pane")) {
        app.ui.notebook_pane_open = true;
    }
    let Some(nb) = app.session.current_notebook() else { return };
    let (name, color) = (nb.name.clone(), t.section_fill(nb.color));
    let body = Rect::from_min_max(pos2(full.min.x, full.min.y + 32.0), pos2(full.max.x - 1.0, full.max.y - 8.0));
    let resp = ui.interact(body, ui.scope_id().with("nc_notebook_bar"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    if resp.hovered() {
        ui.painter().rect_filled(body, 0.0, t.hover);
    }
    icons::paint_notebook(ui.painter(), Rect::from_center_size(pos2(full.center().x, body.min.y + 14.0), vec2(18.0, 18.0)), color);
    // The name reads top to bottom, turned a quarter clockwise.
    let font = regular(12.5);
    let shown = fit_text(ui.ctx(), &name, &font, (body.height() - 40.0).max(0.0));
    let galley = ui.painter().layout_no_wrap(shown, font, t.text);
    let at = pos2(full.center().x + galley.size().y / 2.0, body.min.y + 32.0);
    ui.painter().add(egui::epaint::TextShape::new(at, galley, t.text).with_angle(std::f32::consts::FRAC_PI_2));
    let resp = resp.on_hover_text(tl!("Show notebooks"));
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(220.0);
        let list: Vec<(Guid, String, egui::Color32)> =
            app.session.notebooks.iter().map(|n| (n.id, n.name.clone(), app.tokens.section_fill(n.color))).collect();
        for (id, name, color) in list {
            let current = app.session.current.notebook == Some(id);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(200.0), 26.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, &name));
            if resp.hovered() || current {
                ui.painter().rect_filled(r, RADIUS_SMALL, if resp.hovered() { t.hover } else { t.checked });
            }
            icons::paint_notebook(ui.painter(), Rect::from_center_size(pos2(r.min.x + 14.0, r.center().y), vec2(16.0, 16.0)), color);
            ui.painter().text(pos2(r.min.x + 30.0, r.center().y), Align2::LEFT_CENTER, &name, regular(12.5), t.text);
            if resp.clicked() {
                app.act(ids::NAV_NOTEBOOK, json!({ "id": id }));
                ui.close();
            }
        }
        ui.separator();
        if widgets::menu_item(ui, Some(Icon::Pin), tl!("Pin Notebook Pane to Side"), true) {
            app.ui.notebook_pane_open = true;
        }
    });
}
