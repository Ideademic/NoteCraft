//! Crash recovery and whole-session blobs.
//!
//! **Recovery (desktop).** With [`Session::enable_recovery`] the session keeps a snapshot of its
//! unsaved notebooks in a folder the app chooses (its data folder): written on a worker thread
//! at the first change and then at most every [`RECOVERY_INTERVAL_SECS`] while anything is
//! unsaved, removed once everything is saved. After a crash the next start lists them with
//! [`Session::recovery_candidates`] and offers [`Session::restore_recovery`] or
//! [`Session::discard_recovery`]. Each snapshot is `<id>.json` (the notebooks) plus a small
//! `<id>.meta.json` the list reads.
//!
//! **Blobs (web).** [`Session::to_blob`] / [`Session::from_blob`] keep a whole session (every
//! notebook, where the user was) as one JSON value for browser storage.

use chrono::{DateTime, Utc};
use notecraft_doc::{Guid, Notebook};
use serde::{Deserialize, Serialize};

use super::{IoError, Origin};
use crate::{Location, Session};

/// Most often a recovery snapshot is written while changes are unsaved, seconds.
pub const RECOVERY_INTERVAL_SECS: f64 = 5.0;
/// Largest snapshot or blob read back, bytes.
pub const MAX_SNAPSHOT_BYTES: usize = 1 << 30;
const FORMAT: &str = "notecraft-session";
const VERSION: u32 = 1;

/// One notebook in a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedNotebook {
    pub notebook: Notebook,
    pub origin: Origin,
    /// Sections that were unsaved.
    #[serde(default)]
    pub dirty_sections: Vec<Guid>,
    #[serde(default)]
    pub toc_dirty: bool,
}

/// A recovery snapshot or a session blob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub format: String,
    pub version: u32,
    pub saved_at: DateTime<Utc>,
    pub notebooks: Vec<SavedNotebook>,
    #[serde(default)]
    pub current: Location,
}

/// A recovery snapshot left by an earlier run, as listed for the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    /// The snapshot file (pass it to restore or discard).
    pub path: std::path::PathBuf,
    pub saved_at: DateTime<Utc>,
    /// Names of the notebooks it holds.
    pub notebooks: Vec<String>,
}

impl Session {
    /// A snapshot of the notebooks with unsaved changes (`only_dirty`) or of all of them.
    fn snapshot_of(&self, only_dirty: bool) -> SessionSnapshot {
        let notebooks = self
            .notebooks
            .iter()
            .filter(|nb| !only_dirty || self.notebook_is_dirty(nb.id))
            .map(|nb| SavedNotebook {
                notebook: nb.clone(),
                origin: self.origins.get(&nb.id).cloned().unwrap_or(Origin::Scratch),
                dirty_sections: nb.all_sections().iter().map(|s| s.id).filter(|id| self.dirty.contains(id)).collect(),
                toc_dirty: self.dirty_tocs.contains(&nb.id),
            })
            .collect();
        SessionSnapshot { format: FORMAT.into(), version: VERSION, saved_at: notecraft_doc::now(), notebooks, current: self.current }
    }

    /// The whole session (every notebook, where the user is) as one JSON blob, for browser
    /// storage.
    pub fn to_blob(&self) -> Result<Vec<u8>, IoError> {
        serde_json::to_vec(&self.snapshot_of(false)).map_err(|e| IoError::Io(e.to_string()))
    }

    /// A session from [`Session::to_blob`]'s bytes. Unsaved state is kept (dirty sections stay
    /// dirty); undo starts empty.
    pub fn from_blob(bytes: &[u8]) -> Result<Session, IoError> {
        let snap = parse_snapshot(bytes)?;
        let mut s = Session::empty();
        for saved in snap.notebooks {
            s.adopt(saved);
        }
        s.current = snap.current;
        s.normalize_location();
        Ok(s)
    }

    /// Opens (or, when its id is already open, replaces) a notebook from a snapshot, unsaved
    /// changes marked.
    fn adopt(&mut self, saved: SavedNotebook) -> Guid {
        let SavedNotebook { notebook, origin, dirty_sections, toc_dirty } = saved;
        let nb = sanitize(notebook);
        let id = nb.id;
        if let Some(slot) = self.notebooks.iter_mut().find(|n| n.id == id) {
            *slot = nb;
        } else {
            if !self.layouts.contains_key(&id)
                && let Some(l) = super::initial_layout(&nb, &origin)
            {
                self.layouts.insert(id, l);
            }
            self.notebooks.push(nb);
        }
        self.origins.insert(id, origin);
        self.dirty.extend(dirty_sections);
        if toc_dirty {
            self.dirty_tocs.insert(id);
        }
        self.history.clear();
        self.revision += 1;
        id
    }
}

