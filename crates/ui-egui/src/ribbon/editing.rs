//! The editing actions the classic and simplified ribbons share: Paste (and Keep Text Only),
//! Cut, Copy and the Format Painter on Home; Insert Space, Page Templates on Insert; Arrange,
//! Rotate, Crop and Alt Text on Draw; Find Tags on Home. Each runs engine commands (or the
//! canvas's clipboard, which does), never reaching into the canvas otherwise.

use egui::Ui;
use notecraft_doc::ElementKind;
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::canvas::{Tool, clipboard, crop, format_painter};
use crate::icons::Icon;
use crate::widgets;
use crate::{NoteApp, tl};

/// Copy or cut what is selected on the page (text, items or ink).
pub fn copy(ui: &Ui, app: &mut NoteApp, cut: bool) {
    let ctx = ui.ctx().clone();
    if !clipboard::copy(&ctx, &mut app.canvas, &mut app.session, &app.services, cut) {
        app.set_status(tl!("Select some text, a note container, a picture or ink first."), false);
    }
}

/// Whether Copy and Cut have something to take.
pub fn can_copy(app: &NoteApp) -> bool {
    clipboard::can_copy(&app.canvas, &app.session)
}

/// Paste (`plain`: Keep Text Only): what NoteCraft copied, else the system clipboard's text
/// (with its HTML) or picture, at the caret or as a new note.
pub fn paste(ui: &Ui, app: &mut NoteApp, plain: bool) {
    let ctx = ui.ctx().clone();
    if app.session.current_page().is_none() {
        return;
    }
    let text = app.services.clipboard_text.as_ref().and_then(|f| f());
    let internal = clipboard::internal(&ctx).is_some_and(|i| text.as_deref().is_none_or(|t| t.trim_end() == i.text.trim_end()));
    if internal || text.is_some() {
        clipboard::paste(&ctx, &mut app.canvas, &mut app.session, &app.services, None, text.as_deref(), plain);
        return;
    }
    if app.services.clipboard_text.is_some() && cfg!(target_arch = "wasm32") {
        // The browser hands the text over a frame later, as a paste event.
        app.canvas.paste_plain = plain;
    }
    if !plain && let Some(img) = app.services.clipboard_image.as_ref().and_then(|f| f()) {
        match crate::app::files::png_from_rgba(img.width, img.height, &img.rgba) {
            Some(png) => clipboard::paste_picture_bytes(&ctx, &mut app.canvas, &mut app.session, None, &png),
            None => app.set_status(tl!("The picture on the clipboard couldn't be read."), true),
        }
        return;
    }
    if !cfg!(target_arch = "wasm32") {
        let key = widgets::shortcut_text("Mod+V");
        app.set_status(crate::i18n::fmt_plain(tl!("Nothing to paste here. Press {key} to paste into a note."), &[("key", &key)]), false);
    }
}

/// The Paste menu: Paste, Keep Text Only.
pub fn paste_menu(ui: &mut Ui, app: &mut NoteApp) {
    if widgets::menu_item(ui, Some(Icon::Paste), tl!("Paste"), true) {
        paste(ui, app, false);
    }
    let plain = format!("{}    {}", tl!("Keep Text Only"), widgets::shortcut_text("Mod+Shift+V"));
    if widgets::menu_item(ui, Some(Icon::ClearFormat), &plain, true) {
        paste(ui, app, true);
    }
}

/// The Format Painter button: a click picks up the caret's formatting for one use (or turns an
/// armed painter off), a double-click keeps it on until Esc.
pub fn format_painter(ui: &Ui, app: &mut NoteApp) {
    let double = ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary));
    if !double && app.canvas.painter.is_some() {
        format_painter::cancel(&mut app.canvas);
        return;
    }
    if format_painter::pick(&mut app.canvas, &mut app.session, double) {
        let msg = if double {
            tl!("Format Painter is on: select text to paint its formatting. Press Esc to stop.")
        } else {
            tl!("Select the text to paint this formatting onto.")
        };
        app.set_status(msg, false);
    } else {
        app.set_status(tl!("Click in the text whose formatting you want to copy first."), false);
    }
}

/// Whether the Format Painter can pick something up (a caret in a note).
pub fn can_paint(app: &NoteApp) -> bool {
    app.canvas.caret.is_some() || app.canvas.painter.is_some()
}

/// Insert Space: turns the tool on (or back to Type).
pub fn insert_space(app: &mut NoteApp) {
    let next = if app.canvas.tool == Tool::InsertSpace { Tool::Type } else { Tool::InsertSpace };
    app.canvas.set_tool(next);
    if next == Tool::InsertSpace {
        app.set_status(tl!("Drag down from where you want more space (drag up to take space away)."), false);
    }
}

/// The selected pictures on the current page.
pub fn selected_pictures(app: &NoteApp) -> Vec<notecraft_doc::Guid> {
    let Some(page) = app.session.current_page() else { return Vec::new() };
    app.canvas.selection.elements.iter().copied().filter(|id| page.element(*id).is_some_and(|e| matches!(e.kind, ElementKind::Image(_)))).collect()
}

fn has_ink_selection(app: &NoteApp) -> bool {
    !app.session.ink_selection.is_empty()
}

/// What Arrange and Rotate act on: selected items, else selected ink.
pub fn can_arrange(app: &NoteApp) -> bool {
    !app.canvas.selection.elements.is_empty() || has_ink_selection(app)
}

