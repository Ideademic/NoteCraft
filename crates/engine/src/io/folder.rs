//! Saving a notebook folder (desktop): each changed section to its `.one` file, the tables of
//! contents, sections that moved or were deleted out of the way. Every write is a safe save.
//!
//! - A file NoteCraft didn't write (`authored_by_notecraft` false) is copied to `<file>.bak`
//!   the first time it is overwritten.
//! - The notebook's recycle bin is its `OneNote_RecycleBin` folder: each deleted section as a
//!   `.one`, deleted pages in `OneNote_DeletedPages.one` ([`notecraft_one::recycle_bin_files`]).
//!   A deleted section's old file is removed once its recycle-bin copy is written; a renamed or
//!   moved section's old file is removed when NoteCraft wrote it. A file NoteCraft didn't write
//!   is never removed: it is moved into the recycle bin folder as a `.bak`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use notecraft_doc::{DeletedPage, DeletedSection, Guid, Notebook, Section};
use notecraft_one::{NotebookToc, TOC_FILE_NAME};

use super::layout::{bin_files, deleted_pages_key, in_bin, plan};
use super::safe_save::{backup_once, write};
use super::{IoError, Origin, RECYCLE_BIN, SaveJob, SaveOutcome};

/// How sections, tables of contents and packages are encoded (the `one`/`onepkg` crates;
/// tests swap in their own).
#[derive(Clone, Copy)]
pub(crate) struct Codec {
    pub section: fn(&Section) -> Result<Vec<u8>, IoError>,
    pub toc: fn(&NotebookToc) -> Result<Vec<u8>, IoError>,
    pub package: fn(&Notebook) -> Result<Vec<u8>, IoError>,
    /// A deleted section's file in the recycle bin (its file name given).
    pub deleted_section: fn(&DeletedSection, &str) -> Result<Vec<u8>, IoError>,
    /// The recycle bin's deleted pages section.
    pub deleted_pages: fn(&[DeletedPage]) -> Result<Vec<u8>, IoError>,
}

impl Codec {
    pub(crate) fn one() -> Codec {
        Codec {
            section: |s| notecraft_one::write_section(s).map_err(IoError::from),
            toc: |t| notecraft_one::write_toc(t).map_err(IoError::from),
            package: |n| notecraft_onepkg::write(n).map_err(IoError::from),
            deleted_section: |d, name| notecraft_one::write_deleted_section(d, name).map_err(IoError::from),
            deleted_pages: |p| notecraft_one::write_deleted_pages(p).map_err(IoError::from),
        }
    }
}

fn io_err(path: &Path, e: std::io::Error) -> IoError {
    IoError::Io(format!("{}: {e}", path.display()))
}

/// Saves the job to wherever its origin says; `root` is where a scratch notebook gets its
/// folder.
pub(crate) fn save(job: &SaveJob<'_>, root: &Path, codec: &Codec) -> Result<SaveOutcome, IoError> {
    match job.origin {
        Origin::Folder(dir) => save_folder(job, dir, codec, true, None),
        Origin::Scratch => {
            let dir = new_folder(root, &notecraft_one::safe_file_stem(&job.notebook.name))?;
            let fresh = SaveJob { previous: None, ..*job };
            let mut out = save_folder(&fresh, &dir, codec, true, None)?;
            out.origin = Some(Origin::Folder(dir));
            Ok(out)
        }
        Origin::SectionFile(path) => {
            let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| IoError::NoLocation(path.display().to_string()))?;
            let pin = job.notebook.first_section().map(|s| (s.id, name.as_str()));
            save_folder(job, dir, codec, false, pin)
        }
        Origin::Package(path) => save_package(job, path, codec),
    }
}

/// A folder for a new notebook under `root`: `name`, or `name (2)`… when taken.
fn new_folder(root: &Path, name: &str) -> Result<PathBuf, IoError> {
    fs::create_dir_all(root).map_err(|e| io_err(root, e))?;
    for n in 1..10_000 {
        let candidate = if n == 1 { root.join(name) } else { root.join(format!("{name} ({n})")) };
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(&candidate, e)),
        }
    }
    Err(IoError::NoLocation(name.to_owned()))
}

