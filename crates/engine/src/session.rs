//! The editing session: open notebooks, where the user is, undo, what needs saving.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use notecraft_doc::{Guid, Notebook, Page, Section, SectionEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::history::{History, Snapshot};
use crate::io::{IoError, Origin, Storage};
use crate::{CmdError, CmdOutput, registry};

/// Autosave waits this long after the last change, seconds.
pub const AUTOSAVE_DELAY_SECS: f64 = 2.0;
/// …but never longer than this after the first unsaved change, seconds (steady typing).
pub const AUTOSAVE_MAX_WAIT_SECS: f64 = 30.0;

/// Where the user is: the current notebook, section and page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub notebook: Option<Guid>,
    pub section: Option<Guid>,
    pub page: Option<Guid>,
}

/// The editing session. Front ends hold one and change it only through [`Session::run`]
/// (navigation and undo included), reading the public fields to draw.
pub struct Session {
    /// Open notebooks, in the notebook pane's order.
    pub notebooks: Vec<Notebook>,
    /// Where each open notebook lives (by notebook id).
    pub origins: HashMap<Guid, Origin>,
    pub current: Location,
    /// Sections changed since they were last saved.
    pub dirty: HashSet<Guid>,
    /// Notebooks whose table of contents (name, colour, entries, their order or colours) changed
    /// since it was last saved.
    pub dirty_tocs: HashSet<Guid>,
    /// Ink strokes on the current page selected by the lasso.
    pub ink_selection: Vec<Guid>,
    /// Saves notebooks ([`Session::save_dirty`], autosave). `None`: nothing is saved (tests,
    /// the CLI).
    pub storage: Option<Box<dyn Storage>>,
    /// Messages for the user (a file with content NoteCraft can't keep, a failed autosave…),
    /// oldest first. The UI shows and drains them.
    pub notices: Vec<String>,
    /// Bumped by every change to the notebooks (commands, undo, redo): caches compare it.
    pub revision: u64,
    pub(crate) history: History,
    autosave: AutosaveClock,
}

#[derive(Default)]
struct AutosaveClock {
    seen_revision: u64,
    last_change: Option<f64>,
    first_change: Option<f64>,
}

impl Default for Session {
    fn default() -> Self {
        Session::empty()
    }
}

impl Session {
    /// No notebooks open.
    pub fn empty() -> Session {
        Session {
            notebooks: Vec::new(),
            origins: HashMap::new(),
            current: Location::default(),
            dirty: HashSet::new(),
            dirty_tocs: HashSet::new(),
            ink_selection: Vec::new(),
            storage: None,
            notices: Vec::new(),
            revision: 0,
            history: History::default(),
            autosave: AutosaveClock::default(),
        }
    }

    /// One notebook open, at its first page.
    pub fn new(notebook: Notebook, origin: Origin) -> Session {
        let mut s = Session::empty();
        s.add_notebook(notebook, origin);
        s
    }

    /// The first-start notebook ([`Notebook::new_default`]), not saved anywhere yet.
    pub fn new_default() -> Session {
        Session::new(Notebook::new_default(), Origin::Scratch)
    }

    /// Opens `notebook` (after the others) and goes to its first page. Clears undo.
    pub fn add_notebook(&mut self, notebook: Notebook, origin: Origin) -> Guid {
        let id = notebook.id;
        if let Some(s) = notebook.all_sections().iter().find(|s| s.source.unsupported_content) {
            self.notices.push(format!("\u{201c}{}\u{201d} has content NoteCraft can show but not edit; it is kept when saving.", s.name));
        }
        self.notebooks.push(notebook);
        self.origins.insert(id, origin);
        self.history.clear();
        self.current = Location { notebook: Some(id), section: None, page: None };
        self.normalize_location();
        self.revision += 1;
        id
    }

