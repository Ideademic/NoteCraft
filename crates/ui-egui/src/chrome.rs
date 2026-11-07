//! Window chrome: the accent-coloured title bar (app mark, Quick Access Toolbar, the title
//! "<page> - NoteCraft", window buttons when the app draws the frame) and the status bar (the
//! last notice, whether everything is saved, the zoom).

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use serde_json::json;

use crate::app::{QAT_BACK, QAT_CHOICES, QAT_FULL_PAGE, SaveState};
use crate::icons::{self, Icon};
use crate::theme::{fit_text, regular};
use crate::{NoteApp, tl};

/// The window's title: the page's, then the app's.
pub fn window_title(app: &NoteApp) -> String {
    match app.session.current_page() {
        Some(p) if !p.title.trim().is_empty() => format!("{} - NoteCraft", p.title.trim()),
        Some(_) => format!("{} - NoteCraft", tl!("Untitled page")),
        None => "NoteCraft".to_owned(),
    }
}

/// The title bar.
pub fn title_bar(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.title_bar);
    // The empty part of the strip moves the window (registered first: controls drawn after it
    // keep their own clicks).
    craft_window::title_bar::empty_area(ui, full);
    let h = full.height();
    let mut x = full.min.x + if app.integrated_titlebar { crate::app::MAC_CONTENT_LEFT } else { 8.0 };
    let mut right = full.max.x;
    if let Some(layout) = app.window_buttons.clone() {
        for b in &layout.left {
            window_button(ui, app, Rect::from_min_size(pos2(x, full.min.y), vec2(42.0, h)), *b);
            x += 42.0;
        }
        for b in layout.right.iter().rev() {
            right -= 46.0;
            window_button(ui, app, Rect::from_min_size(pos2(right, full.min.y), vec2(46.0, h)), *b);
        }
    }
    if !app.integrated_titlebar {
        let mark = Rect::from_center_size(pos2(x + 9.0, full.center().y), vec2(18.0, 18.0));
        icons::paint_app_mark(ui.painter(), mark);
        x += 26.0;
    }
    // Quick Access Toolbar.
    let qat: Vec<String> = app.ui.prefs.qat.clone();
    for (i, id) in qat.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, full.min.y + 3.0), vec2(28.0, h - 6.0));
        x += 29.0;
        qat_button(ui, app, r, id, i);
    }
    let r = Rect::from_min_size(pos2(x, full.min.y + 3.0), vec2(20.0, h - 6.0));
    x += 22.0;
    let resp = ui.interact(r, ui.scope_id().with("qat_more"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Customize Quick Access Toolbar")));
    hover_fill(ui, r, &resp, &t);
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(12.0, 12.0)), Icon::More, icons::Style::mono(t.on_title_bar));
    let resp = resp.on_hover_text(tl!("Customize Quick Access Toolbar"));
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(220.0);
        crate::widgets::menu_heading(ui, tl!("Customize Quick Access Toolbar"));
        for id in QAT_CHOICES {
            let on = app.ui.prefs.qat.iter().any(|q| q == id);
            if crate::widgets::check_item(ui, on, qat_label(id)) {
                if on {
                    app.ui.prefs.qat.retain(|q| q != id);
                } else {
                    app.ui.prefs.qat.push(id.to_owned());
                    let order = |q: &String| QAT_CHOICES.iter().position(|c| *c == q.as_str()).unwrap_or(usize::MAX);
                    app.ui.prefs.qat.sort_by_key(order);
                }
            }
        }
    });
    // Full Page View: the way back is on the title bar.
    if app.ui.full_page_view {
        right -= 34.0;
        let r = Rect::from_min_size(pos2(right, full.min.y + 3.0), vec2(30.0, h - 6.0));
        let resp = ui.interact(r, ui.scope_id().with("exit_full_page"), Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Normal View")));
        hover_fill(ui, r, &resp, &t);
        icons::paint_styled(
            ui.painter(),
            Rect::from_center_size(r.center(), vec2(16.0, 16.0)),
            Icon::FullPageView,
            icons::Style::mono(t.on_title_bar),
        );
        if resp.on_hover_text(tl!("Normal View (F11)")).clicked() {
            app.ui.full_page_view = false;
        }
    }
    // The title, centred on the window where it fits, else in the room between.
    let title = window_title(app);
    let font = regular(12.5);
    let room = (right - x - 16.0).max(0.0);
    let shown = fit_text(ui.ctx(), &title, &font, room.min(full.width() * 0.6));
    let w = crate::theme::text_width(ui.ctx(), &shown, font.clone());
    let cx = full.center().x.clamp(x + 8.0 + w / 2.0, (right - 8.0 - w / 2.0).max(x + 8.0 + w / 2.0));
    if !shown.is_empty() {
        ui.painter().text(pos2(cx, full.center().y), Align2::CENTER_CENTER, shown, font, t.on_title_bar);
    }
    // The OS window title (task bar, window lists), only when it changes. Not on the web: the
    // web shell sets `document.title` itself (eframe's web runner would log the command as
    // unhandled).
    if cfg!(target_arch = "wasm32") {
        return;
    }
    let key = egui::Id::unique("nc_window_title");
    if ui.data(|d| d.get_temp::<String>(key)).as_deref() != Some(title.as_str()) {
        ui.data_mut(|d| d.insert_temp(key, title.clone()));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(title));
    }
}

