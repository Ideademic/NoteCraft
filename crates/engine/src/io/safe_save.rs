//! Safe saving: a file is replaced only by renaming a fully written temporary file beside it
//! over it, so a crash or a full disk mid-save never leaves a half-written notebook.
//!
//! When the folder refuses the temporary file (a read-only folder, macOS privacy protection),
//! an existing file is never written in place: it is left as it was and the error says so. A
//! file that doesn't exist yet (nothing to lose) is created directly, and removed again if the
//! write fails.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes `bytes` to `path` safely (see the module docs). Creates missing parent folders.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.file_name().is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a file name is needed"));
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let permissions = match fs::metadata(path) {
        Ok(meta) if meta.is_file() => Some(meta.permissions()),
        Ok(_) => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} is a folder", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut temp = match Temporary::create(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return create_directly(path, bytes, e),
        Err(e) => return Err(e),
    };
    let file = temp.file()?;
    file.write_all(bytes)?;
    if let Some(p) = permissions {
        // A folder that keeps no modes (some network shares) refuses this; the save goes on.
        let _ = file.set_permissions(p);
    }
    file.sync_all()?;
    temp.commit(path)
}

/// No temporary file may be made beside `path`. Only a new file is created directly.
fn create_directly(path: &Path, bytes: &[u8], refused: io::Error) -> io::Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(io::Error::new(
                refused.kind(),
                format!(
                    "{refused}. This folder doesn't let NoteCraft make the temporary file a safe save needs, so {} was left unchanged.",
                    path.display()
                ),
            ));
        }
        Err(e) => return Err(e),
    };
    match file.write_all(bytes).and_then(|()| file.sync_all()) {
        Ok(()) => Ok(()),
        Err(e) => {
            drop(file);
            let _ = fs::remove_file(path);
            Err(e)
        }
    }
}

/// Copies `path` to `<path>.bak` unless that backup already exists. Returns whether a backup
/// was made now.
pub fn backup_once(path: &Path) -> io::Result<bool> {
    let bak = bak_path(path);
    if bak.exists() || !path.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(path)?;
    write(&bak, &bytes)?;
    Ok(true)
}

/// `Quick Notes.one` → `Quick Notes.one.bak`.
pub fn bak_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".bak");
    path.with_file_name(name)
}

struct Temporary {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl Temporary {
    fn create(destination: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = destination.with_file_name(format!(".notecraft-save-{}-{id}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Temporary { path, file: Some(file), committed: false }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "could not create a temporary file to save"))
    }

    fn file(&mut self) -> io::Result<&mut File> {
        self.file.as_mut().ok_or_else(|| io::Error::other("the temporary file is closed"))
    }

    fn commit(mut self, destination: &Path) -> io::Result<()> {
        // Closed before the rename (Windows can't rename an open file).
        drop(self.file.take());
        fs::rename(&self.path, destination)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!("notecraft-test-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn replaces_without_leftovers_and_backs_up_once() {
        let d = temp_dir("safe");
        let f = d.join("sub").join("a.one");
        write(&f, b"one").unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"one");
        assert!(backup_once(&f).unwrap());
        write(&f, b"two").unwrap();
        assert!(!backup_once(&f).unwrap(), "only the first time");
        assert_eq!(fs::read(bak_path(&f)).unwrap(), b"one");
        assert_eq!(fs::read(&f).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(d.join("sub")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        assert!(write(&d, b"x").is_err(), "a folder is never replaced");
        let _ = fs::remove_dir_all(&d);
    }
}
