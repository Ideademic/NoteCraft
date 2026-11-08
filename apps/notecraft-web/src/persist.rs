//! Keeping the session in the browser between visits (the parts that don't touch the DOM).
//!
//! The whole session (every open notebook, unsaved state included) is one JSON blob from
//! [`Session::to_blob`]. It goes to `localStorage` while it fits ([`LOCAL_MAX_CHARS`]: about half
//! of the 5 MB browsers give a site, leaving room for the preferences), else to IndexedDB, with
//! `localStorage` holding [`IDB_MARKER`] in its place. When neither can hold it the user is told
//! to export ([`TOO_LARGE`]).
//!
//! When to write: the engine's autosave decides. [`BrowserStorage`] is the session's
//! [`notecraft_engine::Storage`]: a save only raises a flag, and the shell writes the blob right
//! after that frame's autosave. Changes that leave nothing dirty (closing a notebook) are written
//! by [`Debounce`] after the same delay. Where the user is (notebook, section, page) is stored
//! apart ([`LOCATION_KEY`]), so moving between pages doesn't rewrite the notebooks.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use notecraft_doc::{Guid, Notebook};
use notecraft_engine::{IoError, Location, Origin, Session, Storage};

/// `localStorage`: the session blob, or [`IDB_MARKER`] when it is in IndexedDB.
pub const SESSION_KEY: &str = "notecraft.session";
/// `localStorage`: the preferences ([`crate::prefs`]).
pub const PREFS_KEY: &str = "notecraft.prefs";
/// `localStorage`: the user's page templates (Insert › Page Templates).
pub const TEMPLATES_KEY: &str = "notecraft.templates";
/// `localStorage`: where the user was ([`Location`] as JSON).
pub const LOCATION_KEY: &str = "notecraft.location";
/// [`SESSION_KEY`]'s value while the blob lives in IndexedDB.
pub const IDB_MARKER: &str = "indexeddb";
/// IndexedDB database, object store and key of the blob.
pub const IDB_NAME: &str = "notecraft";
pub const IDB_STORE: &str = "session";
pub const IDB_KEY: &str = "session";
/// Largest blob kept in `localStorage`, characters (browsers allow about 5 million per site).
pub const LOCAL_MAX_CHARS: usize = 2_500_000;
/// Largest blob written at all (wasm memory is small; IndexedDB quotas vary).
pub const MAX_BLOB_BYTES: usize = 512 << 20;
/// Seconds a change that leaves nothing dirty waits before it is written (the autosave delay).
pub const QUIET_SECS: f64 = notecraft_engine::AUTOSAVE_DELAY_SECS;
/// What the user is told when the browser can't keep the notebooks.
pub const TOO_LARGE: &str = "Notebook too large for browser storage \u{2014} export it as .onepkg (File \u{203a} Export) to keep it";

/// Where a blob of a given size goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Local,
    IndexedDb,
    TooLarge,
}

impl Backend {
    pub fn for_len(len: usize) -> Backend {
        if len > MAX_BLOB_BYTES {
            Backend::TooLarge
        } else if len <= LOCAL_MAX_CHARS {
            Backend::Local
        } else {
            Backend::IndexedDb
        }
    }
}

/// The session's storage on the web: saving raises [`BrowserStorage::wanted`]; the shell then
/// writes the whole session blob.
pub struct BrowserStorage {
    pub wanted: Rc<Cell<bool>>,
}

impl Storage for BrowserStorage {
    fn save(&mut self, _notebook: &Notebook, _origin: &Origin, _dirty: &HashSet<Guid>, _toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        self.wanted.set(true);
        Ok(None)
    }
}

/// Writes changes that leave nothing dirty (closing a notebook) once things are quiet.
#[derive(Debug, Default)]
pub struct Debounce {
    seen: u64,
    changed_at: Option<f64>,
}

impl Debounce {
    pub fn new(revision: u64) -> Debounce {
        Debounce { seen: revision, changed_at: None }
    }

    /// Whether to write the session now: the storage asked (`wanted`), or the notebooks changed
    /// without anything left to autosave and [`QUIET_SECS`] passed since.
    pub fn due(&mut self, revision: u64, dirty: bool, wanted: bool, now: f64) -> bool {
        if revision != self.seen {
            self.seen = revision;
            self.changed_at = Some(now);
        }
        let quiet = self.changed_at.is_some_and(|t| !dirty && (now - t >= QUIET_SECS || now < t));
        if wanted || quiet {
            self.changed_at = None;
            return true;
        }
        false
    }

