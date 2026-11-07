//! File › (Backstage): Info, New, Open, Export, Import, Options, About, with a back arrow, on a
//! flat accent-coloured rail like Office 2016's. Esc goes back to the notes.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::Guid;
use notecraft_engine::{Origin, ids};
use serde_json::json;

use crate::app::Prompt;
use crate::app::files::{self, ExportFormat, ExportScope};
use crate::icons::{self, Icon};
use crate::services::OpenPurpose;
use crate::theme::{Appearance, RADIUS_SMALL, fit_text, paint_strong, regular};
use crate::widgets::{self, flat_button};
use crate::{NoteApp, tl};

/// Width of the rail.
pub const RAIL_W: f32 = 190.0;

/// The backstage's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BackstagePage {
    #[default]
    Info,
    New,
    Open,
    Export,
    Import,
    Options,
    About,
}

impl BackstagePage {
    pub const ALL: [BackstagePage; 7] = [
        BackstagePage::Info,
        BackstagePage::New,
        BackstagePage::Open,
        BackstagePage::Export,
        BackstagePage::Import,
        BackstagePage::Options,
        BackstagePage::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BackstagePage::Info => tl!("Info"),
            BackstagePage::New => tl!("New"),
            BackstagePage::Open => tl!("Open"),
            BackstagePage::Export => tl!("Export"),
            BackstagePage::Import => tl!("Import"),
            BackstagePage::Options => tl!("Options"),
            BackstagePage::About => tl!("About"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            BackstagePage::Info => Icon::Info,
            BackstagePage::New => Icon::NewNotebook,
            BackstagePage::Open => Icon::Open,
            BackstagePage::Export => Icon::Export,
            BackstagePage::Import => Icon::Import,
            BackstagePage::Options => Icon::Options,
            BackstagePage::About => Icon::About,
        }
    }
}

/// Opens the backstage at `page`.
pub fn open(app: &mut NoteApp, page: BackstagePage) {
    app.ui.backstage = true;
    app.ui.backstage_page = page;
}

/// Back to the notes.
pub fn close(app: &mut NoteApp) {
    app.ui.backstage = false;
}

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.panel);
    let rail = Rect::from_min_max(full.min, pos2(full.min.x + RAIL_W, full.max.y));
    ui.painter().rect_filled(rail, 0.0, t.nav);
    // Back.
    let back = Rect::from_min_size(rail.min + vec2(14.0, 14.0), vec2(36.0, 36.0));
    let resp = ui.interact(back, ui.scope_id().with("backstage_back"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Back to Notes")));
    if resp.hovered() {
        ui.painter().circle_filled(back.center(), 18.0, t.nav_hover);
    }
    icons::paint_styled(ui.painter(), back.shrink(4.0), Icon::Back, icons::Style::flat(t.on_nav));
    if resp.on_hover_text(tl!("Back (Esc)")).clicked() || (ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input()) {
        close(app);
    }
    let mut y = rail.min.y + 70.0;
    for page in BackstagePage::ALL {
        if page == BackstagePage::Options {
            y += 12.0;
            ui.painter().line_segment([pos2(rail.min.x + 20.0, y - 6.0), pos2(rail.max.x - 20.0, y - 6.0)], Stroke::new(1.0, t.nav_hover));
        }
        let r = Rect::from_min_size(pos2(rail.min.x, y), vec2(RAIL_W, 40.0));
        y += 40.0;
        let resp = ui.interact(r, ui.scope_id().with(("backstage_page", page.label())), Sense::click());
        let selected = app.ui.backstage_page == page;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, page.label()));
        if selected {
            ui.painter().rect_filled(r, 0.0, t.nav_selected);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.nav_hover);
        }
        icons::paint_styled(
            ui.painter(),
            Rect::from_center_size(pos2(r.min.x + 30.0, r.center().y), vec2(18.0, 18.0)),
            page.icon(),
            // White glyphs: accents clash with (or vanish into) the purple rail.
            icons::Style::flat(t.on_nav),
        );
        let at = pos2(r.min.x + 50.0, r.center().y);
        if selected {
            paint_strong(ui.painter(), at, Align2::LEFT_CENTER, page.label(), regular(14.5), t.on_nav);
        } else {
            ui.painter().text(at, Align2::LEFT_CENTER, page.label(), regular(14.5), t.on_nav);
        }
        if resp.clicked() {
            app.ui.backstage_page = page;
        }
    }
    let content = Rect::from_min_max(pos2(rail.max.x + 40.0, full.min.y + 24.0), pos2(full.max.x - 32.0, full.max.y - 16.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
        egui::ScrollArea::vertical().id_salt("nc_backstage_page").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            match app.ui.backstage_page {
                BackstagePage::Info => info(ui, app),
                BackstagePage::New => new_notebook(ui, app),
                BackstagePage::Open => open_page(ui, app),
                BackstagePage::Export => export(ui, app),
                BackstagePage::Import => import(ui, app),
                BackstagePage::Options => options(ui, app),
                BackstagePage::About => about(ui, app),
            }
        });
    });
}

