//! Clean-room reader and writer for the **MS-ONESTORE** revision-store container that `.one`
//! (section) and `.onetoc2` (notebook table of contents) files use.
//!
//! Written only from Microsoft's public Open Specifications ([MS-ONESTORE], [MS-DTYP],
//! [MS-OSHARED] for MsoCrc32). Never from other implementations' source (AGENTS.md, Clean room).
//!
//! Scope (phase 2, format agent):
//! - Reader: [`header`] (§2.3.1), file node lists and their fragments ([`filenode`], §2.4),
//!   object space and revision manifests, global id tables, CompactID / ExtendedGUID ([`guid`],
//!   §2.2), property sets ([`propset`], §2.6), file data store objects, the transaction log
//!   ([`txlog`], §2.3.3), both CRCs ([`crc`]: RFC 3309 for `.one`, MsoCrc32 for `.onetoc2`).
//!   Every length and offset is checked against the buffer; recursion and counts are bounded;
//!   hostile input returns an [`Error`], never panics.
//! - Writer: one revision, one transaction, an empty free chunk list, exact
//!   `cbExpectedFileLength`.
//! - The desktop variant (`guidFileFormat` for .one / .onetoc2) is read and written; the
//!   FSSHTTP-packaged variant (OneDrive downloads) is detected and reported as
//!   [`Error::Unsupported`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod crc;
pub mod filenode;
pub mod guid;
pub mod header;
pub mod propset;
pub mod txlog;

pub use guid::{CompactId, ExtendedGuid};
pub use header::{FileKind, Header};
pub use propset::{PropertyId, PropertySet, PropertyValue};

/// Errors reading or writing a revision store.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("not a OneNote revision store file")]
    NotOneStore,
    #[error("unsupported OneNote file variant: {0}")]
    Unsupported(String),
    #[error("corrupt file: {0}")]
    Corrupt(String),
    #[error("checksum mismatch: {0}")]
    Crc(String),
    #[error("file too large or too deeply nested: {0}")]
    Limit(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One object: its id, its JCID (type) and property set, plus any file data it references.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Object {
    pub id: ExtendedGuid,
    pub jcid: u32,
    pub props: PropertySet,
    /// Bytes of a referenced file data store object (pictures, embedded files, printouts).
    pub file_data: Option<std::sync::Arc<Vec<u8>>>,
}

/// One object space (a page, the section's page series, a notebook TOC…) at its current
/// revision: its objects and its root objects by role.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjectSpace {
    pub id: ExtendedGuid,
    /// Root objects: (root role, object id). Role 1 is the default content root (§2.1.8).
    pub roots: Vec<(u32, ExtendedGuid)>,
    pub objects: Vec<Object>,
}

/// A parsed revision store: the header's file type and the object spaces at their current
/// revisions. The `one` crate maps this to and from the model.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Store {
    pub kind: FileKind,
    /// `guidFile`: this file's identity.
    pub file_id: notecraft_doc::Guid,
    /// The root object space (`gosidRoot`).
    pub root: ExtendedGuid,
    pub spaces: Vec<ObjectSpace>,
}

/// Parses a `.one` or `.onetoc2` file.
pub fn read(bytes: &[u8]) -> Result<Store> {
    let _ = header::Header::parse(bytes)?;
    Err(Error::NotImplemented("notecraft_onestore::read"))
}

/// Writes a store as a desktop-format file.
pub fn write(store: &Store) -> Result<Vec<u8>> {
    let _ = store;
    Err(Error::NotImplemented("notecraft_onestore::write"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn junk_is_an_error_not_a_panic() {
        for input in [&b""[..], b"hello", &[0u8; 2048]] {
            assert!(super::read(input).is_err());
        }
        assert!(super::write(&super::Store::default()).is_err());
    }
}
