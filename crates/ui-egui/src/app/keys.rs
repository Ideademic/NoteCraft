//! Window-wide keyboard shortcuts: the engine's command shortcuts (`Mod+N` New Page, `Mod+T`
//! New Section, `Mod+Z` Undo…) while no text box has the keyboard, plus the UI's own: Ctrl+E
//! search, F11 Full Page View, Ctrl+F1 ribbon, Ctrl+Shift+[ / ] page list, KeyTips.
//!
//! While the canvas edits text it owns the keyboard (egui's focus says so), so typing and its own
//! shortcuts (Ctrl+B in a note) never reach these.

use egui::{Key, KeyboardShortcut, Modifiers};
use serde_json::json;

use crate::NoteApp;

/// A registry shortcut (`Mod+Shift+N`, `Alt+Shift+Right`, `Mod+-`) as an egui shortcut.
pub fn parse_shortcut(s: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Mod" => mods |= Modifiers::COMMAND,
            "Ctrl" => mods |= Modifiers::CTRL,
            "Alt" => mods |= Modifiers::ALT,
            "Shift" => mods |= Modifiers::SHIFT,
            "" => key = Some(Key::Plus),
            k => {
                key = Some(match k {
                    "-" => Key::Minus,
                    "." => Key::Period,
                    "/" => Key::Slash,
                    "," => Key::Comma,
                    ";" => Key::Semicolon,
                    "[" => Key::OpenBracket,
                    "]" => Key::CloseBracket,
                    "=" => Key::Equals,
                    other => Key::from_name(other)?,
                })
            }
        }
    }
    Some(KeyboardShortcut::new(mods, key?))
}

/// Commands whose shortcuts the window handles itself (others belong to the canvas: formatting
/// applies to the text being edited).
const WINDOW_COMMANDS: [&str; 8] = [
    notecraft_engine::ids::PAGE_NEW,
    notecraft_engine::ids::PAGE_NEW_SUBPAGE,
    notecraft_engine::ids::SECTION_NEW,
    notecraft_engine::ids::EDIT_UNDO,
    notecraft_engine::ids::EDIT_REDO,
    notecraft_engine::ids::PAGE_INDENT,
    notecraft_engine::ids::PAGE_OUTDENT,
    notecraft_engine::ids::NOTEBOOK_OPEN,
];

/// Reads this frame's shortcuts.
pub fn handle(app: &mut NoteApp, ctx: &egui::Context) {
    app.ui.keytips.begin_frame();
    if app.ui.prompt.is_some() {
        return;
    }
    let typing = ctx.egui_wants_keyboard_input();
    if !typing && let Some(tab) = app.ui.keytips.handle_keys(ctx) {
        crate::ribbon::select(app, tab);
    }
    let take = |ctx: &egui::Context, s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
    // Always: search, full page view, ribbon.
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND, Key::E)) {
        app.ui.full_page_view = false;
        app.ui.backstage = false;
        app.ui.search_focus = true;
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::NONE, Key::F11)) {
        app.ui.full_page_view = !app.ui.full_page_view;
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND, Key::F1)) {
        crate::ribbon::toggle_collapsed(app);
    }
    if app.ui.backstage || typing {
        return;
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::OpenBracket)) {
        app.ui.page_list_open = !app.ui.page_list_open;
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::CloseBracket)) {
        app.ui.notebook_pane_open = !app.ui.notebook_pane_open;
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus)) || take(ctx, KeyboardShortcut::new(Modifiers::COMMAND, Key::Equals)) {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_in(app.canvas.zoom));
    }
    if take(ctx, KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)) {
        app.canvas.set_zoom(notecraft_geom::zoom::zoom_out(app.canvas.zoom));
    }
    for id in WINDOW_COMMANDS {
        let Some(spec) = notecraft_engine::registry().get(id) else { continue };
        let Some(keys) = spec.shortcut else { continue };
        for k in keys.split(" / ") {
            let Some(sc) = parse_shortcut(k) else { continue };
            if take(ctx, sc) {
                if id == notecraft_engine::ids::NOTEBOOK_OPEN {
                    crate::backstage::open(app, crate::backstage::BackstagePage::Open);
                } else {
                    app.act(id, json!({}));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registry_shortcut_parses() {
        for spec in notecraft_engine::registry().all() {
            if let Some(keys) = spec.shortcut {
                for k in keys.split(" / ") {
                    assert!(parse_shortcut(k).is_some(), "{} {k}", spec.id);
                }
            }
        }
        assert_eq!(parse_shortcut("Mod+Shift+N"), Some(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::N)));
        assert_eq!(parse_shortcut("Mod+-"), Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Minus)));
        assert_eq!(parse_shortcut("Mod+Alt+]"), Some(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, Key::CloseBracket)));
        assert_eq!(parse_shortcut("Mod+Nope"), None);
    }
}