fn hover_fill(ui: &Ui, r: Rect, resp: &egui::Response, t: &crate::Tokens) {
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(r, 2.0, t.title_pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, t.title_hover);
    }
}

/// A Quick Access Toolbar entry's name.
pub fn qat_label(id: &str) -> &'static str {
    match id {
        QAT_BACK => tl!("Back"),
        notecraft_engine::ids::EDIT_UNDO => tl!("Undo"),
        notecraft_engine::ids::EDIT_REDO => tl!("Redo"),
        notecraft_engine::ids::PAGE_NEW => tl!("New Page"),
        QAT_FULL_PAGE => tl!("Full Page View"),
        _ => "",
    }
}

fn qat_button(ui: &mut Ui, app: &mut NoteApp, r: Rect, id: &str, index: usize) {
    let t = app.tokens.clone();
    let (icon, enabled) = match id {
        QAT_BACK => (Icon::Back, app.can_go_back()),
        QAT_FULL_PAGE => (Icon::FullPageView, true),
        notecraft_engine::ids::EDIT_UNDO => (Icon::Undo, app.can(id)),
        notecraft_engine::ids::EDIT_REDO => (Icon::Redo, app.can(id)),
        notecraft_engine::ids::PAGE_NEW => (Icon::NewPage, app.can(id)),
        _ => return,
    };
    let label = qat_label(id);
    let resp = ui.interact(r, ui.scope_id().with(("qat", id)), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    app.ui.keytips.tab(r, &(index + 1).to_string());
    if enabled {
        hover_fill(ui, r, &resp, &t);
    }
    let color = if enabled { t.on_title_bar } else { crate::theme::mix(t.on_title_bar, t.title_bar, 0.55) };
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(16.0, 16.0)), icon, icons::Style::mono(color));
    let key = notecraft_engine::registry().get(id).and_then(|s| s.shortcut).map(crate::widgets::shortcut_text);
    let tip = match key {
        Some(k) => format!("{label} ({k})"),
        None if id == QAT_FULL_PAGE => format!("{label} (F11)"),
        None => label.to_owned(),
    };
    if resp.on_hover_text(tip).clicked() && enabled {
        match id {
            QAT_BACK => app.go_back(),
            QAT_FULL_PAGE => app.ui.full_page_view = !app.ui.full_page_view,
            _ => app.act(id, json!({})),
        }
    }
}

fn window_button(ui: &mut Ui, app: &NoteApp, r: Rect, b: craft_window::Button) {
    let t = &app.tokens;
    let maximized = ui.input(|i| i.viewport().maximized).unwrap_or(false);
    let (icon, label, cmd) = match b {
        craft_window::Button::Minimize => (Icon::Minimize, tl!("Minimize"), egui::ViewportCommand::Minimized(true)),
        craft_window::Button::Maximize if maximized => (Icon::Restore, tl!("Restore Down"), egui::ViewportCommand::Maximized(false)),
        craft_window::Button::Maximize => (Icon::Maximize, tl!("Maximize"), egui::ViewportCommand::Maximized(true)),
        craft_window::Button::Close => (Icon::WindowClose, tl!("Close"), egui::ViewportCommand::Close),
    };
    let resp = ui.interact(r, ui.scope_id().with(("window_button", label)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if resp.hovered() {
        let fill = if b == craft_window::Button::Close { t.close_hover } else { t.title_hover };
        ui.painter().rect_filled(r, 0.0, fill);
    }
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(12.0, 12.0)), icon, icons::Style::mono(t.on_title_bar));
    if resp.on_hover_text(label).clicked() {
        ui.ctx().send_viewport_cmd(cmd);
    }
}

/// Height of the status bar.
pub const STATUS_BAR_H: f32 = 22.0;

/// What the status bar says about saving.
pub fn save_text(s: &SaveState) -> String {
    match s {
        SaveState::Saved => tl!("All changes saved").to_owned(),
        SaveState::Saving => tl!("Saving…").to_owned(),
        SaveState::NotSaved => tl!("Not saved yet").to_owned(),
        SaveState::Failed(e) => crate::i18n::fmt_plain(tl!("Couldn't save: {reason}"), &[("reason", e)]),
    }
}

