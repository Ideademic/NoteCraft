//! ExtendedGUID and CompactID ([MS-ONESTORE] §2.2.1, §2.2.2).

/// A GUID plus a counter `n`: object, object space and revision ids.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExtendedGuid {
    pub guid: [u8; 16],
    pub n: u32,
}

impl ExtendedGuid {
    pub const NIL: ExtendedGuid = ExtendedGuid { guid: [0; 16], n: 0 };

    pub fn is_nil(&self) -> bool {
        *self == ExtendedGuid::NIL
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_id_bits() {
        let c = CompactId::from_u32(0x0012_3405);
        assert_eq!((c.n, c.guid_index), (5, 0x1234));
        assert_eq!(c.to_u32(), 0x0012_3405);
    }
}