fn save_package(job: &SaveJob<'_>, path: &Path, codec: &Codec) -> Result<SaveOutcome, IoError> {
    let nb = job.notebook;
    let bytes = (codec.package)(nb)?;
    let mut out = SaveOutcome::default();
    if nb.all_sections().iter().any(|s| !s.source.authored_by_notecraft) && backup_once(path).map_err(|e| io_err(path, e))? {
        out.backed_up = nb.all_sections().iter().filter(|s| !s.source.authored_by_notecraft).map(|s| s.id).collect();
    }
    write(path, &bytes).map_err(|e| io_err(path, e))?;
    Ok(out)
}

/// Writes the notebook into `dir`: changed, moved or missing sections; the tables of contents
/// (when `with_toc`) if anything about them changed; then moves old files out of the way.
pub(crate) fn save_folder(job: &SaveJob<'_>, dir: &Path, codec: &Codec, with_toc: bool, pin: Option<(Guid, &str)>) -> Result<SaveOutcome, IoError> {
    let nb = job.notebook;
    let (layout, folders) = plan(nb, job.previous, pin);
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let mut out = SaveOutcome::default();
    let mut moved = false;
    for sec in nb.all_sections() {
        let Some(rel) = layout.get(&sec.id) else { continue };
        let path = dir.join(rel);
        let was_here = job.previous.is_some_and(|p| p.get(&sec.id) == Some(rel));
        moved |= !was_here;
        if was_here && !job.dirty_sections.contains(&sec.id) && path.is_file() {
            continue;
        }
        let bytes = (codec.section)(sec)?;
        if !sec.source.authored_by_notecraft && backup_once(&path).map_err(|e| io_err(&path, e))? {
            out.backed_up.push(sec.id);
        }
        write(&path, &bytes).map_err(|e| io_err(&path, e))?;
    }
    // The recycle bin: a deleted section's file once (it doesn't change there), the deleted pages
    // section whenever the bin changed.
    let pages_key = deleted_pages_key(nb);
    for (id, rel) in bin_files(nb) {
        let path = dir.join(&rel);
        let was_here = job.previous.is_some_and(|p| p.get(&id) == Some(&rel)) && path.is_file();
        let bytes = if id == pages_key {
            if was_here && !job.bin_dirty {
                continue;
            }
            (codec.deleted_pages)(&nb.recycle_bin.pages)?
        } else {
            if was_here {
                continue;
            }
            let Some(d) = nb.recycle_bin.section(id) else { continue };
            let name = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (codec.deleted_section)(d, &name)?
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        }
        write(&path, &bytes).map_err(|e| io_err(&path, e))?;
    }
    if with_toc {
        let foreign = nb.all_sections().iter().any(|s| !s.source.authored_by_notecraft);
        let missing = folders.iter().any(|f| !dir.join(&f.dir).join(TOC_FILE_NAME).is_file());
        if job.toc_dirty || job.previous.is_none() || moved || missing {
            for f in &folders {
                let (id, name) = f.group.clone().unwrap_or((nb.id, nb.name.clone()));
                let history_disabled = f.group.is_none() && nb.history_disabled;
                let toc = NotebookToc { id, name, color: nb.color, entries: f.entries.clone(), history_disabled };
                let path = dir.join(&f.dir).join(TOC_FILE_NAME);
                let bytes = (codec.toc)(&toc)?;
                if foreign {
                    backup_once(&path).map_err(|e| io_err(&path, e))?;
                }
                write(&path, &bytes).map_err(|e| io_err(&path, e))?;
            }
        }
    }
    if let Some(prev) = job.previous {
        clear_old_files(nb, dir, prev, &layout, &folders);
    }
    out.layout = Some(layout);
    Ok(out)
}

