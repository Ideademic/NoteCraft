//! The paragraph controls the classic and simplified Home tabs share: Bullets▾ and Numbering▾
//! with their styles, Paragraph Alignment▾, and the tags with their shortcuts.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};

use notecraft_doc::{Align, ListKind, ListStyle, Tag};
use notecraft_engine::cmd::format::{TAG_REMOVE_KEY, tag_key};
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::icons::{self, Icon};
use crate::ribbon::home::{format, format_state};
use crate::theme::{RADIUS_SMALL, Tokens, regular};
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

/// A bullet style's name in the menu.
pub fn bullet_label(s: ListStyle) -> &'static str {
    match s {
        ListStyle::Disc => tl!("Filled round bullet"),
        ListStyle::Circle => tl!("Hollow round bullet"),
        ListStyle::Square => tl!("Square bullet"),
        ListStyle::Dash => tl!("Dash bullet"),
        ListStyle::Arrow => tl!("Arrow bullet"),
        _ => "",
    }
}

/// A numbering style's sample in the menu.
pub fn number_sample(s: ListStyle) -> String {
    (1..=3).map(|n| s.label(n)).collect::<Vec<_>>().join("  ")
}


/// A menu row with a bullet drawn in code (`Some`) or a numbering sample, ticked when current.
fn list_row(ui: &mut Ui, bullet: Option<ListStyle>, text: &str, current: bool, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(200.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, current, text));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    } else if current {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.checked);
    }
    let ink = if enabled { t.text } else { t.text_disabled };
    if let Some(b) = bullet {
        icons::paint_bullet(ui.painter(), pos2(r.min.x + 16.0, r.center().y), 3.2, b, ink);
    }
    ui.painter().text(pos2(r.min.x + 32.0, r.center().y), Align2::LEFT_CENTER, text, regular(12.0), ink);
    let clicked = enabled && resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Bullets▾: the button toggles bullets (the level's default style); the menu picks a style.
pub fn bullets(ui: &mut Ui, app: &mut NoteApp) {
    let st = format_state(app);
    let list = st.as_ref().and_then(|f| f.list).flatten();
    let style = st.as_ref().and_then(|f| f.list_style).flatten();
    let b = Btn::new(Icon::Bullets, tl!("Bullets")).checked(list == Some(ListKind::Bullet)).for_command(app, ids::FORMAT_BULLETS);
    let enabled = app.can(ids::FORMAT_BULLETS);
    if widgets::small_split(ui, app, b, |ui, app| {
        widgets::menu_heading(ui, tl!("Bullet Library"));
        for s in ListStyle::BULLETS {
            if list_row(ui, Some(s), bullet_label(s), list == Some(ListKind::Bullet) && style == Some(s), enabled) {
                format(app, ids::FORMAT_BULLETS, json!({ "style": s.name(), "value": true }));
            }
        }
        if widgets::menu_item(ui, None, tl!("None"), enabled) {
            format(app, ids::FORMAT_BULLETS, json!({ "value": false }));
        }
    }) {
        format(app, ids::FORMAT_BULLETS, Value::Null);
    }
}

/// Numbering▾: the button toggles numbering (the level's default format); the menu picks one.
pub fn numbering(ui: &mut Ui, app: &mut NoteApp) {
    let st = format_state(app);
    let list = st.as_ref().and_then(|f| f.list).flatten();
    let style = st.as_ref().and_then(|f| f.list_style).flatten();
    let b = Btn::new(Icon::Numbering, tl!("Numbering")).checked(list == Some(ListKind::Numbered)).for_command(app, ids::FORMAT_NUMBERING);
    let enabled = app.can(ids::FORMAT_NUMBERING);
    if widgets::small_split(ui, app, b, |ui, app| {
        widgets::menu_heading(ui, tl!("Numbering Library"));
        for s in ListStyle::NUMBERS {
            if list_row(ui, None, &number_sample(s), list == Some(ListKind::Numbered) && style == Some(s), enabled) {
                format(app, ids::FORMAT_NUMBERING, json!({ "style": s.name(), "value": true }));
            }
        }
        if widgets::menu_item(ui, None, tl!("None"), enabled) {
            format(app, ids::FORMAT_NUMBERING, json!({ "value": false }));
        }
    }) {
        format(app, ids::FORMAT_NUMBERING, Value::Null);
    }
}

/// An alignment's icon and menu label.
pub fn align_item(a: Align) -> (Icon, &'static str) {
    match a {
        Align::Left => (Icon::AlignLeft, tl!("Align Left")),
        Align::Center => (Icon::AlignCenter, tl!("Center")),
        Align::Right => (Icon::AlignRight, tl!("Align Right")),
        Align::Justify => (Icon::Justify, tl!("Justify")),
    }
}

/// Paragraph Alignment▾: the button shows the caret's alignment; the menu sets one.
pub fn alignment(ui: &mut Ui, app: &mut NoteApp) {
    let current = format_state(app).and_then(|f| f.align);
    let (icon, _) = align_item(current.unwrap_or(Align::Left));
    let b = Btn::new(icon, tl!("Paragraph Alignment")).icon_only().for_command(app, ids::FORMAT_ALIGN);
    let enabled = app.can(ids::FORMAT_ALIGN);
    widgets::small_menu(ui, app, b, |ui, app| {
        for a in Align::ALL {
            let (icon, label) = align_item(a);
            if widgets::menu_item(ui, Some(icon), label, enabled) {
                format(app, ids::FORMAT_ALIGN, json!({ "align": a.name() }));
            }
        }
    });
}

/// A tag's name in the gallery and menus.
pub fn tag_label(t: Tag) -> &'static str {
    match t {
        Tag::ToDo { .. } => tl!("To Do"),
        Tag::Important => tl!("Important"),
        Tag::Question => tl!("Question"),
        Tag::RememberForLater => tl!("Remember for later"),
        Tag::Definition => tl!("Definition"),
        Tag::Highlight => tl!("Highlight"),
        Tag::Contact => tl!("Contact"),
        Tag::Address => tl!("Address"),
        Tag::Phone => tl!("Phone number"),
        Tag::Idea => tl!("Idea"),
        Tag::Critical => tl!("Critical"),
    }
}

