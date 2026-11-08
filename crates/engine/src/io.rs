//! File I/O: opening `.one` / `.onetoc2` / `.onepkg` / notebook folders, and saving through a
//! [`Storage`].
//!
//! OneNote has no Save button: changes are saved automatically ([`crate::Session::tick`]). The
//! desktop saves each notebook back where it came from ([`FolderStorage`]: one safe save per
//! changed section, `.bak` of files NoteCraft didn't write, deleted sections to the recycle
//! bin); the web keeps files in memory and hands them to the browser ([`MemoryStorage`]).
//!
//! Also here: crash [`recovery`] snapshots, the [`recent`] notebooks list, and
//! [`Session::to_blob`](crate::Session::to_blob) for keeping a whole session in browser storage.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod folder;
pub mod layout;
pub mod recent;
pub mod recovery;
#[cfg(not(target_arch = "wasm32"))]
pub mod safe_save;

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use notecraft_doc::{Guid, Notebook, Section, SectionEntry};
use serde::{Deserialize, Serialize};

pub use layout::Layout;

use crate::Session;

/// The folder inside a notebook where deleted sections' files go (OneNote's own name for it,
/// so readers skip it).
pub const RECYCLE_BIN: &str = "OneNote_RecycleBin";

/// Errors opening or saving.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IoError {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("NoteCraft can't open {0} files")]
    Unsupported(String),
    #[error(transparent)]
    One(#[from] notecraft_one::Error),
    #[error(transparent)]
    Onepkg(#[from] notecraft_onepkg::Error),
    #[error("{0}")]
    Io(String),
    #[error("nowhere to save {0}: choose a location")]
    NoLocation(String),
}

/// Where an open notebook lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "camelCase")]
pub enum Origin {
    /// Not saved anywhere yet (a new notebook; the web's scratch notebook).
    Scratch,
    /// A notebook folder (`Open Notebook.onetoc2` + `.one` files).
    Folder(PathBuf),
    /// A `.onepkg` package, saved back as one.
    Package(PathBuf),
    /// A single `.one` file opened on its own: the notebook wraps that one section.
    SectionFile(PathBuf),
}

/// What a path holds, by extension (or being a folder).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// `.one`
    Section,
    /// `.onetoc2`
    Toc,
    /// `.onepkg`
    Package,
    /// A notebook folder.
    Folder,
}

impl FileKind {
    /// From a file name's extension (case-insensitive).
    pub fn from_name(name: &str) -> Option<FileKind> {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
        match ext.as_str() {
            "one" => Some(FileKind::Section),
            "onetoc2" => Some(FileKind::Toc),
            "onepkg" => Some(FileKind::Package),
            _ => None,
        }
    }
}

/// One notebook to save: what [`Session::save_dirty`] hands a [`Storage`].
#[derive(Clone, Copy)]
pub struct SaveJob<'a> {
    pub notebook: &'a Notebook,
    pub origin: &'a Origin,
    /// Sections changed since they were last saved.
    pub dirty_sections: &'a HashSet<Guid>,
    /// The table of contents (names, colours, order) changed.
    pub toc_dirty: bool,
    /// The recycle bin changed.
    pub bin_dirty: bool,
    /// Where each section was saved last time (from opening, or the previous save); `None`
    /// before the first save.
    pub previous: Option<&'a Layout>,
}

/// What a save did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveOutcome {
    /// The notebook's new origin when the save moved it (a scratch notebook saved for the first
    /// time).
    pub origin: Option<Origin>,
    /// Where each section is now, for the next save's [`SaveJob::previous`].
    pub layout: Option<Layout>,
    /// Sections whose original file (not written by NoteCraft) was kept as `<file>.bak` now.
    pub backed_up: Vec<Guid>,
}

/// Saves notebooks. Implementations: [`FolderStorage`] (desktop), [`MemoryStorage`] (web,
/// tests).
pub trait Storage {
    /// Persists `notebook`, which lives at `origin`. `dirty_sections` are the ids of its sections
    /// changed since the last save (an implementation may write only those, plus the TOC when
    /// `toc_dirty`; sections no longer in the notebook are removed from storage). Returns the
    /// notebook's new origin when the save moved it (a scratch notebook saved for the first time).
    fn save(&mut self, notebook: &Notebook, origin: &Origin, dirty_sections: &HashSet<Guid>, toc_dirty: bool) -> Result<Option<Origin>, IoError>;

    /// The save [`Session::save_dirty`] calls: like [`Storage::save`], also told where each
    /// section was saved before, and reporting backups. The default calls `save`.
    fn save_notebook(&mut self, job: &SaveJob<'_>) -> Result<SaveOutcome, IoError> {
        let origin = self.save(job.notebook, job.origin, job.dirty_sections, job.toc_dirty)?;
        Ok(SaveOutcome { origin, ..SaveOutcome::default() })
    }
}