/// Files of the previous layout that the new one doesn't use: removed (a section NoteCraft
/// wrote, now elsewhere or in the recycle bin; anything in the recycle bin folder) or kept as a
/// `.bak` in the recycle bin folder (anything else). Emptied group folders go too. Failures are
/// logged, not fatal: the notebook itself is saved.
fn clear_old_files(nb: &Notebook, dir: &Path, prev: &super::Layout, layout: &super::Layout, folders: &[super::layout::FolderPlan]) {
    let now: HashSet<&PathBuf> = layout.values().collect();
    let live_dirs: HashSet<&Path> = folders.iter().map(|f| f.dir.as_path()).collect();
    let mut old_dirs: Vec<PathBuf> = Vec::new();
    for (id, old) in prev {
        if now.contains(old) {
            continue;
        }
        let path = dir.join(old);
        if !path.is_file() {
            continue;
        }
        let ours = nb.section(*id).or_else(|| nb.recycle_bin.section(*id).map(|d| &d.section)).is_some_and(|s| s.source.authored_by_notecraft);
        let result = if ours || in_bin(old) { fs::remove_file(&path) } else { recycle(dir, &path) };
        if let Err(e) = result {
            log::warn!("couldn't move {} out of the way: {e}", path.display());
        }
        if let Some(parent) = old.parent().filter(|p| !p.as_os_str().is_empty() && !live_dirs.contains(p)) {
            old_dirs.push(parent.to_path_buf());
        }
    }
    // Deepest first, so a parent emptied by its child goes too.
    old_dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    old_dirs.dedup();
    for d in old_dirs {
        prune_dir(&dir.join(d));
    }
}

/// Moves `path` (a file NoteCraft didn't write) into the notebook's recycle bin folder as a
/// `.bak` under a free name: kept, but not read back as a deleted section (that is written from
/// the model).
fn recycle(dir: &Path, path: &Path) -> std::io::Result<()> {
    let bin = dir.join(RECYCLE_BIN);
    fs::create_dir_all(&bin)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "section.one".into());
    for n in 1..10_000 {
        let candidate = if n == 1 { bin.join(format!("{name}.bak")) } else { bin.join(format!("{name} ({n}).bak")) };
        if !candidate.exists() {
            return fs::rename(path, candidate);
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "the recycle bin is full"))
}

