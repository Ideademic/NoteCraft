//! The file header ([MS-ONESTORE] §2.3.1): 1024 bytes at offset 0.

use crate::{Error, Result};

/// Size of the header, bytes.
pub const HEADER_SIZE: usize = 1024;

/// What kind of revision store a file is (from `guidFileType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileKind {
    /// A section (`.one`).
    #[default]
    Section,
    /// A notebook table of contents (`.onetoc2`).
    Toc,
}

/// The parsed header fields NoteCraft uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub kind: FileKind,
    /// `guidFile`.
    pub file_guid: [u8; 16],
    /// `cbExpectedFileLength`.
    pub expected_length: u64,
}

impl Header {
    /// Parses and validates the header.
    pub fn parse(bytes: &[u8]) -> Result<Header> {
        if bytes.len() < HEADER_SIZE {
            return Err(Error::NotOneStore);
        }
        Err(Error::NotImplemented("header parsing"))
    }
}