fn heading(ui: &mut Ui, text: &str) {
    let t = crate::Tokens::get(ui.ctx());
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).font(regular(30.0)).color(t.text));
    ui.add_space(10.0);
}

fn subheading(ui: &mut Ui, text: &str) {
    let t = crate::Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text).font(regular(17.0)).color(t.accent_text));
}

fn dim(ui: &mut Ui, text: &str) {
    let t = crate::Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(text).font(regular(12.5)).color(t.text_dim));
}

/// Where a notebook lives, for people.
pub fn location_label(origin: Option<&Origin>) -> String {
    match origin {
        Some(Origin::Folder(p) | Origin::Package(p) | Origin::SectionFile(p)) => p.display().to_string(),
        _ => tl!("Not saved to a folder yet").to_owned(),
    }
}

fn info(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    heading(ui, tl!("Notebook Information"));
    if app.session.notebooks.is_empty() {
        dim(ui, tl!("No notebooks are open. Create one with New, or open one with Open."));
        return;
    }
    let list: Vec<(Guid, String, notecraft_doc::Color, String, usize, usize)> = app
        .session
        .notebooks
        .iter()
        .map(|n| {
            let sections = n.all_sections();
            let pages = sections.iter().map(|s| s.pages.len()).sum();
            (n.id, n.name.clone(), n.color, location_label(app.session.origins.get(&n.id)), sections.len(), pages)
        })
        .collect();
    for (id, name, color, place, sections, pages) in list {
        let selected = app.ui.info_notebook == Some(id) || (app.ui.info_notebook.is_none() && app.session.current.notebook == Some(id));
        let w = ui.available_width().min(760.0);
        let (r, resp) = ui.allocate_exact_size(vec2(w, 74.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &name));
        let fill = if selected {
            t.checked
        } else if resp.hovered() {
            t.hover
        } else {
            t.input
        };
        ui.painter().rect(r, RADIUS_SMALL, fill, Stroke::new(1.0, if selected { t.checked_border } else { t.border }), egui::StrokeKind::Inside);
        icons::paint_notebook(ui.painter(), Rect::from_center_size(pos2(r.min.x + 34.0, r.center().y), vec2(40.0, 40.0)), t.section_fill(color));
        let tx = r.min.x + 66.0;
        let text_w = r.width() - 66.0 - 16.0;
        paint_strong(
            ui.painter(),
            pos2(tx, r.min.y + 18.0),
            Align2::LEFT_CENTER,
            &fit_text(ui.ctx(), &name, &regular(16.0), text_w),
            regular(16.0),
            t.text,
        );
        ui.painter().text(
            pos2(tx, r.min.y + 40.0),
            Align2::LEFT_CENTER,
            fit_text(ui.ctx(), &place, &regular(12.0), text_w),
            regular(12.0),
            t.text_dim,
        );
        let sections_text =
            if sections == 1 { tl!("1 section").to_owned() } else { crate::i18n::fmt_plain(tl!("{n} sections"), &[("n", &sections.to_string())]) };
        let pages_text = if pages == 1 { tl!("1 page").to_owned() } else { crate::i18n::fmt_plain(tl!("{n} pages"), &[("n", &pages.to_string())]) };
        let counts = format!("{sections_text} · {pages_text}");
        ui.painter().text(pos2(tx, r.min.y + 58.0), Align2::LEFT_CENTER, counts, regular(12.0), t.text_dim);
        if resp.clicked() {
            app.ui.info_notebook = Some(id);
        }
        if selected {
            notebook_settings(ui, app, id, &name, color);
        }
    }
}

