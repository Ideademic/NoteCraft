//! Unread pages (History ▸ Unread): pages that changed since the user last saw them, from
//! outside NoteCraft (another app or device writing the notebook's files) or marked unread.
//!
//! [`ReadState`] remembers, per page, the `modified` time the user last saw. A notebook seen for
//! the first time counts as read through; after that a page whose `modified` is newer than what
//! was seen (or that is new) is unread. Viewing or editing a page in NoteCraft marks it read.
//! The app stores the state between runs ([`ReadState::to_json`]); it is not part of the
//! notebook files.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use notecraft_doc::{Guid, Notebook, Page};
use serde::{Deserialize, Serialize};

/// Most pages remembered (the oldest-seen are forgotten first).
pub const MAX_READ_PAGES: usize = 200_000;
/// Most notebooks remembered.
pub const MAX_READ_NOTEBOOKS: usize = 1_000;

/// What the user has seen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReadState {
    /// Notebooks seen before: their pages can be unread.
    notebooks: BTreeSet<Guid>,
    /// Page → the `modified` time last seen.
    pages: BTreeMap<Guid, DateTime<Utc>>,
}

impl ReadState {
    /// From the app's stored JSON; junk gives an empty state.
    pub fn from_json(json: &str) -> ReadState {
        let mut s: ReadState = serde_json::from_str(json).unwrap_or_default();
        s.bound();
        s
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    /// Whether `page` of notebook `notebook` is unread.
    pub fn is_unread(&self, notebook: Guid, page: &Page) -> bool {
        self.notebooks.contains(&notebook) && self.pages.get(&page.id).is_none_or(|seen| page.modified > *seen)
    }

    /// The first time a notebook is opened every page in it counts as read; later opens leave
    /// what changed meanwhile unread.
    pub fn adopt(&mut self, nb: &Notebook) {
        if self.notebooks.contains(&nb.id) {
            return;
        }
        self.notebooks.insert(nb.id);
        for s in nb.all_sections() {
            for p in &s.pages {
                self.pages.insert(p.id, p.modified);
            }
        }
        self.bound();
    }

    pub fn mark_read(&mut self, page: &Page) {
        if self.pages.get(&page.id) != Some(&page.modified) {
            self.pages.insert(page.id, page.modified);
            if self.pages.len() > MAX_READ_PAGES {
                self.bound();
            }
        }
    }

    pub fn mark_unread(&mut self, page: Guid) {
        self.pages.remove(&page);
    }

    /// Keeps the maps within their caps, dropping the pages seen longest ago.
    fn bound(&mut self) {
        while self.notebooks.len() > MAX_READ_NOTEBOOKS {
            self.notebooks.pop_first();
        }
        if self.pages.len() > MAX_READ_PAGES {
            let mut by_time: Vec<(DateTime<Utc>, Guid)> = self.pages.iter().map(|(g, t)| (*t, *g)).collect();
            by_time.sort();
            let extra = self.pages.len() - MAX_READ_PAGES;
            for (_, g) in by_time.into_iter().take(extra) {
                self.pages.remove(&g);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_open_is_read_then_outside_changes_are_unread() {
        let mut nb = Notebook::new_default();
        let mut st = ReadState::default();
        let pid = nb.first_section().unwrap().pages[0].id;
        assert!(!st.is_unread(nb.id, &nb.first_section().unwrap().pages[0]), "unknown notebooks have nothing unread");
        st.adopt(&nb);
        let nid = nb.id;
        let page = nb.page_mut(pid).unwrap();
        assert!(!st.is_unread(nid, page));
        page.modified += chrono::Duration::seconds(5);
        assert!(st.is_unread(nid, page));
        st.mark_read(page);
        assert!(!st.is_unread(nid, page));
        st.mark_unread(pid);
        assert!(st.is_unread(nid, page));
        let new = Page::new("new");
        assert!(st.is_unread(nb.id, &new), "a page added elsewhere is unread");
        // Adopting again doesn't mark changes read.
        st.adopt(&nb);
        assert!(st.is_unread(nb.id, nb.find_page(pid).unwrap().1));
        let back = ReadState::from_json(&st.to_json());
        assert_eq!(back, st);
        assert_eq!(ReadState::from_json("junk"), ReadState::default());
    }
}