/// The status bar: the last message (or where you are), the save state, the zoom.
pub fn status_bar(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.status_bar);
    ui.painter().line_segment([full.left_top(), full.right_top()], egui::Stroke::new(1.0, t.border));
    let font = regular(11.5);
    // Right: zoom out, zoom, zoom in; the save state.
    let mut x = full.max.x - 6.0;
    for (icon, label, which) in [(Icon::Plus, tl!("Zoom In"), 1), (Icon::Minimize, tl!("Zoom Out"), -1)] {
        let r = Rect::from_min_max(pos2(x - 20.0, full.min.y + 2.0), pos2(x, full.max.y - 2.0));
        x -= 20.0;
        if which == 1 {
            let pct = notecraft_geom::zoom::percent_label(app.canvas.zoom);
            let pw = crate::theme::text_width(ui.ctx(), &pct, font.clone()) + 12.0;
            let pr = Rect::from_min_max(pos2(r.min.x - pw, r.min.y), pos2(r.min.x, r.max.y));
            let presp = ui.interact(pr, ui.scope_id().with("zoom_pct"), Sense::click());
            presp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Zoom to 100%")));
            if presp.hovered() {
                ui.painter().rect_filled(pr, 2.0, t.hover);
            }
            ui.painter().text(pr.center(), Align2::CENTER_CENTER, &pct, font.clone(), t.text_dim);
            if presp.on_hover_text(tl!("Zoom to 100%")).clicked() {
                app.canvas.set_zoom(1.0);
            }
            let resp = small_status_button(ui, r, icon, label, &t);
            if resp.clicked() {
                app.canvas.set_zoom(notecraft_geom::zoom::zoom_in(app.canvas.zoom));
            }
            x = pr.min.x;
        } else {
            let resp = small_status_button(ui, r, icon, label, &t);
            if resp.clicked() {
                app.canvas.set_zoom(notecraft_geom::zoom::zoom_out(app.canvas.zoom));
            }
        }
    }
    x -= 14.0;
    let save = save_text(&app.ui.save_state);
    let save_color = if matches!(app.ui.save_state, SaveState::Failed(_)) { t.red } else { t.text_dim };
    let sr = ui.painter().text(pos2(x, full.center().y), Align2::RIGHT_CENTER, &save, font.clone(), save_color);
    let dot = pos2(sr.min.x - 8.0, full.center().y);
    let dot_color = match app.ui.save_state {
        SaveState::Saved => t.green,
        SaveState::Saving => t.yellow,
        SaveState::NotSaved => t.text_disabled,
        SaveState::Failed(_) => t.red,
    };
    ui.painter().circle_filled(dot, 3.5, dot_color);
    let left_room = (dot.x - 16.0 - full.min.x - 8.0).max(0.0);
    // Left: the message, or where you are.
    let (text, error) = match &app.ui.status {
        Some(s) => (s.clone(), app.ui.status_error),
        None => (location_text(app), false),
    };
    let mut lx = full.min.x + 8.0;
    if error {
        ui.painter().circle_filled(pos2(lx + 4.0, full.center().y), 4.0, t.red);
        lx += 14.0;
    }
    let shown = fit_text(ui.ctx(), &text, &font, left_room);
    let r = ui.painter().text(pos2(lx, full.center().y), Align2::LEFT_CENTER, &shown, font, if error { t.red } else { t.text_dim });
    let resp = ui.interact(r, ui.scope_id().with("status_text"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
    if app.ui.status.is_some() && resp.on_hover_text(tl!("Click to dismiss")).clicked() {
        app.ui.status = None;
        app.ui.status_error = false;
    }
}

fn small_status_button(ui: &mut Ui, r: Rect, icon: Icon, label: &str, t: &crate::Tokens) -> egui::Response {
    let resp = ui.interact(r, ui.scope_id().with(("status_btn", label)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if resp.hovered() {
        ui.painter().rect_filled(r, 2.0, t.hover);
    }
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(11.0, 11.0)), icon, icons::Style::mono(t.text_dim));
    resp.on_hover_text(label)
}

/// "Notebook › Section › Page" for the status bar.
pub fn location_text(app: &NoteApp) -> String {
    let mut parts = Vec::new();
    if let Some(n) = app.session.current_notebook() {
        parts.push(n.name.clone());
    }
    if let Some(s) = app.session.current_section() {
        parts.push(s.name.clone());
        let n = s.pages.len();
        if let Some(p) = app.session.current_page() {
            parts.push(if p.title.trim().is_empty() { tl!("Untitled page").to_owned() } else { p.title.clone() });
        }
        return format!(
            "{}   ·   {}",
            parts.join(" › "),
            if n == 1 { tl!("1 page").to_owned() } else { crate::i18n::fmt_plain(tl!("{n} pages"), &[("n", &n.to_string())]) }
        );
    }
    if parts.is_empty() { tl!("No notebook open").to_owned() } else { parts.join(" › ") }
}
