//! Preferences between visits: the same [`Prefs`] the desktop app keeps in `prefs.json`
//! (appearance, panes, ribbon, Quick Access Toolbar…), kept in `localStorage` under
//! [`crate::persist::PREFS_KEY`]. Restored at start; written whenever they change, since a reload
//! or a closed tab runs no exit code.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use notecraft_ui_egui::{NoteApp, Prefs};

/// Largest stored value read back.
const MAX_STORED: usize = 1 << 20;

/// Applies preferences stored on an earlier visit. Missing, oversized or unreadable values keep
/// the defaults.
pub fn restore(app: &mut NoteApp, stored: Option<&str>) {
    if let Some(s) = stored.filter(|s| !s.is_empty() && s.len() <= MAX_STORED) {
        app.load_prefs(Prefs::from_json(s));
    }
}

/// Remembers what was last written, so storage is written only when a preference changes.
pub struct PrefsSync {
    last: Option<String>,
}

impl PrefsSync {
    /// Starts from what is stored, so restoring doesn't write it straight back.
    pub fn new(stored: Option<String>) -> PrefsSync {
        PrefsSync { last: stored }
    }

    /// The JSON to store when the app's preferences differ from what was last written.
    pub fn changed(&mut self, app: &NoteApp) -> Option<String> {
        let json = app.prefs().to_json();
        if json.is_empty() || self.last.as_deref() == Some(json.as_str()) {
            return None;
        }
        self.last = Some(json.clone());
        Some(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_ui_egui::{Appearance, Services};

    fn app() -> NoteApp {
        NoteApp::new(notecraft_engine::Session::new_default(), Services::default())
    }

    #[test]
    fn dark_mode_survives_a_reload() {
        let mut first = app();
        let mut sync = PrefsSync::new(None);
        assert!(sync.changed(&first).is_some(), "the first frame stores the preferences");
        assert_eq!(sync.changed(&first), None, "unchanged preferences aren't rewritten");
        first.ui.prefs.appearance = Appearance::Dark;
        first.ui.page_list_open = false;
        let stored = sync.changed(&first).expect("the change is stored");

        let mut reloaded = app();
        restore(&mut reloaded, Some(&stored));
        assert_eq!(reloaded.ui.prefs.appearance, Appearance::Dark);
        assert!(!reloaded.ui.page_list_open);
        assert_eq!(PrefsSync::new(Some(stored)).changed(&reloaded), None, "restoring doesn't write back");
    }

    #[test]
    fn junk_keeps_the_defaults() {
        let long = "x".repeat(MAX_STORED + 1);
        for junk in [None, Some(""), Some("not json"), Some("[1,2]"), Some("{\"appearance\":42}"), Some(long.as_str())] {
            let mut a = app();
            restore(&mut a, junk);
            assert_eq!(a.ui.prefs.appearance, Appearance::System);
        }
    }
}
