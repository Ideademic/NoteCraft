//! The font box of the classic and simplified Home tabs: every installed, bundled and added font,
//! each name drawn in its own face (loaded a few per frame, [`PREVIEWS_PER_FRAME`]), a filter
//! typed at the top, the recently used fonts above the list, and on the web Add Fonts to load
//! font files into the browser.
//!
//! The box shows the font at the caret, the paragraph style's default (Calibri, Consolas for
//! Code) for text without a font of its own, and nothing for a selection in several fonts.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Align2, FontId, Sense, Ui, pos2, vec2};

use notecraft_doc::ParaStyle;
use notecraft_engine::ids;
use serde_json::json;

use crate::canvas::style::default_font;
use crate::fonts;
use crate::ribbon::home::{format, format_state};
use crate::services::OpenPurpose;
use crate::theme::{RADIUS_SMALL, Tokens, fit_text, regular};
use crate::{NoteApp, tl};

/// Most fonts loaded for their names' previews in one frame (each reads a font file once).
pub const PREVIEWS_PER_FRAME: usize = 12;
/// Height of a row in the list.
const ROW_H: f32 = 22.0;
/// Size the names are drawn at.
const PREVIEW_SIZE: f32 = 13.5;
/// Where the classic and simplified boxes remember the font last shown or picked.
pub const REMEMBERED: &str = "nc_font";

/// The font the box shows: the caret's (the style's default for text with no font of its own),
/// empty for a selection in several fonts, else the font last shown or picked.
pub fn shown_font(app: &NoteApp, ui: &Ui) -> String {
    match format_state(app) {
        Some(st) => match st.font {
            Some(Some(f)) => f,
            Some(None) => default_font(st.style.unwrap_or(ParaStyle::Normal)).to_owned(),
            None => String::new(),
        },
        None => ui.data(|d| d.get_temp::<String>(egui::Id::unique(REMEMBERED))).unwrap_or_else(|| fonts::DEFAULT_FONT.to_owned()),
    }
}

/// Which names are drawn in their own face (the others in the interface font, until their turn).
#[derive(Clone, Default)]
struct Previews {
    shown: Arc<Mutex<HashSet<String>>>,
    generation: u64,
    frame: u64,
    used: usize,
}

fn previews_id() -> egui::Id {
    egui::Id::unique("nc_font_previews")
}

/// Whether `name` may be drawn in its own face this frame: it was before, or one of this frame's
/// [`PREVIEWS_PER_FRAME`] new ones.
fn preview_allowed(ctx: &egui::Context, name: &str) -> bool {
    if !fonts::active(ctx) {
        return false;
    }
    let frame = ctx.cumulative_frame_nr();
    let generation = fonts::generation();
    let mut p: Previews = ctx.data(|d| d.get_temp(previews_id())).unwrap_or_default();
    if p.generation != generation {
        p = Previews { generation, ..Previews::default() };
    }
    if p.frame != frame {
        p.frame = frame;
        p.used = 0;
    }
    let allowed = {
        let mut shown = p.shown.lock().unwrap_or_else(PoisonError::into_inner);
        if shown.contains(name) {
            true
        } else if p.used < PREVIEWS_PER_FRAME {
            p.used += 1;
            shown.insert(name.to_owned());
            true
        } else {
            false
        }
    };
    if !allowed {
        // The rest get their turn next frame.
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(previews_id(), p));
    allowed
}

/// One font in the list: its name in its own face once its preview is loaded. `true` when picked.
fn font_row(ui: &mut Ui, name: &str, current: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(180.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::ListItem, true, current, name));
    if resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    } else if current {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.checked);
    }
    let font = if preview_allowed(ui.ctx(), name) { FontId::new(PREVIEW_SIZE, fonts::family_id(name, false, false)) } else { regular(12.0) };
    let room = r.width() - 16.0;
    let shown = fit_text(ui.ctx(), name, &font, room);
    let painter = ui.painter().with_clip_rect(r);
    painter.text(pos2(r.min.x + 8.0, r.center().y), Align2::LEFT_CENTER, shown, font, t.text);
    let picked = resp.clicked();
    if picked {
        ui.close();
    }
    picked
}

fn filter_id(salt: &str) -> egui::Id {
    egui::Id::unique(("nc_font_filter", salt))
}

/// The fonts matching `filter` (any part of the name, ignoring case).
pub fn matching(all: &[String], filter: &str) -> Vec<String> {
    let f = filter.trim().to_lowercase();
    if f.is_empty() {
        return all.to_vec();
    }
    all.iter().filter(|n| n.to_lowercase().contains(&f)).cloned().collect()
}