fn parse_snapshot(bytes: &[u8]) -> Result<SessionSnapshot, IoError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(IoError::Io("the saved session is too large".into()));
    }
    let snap: SessionSnapshot = serde_json::from_slice(bytes).map_err(|e| IoError::Io(format!("not a NoteCraft session: {e}")))?;
    if snap.format != FORMAT || snap.version > VERSION {
        return Err(IoError::Unsupported(format!("a {} v{} session", snap.format.chars().take(40).collect::<String>(), snap.version)));
    }
    Ok(snap)
}

/// Bounds a notebook read from JSON the way the file readers bound theirs.
pub(crate) fn sanitize(mut nb: Notebook) -> Notebook {
    for s in nb.all_sections_mut() {
        for pg in &mut s.pages {
            pg.level = pg.level.min(notecraft_doc::page::MAX_PAGE_LEVEL);
            pg.elements.truncate(notecraft_doc::page::MAX_ELEMENTS_PER_PAGE);
            pg.ink.truncate(notecraft_doc::ink::MAX_INK_PER_PAGE);
            pg.ink = std::mem::take(&mut pg.ink).into_iter().map(notecraft_doc::InkStroke::sanitized).collect();
            for el in &mut pg.elements {
                if let Some(p) = el.pos() {
                    el.set_pos(p);
                }
                if let notecraft_doc::ElementKind::Outline(o) = &mut el.kind {
                    o.paragraphs.truncate(notecraft_doc::page::MAX_PARAGRAPHS);
                    o.paragraphs = std::mem::take(&mut o.paragraphs).into_iter().map(notecraft_doc::Paragraph::sanitized).collect();
                    o.width = if o.width.is_finite() { o.width.clamp(1.0, 100_000.0) } else { notecraft_doc::page::DEFAULT_OUTLINE_WIDTH };
                }
            }
        }
    }
    nb
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use desktop::Recovery;

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread::JoinHandle;

    use super::{IoError, RECOVERY_INTERVAL_SECS, RecoveryInfo, SessionSnapshot, parse_snapshot};
    use crate::Session;
    use crate::io::safe_save;

    /// This session's recovery snapshot.
    pub(crate) struct Recovery {
        dir: PathBuf,
        id: String,
        written_rev: Option<u64>,
        last_write: Option<f64>,
        has_file: bool,
        worker: Option<JoinHandle<()>>,
    }

    impl Recovery {
        fn data(&self) -> PathBuf {
            self.dir.join(format!("{}.json", self.id))
        }
        fn meta(&self) -> PathBuf {
            self.dir.join(format!("{}.meta.json", self.id))
        }
        fn busy(&self) -> bool {
            self.worker.as_ref().is_some_and(|w| !w.is_finished())
        }
        fn wait(&mut self) {
            if let Some(w) = self.worker.take() {
                let _ = w.join();
            }
        }
        fn remove_files(&mut self) {
            self.wait();
            let _ = fs::remove_file(self.data());
            let _ = fs::remove_file(self.meta());
            self.has_file = false;
            self.written_rev = None;
        }
    }

    fn write_snapshot(data: &Path, meta: &Path, snap: &SessionSnapshot) -> Result<(), IoError> {
        let info = RecoveryInfo {
            path: data.to_path_buf(),
            saved_at: snap.saved_at,
            notebooks: snap.notebooks.iter().map(|n| n.notebook.name.clone()).collect(),
        };
        let bytes = serde_json::to_vec(snap).map_err(|e| IoError::Io(e.to_string()))?;
        safe_save::write(data, &bytes).map_err(|e| IoError::Io(format!("{}: {e}", data.display())))?;
        let m = serde_json::to_vec(&info).map_err(|e| IoError::Io(e.to_string()))?;
        safe_save::write(meta, &m).map_err(|e| IoError::Io(format!("{}: {e}", meta.display())))
    }

    impl Session {
        /// Keeps recovery snapshots of unsaved work in `dir` (created if needed). Call once at
        /// start, after listing [`Session::recovery_candidates`].
        pub fn enable_recovery(&mut self, dir: PathBuf) -> Result<(), IoError> {
            fs::create_dir_all(&dir).map_err(|e| IoError::Io(format!("{}: {e}", dir.display())))?;
            let id = format!("session-{}", notecraft_doc::Guid::new().to_string().trim_matches(['{', '}']).to_lowercase());
            self.recovery = Some(Recovery { dir, id, written_rev: None, last_write: None, has_file: false, worker: None });
            Ok(())
        }

        /// Snapshots left in the recovery folder by earlier runs (not this session's), newest
        /// first. Lists `dir` when recovery isn't enabled yet.
        pub fn recovery_candidates_in(&self, dir: &Path) -> Vec<RecoveryInfo> {
            let own = self.recovery.as_ref().map(Recovery::data);
            let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
            let mut out: Vec<RecoveryInfo> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".meta.json")))
                .filter_map(|meta| {
                    let bytes = fs::read(&meta).ok()?;
                    let mut info: RecoveryInfo = serde_json::from_slice(&bytes).ok()?;
                    // The data file sits beside its meta file, whatever the meta file says.
                    let name = meta.file_name()?.to_string_lossy().replace(".meta.json", ".json");
                    info.path = meta.with_file_name(name);
                    (info.path.is_file() && Some(&info.path) != own.as_ref()).then_some(info)
                })
                .collect();
            out.sort_by_key(|a| std::cmp::Reverse(a.saved_at));
            out
        }

        /// [`Session::recovery_candidates_in`] the folder given to [`Session::enable_recovery`].
        pub fn recovery_candidates(&self) -> Vec<RecoveryInfo> {
            match &self.recovery {
                Some(r) => self.recovery_candidates_in(&r.dir.clone()),
                None => Vec::new(),
            }
        }

        /// Opens the notebooks of a snapshot (replacing open notebooks with the same id), their
        /// unsaved changes marked to save, then removes the snapshot. Returns their ids.
        pub fn restore_recovery(&mut self, path: &Path) -> Result<Vec<notecraft_doc::Guid>, IoError> {
            let len = fs::metadata(path).map_err(|e| IoError::Io(format!("{}: {e}", path.display())))?.len();
            if len > super::MAX_SNAPSHOT_BYTES as u64 {
                return Err(IoError::Io("the snapshot is too large".into()));
            }
            let bytes = fs::read(path).map_err(|e| IoError::Io(format!("{}: {e}", path.display())))?;
            let snap = parse_snapshot(&bytes)?;
            let mut ids = Vec::new();
            for mut saved in snap.notebooks {
                // Where its files really are, so the first save replaces the right ones.
                if let crate::Origin::Folder(dir) = &saved.origin
                    && let Ok(disk) = crate::io::open_path(dir)
                    && let Some(l) = crate::io::initial_layout(&disk, &saved.origin)
                {
                    self.layouts.insert(saved.notebook.id, l);
                }
                saved.notebook = super::sanitize(saved.notebook);
                ids.push(self.adopt(saved));
            }
            if let Some(first) = ids.first() {
                self.current = crate::Location { notebook: Some(*first), section: None, page: None };
            }
            self.normalize_location();
            // Our own snapshot takes over before the old one goes.
            self.write_recovery_now()?;
            Session::discard_recovery(path)?;
            Ok(ids)
        }

        /// Deletes a snapshot (and its meta file) the user chose not to restore.
        pub fn discard_recovery(path: &Path) -> Result<(), IoError> {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !name.ends_with(".json") || name.ends_with(".meta.json") {
                return Err(IoError::Unsupported("that isn't a recovery snapshot".into()));
            }
            let meta = path.with_file_name(name.replace(".json", ".meta.json"));
            let _ = fs::remove_file(meta);
            match fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(IoError::Io(format!("{}: {e}", path.display()))),
                _ => Ok(()),
            }
        }

        /// Writes this session's snapshot now, on this thread (no-op when recovery is off or
        /// nothing is unsaved).
        pub fn write_recovery_now(&mut self) -> Result<(), IoError> {
            if self.recovery.is_none() || !self.is_dirty() {
                return Ok(());
            }
            let snap = self.snapshot_of(true);
            let rev = self.revision;
            let Some(r) = self.recovery.as_mut() else { return Ok(()) };
            r.wait();
            write_snapshot(&r.data(), &r.meta(), &snap)?;
            r.has_file = true;
            r.written_rev = Some(rev);
            Ok(())
        }

        /// Called from [`Session::tick`]: snapshots unsaved work on a worker thread, at most
        /// every [`RECOVERY_INTERVAL_SECS`]; removes the snapshot once everything is saved.
        pub(crate) fn recovery_tick(&mut self, now: f64) {
            let dirty = self.is_dirty();
            let rev = self.revision;
            let Some(r) = self.recovery.as_ref() else { return };
            if !dirty {
                if r.has_file
                    && !r.busy()
                    && let Some(r) = self.recovery.as_mut()
                {
                    r.remove_files();
                }
                return;
            }
            let due = r.written_rev != Some(rev) && r.last_write.is_none_or(|t| now - t >= RECOVERY_INTERVAL_SECS || now < t) && !r.busy();
            if !due {
                return;
            }
            let snap = self.snapshot_of(true);
            let Some(r) = self.recovery.as_mut() else { return };
            r.wait();
            let (data, meta) = (r.data(), r.meta());
            r.worker = std::thread::Builder::new()
                .name("notecraft-recovery".into())
                .spawn(move || {
                    if let Err(e) = write_snapshot(&data, &meta, &snap) {
                        log::warn!("recovery snapshot failed: {e}");
                    }
                })
                .map_err(|e| log::warn!("couldn't start the recovery writer: {e}"))
                .ok();
            r.has_file = true;
            r.written_rev = Some(rev);
            r.last_write = Some(now);
        }

        /// Ends recovery for a clean exit: waits for a pending snapshot and removes it when
        /// everything is saved (call after the final [`Session::save_dirty`]).
        pub fn end_recovery(&mut self) {
            let dirty = self.is_dirty();
            if let Some(r) = self.recovery.as_mut() {
                r.wait();
                if !dirty {
                    r.remove_files();
                }
            }
        }
    }
}