/// The command and parameters that put `t` on (To Do keeps its own command, Mod+1).
pub fn tag_command(t: Tag) -> (&'static str, Value) {
    match t {
        Tag::ToDo { .. } => (ids::FORMAT_TAG_TODO, Value::Null),
        other => (ids::FORMAT_TAG, json!({ "tag": other.name() })),
    }
}

/// Paints `t`'s icon in `r` (To Do: its check box).
pub fn paint_tag_icon(painter: &egui::Painter, r: Rect, t: Tag, style: icons::Style) {
    let icon = icons::tag_icon(t).unwrap_or(Icon::ToDo);
    icons::paint_styled(painter, r, icon, style);
}

/// One tag's row (gallery and menus): icon, name, shortcut. `true` when chosen.
pub fn tag_row(ui: &mut Ui, app: &mut NoteApp, t: Tag, w: f32, current: bool) -> bool {
    let tokens = app.tokens.clone();
    let (id, _) = tag_command(t);
    let enabled = app.can(id);
    let label = tag_label(t);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 20.0), if enabled { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, current, label));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, tokens.hover);
    } else if current {
        ui.painter().rect_filled(r, RADIUS_SMALL, tokens.checked);
    }
    let style = if enabled { icons::Style::new(tokens.icon, tokens.accent) } else { icons::Style::disabled(tokens.icon) };
    paint_tag_icon(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(15.0, 15.0)), t, style);
    let color = if enabled { tokens.text } else { tokens.text_disabled };
    ui.painter().text(pos2(r.min.x + 24.0, r.center().y), Align2::LEFT_CENTER, label, regular(11.5), color);
    if let Some(k) = tag_key(t) {
        ui.painter().text(pos2(r.max.x - 4.0, r.center().y), Align2::RIGHT_CENTER, widgets::shortcut_text(k), regular(10.5), tokens.text_dim);
    }
    let keyed = app.ui.keytips.control(r);
    enabled && (resp.clicked() || keyed)
}

/// Puts tag `t` on what the caret is in (or takes it off when everything has it).
pub fn apply_tag(app: &mut NoteApp, t: Tag) {
    let (id, params) = tag_command(t);
    format(app, id, params);
}

/// The tag on the caret's paragraphs, when they share one.
pub fn current_tag(app: &NoteApp) -> Option<Tag> {
    format_state(app).and_then(|f| f.tag).flatten()
}

/// Remove Tag (Mod+0), for the end of the tag menus.
pub fn remove_tags_item(ui: &mut Ui, app: &mut NoteApp) {
    let text = format!("{}    {}", tl!("Remove Tag"), widgets::shortcut_text(TAG_REMOVE_KEY));
    if widgets::menu_item(ui, Some(Icon::Delete), &text, app.can(ids::FORMAT_TAG)) {
        format(app, ids::FORMAT_TAG, json!({ "tag": null }));
    }
}

/// Every tag as a menu (the simplified Tags▾ and the classic gallery's More).
pub fn tag_menu(ui: &mut Ui, app: &mut NoteApp) {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    widgets::menu_heading(ui, tl!("Tags"));
    let current = current_tag(app);
    let w = ui.available_width().max(220.0);
    for t in Tag::ALL {
        if tag_row(ui, app, t, w, current.is_some_and(|c| c.name() == t.name())) {
            apply_tag(app, t);
            ui.close();
        }
    }
    ui.separator();
    remove_tags_item(ui, app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_style_and_tag_has_a_label() {
        for s in ListStyle::BULLETS {
            assert!(!bullet_label(s).is_empty());
        }
        assert_eq!(number_sample(ListStyle::UpperRoman), "I.  II.  III.");
        for t in Tag::ALL {
            assert!(!tag_label(t).is_empty());
            let (id, _) = tag_command(t);
            assert!(notecraft_engine::registry().get(id).is_some(), "{id}");
        }
        for a in Align::ALL {
            assert!(!align_item(a).1.is_empty());
        }
    }
}
