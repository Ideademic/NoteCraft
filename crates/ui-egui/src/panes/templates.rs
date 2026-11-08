//! Insert › Page Templates: the Templates pane on the right. NoteCraft's own templates (blank
//! pages in several colours and rulings, meeting notes, a to-do list, lecture notes, a project
//! overview, a weekly planner, a journal) and the user's, by category, each with a small swatch
//! of its paper. A click adds a page made from it (`page.newFromTemplate`). Below: the template
//! new pages in this section start from (`section.defaultTemplate`), and saving the current page
//! as a template (`template.saveCurrent`; the host stores them, see
//! [`crate::NoteApp::templates_json`]).

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_engine::ids;
use notecraft_engine::templates::{self, Template};
use serde_json::json;

use crate::icons::Icon;
use crate::theme::{RADIUS_SMALL, color32, fit_text, mix, paint_strong, regular};
use crate::{NoteApp, tl, widgets};

/// Default width of the pane.
pub const PANE_W: f32 = 280.0;

/// The pane's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplatesPane {
    /// The name typed for "Save current page as a template".
    pub name: String,
    /// Categories folded away.
    pub folded: Vec<String>,
}

/// Opens or closes the pane (Insert › Page Templates).
pub fn toggle(app: &mut NoteApp) {
    app.ui.templates_pane = match app.ui.templates_pane.take() {
        Some(_) => None,
        None => Some(TemplatesPane::default()),
    };
}

/// Every template: NoteCraft's, then the user's.
pub fn all(app: &NoteApp) -> Vec<Template> {
    templates::builtins().into_iter().chain(app.session.templates.iter().cloned()).collect()
}

/// Adds a page made from template `id`.
pub fn use_template(app: &mut NoteApp, id: &str) {
    app.ui.bin_view = None;
    let _ = app.run(ids::PAGE_NEW_FROM_TEMPLATE, json!({ "template": id }));
}

/// Saves the current page as a template called `name`.
pub fn save_current(app: &mut NoteApp, name: &str) -> bool {
    let ok = app.run(ids::TEMPLATE_SAVE_CURRENT, json!({ "name": name })).is_ok();
    if ok {
        app.ui.templates_revision = app.ui.templates_revision.wrapping_add(1);
        app.set_status(crate::i18n::fmt_plain(tl!("Saved \u{201c}{name}\u{201d} under My Templates."), &[("name", name)]), false);
    }
    ok
}

/// A small picture of a template's paper: its colour, rule lines and where its notes are.
fn swatch(ui: &Ui, app: &NoteApp, r: Rect, t: &Template) {
    let tokens = &app.tokens;
    let paper = t.page.background.color.map_or(egui::Color32::WHITE, color32);
    ui.painter().rect(r, 1.0, paper, Stroke::new(1.0, tokens.border_strong), egui::StrokeKind::Inside);
    let line = mix(paper, egui::Color32::from_rgb(0x6B, 0x8B, 0xC8), 0.45);
    if let Some(step) = t.page.background.rule.spacing() {
        // Paper ~ 600 pt wide shown in the swatch's width.
        let k = r.width() / 600.0;
        let gap = (step * k * 3.0).max(3.0);
        let mut y = r.min.y + gap;
        while y < r.max.y - 1.0 {
            ui.painter().line_segment([pos2(r.min.x + 1.0, y), pos2(r.max.x - 1.0, y)], Stroke::new(0.6, line));
            y += gap;
        }
        if t.page.background.rule.is_grid() {
            let mut x = r.min.x + gap;
            while x < r.max.x - 1.0 {
                ui.painter().line_segment([pos2(x, r.min.y + 1.0), pos2(x, r.max.y - 1.0)], Stroke::new(0.6, line));
                x += gap;
            }
        }
    }
    let k = r.width() / 700.0;
    let ink = mix(paper, egui::Color32::from_rgb(0x30, 0x30, 0x36), 0.55);
    if !t.page.title.is_empty() {
        ui.painter().line_segment([pos2(r.min.x + 4.0, r.min.y + 6.0), pos2(r.min.x + r.width() * 0.45, r.min.y + 6.0)], Stroke::new(1.6, ink));
    }
    for el in &t.page.elements {
        if let Some(b) = el.bounds() {
            let x0 = r.min.x + b.x * k;
            let y0 = r.min.y + 4.0 + b.y * k * 0.9;
            let w = (b.w * k).min(r.max.x - 2.0 - x0).max(0.0);
            let rows = (b.h * k / 3.0).clamp(1.0, 6.0) as usize;
            for i in 0..rows {
                let y = y0 + i as f32 * 3.0;
                if y < r.max.y - 2.0 {
                    let len = if i % 3 == 2 { w * 0.6 } else { w };
                    ui.painter().line_segment([pos2(x0, y), pos2(x0 + len, y)], Stroke::new(0.8, ink));
                }
            }
        }
    }
}

