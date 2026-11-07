//! `.onepkg` notebook packages: a Microsoft Cabinet file holding a notebook folder (its
//! `Open Notebook.onetoc2`, the section `.one` files and section group subfolders).
//!
//! Scope (phase 2, format agent): read cabinets compressed with none / MSZIP / LZX, write
//! MSZIP, through the `cab` crate (MIT). Entry counts, names and sizes are bounded; paths are
//! normalised and never escape the package.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use notecraft_doc::Notebook;

/// Most files one package may hold.
pub const MAX_ENTRIES: usize = 10_000;
/// Largest total uncompressed size read, bytes (2 GiB).
pub const MAX_TOTAL_SIZE: u64 = 2 << 30;

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

/// Reads a `.onepkg` into a notebook.
pub fn read(bytes: &[u8]) -> Result<Notebook> {
    if !bytes.starts_with(b"MSCF") {
        return Err(Error::NotPackage("no cabinet signature".into()));
    }
    Err(Error::NotImplemented("notecraft_onepkg::read"))
}

/// Writes a notebook as a `.onepkg`.
pub fn write(notebook: &Notebook) -> Result<Vec<u8>> {
    let _ = notebook;
    Err(Error::NotImplemented("notecraft_onepkg::write"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn junk_is_refused() {
        assert!(matches!(super::read(b"PK\x03\x04"), Err(super::Error::NotPackage(_))));
        assert!(super::read(b"MSCF").is_err());
    }
}