/// Properties of the selected notebook: name, colour, close.
fn notebook_settings(ui: &mut Ui, app: &mut NoteApp, id: Guid, name: &str, color: notecraft_doc::Color) {
    let t = app.tokens.clone();
    egui::Frame::NONE.inner_margin(egui::Margin { left: 66, right: 0, top: 4, bottom: 12 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let label = ui.label(egui::RichText::new(tl!("Display name")).font(regular(12.5)).color(t.text_dim));
            let key = egui::Id::unique(("nc_info_name", id));
            let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| name.to_owned());
            let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(260.0).font(regular(13.0))).labelled_by(label.id);
            if resp.changed() {
                ui.data_mut(|d| d.insert_temp(key, text.clone()));
            }
            if (resp.lost_focus() || flat_button(ui, tl!("Rename"), false, text.trim() != name && !text.trim().is_empty())) && text.trim() != name {
                if !text.trim().is_empty() {
                    app.act(ids::NOTEBOOK_RENAME, json!({ "id": id, "name": text.trim() }));
                }
                ui.data_mut(|d| d.remove::<String>(key));
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("Color")).font(regular(12.5)).color(t.text_dim));
            let colors: Vec<egui::Color32> = notecraft_doc::Color::SECTION_PALETTE.iter().map(|c| crate::theme::color32(*c)).collect();
            let current = Some(crate::theme::color32(color));
            for c in colors {
                if widgets::color::swatch(ui, c, current == Some(c), &t) {
                    app.act(ids::NOTEBOOK_COLOR, json!({ "id": id, "color": widgets::color::hex(c) }));
                }
            }
        });
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("Close This Notebook"), false, true) {
                app.ui.prompt = Some(Prompt::CloseNotebook { id, name: name.to_owned() });
            }
            if flat_button(ui, tl!("Go to Notebook"), false, app.session.current.notebook != Some(id)) {
                app.act(ids::NAV_NOTEBOOK, json!({ "id": id }));
                close(app);
            }
        });
    });
}

fn new_notebook(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    heading(ui, tl!("New Notebook"));
    let label = ui.label(egui::RichText::new(tl!("Notebook Name")).font(regular(13.0)).color(t.text));
    let resp = ui.add(
        egui::TextEdit::singleline(&mut app.ui.new_notebook_name)
            .hint_text(tl!("My Notebook"))
            .desired_width(360.0)
            .font(regular(15.0))
            .margin(vec2(8.0, 6.0)),
    );
    let resp = resp.labelled_by(label.id);
    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.add_space(6.0);
    let desktop = app.services.pick_folder.is_some();
    if desktop {
        ui.label(egui::RichText::new(tl!("Location")).font(regular(13.0)).color(t.text));
        let dir = app.ui.new_notebook_dir.clone().or_else(|| app.ui.prefs.notebooks_dir.clone());
        ui.horizontal(|ui| {
            let shown = dir.clone().unwrap_or_else(|| tl!("Documents › NoteCraft Notebooks").to_owned());
            ui.label(egui::RichText::new(shown).font(regular(12.5)).color(t.text_dim));
            if flat_button(ui, tl!("Browse…"), false, true)
                && let Some(p) = app.services.pick_folder.as_ref().and_then(|f| f())
            {
                app.ui.new_notebook_dir = Some(p.to_string_lossy().into_owned());
            }
        });
    } else {
        dim(ui, tl!("The notebook is kept in this browser. Export it from File › Export to keep a copy."));
    }
    ui.add_space(10.0);
    if flat_button(ui, tl!("Create Notebook"), true, true) || enter {
        files::create_notebook(app);
    }
}

