//! `.onepkg` notebook packages: a Microsoft Cabinet file holding a notebook folder (its
//! `Open Notebook.onetoc2`, the section `.one` files and section group subfolders).
//!
//! Read cabinets compressed with none / MSZIP / LZX, write MSZIP, through the `cab` crate (MIT).
//! Entry counts, names and sizes are bounded; paths are normalised and never escape the package.
//! Paths in the cabinet use `\` (as Windows tools write them); `/` is accepted too.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::{Cursor, Read, Write};

use notecraft_doc::Notebook;
use notecraft_one::NotebookFile;

/// Most files one package may hold.
pub const MAX_ENTRIES: usize = 10_000;
/// Largest total uncompressed size read, bytes (2 GiB).
pub const MAX_TOTAL_SIZE: u64 = 2 << 30;
/// Longest entry name accepted.
const MAX_NAME_LEN: usize = 1024;

/// Errors reading or writing a package.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("not a OneNote package: {0}")]
    NotPackage(String),
    #[error(transparent)]
    One(#[from] notecraft_one::Error),
    #[error("package too large: {0}")]
    Limit(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn bad(e: impl std::fmt::Display) -> Error {
    Error::NotPackage(e.to_string())
}

/// The notebook files in a package, paths `/`-separated and relative.
pub fn read_files(bytes: &[u8]) -> Result<Vec<NotebookFile>> {
    if !bytes.starts_with(b"MSCF") {
        return Err(Error::NotPackage("no cabinet signature".into()));
    }
    // The cab crate is third-party code: a bug in it on a hostile file must not take NoteCraft
    // down, so it runs behind a panic guard (AGENTS.md, Never crash).
    std::panic::catch_unwind(|| read_files_inner(bytes)).unwrap_or_else(|_| Err(Error::NotPackage("damaged cabinet".into())))
}

fn read_files_inner(bytes: &[u8]) -> Result<Vec<NotebookFile>> {
    let mut cab = cab::Cabinet::new(Cursor::new(bytes)).map_err(bad)?;
    let mut entries = Vec::new();
    let mut total = 0u64;
    for folder in cab.folder_entries() {
        for file in folder.file_entries() {
            if entries.len() >= MAX_ENTRIES {
                return Err(Error::Limit(format!("more than {MAX_ENTRIES} files")));
            }
            total = total.saturating_add(u64::from(file.uncompressed_size()));
            if total > MAX_TOTAL_SIZE {
                return Err(Error::Limit("more than 2 GiB uncompressed".into()));
            }
            entries.push((file.name().to_owned(), file.uncompressed_size()));
        }
    }
    let mut out = Vec::with_capacity(entries.len());
    for (name, size) in entries {
        let Some(path) = clean_path(&name) else {
            log::warn!("package entry {name:?} skipped (unsafe path)");
            continue;
        };
        let lower = path.to_ascii_lowercase();
        if !(lower.ends_with(".one") || lower.ends_with(".onetoc2")) {
            continue;
        }
        let reader = cab.read_file(&name).map_err(bad)?;
        let mut data = Vec::with_capacity((size as usize).min(64 << 20));
        reader.take(u64::from(size)).read_to_end(&mut data).map_err(bad)?;
        out.push(NotebookFile { path, bytes: data });
    }
    Ok(out)
}

/// A relative, `/`-separated path with no `..`, `.`, drive or root parts; `None` if unsafe.
fn clean_path(name: &str) -> Option<String> {
    if name.len() > MAX_NAME_LEN || name.contains('\0') {
        return None;
    }
    let parts: Vec<&str> = name.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() || parts.iter().any(|p| *p == ".." || *p == "." || p.contains(':')) {
        return None;
    }
    Some(parts.join("/"))
}

/// Reads a `.onepkg` into a notebook. Sections that can't be read are skipped (logged).
pub fn read(bytes: &[u8]) -> Result<Notebook> {
    let files = read_files(bytes)?;
    if !files.iter().any(|f| f.path.to_ascii_lowercase().ends_with(".one")) && !files.iter().any(|f| f.path.ends_with(notecraft_one::TOC_FILE_NAME)) {
        return Err(Error::NotPackage("no OneNote notebook inside".into()));
    }
    let (nb, skipped) = notecraft_one::notebook_from_files(&files, "Notebook")?;
    if skipped > 0 {
        log::warn!("package: {skipped} section(s) could not be read");
    }
    Ok(nb)
}

/// Writes a notebook as a `.onepkg`: one MSZIP folder holding the notebook's files.
pub fn write(notebook: &Notebook) -> Result<Vec<u8>> {
    let files = notecraft_one::notebook_files(notebook)?;
    if files.len() > MAX_ENTRIES {
        return Err(Error::Limit(format!("more than {MAX_ENTRIES} files")));
    }
    let mut builder = cab::CabinetBuilder::new();
    let folder = builder.add_folder(cab::CompressionType::MsZip);
    for f in &files {
        if f.bytes.len() as u64 > u64::from(u32::MAX) {
            return Err(Error::Limit(format!("{} is larger than 4 GiB", f.path)));
        }
        folder.add_file(f.path.replace('/', "\\"));
    }
    let mut writer = builder.build(Cursor::new(Vec::new())).map_err(bad)?;
    let mut i = 0usize;
    while let Some(mut fw) = writer.next_file().map_err(bad)? {
        let data = files.get(i).map(|f| f.bytes.as_slice()).unwrap_or_default();
        fw.write_all(data).map_err(bad)?;
        i += 1;
    }
    Ok(writer.finish().map_err(bad)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_doc::{Color, Section, SectionEntry, SectionGroup};

    #[test]
    fn junk_is_refused() {
        assert!(matches!(read(b"PK\x03\x04"), Err(Error::NotPackage(_))));
        assert!(read(b"MSCF").is_err());
    }

    #[test]
    fn paths_are_cleaned() {
        assert_eq!(clean_path("Work\\Plans.one").as_deref(), Some("Work/Plans.one"));
        assert_eq!(clean_path("/a//b.one").as_deref(), Some("a/b.one"));
        for bad in ["..\\evil.one", "a/../b.one", "C:\\x.one", "", "\\", "a\0.one"] {
            assert_eq!(clean_path(bad), None, "{bad:?}");
        }
    }

    fn notebook() -> Notebook {
        let mut nb = Notebook::new_default();
        let mut g = SectionGroup::new("Work");
        let mut s = Section::new("Plans", Color::rgb(0x7F, 0xD3, 0xD8));
        s.pages.push(notecraft_doc::Page::new("Roadmap"));
        g.entries.push(SectionEntry::Section(s));
        nb.sections.push(SectionEntry::Group(g));
        nb
    }

    #[test]
    fn package_round_trip() {
        let nb = notebook();
        let bytes = write(&nb).unwrap();
        assert!(bytes.starts_with(b"MSCF"));
        let files = read_files(&bytes).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Quick Notes.one") && paths.contains(&"Work/Plans.one") && paths.contains(&"Open Notebook.onetoc2"), "{paths:?}");
        let back = read(&bytes).unwrap();
        assert_eq!(back.id, nb.id);
        assert_eq!(back.name, nb.name);
        assert_eq!(back.color, nb.color);
        let names = |n: &Notebook| n.all_sections().iter().map(|s| (s.id, s.name.clone(), s.pages.len())).collect::<Vec<_>>();
        assert_eq!(names(&back), names(&nb));
        assert!(matches!(&back.sections[1], SectionEntry::Group(g) if g.name == "Work"));
        let welcome = &back.first_section().unwrap().pages[0];
        assert_eq!(welcome.title, "Welcome to NoteCraft");
        assert_eq!(welcome.ink, nb.first_section().unwrap().pages[0].ink);
    }

    #[test]
    fn hostile_packages_never_panic() {
        let good = write(&notebook()).unwrap();
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..2000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let mut b = good.clone();
            let i = (state as usize) % b.len();
            b[i] ^= 1 << (state >> 60 & 7);
            let _ = read(&b);
        }
        for len in (0..good.len()).step_by(13) {
            let _ = read(&good[..len]);
        }
    }
}