/// The font box, `width` wide. `salt` keeps the classic and simplified boxes apart.
pub fn show(ui: &mut Ui, app: &mut NoteApp, salt: &str, width: f32) {
    let shown = shown_font(app, ui);
    if !shown.is_empty() && format_state(app).is_some() {
        ui.data_mut(|d| d.insert_temp(egui::Id::unique(REMEMBERED), shown.clone()));
    }
    let enabled = app.can(ids::FORMAT_FONT);
    let recent = app.ui.prefs.recent_fonts.clone();
    let can_add = app.services.open_async.is_some();
    let picked: RefCell<Option<String>> = RefCell::new(None);
    let add = RefCell::new(false);
    let fid = filter_id(salt);
    ui.add_enabled_ui(enabled, |ui| {
        let combo = craft_ui::ComboBox::from_id_salt(("nc_font_box", salt)).width(width).height(380.0).selected_text(shown.clone());
        let r = combo.show_rows_with_header_footer(
            ui,
            |ui| {
                let mut text: String = ui.data(|d| d.get_temp(fid)).unwrap_or_default();
                ui.set_min_width(width.max(220.0));
                let edit = ui.add(egui::TextEdit::singleline(&mut text).hint_text(tl!("Search fonts")).desired_width(f32::INFINITY));
                if !edit.has_focus() && ui.data(|d| d.get_temp::<bool>(fid.with("focused")).is_none()) {
                    edit.request_focus();
                    ui.data_mut(|d| d.insert_temp(fid.with("focused"), true));
                }
                ui.data_mut(|d| d.insert_temp(fid, text.clone()));
                let list = matching(&fonts::families(), &text);
                if edit.has_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && let Some(first) = list.first()
                {
                    *picked.borrow_mut() = Some(first.clone());
                    ui.close();
                }
                if text.trim().is_empty() && !recent.is_empty() {
                    crate::widgets::menu_heading(ui, tl!("Recently Used Fonts"));
                    for f in &recent {
                        if font_row(ui, f, f.eq_ignore_ascii_case(&shown)) {
                            *picked.borrow_mut() = Some(f.clone());
                        }
                    }
                    ui.separator();
                }
                crate::widgets::menu_heading(ui, if text.trim().is_empty() { tl!("All Fonts") } else { tl!("Matching Fonts") });
                if list.is_empty() {
                    ui.label(egui::RichText::new(tl!("No font has that name.")).font(regular(12.0)).color(Tokens::get(ui.ctx()).text_dim));
                }
                list
            },
            ROW_H,
            Vec::len,
            |ui, range, list: Vec<String>| {
                for name in list.get(range).unwrap_or_default() {
                    if font_row(ui, name, name.eq_ignore_ascii_case(&shown)) {
                        *picked.borrow_mut() = Some(name.clone());
                    }
                }
            },
            |ui| {
                if !fonts::library().is_ready() {
                    ui.label(egui::RichText::new(tl!("Reading the installed fonts…")).font(regular(11.5)).color(Tokens::get(ui.ctx()).text_dim));
                }
                if can_add {
                    ui.separator();
                    if crate::widgets::menu_item(ui, Some(crate::icons::Icon::Plus), tl!("Add Fonts…"), true) {
                        *add.borrow_mut() = true;
                        ui.close();
                    }
                }
            },
        );
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tl!("Font")));
        if r.inner.is_none() {
            // Closed: the next opening starts with an empty, focused filter.
            ui.data_mut(|d| {
                d.remove::<String>(fid);
                d.remove::<bool>(fid.with("focused"));
            });
        }
        r.response.on_hover_text(tl!("Font"));
    });
    if let Some(f) = picked.into_inner() {
        ui.data_mut(|d| d.insert_temp(egui::Id::unique(REMEMBERED), f.clone()));
        app.ui.prefs.remember_font(&f);
        format(app, ids::FORMAT_FONT, json!({ "font": f }));
    }
    if add.into_inner()
        && let Some(open) = &app.services.open_async
    {
        // The files arrive in the inbox ([`crate::app::files::add_font`]).
        open(OpenPurpose::Font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_any_part_ignoring_case() {
        let all: Vec<String> = ["Calibri", "Cambria", "Liberation Sans", "Arial"].map(String::from).to_vec();
        assert_eq!(matching(&all, "  "), all);
        assert_eq!(matching(&all, "CAL"), vec!["Calibri".to_owned()]);
        assert_eq!(matching(&all, "a"), all);
        assert!(matching(&all, "zzz").is_empty());
    }
}