    /// Runs the command `id` with `params` (`Value::Null` for none).
    ///
    /// Mutating commands are atomic: on error (or an escaped panic, the last-resort guard) the
    /// notebooks are restored. On success an undo step is recorded unless the output says
    /// nothing changed, or `params.mergeKey` continues the newest step; changed sections are
    /// marked dirty for autosave.
    pub fn run(&mut self, id: &str, params: &Value) -> Result<CmdOutput, CmdError> {
        let spec = *registry().get(id).ok_or_else(|| CmdError::Unknown(id.chars().take(64).collect()))?;
        if !(spec.enabled)(self) {
            return Err(CmdError::Disabled(spec.id.to_owned()));
        }
        let before = spec.mutates.then(|| self.snapshot());
        let result = match catch_unwind(AssertUnwindSafe(|| (spec.run)(self, params))) {
            Ok(r) => r,
            Err(_) => {
                log::error!("command {} panicked; state restored", spec.id);
                Err(CmdError::Failed(format!("internal error in {}", spec.id)))
            }
        };
        let result = result.map_err(|e| match e {
            CmdError::NotImplemented(_) => CmdError::NotImplemented(spec.id),
            e => e,
        });
        match (result, before) {
            (Err(e), Some(before)) => {
                self.restore(before);
                Err(e)
            }
            (Err(e), None) => {
                self.normalize_location();
                Err(e)
            }
            (Ok(out), Some(before)) => {
                if out.changed {
                    self.note_changes(&before.notebooks);
                    let merge = params.get("mergeKey").and_then(Value::as_str).map(str::to_owned);
                    self.history.checkpoint(before, merge);
                }
                self.normalize_location();
                Ok(out)
            }
            (Ok(out), None) => {
                self.normalize_location();
                Ok(out)
            }
        }
    }

    /// The command's [`crate::CommandSpec::enabled`] (false for unknown ids).
    pub fn is_enabled(&self, id: &str) -> bool {
        registry().get(id).is_some_and(|s| (s.enabled)(self))
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Number of undo steps (for tests and the UI's undo list).
    pub fn undo_depth(&self) -> usize {
        self.history.undo_len()
    }

    /// Ends typing-merge, so the next `text.setParagraphs` starts a new undo step (the caret
    /// left the outline, say).
    pub fn break_undo_merge(&mut self) {
        self.history.break_merge();
    }

    /// Goes back one step. `false` when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let now = self.snapshot();
        match self.history.undo(now) {
            Some(prev) => {
                self.restore_and_mark(prev);
                true
            }
            None => false,
        }
    }

    /// Goes forward one step. `false` when there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let now = self.snapshot();
        match self.history.redo(now) {
            Some(next) => {
                self.restore_and_mark(next);
                true
            }
            None => false,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot { notebooks: self.notebooks.clone(), current: self.current }
    }

    fn restore(&mut self, s: Snapshot) {
        self.notebooks = s.notebooks;
        self.current = s.current;
        self.normalize_location();
    }

    fn restore_and_mark(&mut self, s: Snapshot) {
        let before = std::mem::replace(&mut self.notebooks, s.notebooks);
        self.current = s.current;
        self.note_changes(&before);
        self.ink_selection.clear();
        self.normalize_location();
    }

    /// Marks what differs from `before` dirty and bumps [`Session::revision`].
    fn note_changes(&mut self, before: &[Notebook]) {
        self.revision += 1;
        for nb in &self.notebooks {
            let old = before.iter().find(|b| b.id == nb.id);
            if old.is_none_or(|o| toc_signature(o) != toc_signature(nb)) {
                self.dirty_tocs.insert(nb.id);
            }
            for s in nb.all_sections() {
                if old.and_then(|o| o.section(s.id)) != Some(s) {
                    self.dirty.insert(s.id);
                }
            }
        }
    }

    /// Whether anything is unsaved.
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty() || !self.dirty_tocs.is_empty()
    }

    /// Whether notebook `id` has unsaved changes.
    pub fn notebook_is_dirty(&self, id: Guid) -> bool {
        self.dirty_tocs.contains(&id) || self.notebook(id).is_some_and(|nb| nb.all_sections().iter().any(|s| self.dirty.contains(&s.id)))
    }

    /// Saves every notebook with unsaved changes through [`Session::storage`]; returns how many
    /// were saved. Without storage nothing is saved (`Ok(0)`) and the changes stay dirty.
    pub fn save_dirty(&mut self) -> Result<usize, IoError> {
        crate::io::save_dirty(self)
    }

