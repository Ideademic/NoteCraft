//! The body's panes: the notebook pane (notebooks ▸ section groups ▸ sections, colour chips) on
//! the left, the coloured section tabs with the search box across the top of the page area,
//! and the page list (subpages indented, Add Page) on the right. Phase 2 (shell agent).

use notecraft_doc::SectionEntry;
use serde_json::json;

use crate::NoteApp;
use crate::tl;

pub fn notebook_pane(ui: &mut egui::Ui, app: &mut NoteApp) {
    let mut go = None;
    for nb in &app.session.notebooks {
        if ui.selectable_label(app.session.current.notebook == Some(nb.id), nb.name.as_str()).clicked() {
            go = Some(nb.id);
        }
    }
    if let Some(id) = go {
        let _ = app.run(notecraft_engine::ids::NAV_NOTEBOOK, json!({"id": id}));
    }
}

pub fn section_tabs(ui: &mut egui::Ui, app: &mut NoteApp) {
    let mut go = None;
    ui.horizontal(|ui| {
        if let Some(nb) = app.session.current_notebook() {
            for e in &nb.sections {
                if let SectionEntry::Section(s) = e
                    && ui.selectable_label(app.session.current.section == Some(s.id), s.name.as_str()).clicked()
                {
                    go = Some(s.id);
                }
            }
        }
    });
    if let Some(id) = go {
        let _ = app.run(notecraft_engine::ids::NAV_SECTION, json!({"id": id}));
    }
}

pub fn page_list(ui: &mut egui::Ui, app: &mut NoteApp) {
    if ui.button(tl!("+ Add Page")).clicked() {
        let _ = app.run(notecraft_engine::ids::PAGE_NEW, serde_json::Value::Null);
    }
    let mut go = None;
    if let Some(sec) = app.session.current_section() {
        for p in &sec.pages {
            let label = format!("{}{}", "    ".repeat(usize::from(p.level)), p.display_title());
            if ui.selectable_label(app.session.current.page == Some(p.id), label).clicked() {
                go = Some(p.id);
            }
        }
    }
    if let Some(id) = go {
        let _ = app.run(notecraft_engine::ids::NAV_PAGE, json!({"id": id}));
    }
}
