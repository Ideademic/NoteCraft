//! The window frame, top to bottom: title bar ([`crate::chrome`]), ribbon tab strip and body
//! ([`crate::ribbon`]), the body (notebook pane | section tabs over the page | page list,
//! [`crate::panes`]), the status bar. File swaps the body for the backstage; Full Page View keeps
//! only the title bar, the page and the status bar.

use egui::{CentralPanel, Frame, Panel, Ui};
use serde_json::json;

use super::{MAC_TITLE_BAR_HEIGHT, NOTEBOOK_BAR_WIDTH, NOTEBOOK_PANE_WIDTH, NoteApp, PAGE_LIST_WIDTH, Prompt, TITLE_BAR_HEIGHT};
use crate::theme::regular;
use crate::widgets::flat_button;
use crate::{backstage, canvas, chrome, panes, ribbon, tl};

pub fn show(app: &mut NoteApp, ui: &mut Ui) {
    let t = app.tokens.clone();
    let bar_h = if app.integrated_titlebar { MAC_TITLE_BAR_HEIGHT } else { TITLE_BAR_HEIGHT };
    Panel::top("nc_title_bar").exact_size(bar_h).resizable(false).show_separator_line(false).frame(Frame::NONE.fill(t.title_bar)).show(ui, |ui| {
        chrome::title_bar(ui, app);
    });
    if app.ui.backstage {
        CentralPanel::default().frame(Frame::NONE.fill(t.panel)).show(ui, |ui| backstage::show(ui, app));
        finish(app, ui);
        return;
    }
    if !app.ui.full_page_view {
        Panel::top("nc_ribbon_tabs")
            .exact_size(ribbon::TAB_STRIP_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(t.tab_strip))
            .show(ui, |ui| ribbon::tab_strip(ui, app));
        if ribbon::body_visible(app) {
            Panel::top("nc_ribbon").exact_size(ribbon::BODY_H).resizable(false).show_separator_line(false).frame(Frame::NONE.fill(t.ribbon)).show(
                ui,
                |ui| {
                    ribbon::body(ui, app);
                },
            );
        }
    }
    Panel::bottom("nc_status")
        .exact_size(chrome::STATUS_BAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE.fill(t.status_bar))
        .show(ui, |ui| chrome::status_bar(ui, app));
    if !app.ui.full_page_view {
        if app.ui.notebook_pane_open {
            Panel::left("nc_notebooks")
                .default_size(NOTEBOOK_PANE_WIDTH)
                .size_range(160.0..=480.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.notebook_pane))
                .show(ui, |ui| panes::notebook_pane(ui, app));
        } else {
            Panel::left("nc_notebook_bar")
                .exact_size(NOTEBOOK_BAR_WIDTH)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.notebook_pane))
                .show(ui, |ui| panes::notebook_bar(ui, app));
        }
        let tint = app.session.current_section().map_or(t.page_list, |s| t.section_tint(s.color));
        if app.ui.page_list_open {
            Panel::right("nc_pages")
                .default_size(PAGE_LIST_WIDTH)
                .size_range(150.0..=480.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(tint))
                .show(ui, |ui| panes::page_list(ui, app));
        } else {
            Panel::right("nc_page_bar").exact_size(28.0).resizable(false).show_separator_line(false).frame(Frame::NONE.fill(tint)).show(ui, |ui| {
                panes::page_bar(ui, app);
            });
        }
    }
    // The page area: section tabs, then the page on a band of the section's colour.
    let band = app.session.current_section().map_or(t.canvas, |s| t.section_tint(s.color));
    CentralPanel::default().frame(Frame::NONE.fill(band)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        if !app.ui.full_page_view {
            panes::section_tabs(ui, app);
        }
        app.canvas_rect = Some(ui.available_rect_before_wrap());
        let mut tokens = app.tokens.clone();
        tokens.canvas = band;
        app.canvas.hide_title = app.ui.hide_page_title;
        canvas::show(ui, &mut app.canvas, &mut app.session, &tokens, &app.services);
    });
    finish(app, ui);
}

/// After the frame's panels: dialogs, KeyTips, bookkeeping.
fn finish(app: &mut NoteApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    panes::remember_page(&ctx, app);
    prompt(&ctx, app);
    app.ui.keytips.paint(&ctx, &app.tokens);
    app.ui.keytips.end_frame();
}

/// The modal question, if one is up.
fn prompt(ctx: &egui::Context, app: &mut NoteApp) {
    let Some(p) = app.ui.prompt.clone() else { return };
    let t = app.tokens.clone();
    let (title, body, yes, no) = match &p {
        Prompt::DeleteSection { name, .. } => (
            crate::i18n::fmt_plain(tl!("Delete \u{201c}{name}\u{201d}?"), &[("name", name)]),
            tl!("The section and all its pages are deleted. You can undo this with Ctrl+Z.").to_owned(),
            tl!("Delete"),
            Some(tl!("Cancel")),
        ),
        Prompt::CloseNotebook { name, .. } => (
            crate::i18n::fmt_plain(tl!("Close \u{201c}{name}\u{201d}?"), &[("name", name)]),
            tl!("Changes are saved first. You can open it again from File › Open.").to_owned(),
            tl!("Close Notebook"),
            Some(tl!("Cancel")),
        ),
        Prompt::Recovery { names } => (
            tl!("Restore your notes?").to_owned(),
            crate::i18n::fmt_plain(
                tl!("NoteCraft didn't close properly last time. It kept recent changes to: {names}."),
                &[("names", &names.join(", "))],
            ),
            tl!("Restore"),
            Some(tl!("Discard")),
        ),
        Prompt::Message { title, body } => (title.clone(), body.clone(), tl!("OK"), None),
    };
    let mut answer: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_prompt")).backdrop_color(egui::Color32::TRANSPARENT).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(20.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(egui::RichText::new(&title).font(regular(17.0)).color(t.text));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(&body).font(regular(13.0)).color(t.text_dim));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if flat_button(ui, yes, true, true) {
                answer = Some(true);
            }
            if let Some(no) = no
                && flat_button(ui, no, false, true)
            {
                answer = Some(false);
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            answer = Some(true);
        }
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    let Some(yes) = answer else { return };
    app.ui.prompt = None;
    match p {
        Prompt::DeleteSection { id, .. } if yes => app.act(notecraft_engine::ids::SECTION_DELETE, json!({ "id": id })),
        Prompt::CloseNotebook { id, .. } if yes => {
            app.act(notecraft_engine::ids::NOTEBOOK_CLOSE, json!({ "id": id }));
            app.ui.info_notebook = None;
        }
        Prompt::Recovery { .. } => app.ui.recovery_answer = Some(yes),
        _ => {}
    }
}
