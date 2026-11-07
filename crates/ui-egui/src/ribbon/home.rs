//! Home: Clipboard, Basic Text, Styles, Tags.
//!
//! Formatting buttons aim at what the canvas is editing ([`crate::widgets::selection_params`]);
//! with no note container being edited they say so in the status bar.

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::ParaStyle;
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, regular};
use crate::widgets::{self, Btn, CONTENT_H, color};
use crate::{NoteApp, tl};

/// Fonts offered in the font box (names only: the text keeps the name, NoteCraft draws it with
/// the closest face it has).
pub const FONTS: [&str; 10] =
    ["Calibri", "Arial", "Cambria", "Consolas", "Courier New", "Georgia", "Segoe UI", "Tahoma", "Times New Roman", "Verdana"];
pub const SIZES: [f32; 16] = [8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 48.0, 72.0];

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Clipboard"), Icon::Paste, clipboard);
    widgets::group(ui, app, tl!("Basic Text"), Icon::Bold, basic_text);
    widgets::group(ui, app, tl!("Styles"), Icon::Bullets, styles);
    widgets::group(ui, app, tl!("Tags"), Icon::ToDo, tags);
}

/// Runs a formatting command on what the canvas is editing.
pub fn format(app: &mut NoteApp, id: &str, extra: Value) {
    if app.canvas.editing.is_none() {
        app.set_status(tl!("Click in a note first: formatting applies to the text you're editing."), false);
        return;
    }
    let params = widgets::selection_params(app, extra);
    app.act(id, params);
}

/// The selection's formatting (`text.formatAt`) while a note is being edited: each key is the
/// common value or `"mixed"`.
pub fn format_state(app: &mut NoteApp) -> Option<Value> {
    app.canvas.editing?;
    let params = widgets::selection_params(app, json!({}));
    app.session.run("text.formatAt", &params).ok().map(|o| o.value)
}

fn is_on(state: &Option<Value>, key: &str) -> bool {
    state.as_ref().and_then(|v| v.get(key)).and_then(Value::as_bool).unwrap_or(false)
}

fn editing_or_selected(app: &NoteApp) -> bool {
    app.canvas.editing.is_some() || !app.canvas.selection.is_empty()
}

fn clipboard(ui: &mut Ui, app: &mut NoteApp) {
    let paste = Btn::new(Icon::Paste, tl!("Paste")).shortcut(widgets::shortcut_text("Mod+V")).enabled(app.session.current_page().is_some());
    if widgets::big_split(ui, app, paste, |ui, app| {
        if widgets::menu_item(ui, Some(Icon::Paste), tl!("Paste"), true) {
            paste_now(ui, app, true);
        }
        if widgets::menu_item(ui, Some(Icon::ClearFormat), tl!("Keep Text Only"), true) {
            paste_now(ui, app, false);
        }
    }) {
        paste_now(ui, app, true);
    }
    widgets::rows(ui, app, |ui, app| {
        let on = editing_or_selected(app);
        if widgets::small(ui, app, Btn::new(Icon::Cut, tl!("Cut")).shortcut(widgets::shortcut_text("Mod+X")).enabled(on)) {
            ui.ctx().input_mut(|i| i.events.push(egui::Event::Cut));
        }
        if widgets::small(ui, app, Btn::new(Icon::Copy, tl!("Copy")).shortcut(widgets::shortcut_text("Mod+C")).enabled(on)) {
            ui.ctx().input_mut(|i| i.events.push(egui::Event::Copy));
        }
        let painter = Btn::new(Icon::FormatPainter, tl!("Format Painter")).enabled(false).help(tl!("Coming in a later version of NoteCraft."));
        widgets::small(ui, app, painter);
    });
}

