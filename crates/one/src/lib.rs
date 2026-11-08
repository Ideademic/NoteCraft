//! `.one` sections and `.onetoc2` notebook tables of contents: the [MS-ONE] mapping between the
//! revision store ([`notecraft_onestore`]) and the model ([`notecraft_doc`]).
//!
//! Written only from Microsoft's public Open Specifications ([MS-ONE], [MS-ONESTORE]). Never from
//! other implementations' source (AGENTS.md, Clean room).
//!
//! # On disk
//! A local OneNote notebook is a folder:
//!
//! ```text
//! My Notebook/
//!   Open Notebook.onetoc2      the table of contents: notebook id, name, colour, entry order
//!   Quick Notes.one            one file per section
//!   Work/                      a section group is a subfolder,
//!     Open Notebook.onetoc2    with its own table of contents
//!     Plans.one
//! ```
//!
//! See `README.md` for what is supported and what is uncertain. Covered: Section / PageSeries / PageManifest / Page / PageMetaData /
//! Title / Outline / OutlineElement / RichTextOE / Image / NumberList / Table (read-only,
//! flattened) / EmbeddedFile, ink (InkContainer / InkData / InkStroke / StrokeProperties, ISF
//! multibyte delta encoding), and the `.onetoc2` TOC. Unknown objects are kept as
//! [`notecraft_doc::ElementKind::Opaque`] and written back where possible; reading one sets
//! [`notecraft_doc::SourceInfo::unsupported_content`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod build;
mod icon;
pub mod ids;
mod ink;
mod notebook;
mod opaque;
mod read;
mod recycle;
mod table;
mod toc;
mod util;
mod versions;
mod write;

#[cfg(not(target_arch = "wasm32"))]
mod dir;

use notecraft_doc::{Color, Guid, Section};

#[cfg(not(target_arch = "wasm32"))]
use notecraft_doc::Notebook;

pub use notebook::{NotebookFile, notebook_files, notebook_from_files};
pub use recycle::{
    DELETED_PAGES_FILE, RECYCLE_BIN_FOLDER, read_recycle_bin, recycle_bin_files, recycle_bin_names, write_deleted_pages, write_deleted_section,
};
pub use versions::history_context;
pub use write::WRITER_NAME;

/// The table of contents' file name in a notebook folder and in each section group folder.
pub const TOC_FILE_NAME: &str = "Open Notebook.onetoc2";
/// Section file extension.
pub const SECTION_EXT: &str = "one";

