//! File I/O: opening `.one` / `.onetoc2` / `.onepkg` / notebook folders, and saving through a
//! [`Storage`].
//!
//! OneNote has no Save button: changes are saved automatically ([`crate::Session::tick`]). The
//! desktop saves each notebook back where it came from ([`FolderStorage`]); the web keeps files
//! in memory and hands them to the browser ([`MemoryStorage`]).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use notecraft_doc::{Guid, Notebook, Section, SectionEntry};

use crate::Session;

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
#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Saves notebooks. Implementations: [`FolderStorage`] (desktop), [`MemoryStorage`] (web,
/// tests).
pub trait Storage {
    /// Persists `notebook`, which lives at `origin`. `dirty_sections` are the ids of its sections
    /// changed since the last save (an implementation may write only those, plus the TOC when
    /// `toc_dirty`; sections no longer in the notebook are removed from storage). Returns the
    /// notebook's new origin when the save moved it (a scratch notebook saved for the first time).
    fn save(&mut self, notebook: &Notebook, origin: &Origin, dirty_sections: &HashSet<Guid>, toc_dirty: bool) -> Result<Option<Origin>, IoError>;
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
    match FileKind::from_name(name) {
        Some(FileKind::Section) => {
            let mut section = notecraft_one::read_section(bytes)?;
            section.source.file_name = Some(name.rsplit(['/', '\\']).next().unwrap_or(name).to_owned());
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
        match storage.save(nb, &origin, &sections, session.dirty_tocs.contains(&id)) {
            Ok(new_origin) => {
                if let Some(o) = new_origin {
                    session.origins.insert(id, o);
                }
                for s in &sections {
                    session.dirty.remove(s);
                }
                session.dirty_tocs.remove(&id);
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

/// Desktop storage: each notebook goes back to its folder, package or section file; scratch
/// notebooks get a folder under [`FolderStorage::root`] (`Documents/NoteCraft Notebooks`).
#[cfg(not(target_arch = "wasm32"))]
pub struct FolderStorage {
    pub root: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FolderStorage {
    fn save(&mut self, notebook: &Notebook, origin: &Origin, _dirty: &HashSet<Guid>, _toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        match origin {
            Origin::Folder(dir) => {
                notecraft_one::write_notebook_dir(notebook, dir)?;
                Ok(None)
            }
            Origin::Scratch => {
                let dir = self.root.join(notecraft_one::safe_file_stem(&notebook.name));
                notecraft_one::write_notebook_dir(notebook, &dir)?;
                Ok(Some(Origin::Folder(dir)))
            }
            Origin::Package(_) | Origin::SectionFile(_) => Err(IoError::NotImplemented("saving to a package or a single section file")),
        }
    }
}

/// In-memory storage (web, tests): the latest bytes of each saved notebook as a `.onepkg`, by
/// file name. `on_write` lets the web app persist them (browser storage, a download).
#[derive(Default)]
pub struct MemoryStorage {
    pub files: BTreeMap<String, Vec<u8>>,
    pub on_write: Option<Box<dyn FnMut(&str, &[u8])>>,
}

impl Storage for MemoryStorage {
    fn save(&mut self, notebook: &Notebook, _origin: &Origin, _dirty: &HashSet<Guid>, _toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        let bytes = notecraft_onepkg::write(notebook)?;
        let name = format!("{}.onepkg", notecraft_one::safe_file_stem(&notebook.name));
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
