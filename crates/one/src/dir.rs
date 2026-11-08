//! Notebook folders on disk.

use std::fs;
use std::path::{Path, PathBuf};

use notecraft_doc::Notebook;
use notecraft_doc::notebook::MAX_GROUP_DEPTH;

use crate::{Error, NotebookFile, Result, notebook_files, notebook_from_files};

/// Most files read from one notebook folder.
const MAX_FILES: usize = 10_000;
/// Largest file read (bytes).
const MAX_FILE_SIZE: u64 = 2 << 30;

fn io(path: &Path, e: std::io::Error) -> Error {
    Error::Io(format!("{}: {e}", path.display()))
}

/// Files NoteCraft never reads from a notebook folder: backups and its own temporary files.
fn skipped(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".bak") || lower.ends_with(".tmp") || lower.starts_with(".notecraft-save-") || lower.starts_with('.')
}

pub fn read(dir: &Path) -> Result<Notebook> {
    if !dir.is_dir() {
        return Err(Error::Io(format!("{} is not a folder", dir.display())));
    }
    let mut files = Vec::new();
    collect(dir, "", 0, &mut files)?;
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Notebook".into());
    let (nb, skipped) = notebook_from_files(&files, &name)?;
    if skipped > 0 {
        log::warn!("{}: {skipped} section(s) could not be read", dir.display());
    }
    Ok(nb)
}

fn collect(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<NotebookFile>) -> Result<()> {
    if depth > MAX_GROUP_DEPTH {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(dir).map_err(|e| io(dir, e))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
        if skipped(&name) || out.len() >= MAX_FILES {
            continue;
        }
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            if !name.eq_ignore_ascii_case(crate::RECYCLE_BIN_FOLDER) {
                collect(&path, &format!("{prefix}{name}/"), depth + 1, out)?;
            } else if depth == 0 {
                // The notebook's recycle bin: its own files only (deleted sections, deleted pages).
                collect(&path, &format!("{prefix}{name}/"), MAX_GROUP_DEPTH, out)?;
            }
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !meta.is_file() || !(lower.ends_with(".one") || lower.ends_with(".onetoc2")) || meta.len() > MAX_FILE_SIZE {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| io(&path, e))?;
        out.push(NotebookFile { path: format!("{prefix}{name}"), bytes });
    }
    Ok(())
}

pub fn write(notebook: &Notebook, dir: &Path) -> Result<()> {
    let files = notebook_files(notebook)?;
    for f in files {
        let path = f.path.split('/').fold(dir.to_path_buf(), |p, c| p.join(c));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tmp = path.with_file_name(format!(".notecraft-save-{name}.tmp"));
        fs::write(&tmp, &f.bytes).map_err(|e| io(&tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io(&path, e)
        })?;
    }
    Ok(())
}