/// Errors reading or writing `.one` / `.onetoc2`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error(transparent)]
    Store(#[from] notecraft_onestore::Error),
    #[error("unexpected OneNote content: {0}")]
    Format(String),
    #[error("{0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A notebook's table of contents (`.onetoc2`): identity, name, colour and the order of its
/// entries. Sections are files next to the TOC; section groups are subfolders with their own TOC.
#[derive(Clone, Debug, PartialEq)]
pub struct NotebookToc {
    pub id: Guid,
    pub name: String,
    pub color: Color,
    pub entries: Vec<TocEntry>,
    /// History ▸ Disable History for this Notebook: MS-ONE EnableHistory false (only the
    /// notebook's own table of contents carries it).
    pub history_disabled: bool,
}

/// One entry of a [`NotebookToc`], in tab order.
#[derive(Clone, Debug, PartialEq)]
pub enum TocEntry {
    /// A section file in the same folder (`Quick Notes.one`).
    Section { file_name: String, color: Option<Color> },
    /// A section group: the subfolder's name.
    Group { folder_name: String },
}

/// Reads a `.one` section file. Its name is the display name stored in the file (NoteCraft and
/// recent OneNote versions store one), else "Untitled section"; callers that know the file name
/// use [`read_section_named`].
pub fn read_section(bytes: &[u8]) -> Result<Section> {
    read_section_named(bytes, "Untitled section")
}

/// Reads a `.one` section file, naming it `name` when the file stores no display name.
pub fn read_section_named(bytes: &[u8], name: &str) -> Result<Section> {
    let store = notecraft_onestore::read(bytes)?;
    read::section(&store, name)
}

/// Writes a section as a `.one` file (pure: bytes out). Its `crcName` is computed for
/// [`section_file_name`]; it doesn't link to a table of contents (`guidAncestor` nil).
pub fn write_section(section: &Section) -> Result<Vec<u8>> {
    write_section_with(section, &section_file_name(section), Guid::NIL)
}

/// Writes a section as a `.one` file named `file_name`, next to the table of contents whose
/// `guidFile` is `toc_id` (nil for none).
pub fn write_section_with(section: &Section, file_name: &str, toc_id: Guid) -> Result<Vec<u8>> {
    let store = write::section_store(section, file_name, toc_id)?;
    Ok(notecraft_onestore::write(&store)?)
}

/// Reads a `.onetoc2` table of contents.
pub fn read_toc(bytes: &[u8]) -> Result<NotebookToc> {
    read_toc_ids(bytes).map(|(t, _)| t)
}

/// Reads a table of contents and each entry's FileIdentityGuid (the section's `guidFile`, or
/// the group folder's table of contents id).
pub(crate) fn read_toc_ids(bytes: &[u8]) -> Result<(NotebookToc, Vec<Option<Guid>>)> {
    let store = notecraft_onestore::read(bytes)?;
    toc::toc(&store)
}

/// Writes a `.onetoc2` table of contents (pure: bytes out). Its `guidFile` is `toc.id`; it
/// lists only the entries given (one folder level).
pub fn write_toc(toc: &NotebookToc) -> Result<Vec<u8>> {
    toc::write(toc, &[], Guid::NIL)
}

/// The file name a section is saved as: its [`notecraft_doc::SourceInfo::file_name`] when it
/// has one, else its name with characters that aren't allowed in file names replaced
/// (`Quick Notes.one`).
pub fn section_file_name(section: &Section) -> String {
    if let Some(f) = section.source.file_name.as_deref().filter(|f| !f.trim().is_empty()) {
        return f.to_owned();
    }
    format!("{}.{SECTION_EXT}", safe_file_stem(&section.name))
}

/// `name` made safe as a file or folder name on Windows, macOS and Linux: reserved characters
/// and controls become `_`, trailing dots and spaces go, an empty result is `Untitled`, and it is
/// cut to 120 characters.
pub fn safe_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c })
        .take(120)
        .collect();
    let trimmed = cleaned.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() { "Untitled".to_owned() } else { trimmed.to_owned() }
}

/// Reads a notebook folder: its `Open Notebook.onetoc2`, every `.one` file in it and section
/// group subfolders, recursively (bounded). Sections are named after their files, groups after
/// their folders; OneNote's recycle bin, backups and NoteCraft's temporary files are skipped, as
/// are sections that can't be read (logged).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_notebook_dir(dir: &std::path::Path) -> Result<Notebook> {
    dir::read(dir)
}

/// Writes a notebook as a folder (created if needed): the TOC, one `.one` per section, a
/// subfolder per section group. Each file is written to a temporary name and then renamed over
/// the old one. Files the notebook doesn't mention are left alone.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_notebook_dir(notebook: &Notebook, dir: &std::path::Path) -> Result<()> {
    dir::write(notebook, dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_stem("Quick Notes"), "Quick Notes");
        assert_eq!(safe_file_stem("a/b:c?"), "a_b_c_");
        assert_eq!(safe_file_stem(" ... "), "Untitled");
        assert_eq!(safe_file_stem("end. "), "end");
        let mut s = Section::new("Plans/2026", Color::WHITE);
        assert_eq!(section_file_name(&s), "Plans_2026.one");
        s.source.file_name = Some("Old.one".into());
        assert_eq!(section_file_name(&s), "Old.one");
    }

    #[test]
    fn junk_is_an_error() {
        assert!(read_section(b"junk").is_err());
        assert!(read_toc(&[]).is_err());
    }
}

#[cfg(test)]
mod tests_roundtrip;