/// Paste: text goes to the canvas as a paste event (it inserts it where the caret is); a picture
/// on the clipboard becomes a picture on the page.
fn paste_now(ui: &Ui, app: &mut NoteApp, with_pictures: bool) {
    if let Some(text) = app.services.clipboard_text.as_ref().and_then(|f| f()) {
        ui.ctx().input_mut(|i| i.events.push(egui::Event::Paste(text)));
        return;
    }
    if with_pictures && let Some(img) = app.services.clipboard_image.as_ref().and_then(|f| f()) {
        match crate::app::files::png_from_rgba(img.width, img.height, &img.rgba) {
            Some(png) => crate::app::files::insert_bytes(app, crate::services::OpenPurpose::Picture, tl!("Pasted picture.png"), &png),
            None => app.set_status(tl!("The picture on the clipboard couldn't be read."), true),
        }
        return;
    }
    let key = widgets::shortcut_text("Mod+V");
    app.set_status(crate::i18n::fmt_plain(tl!("Nothing to paste here. Press {key} to paste into a note."), &[("key", &key)]), false);
}

fn current_font(ui: &Ui) -> String {
    ui.data(|d| d.get_temp::<String>(egui::Id::unique("nc_font"))).unwrap_or_else(|| "Calibri".to_owned())
}

fn current_size(ui: &Ui) -> f32 {
    ui.data(|d| d.get_temp::<f32>(egui::Id::unique("nc_size"))).unwrap_or(11.0)
}