    /// A change is waiting to be written.
    pub fn pending(&self) -> bool {
        self.changed_at.is_some()
    }
}

/// The session at start: the one kept in this browser (`blob`), at the stored `location`, else
/// a new "My Notebook". The second value is a notice for the status bar.
pub fn restore(blob: Option<&[u8]>, location: Option<&str>) -> (Session, Option<String>) {
    let Some(blob) = blob.filter(|b| !b.is_empty()) else { return (Session::new_default(), None) };
    let mut session = match Session::from_blob(blob) {
        Ok(s) if !s.notebooks.is_empty() => s,
        Ok(_) => return (Session::new_default(), None),
        Err(e) => {
            log::warn!("stored session unreadable: {e}");
            let notice = format!("Couldn't restore the notebooks kept in this browser ({e}); starting with a new notebook.");
            return (Session::new_default(), Some(notice));
        }
    };
    if let Some(loc) = location.and_then(|l| serde_json::from_str::<Location>(l).ok())
        && let Some(page) = loc.page
        && session.notebooks.iter().any(|n| n.find_page(page).is_some())
    {
        session.current = loc;
        session.normalize_location();
    }
    (session, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backends_by_size() {
        assert_eq!(Backend::for_len(10), Backend::Local);
        assert_eq!(Backend::for_len(LOCAL_MAX_CHARS + 1), Backend::IndexedDb);
        assert_eq!(Backend::for_len(MAX_BLOB_BYTES + 1), Backend::TooLarge);
    }

    /// A page typed on one visit is there after a reload, at the same place.
    #[test]
    fn session_survives_a_reload() {
        let wanted = Rc::new(Cell::new(false));
        let mut s = Session::new_default();
        s.storage = Some(Box::new(BrowserStorage { wanted: wanted.clone() }));
        let out = s.run("page.new", &json!({"title": "Shopping"})).unwrap();
        let page = out.value["id"].as_str().unwrap().to_owned();
        assert!(s.is_dirty());
        assert!(s.tick(0.0).is_none(), "autosave waits");
        assert_eq!(s.tick(10.0).map(|r| r.is_ok()), Some(true));
        assert!(wanted.get(), "a save asks the shell to write the blob");
        assert!(!s.is_dirty());
        let blob = s.to_blob().unwrap();
        let loc = serde_json::to_string(&s.current).unwrap();

        let (back, notice) = restore(Some(&blob), Some(&loc));
        assert!(notice.is_none());
        assert_eq!(back.current.page.map(|p| p.to_string()), Some(page));
        assert_eq!(back.notebooks, s.notebooks);
    }

    #[test]
    fn stale_or_junk_storage() {
        let (s, notice) = restore(None, Some("junk"));
        assert_eq!(s.notebooks.len(), 1);
        assert!(notice.is_none());
        let (s, notice) = restore(Some(b"{not json"), None);
        assert_eq!(s.notebooks.first().map(|n| n.name.as_str()), Some("My Notebook"));
        assert!(notice.is_some_and(|n| n.contains("Couldn't restore")));
        // A location for a page that is gone is ignored.
        let blob = Session::new_default().to_blob().unwrap();
        let gone = serde_json::to_string(&Location { page: Some(Guid::new()), ..Location::default() }).unwrap();
        let (s, _) = restore(Some(&blob), Some(&gone));
        assert!(s.current.page.is_some());
        // Every notebook closed: start like a first visit.
        let (s, _) = restore(Some(&Session::empty().to_blob().unwrap()), None);
        assert_eq!(s.notebooks.len(), 1);
    }

    #[test]
    fn quiet_changes_are_written_later() {
        let mut d = Debounce::new(1);
        assert!(!d.due(1, false, false, 0.0));
        assert!(d.due(1, false, true, 0.1), "a save writes at once");
        assert!(!d.due(2, false, false, 1.0), "a change waits");
        assert!(d.pending());
        assert!(!d.due(2, true, false, 5.0), "…for autosave while something is dirty");
        assert!(d.due(2, false, false, 5.0));
        assert!(!d.pending());
        assert!(!d.due(2, false, false, 9.0), "written once");
    }
}
