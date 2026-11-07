//! The recent notebooks list (File ▸ Open). The engine keeps it up to date in
//! [`crate::Session::recent`] when notebooks are opened or first saved; the app loads and stores
//! it with [`RecentNotebooks::from_json`] / [`RecentNotebooks::to_json`] (in its settings
//! folder, or browser storage).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Most notebooks remembered.
pub const MAX_RECENT: usize = 20;

/// One remembered notebook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentNotebook {
    /// The notebook folder, `.onepkg` or `.one` it was opened from.
    pub path: PathBuf,
    pub name: String,
    pub opened: DateTime<Utc>,
}

/// Most recently opened first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentNotebooks {
    pub entries: Vec<RecentNotebook>,
}

impl RecentNotebooks {
    /// Puts `path` first (once), keeping at most [`MAX_RECENT`].
    pub fn add(&mut self, path: PathBuf, name: impl Into<String>) {
        self.entries.retain(|e| e.path != path);
        let name: String = name.into().chars().filter(|c| !c.is_control()).take(255).collect();
        self.entries.insert(0, RecentNotebook { path, name, opened: notecraft_doc::now() });
        self.entries.truncate(MAX_RECENT);
    }

    /// Forgets `path` (it was moved or deleted). Returns whether it was listed.
    pub fn remove(&mut self, path: &std::path::Path) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != path);
        self.entries.len() != before
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Reads a saved list; anything unreadable gives an empty list (it is only a convenience).
    pub fn from_json(text: &str) -> RecentNotebooks {
        let mut r: RecentNotebooks = serde_json::from_str(text).unwrap_or_default();
        r.entries.truncate(MAX_RECENT);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_first_without_duplicates() {
        let mut r = RecentNotebooks::default();
        for i in 0..(MAX_RECENT + 5) {
            r.add(PathBuf::from(format!("/n/{i}")), format!("N{i}"));
        }
        assert_eq!(r.entries.len(), MAX_RECENT);
        r.add(PathBuf::from("/n/10"), "again");
        assert_eq!(r.entries[0].name, "again");
        assert_eq!(r.entries.iter().filter(|e| e.path.as_path() == std::path::Path::new("/n/10")).count(), 1);
        let back = RecentNotebooks::from_json(&r.to_json());
        assert_eq!(back, r);
        assert!(back.clone().remove(std::path::Path::new("/n/10")));
        assert_eq!(RecentNotebooks::from_json("garbage"), RecentNotebooks::default());
    }
}
