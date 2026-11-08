//! Simplified Home: Paste▾ (Cut, Copy, Format Painter inside) | font and size | B I U S,
//! Highlight▾, Font Color▾ | Bullets▾, Numbering▾, Outdent, Indent, Alignment▾, Clear Formatting |
//! Styles▾ | To Do, Tags▾.
//!
//! Formatting goes through [`crate::ribbon::home::format`], as on the classic tab, and the font,
//! size and colours shown are the ones the classic tab remembers.

use egui::{Align2, Color32, Sense, Ui, pos2, vec2};
use notecraft_doc::ParaStyle;
use notecraft_engine::ids;
use serde_json::{Value, json};

use super::Segment;
use crate::canvas::CurrentFormat;
use crate::icons::Icon;
use crate::ribbon::home::{SIZES, STYLE_ORDER, format, format_state, shown_size, style_label};
use crate::theme::{RADIUS_SMALL, regular};
use crate::widgets::{self, Btn, color};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 6] = [
    Segment { key: "clipboard", draw: clipboard },
    Segment { key: "font", draw: font },
    Segment { key: "text", draw: text },
    Segment { key: "paragraph", draw: paragraph },
    Segment { key: "styles", draw: styles },
    Segment { key: "tags", draw: tags },
];

fn is_on(state: Option<&CurrentFormat>, key: &str) -> bool {
    let Some(f) = state else { return false };
    let v = match key {
        "bold" => f.bold,
        "italic" => f.italic,
        "underline" => f.underline,
        "strike" => f.strike,
        _ => None,
    };
    v.unwrap_or(false)
}

fn editing_or_selected(app: &NoteApp) -> bool {
    app.canvas.editing.is_some() || !app.canvas.selection.is_empty()
}

/// Remembered between frames under the classic tab's keys, so both layouts agree.
fn remembered<T: Clone + Send + Sync + 'static>(ui: &Ui, key: &str, default: T) -> T {
    ui.data(|d| d.get_temp::<T>(egui::Id::unique(key))).unwrap_or(default)
}

fn remember<T: Clone + Send + Sync + 'static>(ui: &Ui, key: &str, v: T) {
    ui.data_mut(|d| d.insert_temp(egui::Id::unique(key), v));
}

fn clipboard(ui: &mut Ui, app: &mut NoteApp) {
    let paste = Btn::new(Icon::Paste, tl!("Paste")).shortcut(widgets::shortcut_text("Mod+V")).enabled(app.session.current_page().is_some());
    if widgets::small_split(ui, app, paste, |ui, app| {
        if widgets::menu_item(ui, Some(Icon::Paste), tl!("Paste"), true) {
            paste_now(ui, app, true);
        }
        if widgets::menu_item(ui, Some(Icon::ClearFormat), tl!("Keep Text Only"), true) {
            paste_now(ui, app, false);
        }
        ui.separator();
        let on = editing_or_selected(app);
        if widgets::menu_item(ui, Some(Icon::Cut), tl!("Cut"), on) {
            ui.ctx().input_mut(|i| i.events.push(egui::Event::Cut));
        }
        if widgets::menu_item(ui, Some(Icon::Copy), tl!("Copy"), on) {
            ui.ctx().input_mut(|i| i.events.push(egui::Event::Copy));
        }
        widgets::menu_item(ui, Some(Icon::FormatPainter), tl!("Format Painter"), false);
    }) {
        paste_now(ui, app, true);
    }
}

/// Paste, as on the classic tab: text goes to the canvas as a paste event; a picture on the
/// clipboard becomes a picture on the page.
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

fn size_text(s: f32) -> String {
    if s.fract() == 0.0 { format!("{s:.0}") } else { format!("{s}") }
}

fn font(ui: &mut Ui, app: &mut NoteApp) {
    let size = shown_size(app, ui);
    if let Some(sz) = size
        && format_state(app).is_some()
    {
        remember(ui, "nc_size", sz);
    }
    crate::ribbon::font_box::show(ui, app, "simplified", 104.0);
    ui.add_space(2.0);
    ui.add_enabled_ui(app.can(ids::FORMAT_SIZE), |ui| {
        let mut pick = None;
        let r =
            craft_ui::ComboBox::from_id_salt("nc_simple_size").width(44.0).selected_text(size.map(size_text).unwrap_or_default()).show_ui(ui, |ui| {
                for s in SIZES {
                    if ui.selectable_label(Some(s) == size, size_text(s)).clicked() {
                        pick = Some(s);
                    }
                }
            });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Font Size")));
        r.response.on_hover_text(tl!("Font Size"));
        if let Some(s) = pick {
            remember(ui, "nc_size", s);
            format(app, ids::FORMAT_SIZE, json!({ "size": s }));
        }
    });
}

