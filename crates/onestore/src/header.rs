//! The file header ([MS-ONESTORE] §2.3.1): 1024 bytes at offset 0, and file chunk references
//! (§2.2.4).

use crate::bytes::{Reader, WriteLe};
use crate::guid::consts;
use crate::{Error, Result};

/// Size of the header, bytes.
pub const HEADER_SIZE: usize = 1024;
/// `ffv*` fields of a `.one` file.
pub const FFV_ONE: u32 = 0x2A;
/// `ffv*` fields of a `.onetoc2` file.
pub const FFV_TOC: u32 = 0x1B;

/// What kind of revision store a file is (from `guidFileType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileKind {
    /// A section (`.one`).
    #[default]
    Section,
    /// A notebook table of contents (`.onetoc2`).
    Toc,
}

impl FileKind {
    pub fn file_type(self) -> [u8; 16] {
        match self {
            FileKind::Section => consts::FILE_TYPE_ONE,
            FileKind::Toc => consts::FILE_TYPE_TOC,
        }
    }

    pub fn ffv(self) -> u32 {
        match self {
            FileKind::Section => FFV_ONE,
            FileKind::Toc => FFV_TOC,
        }
    }

    /// The CRC this file type uses for its structures (§2.1.2). `crcName` always uses the
    /// `.one` CRC.
    pub fn crc(self, data: &[u8]) -> u32 {
        match self {
            FileKind::Section => crate::crc::crc32(data),
            FileKind::Toc => crate::crc::mso_crc32(0, data),
        }
    }
}

/// A file chunk reference: an offset and a size in the file. `nil` / `zero` record the special
/// values ([MS-ONESTORE] §2.2.4) as they were stored, whatever the field widths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fcr {
    pub stp: u64,
    pub cb: u64,
    pub nil: bool,
}

impl Fcr {
    pub const NIL: Fcr = Fcr { stp: u64::MAX, cb: 0, nil: true };
    pub const ZERO: Fcr = Fcr { stp: 0, cb: 0, nil: false };

    pub fn new(stp: u64, cb: u64) -> Fcr {
        Fcr { stp, cb, nil: false }
    }

    pub fn is_zero(&self) -> bool {
        !self.nil && self.stp == 0 && self.cb == 0
    }

    /// Neither nil nor zero.
    pub fn is_some(&self) -> bool {
        !self.nil && !self.is_zero()
    }

    /// FileChunkReference32 (§2.2.4.1).
    pub fn read32(r: &mut Reader<'_>) -> Result<Fcr> {
        let stp = r.u32()?;
        let cb = r.u32()?;
        Ok(Fcr { stp: u64::from(stp), cb: u64::from(cb), nil: stp == u32::MAX && cb == 0 })
    }

    /// FileChunkReference64x32 (§2.2.4.4).
    pub fn read64x32(r: &mut Reader<'_>) -> Result<Fcr> {
        let stp = r.u64()?;
        let cb = r.u32()?;
        Ok(Fcr { stp, cb: u64::from(cb), nil: stp == u64::MAX && cb == 0 })
    }

    /// FileChunkReference64 (§2.2.4.3).
    pub fn read64(r: &mut Reader<'_>) -> Result<Fcr> {
        let stp = r.u64()?;
        let cb = r.u64()?;
        Ok(Fcr { stp, cb, nil: stp == u64::MAX && cb == 0 })
    }

    pub fn write64x32(&self, out: &mut Vec<u8>) {
        if self.nil {
            out.put_u64(u64::MAX);
            out.put_u32(0);
        } else {
            out.put_u64(self.stp);
            out.put_u32(u32::try_from(self.cb).unwrap_or(u32::MAX));
        }
    }

    pub fn write32(&self, out: &mut Vec<u8>) {
        if self.nil {
            out.put_u32(u32::MAX);
            out.put_u32(0);
        } else {
            out.put_u32(u32::try_from(self.stp).unwrap_or(u32::MAX));
            out.put_u32(u32::try_from(self.cb).unwrap_or(u32::MAX));
        }
    }
}

