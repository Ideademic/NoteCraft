//! ExtendedGUID and CompactID ([MS-ONESTORE] §2.2.1, §2.2.2), and the GUID constants the
//! format uses.

use notecraft_doc::Guid;

/// A GUID plus a counter `n`: object, object space and revision ids.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtendedGuid {
    pub guid: [u8; 16],
    pub n: u32,
}

impl ExtendedGuid {
    pub const NIL: ExtendedGuid = ExtendedGuid { guid: [0; 16], n: 0 };

    pub const fn new(guid: [u8; 16], n: u32) -> ExtendedGuid {
        ExtendedGuid { guid, n }
    }

    pub fn from_guid(g: Guid, n: u32) -> ExtendedGuid {
        ExtendedGuid { guid: g.0, n }
    }

    pub fn is_nil(&self) -> bool {
        *self == ExtendedGuid::NIL
    }

    /// The GUID part as a model GUID.
    pub fn to_guid(self) -> Guid {
        Guid(self.guid)
    }

    /// From its 20-byte on-disk form.
    pub fn from_bytes(b: &[u8; 20]) -> ExtendedGuid {
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&b[..16]);
        ExtendedGuid { guid, n: u32::from_le_bytes([b[16], b[17], b[18], b[19]]) }
    }

    pub fn to_bytes(self) -> [u8; 20] {
        let mut out = [0u8; 20];
        out[..16].copy_from_slice(&self.guid);
        out[16..].copy_from_slice(&self.n.to_le_bytes());
        out
    }
}

/// A compressed ExtendedGUID: `n` (8 bits) and an index into the global id table (24 bits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CompactId {
    pub n: u8,
    pub guid_index: u32,
}

impl CompactId {
    /// From its 32-bit little-endian form.
    pub fn from_u32(v: u32) -> CompactId {
        CompactId { n: (v & 0xFF) as u8, guid_index: v >> 8 }
    }

    pub fn to_u32(self) -> u32 {
        u32::from(self.n) | (self.guid_index << 8)
    }
}

/// GUIDs in their on-disk byte order ([MS-DTYP] §2.3.4.2).
pub mod consts {
    use notecraft_doc::Guid;

    const fn g(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> [u8; 16] {
        let a = d1.to_le_bytes();
        let b = d2.to_le_bytes();
        let c = d3.to_le_bytes();
        [a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d4[0], d4[1], d4[2], d4[3], d4[4], d4[5], d4[6], d4[7]]
    }

    /// `guidFileType` of a section (`.one`).
    pub const FILE_TYPE_ONE: [u8; 16] = g(0x7B5C_52E4, 0xD88C, 0x4DA7, [0xAE, 0xB1, 0x53, 0x78, 0xD0, 0x29, 0x96, 0xD3]);
    /// `guidFileType` of a notebook table of contents (`.onetoc2`).
    pub const FILE_TYPE_TOC: [u8; 16] = g(0x43FF_2FA1, 0xEFD9, 0x4C76, [0x9E, 0xE2, 0x10, 0xEA, 0x57, 0x22, 0x76, 0x5F]);
    /// `guidFileFormat` of the desktop revision store.
    pub const FILE_FORMAT: [u8; 16] = g(0x109A_DD3F, 0x911B, 0x49F5, [0xA5, 0xD0, 0x17, 0x91, 0xED, 0xC8, 0xAE, 0xD8]);
    /// `guidFileFormat` of the FSSHTTP packaging (§2.8.1): OneDrive / SharePoint downloads.
    pub const FILE_FORMAT_PACKAGE: [u8; 16] = g(0x638D_E92F, 0xA6D4, 0x4BC1, [0x9A, 0x36, 0xB3, 0xFC, 0x25, 0x11, 0xA5, 0xB7]);
    /// FileDataStoreObject header and footer (§2.6.13).
    pub const FILE_DATA_HEADER: [u8; 16] = g(0xBDE3_16E7, 0x2665, 0x4511, [0xA4, 0xC4, 0x8D, 0x4D, 0x0B, 0x7A, 0x9E, 0xAC]);
    pub const FILE_DATA_FOOTER: [u8; 16] = g(0x71FB_A722, 0x0F79, 0x4A0B, [0xBB, 0x13, 0x89, 0x92, 0x56, 0x42, 0x6B, 0x24]);

    pub fn guid(b: [u8; 16]) -> Guid {
        Guid(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_id_bits() {
        let c = CompactId::from_u32(0x0012_3405);
        assert_eq!((c.n, c.guid_index), (5, 0x1234));
        assert_eq!(c.to_u32(), 0x0012_3405);
    }

    #[test]
    fn constants_match_the_registry_form() {
        assert_eq!(consts::guid(consts::FILE_TYPE_ONE).to_string(), "{7B5C52E4-D88C-4DA7-AEB1-5378D02996D3}");
        assert_eq!(consts::guid(consts::FILE_TYPE_TOC).to_string(), "{43FF2FA1-EFD9-4C76-9EE2-10EA5722765F}");
        assert_eq!(consts::guid(consts::FILE_FORMAT).to_string(), "{109ADD3F-911B-49F5-A5D0-1791EDC8AED8}");
        assert_eq!(consts::guid(consts::FILE_FORMAT_PACKAGE).to_string(), "{638DE92F-A6D4-4BC1-9A36-B3FC2511A5B7}");
        let e = ExtendedGuid::new(consts::FILE_DATA_HEADER, 7);
        assert_eq!(ExtendedGuid::from_bytes(&e.to_bytes()), e);
    }
}
