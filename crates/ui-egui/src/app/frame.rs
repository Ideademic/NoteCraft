//! The window frame, top to bottom: title bar ([`crate::chrome`]; in the ribbon's full-screen mode
//! a thin strip, [`crate::ribbon::display`]), ribbon tab strip and body ([`crate::ribbon`], as its
//! display options say), the body (notebook pane | section tabs over the page | page list,
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
    let normal_bar_h = if app.integrated_titlebar { MAC_TITLE_BAR_HEIGHT } else { TITLE_BAR_HEIGHT };
    // Full-screen mode: a thin strip stands in for the title bar (macOS keeps the height its
    // traffic lights need). A title bar with app-drawn window buttons stays, with a strip under it.
    let full_screen = !app.ui.backstage && !app.ui.full_page_view && ribbon::display::full_screen_bar(app);
    let strip_for_bar = full_screen && app.window_buttons.is_none();
    let bar_h = match (strip_for_bar, app.integrated_titlebar) {
        (true, false) => ribbon::display::FULL_SCREEN_BAR_H,
        _ => normal_bar_h,
    };
    let title = Panel::top("nc_title_bar").exact_size(bar_h).resizable(false).show_separator_line(false).frame(Frame::NONE.fill(t.title_bar)).show(
        ui,
        |ui| {
            if strip_for_bar {
                ribbon::display::full_screen_strip(ui, app);
            } else {
                chrome::title_bar(ui, app);
            }
        },
    );
    if app.ui.backstage {
        CentralPanel::default().frame(Frame::NONE.fill(t.panel)).show(ui, |ui| backstage::show(ui, app));
        finish(app, ui);
        return;
    }
    if !app.ui.full_page_view {
        let mut area = title.response.rect;
        if full_screen && !strip_for_bar {
            let r = Panel::top("nc_ribbon_reveal")
                .exact_size(ribbon::display::REVEAL_STRIP_H)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.tab_strip))
                .show(ui, |ui| ribbon::display::reveal_strip(ui, app));
            area = area.union(r.response.rect);
        }
        if ribbon::display::tabs_visible(app) {
            let r = Panel::top("nc_ribbon_tabs")
                .exact_size(ribbon::TAB_STRIP_H)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.tab_strip))
                .show(ui, |ui| ribbon::tab_strip(ui, app));
            area = area.union(r.response.rect);
        }
        if ribbon::body_visible(app) {
            let r = Panel::top("nc_ribbon")
                .exact_size(ribbon::body_h(app))
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.ribbon))
                .show(ui, |ui| ribbon::body(ui, app));
            area = area.union(r.response.rect);
        }
        ribbon::display::end_frame(ui, app, area);
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
        panes::history::fix_bin_view(app);
        if app.ui.recent_edits.is_some() {
            Panel::right("nc_recent_edits")
                .default_size(panes::history::RECENT_W)
                .size_range(200.0..=480.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.panel))
                .show(ui, |ui| panes::history::recent_edits_pane(ui, app));
        }
        if app.ui.review.spelling.is_some() {
            Panel::right("nc_spelling")
                .default_size(panes::spelling::PANE_W)
                .size_range(200.0..=420.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.panel))
                .show(ui, |ui| panes::spelling::pane(ui, app));
        }
        let tint = app.session.current_section().map_or(t.page_list, |s| t.section_tint(s.color));
        if app.ui.bin_view.is_some() {
            Panel::right("nc_bin_pages")
                .default_size(PAGE_LIST_WIDTH)
                .size_range(150.0..=480.0)
                .resizable(true)
                .show_separator_line(false)
                .frame(Frame::NONE.fill(t.page_list))
                .show(ui, |ui| panes::history::bin_page_list(ui, app));
        } else if app.ui.page_list_open {
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
        if app.ui.bin_view.is_some() {
            panes::history::bin_tabs(ui, app);
            panes::history::bin_page_banner(ui, app);
        } else {
            if !app.ui.full_page_view {
                panes::section_tabs(ui, app);
            }
            panes::history::banner(ui, app);
        }
        app.canvas_rect = Some(ui.available_rect_before_wrap());
        let mut tokens = app.tokens.clone();
        tokens.canvas = if app.ui.bin_view.is_some() { t.canvas } else { band };
        app.canvas.hide_title = app.ui.hide_page_title;
        if !panes::history::preview(ui, app, &tokens) {
            canvas::show(ui, &mut app.canvas, &mut app.session, &tokens, &app.services);
        }
    });
    finish(app, ui);
}

/// After the frame's panels: dialogs, KeyTips, bookkeeping.
fn finish(app: &mut NoteApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    panes::remember_page(&ctx, app);
    crate::review::dialogs(&ctx, app);
    prompt(&ctx, app);
    crate::ribbon::table::dialog(&ctx, app);
    crate::updates::show(app, &ctx);
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
            tl!("The section and its pages go to the Notebook Recycle Bin (History tab) for 60 days. You can undo this with Ctrl+Z.").to_owned(),
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
        Prompt::Confirm { title, body, yes, .. } => (title.clone(), body.clone(), yes.as_str(), Some(tl!("Cancel"))),
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
        Prompt::Confirm { command, params, .. } if yes => {
            app.act(&command, params);
            panes::history::fix_bin_view(app);
        }
        _ => {}
    }
}