/// Whether Draw › Delete has selected ink or items (pictures, containers, files) to delete.
pub fn can_delete_selection(app: &NoteApp) -> bool {
    !app.canvas.selection.elements.is_empty() || !app.canvas.selection.strokes.is_empty() || has_ink_selection(app)
}

/// Draw › Delete: the selected ink and items, as the Delete key does on the page.
pub fn delete_selection(app: &mut NoteApp) {
    if has_ink_selection(app) || !app.canvas.selection.strokes.is_empty() {
        // The canvas mirrors its lasso selection into `Session::ink_selection`, which the
        // command deletes.
        app.act(ids::DRAW_DELETE_SELECTION, json!({}));
        app.canvas.selection.strokes.clear();
    }
    let page = app.canvas.page;
    for el in std::mem::take(&mut app.canvas.selection.elements) {
        app.act(ids::OUTLINE_DELETE, json!({"id": el, "page": page}));
    }
}

/// Whether Rotate has a picture or ink to turn.
pub fn can_rotate(app: &NoteApp) -> bool {
    !selected_pictures(app).is_empty() || has_ink_selection(app)
}

fn arrange(app: &mut NoteApp, id: &str) {
    let params = if app.canvas.selection.elements.is_empty() { json!({"ink": true}) } else { json!({"elements": app.canvas.selection.elements}) };
    app.act(id, params);
}

/// Draw › Arrange: the z-order of the selected items or ink.
pub fn arrange_menu(ui: &mut Ui, app: &mut NoteApp) {
    let on = can_arrange(app);
    for (icon, label, id) in [
        (Icon::BringToFront, tl!("Bring to Front"), ids::ARRANGE_BRING_TO_FRONT),
        (Icon::BringForward, tl!("Bring Forward"), ids::ARRANGE_BRING_FORWARD),
        (Icon::SendBackward, tl!("Send Backward"), ids::ARRANGE_SEND_BACKWARD),
        (Icon::SendToBack, tl!("Send to Back"), ids::ARRANGE_SEND_TO_BACK),
    ] {
        if widgets::menu_item(ui, Some(icon), label, on) {
            arrange(app, id);
        }
    }
}

/// A turn or flip of the selected pictures, else of the selected ink.
pub fn rotate(app: &mut NoteApp, what: &str) {
    let pics = selected_pictures(app);
    let (pic_cmd, ink_cmd, p): (&str, &str, Value) = match what {
        "right" => (ids::PICTURE_ROTATE, ids::INK_ROTATE, json!({"degrees": 90})),
        "left" => (ids::PICTURE_ROTATE, ids::INK_ROTATE, json!({"degrees": -90})),
        "flipH" => (ids::PICTURE_FLIP, ids::INK_FLIP, json!({"axis": "horizontal"})),
        _ => (ids::PICTURE_FLIP, ids::INK_FLIP, json!({"axis": "vertical"})),
    };
    if pics.is_empty() {
        if has_ink_selection(app) {
            app.act(ink_cmd, p);
        }
        return;
    }
    for id in pics {
        let mut params = p.clone();
        if let Value::Object(m) = &mut params {
            m.insert("element".into(), json!(id));
        }
        app.act(pic_cmd, params);
    }
}

/// Draw › Rotate: turn or flip pictures and ink.
pub fn rotate_menu(ui: &mut Ui, app: &mut NoteApp) {
    let on = can_rotate(app);
    for (icon, label, what) in [
        (Icon::RotateRight, tl!("Rotate Right 90°"), "right"),
        (Icon::RotateLeft, tl!("Rotate Left 90°"), "left"),
        (Icon::FlipHorizontal, tl!("Flip Horizontal"), "flipH"),
        (Icon::FlipVertical, tl!("Flip Vertical"), "flipV"),
    ] {
        if widgets::menu_item(ui, Some(icon), label, on) {
            rotate(app, what);
        }
    }
    let one = selected_pictures(app).len() == 1;
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::ResetPicture), tl!("Reset Picture"), one) {
        reset_picture(app);
    }
}

/// Starts cropping the selected picture.
pub fn crop(app: &mut NoteApp) {
    match selected_pictures(app).as_slice() {
        [id] => {
            let id = *id;
            if crop::begin(&mut app.canvas, &mut app.session, id) {
                app.set_status(tl!("Drag the black handles to crop. Press Enter to keep it, Esc to cancel."), false);
            }
        }
        _ => app.set_status(tl!("Select a picture to crop first."), false),
    }
}

/// Opens Alt Text for the selected picture.
pub fn alt_text(app: &mut NoteApp) {
    match crate::dialogs::alt_text::selected_picture(app) {
        Some(id) => crate::dialogs::alt_text::open(app, id),
        None => app.set_status(tl!("Select a picture first."), false),
    }
}

/// Takes the selected picture back to how it was inserted (no turn, flip or crop).
pub fn reset_picture(app: &mut NoteApp) {
    if let [id] = selected_pictures(app).as_slice() {
        let id = *id;
        app.act(ids::PICTURE_RESET, json!({"element": id}));
    }
}

/// Whether one picture is selected (Crop, Alt Text).
pub fn one_picture(app: &NoteApp) -> bool {
    selected_pictures(app).len() == 1
}

/// The current page's Hide Page Title.
pub fn title_hidden(app: &NoteApp) -> bool {
    app.session.current_page().is_some_and(|p| p.hide_title)
}

/// View › Hide Page Title (kept with the page).
pub fn toggle_page_title(app: &mut NoteApp) {
    app.act(ids::VIEW_HIDE_PAGE_TITLE, json!({}));
}
