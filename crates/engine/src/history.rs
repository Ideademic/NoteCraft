//! Snapshot undo: each undo step keeps the notebooks as they were before a command (pictures and
//! files are `Arc`-shared, so a snapshot copies text and structure only).

use notecraft_doc::Notebook;

use crate::Location;

/// Most undo steps kept.
pub const MAX_UNDO: usize = 500;

/// The state one undo step goes back to.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub notebooks: Vec<Notebook>,
    pub current: Location,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The merge key of the newest undo step: typing in the same outline (`text.setParagraphs`
    /// with the same `mergeKey`) folds into one step.
    last_merge: Option<String>,
}

impl History {
    /// Records `before` as an undo step, unless `merge` continues the newest step.
    pub(crate) fn checkpoint(&mut self, before: Snapshot, merge: Option<String>) {
        self.redo.clear();
        if merge.is_some() && merge == self.last_merge && !self.undo.is_empty() {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.last_merge = merge;
    }

    /// Ends typing-merge: the next command starts a new step.
    pub(crate) fn break_merge(&mut self) {
        self.last_merge = None;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The state to go back to; `now` goes on the redo stack.
    pub(crate) fn undo(&mut self, now: Snapshot) -> Option<Snapshot> {
        let prev = self.undo.pop()?;
        self.redo.push(now);
        self.last_merge = None;
        Some(prev)
    }

    /// The state to go forward to; `now` goes back on the undo stack.
    pub(crate) fn redo(&mut self, now: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(now);
        self.last_merge = None;
        Some(next)
    }

    pub(crate) fn clear(&mut self) {
        *self = History::default();
    }

    pub(crate) fn undo_len(&self) -> usize {
        self.undo.len()
    }
}