/// The pane.
pub fn pane(ui: &mut Ui, app: &mut NoteApp) {
    let Some(mut pane) = app.ui.templates_pane.clone() else { return };
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.panel);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let header = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
    paint_strong(ui.painter(), pos2(header.min.x + 12.0, header.center().y), Align2::LEFT_CENTER, tl!("Templates"), regular(14.0), t.text);
    let close = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, close, Icon::Close, tl!("Close")) {
        app.ui.templates_pane = None;
        return;
    }
    ui.painter().text(
        pos2(header.min.x + 12.0, header.max.y + 2.0),
        Align2::LEFT_TOP,
        tl!("Add a page made from a template."),
        regular(11.5),
        t.text_dim,
    );
    let list_top = header.max.y + 22.0;
    let footer_h = 168.0;
    let footer = Rect::from_min_max(pos2(full.min.x + 12.0, full.max.y - footer_h), pos2(full.max.x - 12.0, full.max.y - 6.0));
    let list = Rect::from_min_max(pos2(full.min.x + 1.0, list_top), pos2(full.max.x, footer.min.y - 8.0));
    let all = all(app);
    let mut use_id: Option<String> = None;
    let mut delete_id: Option<String> = None;
    let has_section = app.session.current_section().is_some();
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_template_rows").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let mut cats: Vec<String> = Vec::new();
            for tpl in &all {
                if !cats.contains(&tpl.category) {
                    cats.push(tpl.category.clone());
                }
            }
            for cat in cats {
                let folded = pane.folded.contains(&cat);
                let (hr, hresp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                hresp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, !folded, &cat));
                let chevron = Rect::from_center_size(pos2(hr.min.x + 16.0, hr.center().y), vec2(10.0, 10.0));
                crate::icons::paint_styled(
                    ui.painter(),
                    chevron,
                    if folded { Icon::ChevronRight } else { Icon::ChevronDown },
                    crate::icons::Style::mono(t.text_dim),
                );
                paint_strong(ui.painter(), pos2(hr.min.x + 28.0, hr.center().y), Align2::LEFT_CENTER, &cat, regular(12.5), t.text);
                if hresp.clicked() {
                    if folded {
                        pane.folded.retain(|c| *c != cat);
                    } else {
                        pane.folded.push(cat.clone());
                    }
                }
                if folded {
                    continue;
                }
                for tpl in all.iter().filter(|x| x.category == cat) {
                    let (r, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 40.0), if has_section { Sense::click() } else { Sense::hover() });
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, has_section, &tpl.name));
                    if resp.hovered() && has_section {
                        ui.painter().rect_filled(r.shrink2(vec2(6.0, 1.0)), RADIUS_SMALL, t.hover);
                    }
                    let sw = Rect::from_min_size(pos2(r.min.x + 26.0, r.min.y + 5.0), vec2(40.0, 30.0));
                    swatch(ui, app, sw, tpl);
                    let w = r.width() - 110.0;
                    ui.painter().text(
                        pos2(sw.max.x + 10.0, r.center().y),
                        Align2::LEFT_CENTER,
                        fit_text(ui.ctx(), &tpl.name, &regular(12.5), w),
                        regular(12.5),
                        t.text,
                    );
                    if !tpl.is_builtin() {
                        let del = Rect::from_center_size(pos2(r.max.x - 22.0, r.center().y), vec2(22.0, 22.0));
                        if super::icon_button(ui, app, del, Icon::Delete, tl!("Delete this template")) {
                            delete_id = Some(tpl.id.clone());
                            continue;
                        }
                    }
                    if resp.clicked() {
                        use_id = Some(tpl.id.clone());
                    }
                }
            }
        });
    });
    ui.painter().line_segment([pos2(full.min.x, footer.min.y - 4.0), pos2(full.max.x, footer.min.y - 4.0)], Stroke::new(1.0, t.border));
    let current_default = app.session.current_section().and_then(|s| s.default_template.clone());
    let default_name =
        current_default.as_deref().and_then(|id| all.iter().find(|x| x.id == id)).map_or(tl!("No Default Template").to_owned(), |x| x.name.clone());
    let mut set_default: Option<Option<String>> = None;
    let mut save: Option<String> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(footer).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.label(egui::RichText::new(tl!("Always use a specific template")).font(regular(12.5)).color(t.text));
        ui.label(egui::RichText::new(tl!("New pages in this section start from it.")).font(regular(11.0)).color(t.text_dim));
        ui.add_space(3.0);
        crate::ribbon::simplified::text_menu(
            ui,
            app,
            &default_name,
            tl!("Default template for new pages in this section"),
            has_section,
            |ui, _app| {
                if widgets::check_item(ui, current_default.is_none(), tl!("No Default Template")) {
                    set_default = Some(None);
                }
                for tpl in &all {
                    if widgets::check_item(ui, current_default.as_deref() == Some(tpl.id.as_str()), &tpl.name) {
                        set_default = Some(Some(tpl.id.clone()));
                    }
                }
            },
        );
        ui.add_space(10.0);
        ui.label(egui::RichText::new(tl!("Create new template")).font(regular(12.5)).color(t.text));
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut pane.name)
                    .hint_text(egui::RichText::new(tl!("Template name")).color(t.text_dim))
                    .desired_width((footer.width() - 100.0).max(60.0))
                    .font(regular(12.5)),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ok = !pane.name.trim().is_empty() && app.session.current_page().is_some();
            let clicked = widgets::flat_button(ui, tl!("Save"), false, ok);
            if ok && (clicked || enter) {
                save = Some(pane.name.trim().to_owned());
            }
        });
        ui.label(egui::RichText::new(tl!("Saves the current page as a template.")).font(regular(11.0)).color(t.text_dim));
    });
    if let Some(name) = save
        && save_current(app, &name)
    {
        pane.name.clear();
    }
    app.ui.templates_pane = Some(pane);
    if let Some(id) = use_id {
        use_template(app, &id);
    }
    if let Some(id) = delete_id
        && app.run(ids::TEMPLATE_DELETE, json!({ "id": id })).is_ok()
    {
        app.ui.templates_revision = app.ui.templates_revision.wrapping_add(1);
    }
    if let Some(d) = set_default {
        app.act(ids::SECTION_DEFAULT_TEMPLATE, json!({ "template": d }));
    }
}