/// Removes a group folder left holding only its table of contents (or nothing).
fn prune_dir(d: &Path) {
    let Ok(entries) = fs::read_dir(d) else { return };
    let names: Vec<String> = entries.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    if names.iter().all(|n| n == TOC_FILE_NAME) {
        let _ = fs::remove_file(d.join(TOC_FILE_NAME));
        let _ = fs::remove_dir(d);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashSet;

    use notecraft_doc::{Color, SectionEntry, SectionGroup};

    use super::*;
    use crate::io::safe_save::tests::temp_dir;

    /// JSON stands in for `.one` while the format crate is being written.
    pub(crate) fn json_codec() -> Codec {
        Codec {
            section: |s| serde_json::to_vec(s).map_err(|e| IoError::Io(e.to_string())),
            toc: |t| Ok(format!("{}|{}|{:?}", t.id, t.name, t.entries).into_bytes()),
            package: |n| serde_json::to_vec(n).map_err(|e| IoError::Io(e.to_string())),
            deleted_section: |d, _| serde_json::to_vec(d).map_err(|e| IoError::Io(e.to_string())),
            deleted_pages: |p| serde_json::to_vec(p).map_err(|e| IoError::Io(e.to_string())),
        }
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.push(p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn writes_moves_backs_up_and_recycles() {
        let root = temp_dir("folder");
        let mut nb = Notebook::new("Book");
        let a = Section::new("Alpha", Color::WHITE);
        let mut b = Section::new("Beta", Color::WHITE);
        b.source.authored_by_notecraft = false;
        b.source.file_name = Some("Beta.one".into());
        let mut g = SectionGroup::new("Group");
        let c = Section::new("Gamma", Color::WHITE);
        let (ia, ib, ic) = (a.id, b.id, c.id);
        g.entries.push(SectionEntry::Section(c));
        nb.sections = vec![SectionEntry::Section(a), SectionEntry::Section(b), SectionEntry::Group(g)];
        let codec = json_codec();
        let none = HashSet::new();
        // First save of a scratch notebook: a new folder with everything.
        let job = SaveJob { notebook: &nb, origin: &Origin::Scratch, dirty_sections: &none, toc_dirty: true, bin_dirty: true, previous: None };
        let out = save(&job, &root, &codec).unwrap();
        let Some(Origin::Folder(dir)) = out.origin.clone() else { panic!("{:?}", out.origin) };
        assert_eq!(dir, root.join("Book"));
        assert_eq!(files(&dir), vec!["Alpha.one", "Beta.one", "Group/Gamma.one", "Group/Open Notebook.onetoc2", "Open Notebook.onetoc2"]);
        let l1 = out.layout.unwrap();
        // A second notebook with the same name gets its own folder.
        let job2 = SaveJob {
            notebook: &Notebook::new("Book"),
            origin: &Origin::Scratch,
            dirty_sections: &none,
            toc_dirty: true,
            bin_dirty: true,
            previous: None,
        };
        assert!(matches!(save(&job2, &root, &codec).unwrap().origin, Some(Origin::Folder(d)) if d == root.join("Book (2)")));
        // Overwriting the foreign section keeps a backup, once.
        let dirty: HashSet<Guid> = [ib].into();
        let origin = Origin::Folder(dir.clone());
        let job = SaveJob { notebook: &nb, origin: &origin, dirty_sections: &dirty, toc_dirty: false, bin_dirty: true, previous: Some(&l1) };
        let out = save(&job, &root, &codec).unwrap();
        assert_eq!(out.backed_up, vec![ib]);
        assert!(dir.join("Beta.one.bak").is_file());
        let out = save(&job, &root, &codec).unwrap();
        assert!(out.backed_up.is_empty());
        // Rename Alpha (ours: old file removed), delete Beta and the group into the recycle bin:
        // written there from the model, Beta's original (not ours) kept beside them as a backup.
        let l2 = out.layout.unwrap();
        if let Some(s) = nb.section_mut(ia) {
            s.name = "Apple".into();
        }
        let deleted = |e: Option<SectionEntry>| match e {
            Some(SectionEntry::Section(section)) => vec![section],
            Some(SectionEntry::Group(g)) => {
                g.entries.into_iter().filter_map(|e| if let SectionEntry::Section(s) = e { Some(s) } else { None }).collect()
            }
            None => vec![],
        };
        let mut gone = deleted(nb.remove_entry(ib));
        let gid = nb.sections.iter().find(|e| matches!(e, SectionEntry::Group(_))).map(SectionEntry::id).unwrap();
        gone.extend(deleted(nb.remove_entry(gid)));
        assert_eq!(gone.iter().map(|s| s.id).collect::<Vec<_>>(), vec![ib, ic]);
        for section in gone {
            nb.recycle_bin.sections.push(notecraft_doc::DeletedSection { section, deleted: notecraft_doc::now(), group_path: vec![], index: 0 });
        }
        let dirty: HashSet<Guid> = [ia].into();
        let job = SaveJob { notebook: &nb, origin: &origin, dirty_sections: &dirty, toc_dirty: true, bin_dirty: true, previous: Some(&l2) };
        save(&job, &root, &codec).unwrap();
        assert_eq!(
            files(&dir),
            vec![
                "Apple.one",
                "Beta.one.bak",
                "OneNote_RecycleBin/Beta.one",
                "OneNote_RecycleBin/Beta.one.bak",
                "OneNote_RecycleBin/Gamma.one",
                "Open Notebook.onetoc2"
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn section_files_and_packages() {
        let root = temp_dir("single");
        let path = root.join("Loose.one");
        fs::write(&path, b"original").unwrap();
        let mut s = Section::new("Something else", Color::WHITE);
        s.source.authored_by_notecraft = false;
        let nb = crate::io::wrap_section(s);
        let none = HashSet::new();
        let origin = Origin::SectionFile(path.clone());
        let job = SaveJob { notebook: &nb, origin: &origin, dirty_sections: &none, toc_dirty: true, bin_dirty: true, previous: None };
        let out = save(&job, &root, &json_codec()).unwrap();
        assert_eq!(out.backed_up.len(), 1);
        assert_eq!(files(&root), vec!["Loose.one", "Loose.one.bak"], "no table of contents beside a lone section");
        let pkg = root.join("Book.onepkg");
        let origin = Origin::Package(pkg.clone());
        let job = SaveJob { notebook: &nb, origin: &origin, dirty_sections: &none, toc_dirty: true, bin_dirty: true, previous: None };
        save(&job, &root, &json_codec()).unwrap();
        assert!(pkg.is_file());
        let _ = fs::remove_dir_all(&root);
    }
}
