//! Page history: earlier versions of a page, and the notebook's recycle bin of deleted pages and
//! sections (History tab).
//!
//! - A [`PageVersion`] is a read-only copy of a page as it was, kept on the page
//!   ([`crate::Page::versions`], oldest first). The engine takes them automatically while the page
//!   is edited; the `one` crate stores them as MS-ONE version history pages.
//! - A notebook's [`RecycleBin`] holds deleted sections and pages with where they came from, so
//!   they can be restored. On disk it is the notebook's `OneNote_RecycleBin` folder: each deleted
//!   section as its own `.one`, deleted pages in `OneNote_DeletedPages.one`.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{Guid, Page, Section};

/// Most versions kept per page: the oldest go first.
pub const MAX_VERSIONS_PER_PAGE: usize = 20;
/// Most bytes (text and pictures, as [`page_size`] counts them) kept in one page's versions:
/// the oldest go first.
pub const MAX_VERSION_BYTES: usize = 32 * 1024 * 1024;
/// Days a deleted page or section stays in the recycle bin.
pub const RECYCLE_BIN_DAYS: i64 = 60;
/// Most entries the recycle bin keeps (hostile files): the oldest go first.
pub const MAX_RECYCLE_BIN_ENTRIES: usize = 10_000;

/// An earlier version of a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageVersion {
    /// Identifies the version (`history.restoreVersion {version}`).
    pub id: Guid,
    /// When the page last changed before this version was kept: the version's date in the page
    /// list.
    pub saved: DateTime<Utc>,
    /// The page as it was (its own `versions` empty). Shared, so undo snapshots stay cheap.
    pub page: Arc<Page>,
}

impl PageVersion {
    /// A version of `page` as it is now (its versions dropped), dated by its last change.
    pub fn of(page: &Page) -> PageVersion {
        let mut copy = page.clone();
        copy.versions.clear();
        PageVersion { id: Guid::new(), saved: page.modified, page: Arc::new(copy) }
    }
}

/// Roughly how many bytes a page holds: its text and every picture, file and stroke.
pub fn page_size(page: &Page) -> usize {
    use crate::ElementKind;
    let mut n = page.title.len();
    for e in &page.elements {
        n = n.saturating_add(match &e.kind {
            ElementKind::Outline(o) => o.paragraphs().map(|p| p.runs.iter().map(|r| r.text.len() + 16).sum::<usize>() + 16).sum(),
            ElementKind::Image(i) => i.data.len(),
            ElementKind::EmbeddedFile { bytes, .. } => bytes.len(),
            ElementKind::Printout(p) => p.file_bytes.len().saturating_add(p.pages.iter().map(|i| i.data.len()).sum()),
            ElementKind::Opaque { data, .. } => data.len(),
        });
    }
    n.saturating_add(page.ink.iter().map(|s| s.points.len() * 12 + 32).sum())
}

/// Drops the oldest versions beyond [`MAX_VERSIONS_PER_PAGE`] and [`MAX_VERSION_BYTES`].
pub fn trim_versions(versions: &mut Vec<PageVersion>) {
    versions.sort_by_key(|v| v.saved);
    if versions.len() > MAX_VERSIONS_PER_PAGE {
        let extra = versions.len() - MAX_VERSIONS_PER_PAGE;
        versions.drain(..extra);
    }
    let mut total = 0usize;
    let mut keep_from = 0;
    for (i, v) in versions.iter().enumerate().rev() {
        total = total.saturating_add(page_size(&v.page));
        if total > MAX_VERSION_BYTES {
            keep_from = i + 1;
            break;
        }
    }
    versions.drain(..keep_from.min(versions.len()));
}

/// A deleted section and where it was.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedSection {
    pub section: Section,
    pub deleted: DateTime<Utc>,
    /// The section groups it was in, from the notebook root down (empty: top level).
    #[serde(default)]
    pub group_path: Vec<Guid>,
    /// Its position among its group's entries.
    #[serde(default)]
    pub index: usize,
}

/// A deleted page (with its subpages, which follow it with higher levels) and where it was.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedPage {
    pub page: Page,
    pub deleted: DateTime<Utc>,
    /// The section it was in (nil when unknown: a page OneNote deleted).
    #[serde(default)]
    pub section: Guid,
    /// That section's name when it was deleted (shown in the recycle bin).
    #[serde(default)]
    pub section_name: String,
    /// Its position in the section's page list.
    #[serde(default)]
    pub index: usize,
}

/// A notebook's deleted sections and pages, oldest deletion first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecycleBin {
    pub sections: Vec<DeletedSection>,
    pub pages: Vec<DeletedPage>,
}