fn basic_text(ui: &mut Ui, app: &mut NoteApp) {
    let state = format_state(app);
    if let Some(st) = &state {
        if let Some(f) = st.get("font").and_then(Value::as_str).filter(|f| *f != "mixed") {
            ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_font"), f.to_owned()));
        }
        if let Some(sz) = st.get("size").and_then(Value::as_f64) {
            ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_size"), sz as f32));
        }
    }
    widgets::rows(ui, app, |ui, app| {
        widgets::row(ui, app, |ui, app| {
            ui.add_enabled_ui(app.can(ids::FORMAT_FONT), |ui| {
                let font = current_font(ui);
                let mut pick = None;
                let r = craft_ui::ComboBox::from_id_salt("nc_font_box").width(118.0).selected_text(font.clone()).show_ui(ui, |ui| {
                    for f in FONTS {
                        if ui.selectable_label(f == font, f).clicked() {
                            pick = Some(f);
                        }
                    }
                });
                r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Font")));
                r.response.on_hover_text(tl!("Font"));
                if let Some(f) = pick {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_font"), f.to_owned()));
                    format(app, ids::FORMAT_FONT, json!({ "font": f }));
                }
            });
            ui.add_enabled_ui(app.can(ids::FORMAT_SIZE), |ui| {
                let size = current_size(ui);
                let mut pick = None;
                let r = craft_ui::ComboBox::from_id_salt("nc_size_box").width(48.0).selected_text(size_text(size)).show_ui(ui, |ui| {
                    for s in SIZES {
                        if ui.selectable_label(s == size, size_text(s)).clicked() {
                            pick = Some(s);
                        }
                    }
                });
                r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Font Size")));
                r.response.on_hover_text(tl!("Font Size"));
                if let Some(s) = pick {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_size"), s));
                    format(app, ids::FORMAT_SIZE, json!({ "size": s }));
                }
            });
            ui.add_space(4.0);
            let list = state.as_ref().and_then(|v| v.get("list")).and_then(Value::as_str).unwrap_or("");
            let b = Btn::new(Icon::Bullets, tl!("Bullets")).checked(list == "bullet").for_command(app, ids::FORMAT_BULLETS);
            if widgets::small_split(ui, app, b, |ui, app| {
                for (glyph, name) in [("•", "disc"), ("o", "circle"), ("·", "square"), ("–", "dash")] {
                    if widgets::menu_item(ui, None, &format!("{glyph}   {}", bullet_name(name)), true) {
                        format(app, ids::FORMAT_BULLETS, json!({ "bullet": name }));
                    }
                }
                if widgets::menu_item(ui, None, tl!("None"), true) {
                    format(app, ids::FORMAT_BULLETS, json!({ "value": false }));
                }
            }) {
                format(app, ids::FORMAT_BULLETS, Value::Null);
            }
            let b = Btn::new(Icon::Numbering, tl!("Numbering")).checked(list == "numbered").for_command(app, ids::FORMAT_NUMBERING);
            if widgets::small_split(ui, app, b, |ui, app| {
                for (sample, name) in [("1. 2. 3.", "decimal"), ("a. b. c.", "lowerAlpha"), ("A. B. C.", "upperAlpha"), ("i. ii. iii.", "lowerRoman")]
                {
                    if widgets::menu_item(ui, None, sample, true) {
                        format(app, ids::FORMAT_NUMBERING, json!({ "format": name }));
                    }
                }
                if widgets::menu_item(ui, None, tl!("None"), true) {
                    format(app, ids::FORMAT_NUMBERING, json!({ "value": false }));
                }
            }) {
                format(app, ids::FORMAT_NUMBERING, Value::Null);
            }
            for (icon, label, id) in
                [(Icon::Outdent, tl!("Decrease Indent"), ids::FORMAT_OUTDENT), (Icon::Indent, tl!("Increase Indent"), ids::FORMAT_INDENT)]
            {
                if widgets::small(ui, app, Btn::new(icon, label).icon_only().for_command(app, id)) {
                    format(app, id, Value::Null);
                }
            }
            let clear = Btn::new(Icon::ClearFormat, tl!("Clear Formatting")).icon_only().for_command(app, "format.clear");
            if widgets::small(ui, app, clear) {
                format(app, "format.clear", Value::Null);
            }
        });
        ui.add_space(3.0);
        widgets::row(ui, app, |ui, app| {
            for (icon, label, id, key) in [
                (Icon::Bold, tl!("Bold"), ids::FORMAT_BOLD, "bold"),
                (Icon::Italic, tl!("Italic"), ids::FORMAT_ITALIC, "italic"),
                (Icon::Underline, tl!("Underline"), ids::FORMAT_UNDERLINE, "underline"),
                (Icon::Strike, tl!("Strikethrough"), ids::FORMAT_STRIKE, "strike"),
            ] {
                if widgets::small(ui, app, Btn::new(icon, label).icon_only().checked(is_on(&state, key)).for_command(app, id)) {
                    format(app, id, Value::Null);
                }
            }
            ui.add_space(4.0);
            let hl = remembered_color(ui, "nc_highlight", color::HIGHLIGHT[0]);
            let b = Btn::new(Icon::Highlight, tl!("Text Highlight Color")).icon_only().swatch(Some(hl)).for_command(app, ids::FORMAT_HIGHLIGHT);
            if widgets::small_split(ui, app, b, |ui, app| {
                widgets::menu_heading(ui, tl!("Highlight"));
                if let Some(c) = color::grid(ui, &color::HIGHLIGHT, 5, Some(hl)) {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_highlight"), c));
                    format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": color::hex(c) }));
                }
                if widgets::menu_item(ui, None, tl!("No Color"), true) {
                    format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": null }));
                }
            }) {
                format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": color::hex(hl) }));
            }
            let fc = remembered_color(ui, "nc_font_color", color::TEXT[2]);
            let b = Btn::new(Icon::FontColor, tl!("Font Color")).icon_only().swatch(Some(fc)).for_command(app, ids::FORMAT_COLOR);
            if widgets::small_split(ui, app, b, |ui, app| {
                widgets::menu_heading(ui, tl!("Colors"));
                if let Some(c) = color::grid(ui, &color::TEXT, 10, Some(fc)) {
                    ui.data_mut(|d| d.insert_temp(egui::Id::unique("nc_font_color"), c));
                    format(app, ids::FORMAT_COLOR, json!({ "color": color::hex(c) }));
                }
                if widgets::menu_item(ui, None, tl!("Automatic"), true) {
                    format(app, ids::FORMAT_COLOR, json!({ "color": null }));
                }
            }) {
                format(app, ids::FORMAT_COLOR, json!({ "color": color::hex(fc) }));
            }
            ui.add_space(4.0);
            let align = Btn::new(Icon::AlignLeft, tl!("Paragraph Alignment")).icon_only().for_command(app, "format.align");
            widgets::small_menu(ui, app, align, |ui, app| {
                for (icon, label, value) in [
                    (Icon::AlignLeft, tl!("Align Left"), "left"),
                    (Icon::AlignCenter, tl!("Center"), "center"),
                    (Icon::AlignRight, tl!("Align Right"), "right"),
                ] {
                    if widgets::menu_item(ui, Some(icon), label, true) {
                        format(app, "format.align", json!({ "align": value }));
                    }
                }
            });
        });
    });
}