fn open_page(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    heading(ui, tl!("Open Notebook"));
    ui.horizontal(|ui| {
        if flat_button(ui, tl!("Open Notebook…"), true, true) {
            files::pick_and_open(app, OpenPurpose::Notebook);
        }
        if app.services.pick_folder.is_some() && flat_button(ui, tl!("Open Notebook Folder…"), false, true) {
            files::pick_and_open(app, OpenPurpose::NotebookFolder);
        }
    });
    dim(ui, tl!("A notebook folder, its Open Notebook.onetoc2, a .onepkg package or a single .one section."));
    subheading(ui, tl!("Recent Notebooks"));
    // The engine keeps the list (`Session::recent`); the prefs remember each one's colour.
    let recent: Vec<(String, String)> = app.session.recent.entries.iter().map(|e| (e.name.clone(), e.path.to_string_lossy().into_owned())).collect();
    if recent.is_empty() {
        dim(ui, tl!("Notebooks you open appear here."));
        return;
    }
    let mut remove = None;
    for (name, path) in &recent {
        let w = ui.available_width().min(760.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 48.0), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
        if resp.hovered() {
            ui.painter().rect_filled(rect, RADIUS_SMALL, t.hover);
        }
        let color = app
            .ui
            .prefs
            .recent
            .iter()
            .find(|r| r.path == *path)
            .and_then(|r| r.color.as_deref())
            .and_then(notecraft_doc::Color::parse_hex)
            .map_or(t.accent, |c| t.section_fill(c));
        icons::paint_notebook(ui.painter(), Rect::from_center_size(pos2(rect.min.x + 22.0, rect.center().y), vec2(28.0, 28.0)), color);
        let tw = rect.width() - 60.0;
        ui.painter().text(
            pos2(rect.min.x + 48.0, rect.min.y + 15.0),
            Align2::LEFT_CENTER,
            fit_text(ui.ctx(), name, &regular(14.0), tw),
            regular(14.0),
            t.text,
        );
        ui.painter().text(
            pos2(rect.min.x + 48.0, rect.min.y + 34.0),
            Align2::LEFT_CENTER,
            fit_text(ui.ctx(), path, &regular(11.5), tw),
            regular(11.5),
            t.text_dim,
        );
        if resp.clicked() {
            files::open_path(app, path);
        }
        resp.context_menu(|ui| {
            if widgets::menu_item(ui, Some(Icon::Close), tl!("Remove from List"), true) {
                remove = Some(path.clone());
            }
        });
    }
    if let Some(p) = remove {
        app.session.recent.remove(std::path::Path::new(&p));
        app.ui.prefs.recent.retain(|r| r.path != p);
    }
}

fn choice_row<T: Copy + PartialEq>(ui: &mut Ui, key: &str, options: &[(T, &str, Icon)], default: T) -> T {
    let t = crate::Tokens::get(ui.ctx());
    let id = egui::Id::unique(("nc_export_choice", key));
    let mut current = ui.data(|d| d.get_temp::<usize>(id)).and_then(|i| options.get(i).map(|o| o.0)).unwrap_or(default);
    ui.horizontal(|ui| {
        for (i, (value, label, icon)) in options.iter().enumerate() {
            let selected = *value == current;
            let (r, resp) = ui.allocate_exact_size(vec2(150.0, 84.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, *label));
            let fill = if selected {
                t.checked
            } else if resp.hovered() {
                t.hover
            } else {
                t.input
            };
            ui.painter().rect(r, RADIUS_SMALL, fill, Stroke::new(1.0, if selected { t.checked_border } else { t.border }), egui::StrokeKind::Inside);
            icons::paint_styled(
                ui.painter(),
                Rect::from_center_size(pos2(r.center().x, r.min.y + 30.0), vec2(32.0, 32.0)),
                *icon,
                icons::Style::new(t.icon, t.accent),
            );
            ui.painter().text(pos2(r.center().x, r.max.y - 18.0), Align2::CENTER_CENTER, *label, regular(13.0), t.text);
            if resp.clicked() {
                current = *value;
                ui.data_mut(|d| d.insert_temp(id, i));
            }
        }
    });
    current
}

fn export(ui: &mut Ui, app: &mut NoteApp) {
    heading(ui, tl!("Export"));
    subheading(ui, tl!("1. Export Current"));
    let scope = choice_row(
        ui,
        "scope",
        &[
            (ExportScope::Page, tl!("Page"), Icon::Page),
            (ExportScope::Section, tl!("Section"), Icon::NewSection),
            (ExportScope::Notebook, tl!("Notebook"), Icon::Notebook),
        ],
        ExportScope::Page,
    );
    subheading(ui, tl!("2. Select Format"));
    let formats: Vec<(ExportFormat, &str, Icon)> = match scope {
        ExportScope::Notebook => vec![(ExportFormat::Onepkg, tl!("OneNote Package (.onepkg)"), Icon::Notebook)],
        _ => vec![
            (ExportFormat::Html, tl!("Web Page (.html)"), Icon::Export),
            (ExportFormat::Markdown, tl!("Markdown (.md)"), Icon::Export),
            (ExportFormat::Text, tl!("Plain Text (.txt)"), Icon::Export),
        ],
    };
    let default = formats.first().map_or(ExportFormat::Html, |f| f.0);
    let key = if scope == ExportScope::Notebook { "format_notebook" } else { "format_page" };
    let format = choice_row(ui, key, &formats, default);
    ui.add_space(12.0);
    let what = files::export_name(app, scope, format);
    dim(ui, &crate::i18n::fmt_plain(tl!("Saves {name}."), &[("name", &what)]));
    if flat_button(ui, tl!("Export"), true, app.session.current_notebook().is_some()) {
        files::export(app, scope, format);
    }
}