/// The header fields NoteCraft reads and writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub kind: FileKind,
    /// `guidFile`.
    pub file_guid: [u8; 16],
    /// `guidAncestor`: the `guidFile` of the table of contents next to (or above) this file.
    pub ancestor: [u8; 16],
    /// `crcName`.
    pub crc_name: u32,
    /// `cTransactionsInLog`.
    pub transactions: u32,
    pub fcr_hashed_chunk_list: Fcr,
    pub fcr_transaction_log: Fcr,
    pub fcr_file_node_list_root: Fcr,
    pub fcr_free_chunk_list: Fcr,
    /// `cbExpectedFileLength`.
    pub expected_length: u64,
    pub file_version: [u8; 16],
    pub file_version_generation: u64,
    pub deny_read_file_version: [u8; 16],
}

impl Header {
    /// Parses and validates the header.
    pub fn parse(bytes: &[u8]) -> Result<Header> {
        let head = bytes.get(..HEADER_SIZE).ok_or(Error::NotOneStore)?;
        let mut r = Reader::new(head);
        let file_type = r.guid()?;
        let file_guid = r.guid()?;
        let _legacy_version = r.guid()?;
        let format = r.guid()?;
        if format == consts::FILE_FORMAT_PACKAGE {
            return Err(Error::Unsupported(
                "OneDrive/SharePoint-packaged section (FSSHTTP packaging, [MS-ONESTORE] §2.8). NoteCraft reads local OneNote 2010–2016 files: \
                 open the notebook in OneNote desktop and export it, or download it as a .onepkg"
                    .into(),
            ));
        }
        if format != consts::FILE_FORMAT {
            return Err(Error::NotOneStore);
        }
        let kind = match file_type {
            t if t == consts::FILE_TYPE_ONE => FileKind::Section,
            t if t == consts::FILE_TYPE_TOC => FileKind::Toc,
            _ => return Err(Error::NotOneStore),
        };
        let _ffv_last = r.u32()?;
        let _ffv_oldest_written = r.u32()?;
        let _ffv_newest_written = r.u32()?;
        let ffv_oldest_reader = r.u32()?;
        if ffv_oldest_reader > FFV_ONE {
            // §1.6: written by a newer program that older readers must not read.
            return Err(Error::Unsupported(format!("written by a newer OneNote (file format version {ffv_oldest_reader:#x})")));
        }
        let _fcr_legacy_free = Fcr::read32(&mut r)?;
        let _fcr_legacy_log = Fcr::read32(&mut r)?;
        let transactions = r.u32()?;
        let _cb_legacy_len = r.u32()?;
        let _placeholder = r.u64()?;
        let _fcr_legacy_root = Fcr::read32(&mut r)?;
        let _cb_legacy_free = r.u32()?;
        r.skip(4)?; // fNeedsDefrag, fRepairedFile, fNeedsGarbageCollect, fHasNoEmbeddedFileObjects
        let ancestor = r.guid()?;
        let crc_name = r.u32()?;
        let fcr_hashed_chunk_list = Fcr::read64x32(&mut r)?;
        let fcr_transaction_log = Fcr::read64x32(&mut r)?;
        let fcr_file_node_list_root = Fcr::read64x32(&mut r)?;
        let fcr_free_chunk_list = Fcr::read64x32(&mut r)?;
        let expected_length = r.u64()?;
        let _cb_free = r.u64()?;
        let file_version = r.guid()?;
        let file_version_generation = r.u64()?;
        let deny_read_file_version = r.guid()?;
        if !fcr_transaction_log.is_some() || !fcr_file_node_list_root.is_some() {
            return Err(Error::Corrupt("header: missing transaction log or root file node list".into()));
        }
        Ok(Header {
            kind,
            file_guid,
            ancestor,
            crc_name,
            transactions,
            fcr_hashed_chunk_list,
            fcr_transaction_log,
            fcr_file_node_list_root,
            fcr_free_chunk_list,
            expected_length,
            file_version,
            file_version_generation,
            deny_read_file_version,
        })
    }