/// Opens a file or folder by kind: a folder or `.onetoc2` as its notebook, a `.onepkg`, or a
/// single `.one` section wrapped in a notebook named after it.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_path(path: &std::path::Path) -> Result<Notebook, IoError> {
    if path.is_dir() {
        return Ok(notecraft_one::read_notebook_dir(path)?);
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match FileKind::from_name(&name) {
        Some(FileKind::Toc) => {
            let dir = path.parent().ok_or_else(|| IoError::Io(format!("{} has no folder", path.display())))?;
            Ok(notecraft_one::read_notebook_dir(dir)?)
        }
        Some(_) => {
            let bytes = std::fs::read(path).map_err(|e| IoError::Io(format!("{}: {e}", path.display())))?;
            open_bytes(&name, &bytes)
        }
        None => Err(IoError::Unsupported(name)),
    }
}

/// The [`Origin`] a path opened with [`open_path`] saves back to.
#[cfg(not(target_arch = "wasm32"))]
pub fn origin_of(path: &std::path::Path) -> Origin {
    if path.is_dir() {
        return Origin::Folder(path.to_path_buf());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match FileKind::from_name(&name) {
        Some(FileKind::Toc) => path.parent().map_or(Origin::Scratch, |d| Origin::Folder(d.to_path_buf())),
        Some(FileKind::Package) => Origin::Package(path.to_path_buf()),
        Some(FileKind::Section) => Origin::SectionFile(path.to_path_buf()),
        _ => Origin::Scratch,
    }
}

/// Opens file bytes by the file name's extension (`.one` or `.onepkg`; a `.onetoc2` alone has
/// no sections, so it needs its folder).
pub fn open_bytes(name: &str, bytes: &[u8]) -> Result<Notebook, IoError> {
    if name.to_ascii_lowercase().ends_with(".json") {
        // A notebook saved by `MemoryStorage` as JSON, or `notecraft-cli convert … x.json`.
        let nb: Notebook = serde_json::from_slice(bytes).map_err(|e| IoError::Io(format!("not a NoteCraft notebook: {e}")))?;
        return Ok(recovery::sanitize(nb));
    }
    match FileKind::from_name(name) {
        Some(FileKind::Section) => {
            let file_name = name.rsplit(['/', '\\']).next().unwrap_or(name);
            let stem = file_name.rsplit_once('.').map_or(file_name, |(stem, _)| stem);
            let mut section = notecraft_one::read_section_named(bytes, stem)?;
            section.source.file_name = Some(file_name.to_owned());
            Ok(wrap_section(section))
        }
        Some(FileKind::Package) => Ok(notecraft_onepkg::read(bytes)?),
        Some(FileKind::Toc) => Err(IoError::Unsupported("a notebook's table of contents on its own (open its folder)".into())),
        _ => Err(IoError::Unsupported(name.chars().take(80).collect())),
    }
}

/// A notebook holding just `section`, named after it.
pub fn wrap_section(section: Section) -> Notebook {
    let mut nb = Notebook::new(section.name.clone());
    nb.sections.push(SectionEntry::Section(section));
    nb
}

/// Where the sections of a notebook just opened from `origin` are on disk, so the first save
/// knows which files it replaces. `None` for packages and scratch notebooks.
pub fn initial_layout(nb: &Notebook, origin: &Origin) -> Option<Layout> {
    match origin {
        Origin::Folder(_) => Some(layout::plan(nb, None, None).0),
        Origin::SectionFile(path) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned())?;
            let pin = nb.first_section().map(|s| s.id);
            Some(layout::plan(nb, None, pin.map(|id| (id, name.as_str()))).0)
        }
        Origin::Scratch | Origin::Package(_) => None,
    }
}

/// Saves every dirty notebook through the session's storage; see [`Session::save_dirty`].
pub fn save_dirty(session: &mut Session) -> Result<usize, IoError> {
    let Some(mut storage) = session.storage.take() else { return Ok(0) };
    let mut saved = 0;
    let mut first_err = None;
    let ids: Vec<Guid> = session.notebooks.iter().map(|n| n.id).collect();
    for id in ids {
        if !session.notebook_is_dirty(id) {
            continue;
        }
        let Some(nb) = session.notebook(id) else { continue };
        let sections: HashSet<Guid> = nb.all_sections().iter().map(|s| s.id).filter(|s| session.dirty.contains(s)).collect();
        let origin = session.origins.get(&id).cloned().unwrap_or(Origin::Scratch);
        let job = SaveJob {
            notebook: nb,
            origin: &origin,
            dirty_sections: &sections,
            toc_dirty: session.dirty_tocs.contains(&id),
            bin_dirty: session.dirty_bins.contains(&id),
            previous: session.layouts.get(&id),
        };
        match storage.save_notebook(&job) {
            Ok(out) => {
                session.after_save(id, &sections, out);
                saved += 1;
            }
            Err(e) => {
                log::warn!("saving notebook {id} failed: {e}");
                first_err.get_or_insert(e);
            }
        }
    }
    // Sections deleted from every notebook can't be saved; forget them.
    let live: HashSet<Guid> = session.notebooks.iter().flat_map(|n| n.all_sections().into_iter().map(|s| s.id)).collect();
    session.dirty.retain(|s| live.contains(s));
    session.storage = Some(storage);
    match first_err {
        Some(e) => Err(e),
        None => Ok(saved),
    }
}