fn text(ui: &mut Ui, app: &mut NoteApp) {
    let state = format_state(app);
    for (icon, label, id, key) in [
        (Icon::Bold, tl!("Bold"), ids::FORMAT_BOLD, "bold"),
        (Icon::Italic, tl!("Italic"), ids::FORMAT_ITALIC, "italic"),
        (Icon::Underline, tl!("Underline"), ids::FORMAT_UNDERLINE, "underline"),
        (Icon::Strike, tl!("Strikethrough"), ids::FORMAT_STRIKE, "strike"),
    ] {
        if widgets::small(ui, app, Btn::new(icon, label).icon_only().checked(is_on(state.as_ref(), key)).for_command(app, id)) {
            format(app, id, Value::Null);
        }
    }
    ui.add_space(3.0);
    let hl: Color32 = remembered(ui, "nc_highlight", color::HIGHLIGHT[0]);
    let b = Btn::new(Icon::Highlight, tl!("Text Highlight Color")).icon_only().swatch(Some(hl)).for_command(app, ids::FORMAT_HIGHLIGHT);
    if widgets::small_split(ui, app, b, |ui, app| {
        widgets::menu_heading(ui, tl!("Highlight"));
        if let Some(c) = color::grid(ui, &color::HIGHLIGHT, 5, Some(hl)) {
            remember(ui, "nc_highlight", c);
            format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": color::hex(c) }));
        }
        if widgets::menu_item(ui, None, tl!("No Color"), true) {
            format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": null }));
        }
    }) {
        format(app, ids::FORMAT_HIGHLIGHT, json!({ "color": color::hex(hl) }));
    }
    let fc: Color32 = remembered(ui, "nc_font_color", color::TEXT[2]);
    let b = Btn::new(Icon::FontColor, tl!("Font Color")).icon_only().swatch(Some(fc)).for_command(app, ids::FORMAT_COLOR);
    if widgets::small_split(ui, app, b, |ui, app| {
        widgets::menu_heading(ui, tl!("Colors"));
        if let Some(c) = color::grid(ui, &color::TEXT, 10, Some(fc)) {
            remember(ui, "nc_font_color", c);
            format(app, ids::FORMAT_COLOR, json!({ "color": color::hex(c) }));
        }
        if widgets::menu_item(ui, None, tl!("Automatic"), true) {
            format(app, ids::FORMAT_COLOR, json!({ "color": null }));
        }
    }) {
        format(app, ids::FORMAT_COLOR, json!({ "color": color::hex(fc) }));
    }
}

fn paragraph(ui: &mut Ui, app: &mut NoteApp) {
    crate::ribbon::paragraph::bullets(ui, app);
    crate::ribbon::paragraph::numbering(ui, app);
    for (icon, label, id) in
        [(Icon::Outdent, tl!("Decrease Indent"), ids::FORMAT_OUTDENT), (Icon::Indent, tl!("Increase Indent"), ids::FORMAT_INDENT)]
    {
        if widgets::small(ui, app, Btn::new(icon, label).icon_only().for_command(app, id)) {
            format(app, id, Value::Null);
        }
    }
    crate::ribbon::paragraph::alignment(ui, app);
    let clear = Btn::new(Icon::ClearFormat, tl!("Clear Formatting")).icon_only().for_command(app, "format.clear");
    if widgets::small(ui, app, clear) {
        format(app, "format.clear", Value::Null);
    }
}

/// One style in the Styles▾ list: its name drawn in the style, a check on the current one.
fn style_item(ui: &mut Ui, app: &NoteApp, s: ParaStyle, current: bool, enabled: bool) -> bool {
    let t = &app.tokens;
    let w = ui.available_width().max(180.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, current, style_label(s)));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    } else if current {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.checked);
    }
    let size = match s {
        ParaStyle::Heading1 | ParaStyle::Title => 15.0,
        ParaStyle::Heading2 => 14.0,
        ParaStyle::Heading3 | ParaStyle::Heading4 => 13.0,
        _ => 12.0,
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
    let font = if s == ParaStyle::Code { egui::FontId::monospace(11.5) } else { regular(size) };
    let at = pos2(r.min.x + 10.0, r.center().y);
    if s.heading_level().is_some_and(|l| l <= 4) || s == ParaStyle::Title {
        crate::theme::paint_strong(ui.painter(), at, Align2::LEFT_CENTER, style_label(s), font, color);
    } else {
        ui.painter().text(at, Align2::LEFT_CENTER, style_label(s), font, color);
    }
    let clicked = enabled && resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn styles(ui: &mut Ui, app: &mut NoteApp) {
    let enabled = app.can(ids::FORMAT_STYLE);
    let current = format_state(app).and_then(|f| f.style);
    let shown = style_label(current.unwrap_or(ParaStyle::Normal));
    super::text_menu(ui, app, shown, tl!("Styles"), enabled, |ui, app| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        widgets::menu_heading(ui, tl!("Styles"));
        for s in STYLE_ORDER {
            if style_item(ui, app, s, current == Some(s), enabled) {
                format(app, ids::FORMAT_STYLE, json!({ "style": s.name() }));
            }
        }
    });
}

fn tags(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::ToDo, tl!("To Do")).tip(tl!("To Do Tag")).for_command(app, ids::FORMAT_TAG_TODO);
    if widgets::small(ui, app, b) {
        format(app, ids::FORMAT_TAG_TODO, Value::Null);
    }
    let b = Btn::new(Icon::Important, tl!("Tags")).icon_only().help(tl!("Mark notes with a tag.")).for_command(app, ids::FORMAT_TAG);
    widgets::small_menu(ui, app, b, crate::ribbon::paragraph::tag_menu);
}