    /// The 1024-byte header, with every legacy and reserved field at its required value.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_SIZE);
        out.put(&self.kind.file_type());
        out.put(&self.file_guid);
        out.put(&[0u8; 16]); // guidLegacyFileVersion: MUST be zero
        out.put(&consts::FILE_FORMAT);
        let ffv = self.kind.ffv();
        for _ in 0..4 {
            out.put_u32(ffv);
        }
        Fcr::ZERO.write32(&mut out); // fcrLegacyFreeChunkList: fcrZero
        Fcr::NIL.write32(&mut out); // fcrLegacyTransactionLog: fcrNil
        out.put_u32(self.transactions);
        out.put_u32(0); // cbLegacyExpectedFileLength
        out.put_u64(0); // rgbPlaceholder
        Fcr::NIL.write32(&mut out); // fcrLegacyFileNodeListRoot: fcrNil
        out.put_u32(0); // cbLegacyFreeSpaceInFreeChunkList
        out.put(&[0, 0, 0, 0]); // fNeedsDefrag, fRepairedFile, fNeedsGarbageCollect, fHasNoEmbeddedFileObjects
        out.put(&self.ancestor);
        out.put_u32(self.crc_name);
        self.fcr_hashed_chunk_list.write64x32(&mut out);
        self.fcr_transaction_log.write64x32(&mut out);
        self.fcr_file_node_list_root.write64x32(&mut out);
        self.fcr_free_chunk_list.write64x32(&mut out);
        out.put_u64(self.expected_length);
        out.put_u64(0); // cbFreeSpaceInFreeChunkList: no free chunk list
        out.put(&self.file_version);
        out.put_u64(self.file_version_generation);
        out.put(&self.deny_read_file_version);
        out.put_u32(0); // grfDebugLogFlags
        Fcr::ZERO.write64x32(&mut out); // fcrDebugLog
        Fcr::ZERO.write64x32(&mut out); // fcrAllocVerificationFreeChunkList
        for _ in 0..4 {
            out.put_u32(0); // bnCreated, bnLastWroteToThisFile, bnOldestWritten, bnNewestWritten
        }
        out.resize(HEADER_SIZE, 0); // rgbReserved
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Header {
        Header {
            kind: FileKind::Section,
            file_guid: [7; 16],
            ancestor: [9; 16],
            crc_name: 0xCEBE_8422,
            transactions: 1,
            fcr_hashed_chunk_list: Fcr::ZERO,
            fcr_transaction_log: Fcr::new(0x400, 0x40),
            fcr_file_node_list_root: Fcr::new(0x440, 0x80),
            fcr_free_chunk_list: Fcr::NIL,
            expected_length: 0x1000,
            file_version: [1; 16],
            file_version_generation: 1,
            deny_read_file_version: [2; 16],
        }
    }

    #[test]
    fn header_round_trip_and_offsets() {
        let h = sample();
        let b = h.to_bytes();
        assert_eq!(b.len(), HEADER_SIZE);
        // Offsets from the [MS-ONESTORE] §3.1 example.
        assert_eq!(&b[0x30..0x40], &consts::FILE_FORMAT);
        assert_eq!(u32::from_le_bytes(b[0x40..0x44].try_into().unwrap()), 0x2A);
        assert_eq!(u32::from_le_bytes(b[0x58..0x5C].try_into().unwrap()), u32::MAX);
        assert_eq!(u32::from_le_bytes(b[0x90..0x94].try_into().unwrap()), 0xCEBE_8422);
        assert_eq!(u64::from_le_bytes(b[0xB8..0xC0].try_into().unwrap()), u64::MAX);
        assert_eq!(u64::from_le_bytes(b[0xC4..0xCC].try_into().unwrap()), 0x1000);
        assert_eq!(Header::parse(&b).unwrap(), h);
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(Header::parse(&[0; 100]), Err(Error::NotOneStore));
        assert_eq!(Header::parse(&[0; 2048]), Err(Error::NotOneStore));
        let mut b = sample().to_bytes();
        b[0x30..0x40].copy_from_slice(&consts::FILE_FORMAT_PACKAGE);
        assert!(matches!(Header::parse(&b), Err(Error::Unsupported(m)) if m.contains("OneDrive/SharePoint")));
        let mut b = sample().to_bytes();
        b[0x4C] = 0x40;
        assert!(matches!(Header::parse(&b), Err(Error::Unsupported(_))));
    }
}