impl Session {
    /// Records a successful save of notebook `id`.
    fn after_save(&mut self, id: Guid, sections: &HashSet<Guid>, out: SaveOutcome) {
        if let Some(o) = out.origin {
            if let Origin::Folder(dir) = &o
                && let Some(nb) = self.notebook(id)
            {
                let name = nb.name.clone();
                self.recent.add(dir.clone(), name);
            }
            self.origins.insert(id, o);
        }
        if let Some(l) = out.layout {
            self.layouts.insert(id, l);
        }
        for s in sections {
            self.dirty.remove(s);
        }
        self.dirty_tocs.remove(&id);
        self.dirty_bins.remove(&id);
        let fresh: Vec<Guid> = out.backed_up.iter().copied().filter(|s| self.warned_unsupported.insert(*s)).collect();
        let warn: Vec<String> = fresh
            .iter()
            .filter_map(|s| self.notebook(id).and_then(|nb| nb.section(*s)))
            .filter(|s| s.source.unsupported_content)
            .map(|s| format!("\u{201c}{}\u{201d} has content NoteCraft can't edit yet; a backup of the original file was kept (.bak).", s.name))
            .collect();
        self.notices.extend(warn);
    }
}

/// Desktop storage: each notebook goes back to its folder, package or section file; scratch
/// notebooks get a folder under [`FolderStorage::root`] (`Documents/NoteCraft Notebooks`).
///
/// Each section is written with a safe save (a temporary file renamed over the old one). A file
/// NoteCraft didn't write is kept as `<file>.bak` the first time it is replaced. A deleted
/// section's file goes into the notebook's [`RECYCLE_BIN`] folder.
#[cfg(not(target_arch = "wasm32"))]
pub struct FolderStorage {
    pub root: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FolderStorage {
    pub fn new(root: impl Into<PathBuf>) -> FolderStorage {
        FolderStorage { root: root.into() }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FolderStorage {
    fn save(&mut self, notebook: &Notebook, origin: &Origin, dirty: &HashSet<Guid>, toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        let job = SaveJob { notebook, origin, dirty_sections: dirty, toc_dirty, bin_dirty: true, previous: None };
        Ok(self.save_notebook(&job)?.origin)
    }

    fn save_notebook(&mut self, job: &SaveJob<'_>) -> Result<SaveOutcome, IoError> {
        folder::save(job, &self.root, &folder::Codec::one())
    }
}

/// The default folder for new notebooks inside the user's documents folder.
pub fn default_notebook_root(documents: &std::path::Path) -> PathBuf {
    documents.join("NoteCraft Notebooks")
}

/// How [`MemoryStorage`] encodes a notebook.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryFormat {
    /// `<name>.onepkg`, like OneNote's export.
    #[default]
    Onepkg,
    /// `<name>.json`: the model as JSON (lossless, readable by any NoteCraft).
    Json,
}

/// In-memory storage (web, tests): the latest bytes of each saved notebook, by file name.
/// `on_write` lets the web app persist them (browser storage, a download).
#[derive(Default)]
pub struct MemoryStorage {
    pub files: BTreeMap<String, Vec<u8>>,
    pub on_write: Option<Box<dyn FnMut(&str, &[u8])>>,
    pub format: MemoryFormat,
}

impl Storage for MemoryStorage {
    fn save(&mut self, notebook: &Notebook, _origin: &Origin, _dirty: &HashSet<Guid>, _toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        let stem = notecraft_one::safe_file_stem(&notebook.name);
        let (name, bytes) = match self.format {
            MemoryFormat::Onepkg => (format!("{stem}.onepkg"), notecraft_onepkg::write(notebook)?),
            MemoryFormat::Json => (format!("{stem}.json"), serde_json::to_vec(notebook).map_err(|e| IoError::Io(e.to_string()))?),
        };
        if let Some(hook) = self.on_write.as_mut() {
            hook(&name, &bytes);
        }
        self.files.insert(name, bytes);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(FileKind::from_name("Quick Notes.ONE"), Some(FileKind::Section));
        assert_eq!(FileKind::from_name("Open Notebook.onetoc2"), Some(FileKind::Toc));
        assert_eq!(FileKind::from_name("a.onepkg"), Some(FileKind::Package));
        assert_eq!(FileKind::from_name("a.docx"), None);
        assert_eq!(FileKind::from_name("noext"), None);
        assert!(matches!(open_bytes("x.txt", b""), Err(IoError::Unsupported(_))));
        assert!(open_bytes("x.one", b"junk").is_err());
    }
}