fn bullet_name(name: &str) -> &'static str {
    match name {
        "disc" => tl!("Filled round bullet"),
        "circle" => tl!("Hollow round bullet"),
        "square" => tl!("Square bullet"),
        _ => tl!("Dash bullet"),
    }
}

fn size_text(s: f32) -> String {
    if s.fract() == 0.0 { format!("{s:.0}") } else { format!("{s}") }
}

fn remembered_color(ui: &Ui, key: &str, default: Color32) -> Color32 {
    ui.data(|d| d.get_temp::<Color32>(egui::Id::unique(key))).unwrap_or(default)
}

/// Styles in gallery order.
pub const STYLE_ORDER: [ParaStyle; 10] = [
    ParaStyle::Heading1,
    ParaStyle::Heading2,
    ParaStyle::Heading3,
    ParaStyle::Heading4,
    ParaStyle::Heading5,
    ParaStyle::Heading6,
    ParaStyle::Title,
    ParaStyle::Quote,
    ParaStyle::Code,
    ParaStyle::Normal,
];

pub fn style_label(s: ParaStyle) -> &'static str {
    match s {
        ParaStyle::Normal => tl!("Normal"),
        ParaStyle::Heading1 => tl!("Heading 1"),
        ParaStyle::Heading2 => tl!("Heading 2"),
        ParaStyle::Heading3 => tl!("Heading 3"),
        ParaStyle::Heading4 => tl!("Heading 4"),
        ParaStyle::Heading5 => tl!("Heading 5"),
        ParaStyle::Heading6 => tl!("Heading 6"),
        ParaStyle::Title => tl!("Page Title"),
        ParaStyle::Quote => tl!("Quote"),
        ParaStyle::Code => tl!("Code"),
    }
}

/// One style's row in the gallery: its name drawn in the style.
fn style_row(ui: &mut Ui, app: &mut NoteApp, s: ParaStyle, w: f32, enabled: bool) -> bool {
    let t = app.tokens.clone();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, style_label(s)));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    }
    let size = match s {
        ParaStyle::Heading1 | ParaStyle::Title => 14.5,
        ParaStyle::Heading2 => 13.5,
        ParaStyle::Heading3 | ParaStyle::Heading4 => 12.5,
        _ => 11.5,
    };
    let color = if !enabled {
        t.text_disabled
    } else {
        match s {
            ParaStyle::Heading1 | ParaStyle::Heading2 | ParaStyle::Title => t.accent_text,
            ParaStyle::Quote | ParaStyle::Heading5 | ParaStyle::Heading6 => t.text_dim,
            _ => t.text,
        }
    };
    let font = if s == ParaStyle::Code { egui::FontId::monospace(11.0) } else { regular(size) };
    let at = pos2(r.min.x + 6.0, r.center().y);
    if s.heading_level().is_some_and(|l| l <= 4) || s == ParaStyle::Title {
        crate::theme::paint_strong(ui.painter(), at, Align2::LEFT_CENTER, style_label(s), font, color);
    } else {
        ui.painter().text(at, Align2::LEFT_CENTER, style_label(s), font, color);
    }
    let go = app.ui.keytips.control(r);
    enabled && (resp.clicked() || go)
}