    /// Autosave, called by the front end every frame (or timer) with a monotonic clock in
    /// seconds. Saves [`AUTOSAVE_DELAY_SECS`] after the last change, or at the latest
    /// [`AUTOSAVE_MAX_WAIT_SECS`] after the first unsaved one. Returns the save's result when it
    /// tried to save; a failure is also added to [`Session::notices`].
    pub fn tick(&mut self, now: f64) -> Option<Result<usize, IoError>> {
        if !now.is_finite() {
            return None;
        }
        if self.revision != self.autosave.seen_revision {
            self.autosave.seen_revision = self.revision;
            self.autosave.last_change = Some(now);
            self.autosave.first_change.get_or_insert(now);
        }
        if !self.is_dirty() || self.storage.is_none() {
            self.autosave.first_change = None;
            return None;
        }
        let last = self.autosave.last_change.unwrap_or(now);
        let first = self.autosave.first_change.unwrap_or(now);
        if now - last < AUTOSAVE_DELAY_SECS && now - first < AUTOSAVE_MAX_WAIT_SECS {
            return None;
        }
        let result = self.save_dirty();
        match &result {
            Ok(_) => self.autosave.first_change = None,
            Err(e) => {
                self.notices.push(format!("Couldn't save: {e}"));
                // Try again after another delay rather than every frame.
                self.autosave.last_change = Some(now);
                self.autosave.first_change = Some(now);
            }
        }
        Some(result)
    }

    // --- lookups -------------------------------------------------------------------------

    pub fn notebook(&self, id: Guid) -> Option<&Notebook> {
        self.notebooks.iter().find(|n| n.id == id)
    }

    pub fn notebook_mut(&mut self, id: Guid) -> Option<&mut Notebook> {
        self.notebooks.iter_mut().find(|n| n.id == id)
    }

    /// The open notebook holding section `id`.
    pub fn notebook_of_section(&self, id: Guid) -> Option<&Notebook> {
        self.notebooks.iter().find(|n| n.section(id).is_some())
    }

    pub fn current_notebook(&self) -> Option<&Notebook> {
        self.notebook(self.current.notebook?)
    }

    pub fn current_notebook_mut(&mut self) -> Option<&mut Notebook> {
        let id = self.current.notebook?;
        self.notebook_mut(id)
    }

    pub fn current_section(&self) -> Option<&Section> {
        self.current_notebook()?.section(self.current.section?)
    }

    pub fn current_section_mut(&mut self) -> Option<&mut Section> {
        let id = self.current.section?;
        self.current_notebook_mut()?.section_mut(id)
    }

    pub fn current_page(&self) -> Option<&Page> {
        self.current_section()?.page(self.current.page?)
    }

    pub fn current_page_mut(&mut self) -> Option<&mut Page> {
        let id = self.current.page?;
        self.current_section_mut()?.page_mut(id)
    }

    /// Section `id` in any open notebook.
    pub fn section_mut(&mut self, id: Guid) -> Option<&mut Section> {
        self.notebooks.iter_mut().find_map(|n| n.section_mut(id))
    }

    /// Page `id` in any open notebook.
    pub fn page_mut(&mut self, id: Guid) -> Option<&mut Page> {
        self.notebooks.iter_mut().find_map(|n| n.page_mut(id))
    }

    /// Makes [`Session::current`] point at things that exist: the first notebook, its first
    /// section, that section's first page, wherever the current one is gone. Leaves the
    /// lasso selection only when the page is unchanged.
    pub fn normalize_location(&mut self) {
        let old_page = self.current.page;
        let nb = self.current.notebook.and_then(|id| self.notebook(id)).or_else(|| self.notebooks.first());
        let Some(nb) = nb else {
            self.current = Location::default();
            self.ink_selection.clear();
            return;
        };
        let section = self.current.section.and_then(|id| nb.section(id)).or_else(|| nb.first_section());
        let page = section.and_then(|s| self.current.page.and_then(|id| s.page(id)).or_else(|| s.pages.first()));
        self.current = Location { notebook: Some(nb.id), section: section.map(|s| s.id), page: page.map(|p| p.id) };
        if self.current.page != old_page {
            self.ink_selection.clear();
        }
    }
}

/// What a notebook's table of contents records: name, colour, and the entry tree's ids, names
/// and colours in order.
fn toc_signature(nb: &Notebook) -> String {
    fn walk(entries: &[SectionEntry], depth: usize, out: &mut String) {
        if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
            return;
        }
        for e in entries {
            match e {
                SectionEntry::Section(s) => out.push_str(&format!("s{}|{}|{};", s.id, s.name, s.color.to_hex())),
                SectionEntry::Group(g) => {
                    out.push_str(&format!("g{}|{}[", g.id, g.name));
                    walk(&g.entries, depth + 1, out);
                    out.push(']');
                }
            }
        }
    }
    let mut out = format!("{}|{}|", nb.name, nb.color.to_hex());
    walk(&nb.sections, 0, &mut out);
    out
}