fn import(ui: &mut Ui, app: &mut NoteApp) {
    heading(ui, tl!("Import"));
    dim(ui, tl!("Bring in a OneNote package (.onepkg) as a notebook, or a single section (.one)."));
    ui.add_space(6.0);
    if flat_button(ui, tl!("Browse…"), true, true) {
        files::pick_and_open(app, OpenPurpose::Import);
    }
}

fn options(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    heading(ui, tl!("NoteCraft Options"));
    subheading(ui, tl!("Appearance"));
    ui.horizontal(|ui| {
        for a in Appearance::ALL {
            if ui.radio(app.ui.prefs.appearance == a, a.label()).clicked() {
                app.ui.prefs.appearance = a;
            }
        }
    });
    subheading(ui, tl!("Saving"));
    ui.checkbox(&mut app.ui.prefs.autosave, tl!("Save changes automatically"));
    dim(
        ui,
        tl!(
            "NoteCraft has no Save button: changes are saved a moment after you make them. With this off, they are saved when you close a notebook or quit."
        ),
    );
    if app.services.pick_folder.is_some() {
        subheading(ui, tl!("Default Notebook Location"));
        ui.horizontal(|ui| {
            let shown = app.ui.prefs.notebooks_dir.clone().unwrap_or_else(|| tl!("Documents › NoteCraft Notebooks").to_owned());
            ui.label(egui::RichText::new(shown).font(regular(12.5)).color(t.text_dim));
            if flat_button(ui, tl!("Browse…"), false, true)
                && let Some(p) = app.services.pick_folder.as_ref().and_then(|f| f())
            {
                app.ui.prefs.notebooks_dir = Some(p.to_string_lossy().into_owned());
            }
            if app.ui.prefs.notebooks_dir.is_some() && flat_button(ui, tl!("Use Default"), false, true) {
                app.ui.prefs.notebooks_dir = None;
            }
        });
    }
    subheading(ui, tl!("Ribbon"));
    crate::ribbon::display::options(ui, app);
    subheading(ui, tl!("Recent Notebooks"));
    if flat_button(ui, tl!("Clear the List"), false, !app.session.recent.entries.is_empty()) {
        app.session.recent.entries.clear();
        app.ui.prefs.recent.clear();
    }
}

/// The About page's text (also checked by tests).
pub const CLEAN_ROOM: &str = "NoteCraft is written from scratch in Rust. It reads and writes OneNote files from Microsoft's published Open Specifications (MS-ONESTORE, MS-ONE) and observed behaviour only; it contains no Microsoft code, icons, images or other assets. OneNote and Office are trademarks of Microsoft; NoteCraft is not affiliated with or endorsed by Microsoft.";

fn about(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        ui.painter().rect_filled(r, 6.0, t.accent);
        icons::paint_app_mark(ui.painter(), r.shrink(12.0));
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(format!("NoteCraft {}", env!("CARGO_PKG_VERSION"))).font(regular(28.0)).color(t.text));
            ui.label(egui::RichText::new(tl!("A community Craft app by Ideademic")).font(regular(14.0)).color(t.accent_text));
        });
    });
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(tl!("Made to fit in with ArtCraft's Crafting Apps: WordCraft, GridCraft, DeckCraft and friends.")).font(regular(13.5)),
    );
    ui.add_space(4.0);
    ui.label(egui::RichText::new(crate::i18n::t(CLEAN_ROOM)).font(regular(12.5)).color(t.text_dim));
    subheading(ui, tl!("Licence"));
    dim(
        ui,
        tl!(
            "NoteCraft is free software under the MIT or Apache-2.0 licence, at your option. It is built with egui and the Craft libraries, under their own permissive licences (MIT, Apache-2.0, BSD, ISC, OFL)."
        ),
    );
    subheading(ui, tl!("Source"));
    ui.hyperlink_to("github.com/Ideademic/NoteCraft", "https://github.com/Ideademic/NoteCraft");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_back() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        open(&mut app, BackstagePage::Export);
        assert!(app.ui.backstage && app.ui.backstage_page == BackstagePage::Export);
        close(&mut app);
        assert!(!app.ui.backstage);
        assert!(CLEAN_ROOM.contains("Open Specifications"));
        assert_eq!(location_label(None), "Not saved to a folder yet");
    }
}