/// A small gallery: three rows visible, arrows to scroll, a drop-down with all of them.
fn gallery_frame(ui: &mut Ui, app: &mut NoteApp, key: &str, w: f32, count: usize, mut row: impl FnMut(&mut Ui, &mut NoteApp, usize, f32)) {
    let t = app.tokens.clone();
    let id = egui::Id::unique(("nc_gallery", key));
    let first = ui.data(|d| d.get_temp::<usize>(id)).unwrap_or(0).min(count.saturating_sub(3));
    let (outer, _) = ui.allocate_exact_size(vec2(w + 16.0, CONTENT_H - 2.0), Sense::hover());
    ui.painter().rect(outer, RADIUS_SMALL, t.input, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let list = Rect::from_min_max(outer.min + vec2(1.0, 2.0), pos2(outer.max.x - 16.0, outer.max.y - 2.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(list).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        for i in first..(first + 3).min(count) {
            row(ui, app, i, list.width());
        }
    });
    let col = Rect::from_min_max(pos2(outer.max.x - 16.0, outer.min.y), outer.max);
    ui.painter().line_segment([col.left_top(), col.left_bottom()], Stroke::new(1.0, t.border));
    let third = col.height() / 3.0;
    let arrows = [(Icon::ChevronUp, 0usize), (Icon::ChevronDown, 1), (Icon::More, 2)];
    for (icon, k) in arrows {
        let r = Rect::from_min_size(pos2(col.min.x, col.min.y + third * k as f32), vec2(col.width(), third));
        let resp = ui.interact(r, id.with(k), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r.shrink(1.0), RADIUS_SMALL, t.hover);
        }
        icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(10.0, 10.0)), icon, icons::Style::mono(t.icon));
        match k {
            0 if resp.clicked() => {
                ui.data_mut(|d| d.insert_temp(id, first.saturating_sub(1)));
            }
            1 if resp.clicked() => {
                ui.data_mut(|d| d.insert_temp(id, (first + 1).min(count.saturating_sub(3))));
            }
            2 => {
                let resp = resp.on_hover_text(tl!("More"));
                egui::Popup::menu(&resp).show(|ui| {
                    ui.set_min_width(w + 16.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    for i in 0..count {
                        row(ui, app, i, w + 16.0);
                    }
                });
            }
            _ => {}
        }
    }
}

fn styles(ui: &mut Ui, app: &mut NoteApp) {
    let enabled = app.can(ids::FORMAT_STYLE);
    gallery_frame(ui, app, "styles", 130.0, STYLE_ORDER.len(), |ui, app, i, w| {
        let Some(s) = STYLE_ORDER.get(i).copied() else { return };
        if style_row(ui, app, s, w, enabled) {
            format(app, ids::FORMAT_STYLE, json!({ "style": s.name() }));
            ui.close();
        }
    });
}

/// Tags in gallery order: (icon, label, shortcut, command, params).
fn tag_rows() -> [(Icon, &'static str, &'static str, &'static str, Value); 3] {
    [
        (Icon::ToDo, tl!("To Do"), "Mod+1", ids::FORMAT_TAG_TODO, Value::Null),
        (Icon::Important, tl!("Important"), "Mod+2", "format.tag", json!({"tag": "important"})),
        (Icon::Question, tl!("Question"), "Mod+3", "format.tag", json!({"tag": "question"})),
    ]
}

fn tags(ui: &mut Ui, app: &mut NoteApp) {
    let rows = tag_rows();
    gallery_frame(ui, app, "tags", 150.0, rows.len(), |ui, app, i, w| {
        let Some((icon, label, key, id, params)) = rows.get(i).cloned() else { return };
        let t = app.tokens.clone();
        let enabled = app.can(id);
        let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), if enabled { Sense::click() } else { Sense::hover() });
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        if enabled && resp.hovered() {
            ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
        }
        let style = if enabled { icons::Style::new(t.icon, t.accent) } else { icons::Style::mono(t.text_disabled) };
        icons::paint_styled(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(15.0, 15.0)), icon, style);
        let color = if enabled { t.text } else { t.text_disabled };
        ui.painter().text(pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.5), color);
        ui.painter().text(pos2(r.max.x - 4.0, r.center().y), Align2::RIGHT_CENTER, widgets::shortcut_text(key), regular(10.5), t.text_dim);
        let keyed = app.ui.keytips.control(r);
        let resp = if enabled { resp } else { resp.on_hover_text(tl!("Coming in a later version of NoteCraft.")) };
        if enabled && (resp.clicked() || keyed) {
            format(app, id, params);
            ui.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_is_listed_once() {
        for s in ParaStyle::ALL {
            assert_eq!(STYLE_ORDER.iter().filter(|x| **x == s).count(), 1, "{s:?}");
            assert!(!style_label(s).is_empty());
        }
    }

    #[test]
    fn formatting_without_a_note_says_so() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        format(&mut app, ids::FORMAT_BOLD, Value::Null);
        assert!(app.ui.status.as_deref().is_some_and(|s| s.contains("Click in a note")));
    }
}
