//! Clean-room reader and writer for the **MS-ONESTORE** revision-store container that `.one`
//! (section) and `.onetoc2` (notebook table of contents) files use.
//!
//! Written only from Microsoft's public Open Specifications ([MS-ONESTORE], [MS-DTYP],
//! [MS-OSHARED] for MsoCrc32). Never from other implementations' source (AGENTS.md, Clean room).
//!
//! Coverage:
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

pub mod bytes;
pub mod crc;
pub mod filenode;
pub mod guid;
pub mod header;
pub mod md5;
pub mod propset;
mod read;
pub mod txlog;
mod write;

pub use guid::{CompactId, ExtendedGuid};
pub use header::{FileKind, Header};
pub use propset::{PropertyId, PropertySet, PropertyValue};
pub use read::{JCID_IS_BINARY, JCID_IS_FILE_DATA, JCID_IS_PROPERTY_SET, JCID_IS_READ_ONLY};

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
    /// The file data object's extension, with its period (`.png`).
    pub file_ext: Option<String>,
}

impl Object {
    /// A property-set object.
    pub fn new(id: ExtendedGuid, jcid: u32, props: PropertySet) -> Object {
        Object { id, jcid, props, file_data: None, file_ext: None }
    }

    /// A file data object (JCID with IsFileData set).
    pub fn file(id: ExtendedGuid, jcid: u32, data: std::sync::Arc<Vec<u8>>, ext: &str) -> Object {
        Object { id, jcid, props: PropertySet::default(), file_data: Some(data), file_ext: Some(ext.to_owned()).filter(|e| !e.is_empty()) }
    }
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

impl ObjectSpace {
    /// The root object with `role`.
    pub fn root(&self, role: u32) -> Option<ExtendedGuid> {
        self.roots.iter().find(|(r, _)| *r == role).map(|(_, id)| *id)
    }

    /// Object id → index into [`ObjectSpace::objects`].
    pub fn index(&self) -> std::collections::HashMap<ExtendedGuid, usize> {
        self.objects.iter().enumerate().map(|(i, o)| (o.id, i)).collect()
    }
}

/// A parsed revision store: the header's file type and the object spaces at their current
/// revisions. The `one` crate maps this to and from the model.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Store {
    pub kind: FileKind,
    /// `guidFile`: this file's identity.
    pub file_id: notecraft_doc::Guid,
    /// `guidAncestor`: the `guidFile` of the table of contents in the same folder (sections) or
    /// the parent folder (section group tables of contents); nil for none.
    pub ancestor: notecraft_doc::Guid,
    /// The file's name with its extension (`Quick Notes.one`): only used when writing, for the
    /// header's `crcName`. Reading leaves it empty (the name isn't stored, only its CRC).
    pub file_name: String,
    /// The root object space (`gosidRoot`).
    pub root: ExtendedGuid,
    pub spaces: Vec<ObjectSpace>,
    /// Problems the reader worked around (skipped object spaces, missing file data…).
    pub warnings: Vec<String>,
}

impl Store {
    pub fn space(&self, id: ExtendedGuid) -> Option<&ObjectSpace> {
        self.spaces.iter().find(|s| s.id == id)
    }

    pub fn root_space(&self) -> Option<&ObjectSpace> {
        self.space(self.root)
    }
}

/// Parses a `.one` or `.onetoc2` file: every object space at its current revision (default
/// context, revision role 1), counting only committed transactions.
pub fn read(bytes: &[u8]) -> Result<Store> {
    read::read(bytes)
}

/// Writes a store as a fresh desktop-format file: one revision per object space in one
/// transaction, no free chunk list, legacy fields at their required values.
pub fn write(store: &Store) -> Result<Vec<u8>> {
    write::write(store)
}

#[cfg(test)]
mod tests;