impl RecycleBin {
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty() && self.pages.is_empty()
    }

    /// Entries in the bin: sections plus pages.
    pub fn len(&self) -> usize {
        self.sections.len() + self.pages.len()
    }

    /// Removes what was deleted more than [`RECYCLE_BIN_DAYS`] before `now`, and the oldest
    /// beyond [`MAX_RECYCLE_BIN_ENTRIES`]; returns how many went.
    pub fn expire(&mut self, now: DateTime<Utc>) -> usize {
        let before = self.len();
        if let Some(cutoff) = now.checked_sub_signed(Duration::days(RECYCLE_BIN_DAYS)) {
            self.sections.retain(|s| s.deleted >= cutoff);
            self.pages.retain(|p| p.deleted >= cutoff);
        }
        self.cap();
        before - self.len()
    }

    /// Removes the oldest entries beyond [`MAX_RECYCLE_BIN_ENTRIES`].
    pub fn cap(&mut self) {
        let Some(extra) = self.len().checked_sub(MAX_RECYCLE_BIN_ENTRIES).filter(|n| *n > 0) else { return };
        self.sections.sort_by_key(|s| s.deleted);
        self.pages.sort_by_key(|p| p.deleted);
        // Drop the `extra` oldest of both lists together.
        let (mut i, mut j) = (0, 0);
        for _ in 0..extra {
            let s = self.sections.get(i).map(|s| s.deleted);
            let p = self.pages.get(j).map(|p| p.deleted);
            match (s, p) {
                (Some(a), Some(b)) if a <= b => i += 1,
                (_, Some(_)) => j += 1,
                (Some(_), None) => i += 1,
                (None, None) => break,
            }
        }
        self.sections.drain(..i);
        self.pages.drain(..j);
    }

    pub fn page(&self, id: Guid) -> Option<&DeletedPage> {
        self.pages.iter().find(|p| p.page.id == id)
    }

    pub fn section(&self, id: Guid) -> Option<&DeletedSection> {
        self.sections.iter().find(|s| s.section.id == id)
    }

    /// Whether `id` is a page or section in the bin.
    pub fn contains(&self, id: Guid) -> bool {
        self.page(id).is_some() || self.section(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, ElementKind, Outline, PageElement, page::CONTENT_ORIGIN};

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    #[test]
    fn versions_are_capped_by_count_and_size() {
        let mut p = Page::new("p");
        let mut vs: Vec<PageVersion> = (0..30)
            .map(|i| {
                p.modified = at(1_700_000_000 + i);
                PageVersion::of(&p)
            })
            .collect();
        vs.reverse();
        trim_versions(&mut vs);
        assert_eq!(vs.len(), MAX_VERSIONS_PER_PAGE);
        assert_eq!(vs.first().unwrap().saved, at(1_700_000_010), "oldest dropped, sorted");
        let mut big = Page::new("big");
        big.elements.push(PageElement::new(ElementKind::EmbeddedFile {
            name: "x".into(),
            bytes: Arc::new(vec![0; MAX_VERSION_BYTES / 2 + 1]),
            pos: CONTENT_ORIGIN,
        }));
        let mut vs: Vec<PageVersion> = (0..3)
            .map(|i| {
                big.modified = at(1_700_000_000 + i);
                PageVersion::of(&big)
            })
            .collect();
        trim_versions(&mut vs);
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].saved, at(1_700_000_002));
    }

    #[test]
    fn version_of_drops_nested_versions() {
        let mut p = Page::new("p");
        p.elements.push(PageElement::new(ElementKind::Outline(Outline::with_text(CONTENT_ORIGIN, "hi"))));
        p.versions.push(PageVersion::of(&p));
        let v = PageVersion::of(&p);
        assert!(v.page.versions.is_empty());
        assert_eq!(v.page.elements, p.elements);
    }

    #[test]
    fn bin_expires_after_sixty_days() {
        let now = at(1_800_000_000);
        let mut bin = RecycleBin::default();
        let old = now - Duration::days(61);
        let recent = now - Duration::days(59);
        bin.pages.push(DeletedPage { page: Page::new("old"), deleted: old, section: Guid::NIL, section_name: String::new(), index: 0 });
        bin.pages.push(DeletedPage { page: Page::new("new"), deleted: recent, section: Guid::NIL, section_name: String::new(), index: 0 });
        bin.sections.push(DeletedSection { section: Section::new("s", Color::WHITE), deleted: old, group_path: vec![], index: 0 });
        assert_eq!(bin.expire(now), 2);
        assert_eq!(bin.len(), 1);
        assert_eq!(bin.pages[0].page.title, "new");
        assert!(bin.contains(bin.pages[0].page.id));
    }
}
