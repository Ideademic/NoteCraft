//! GUIDs: every notebook, section group, section, page, element and ink stroke has one.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A 16-byte GUID.
///
/// The bytes are in the on-disk order of [MS-DTYP] §2.3.4.2 (the layout MS-ONESTORE stores):
/// `Data1` (u32) and `Data2`/`Data3` (u16) little-endian, then the 8 bytes of `Data4`. The text
/// form is the registry form, `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` (upper case), and the
/// JSON form is that string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// The all-zero GUID (`GUID_NULL`).
    pub const NIL: Guid = Guid([0; 16]);

    /// A new random (version 4) GUID. Uses the system's random source (the browser's
    /// `crypto.getRandomValues` on the web); if that fails, a hashed counter keeps ids unique
    /// within this run instead of failing.
    pub fn new() -> Guid {
        let mut b = [0u8; 16];
        if getrandom::fill(&mut b).is_err() {
            b = fallback_bytes();
        }
        // RFC 4122 version 4, variant 10xx (Data3's high nibble is byte 7 in this layout).
        b[7] = (b[7] & 0x0F) | 0x40;
        b[8] = (b[8] & 0x3F) | 0x80;
        Guid(b)
    }

    pub const fn from_bytes(b: [u8; 16]) -> Guid {
        Guid(b)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn is_nil(&self) -> bool {
        self.0 == [0; 16]
    }

    /// The registry form, with or without braces, any case.
    pub fn parse(s: &str) -> Option<Guid> {
        let s = s.trim();
        let s = s.strip_prefix('{').and_then(|t| t.strip_suffix('}')).unwrap_or(s);
        let parts: Vec<&str> = s.split('-').collect();
        let [p1, p2, p3, p4, p5] = parts.as_slice() else { return None };
        if p1.len() != 8 || p2.len() != 4 || p3.len() != 4 || p4.len() != 4 || p5.len() != 12 {
            return None;
        }
        let d1 = u32::from_str_radix(p1, 16).ok()?;
        let d2 = u16::from_str_radix(p2, 16).ok()?;
        let d3 = u16::from_str_radix(p3, 16).ok()?;
        let mut b = [0u8; 16];
        b[0..4].copy_from_slice(&d1.to_le_bytes());
        b[4..6].copy_from_slice(&d2.to_le_bytes());
        b[6..8].copy_from_slice(&d3.to_le_bytes());
        let tail = format!("{p4}{p5}");
        for (i, slot) in b[8..16].iter_mut().enumerate() {
            let hex = tail.get(i * 2..i * 2 + 2)?;
            *slot = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(Guid(b))
    }
}

/// Unique-per-run bytes for when the system has no random source.
fn fallback_bytes() -> [u8; 16] {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let state = RandomState::new();
    let mut out = [0u8; 16];
    for (i, half) in out.chunks_mut(8).enumerate() {
        let mut h = state.build_hasher();
        h.write_u64(n);
        h.write_usize(i);
        half.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            u16::from_le_bytes([b[4], b[5]]),
            u16::from_le_bytes([b[6], b[7]]),
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15]
        )
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for Guid {
    type Err = crate::DocError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Guid::parse(s).ok_or_else(|| crate::DocError::BadGuid(s.chars().take(64).collect()))
    }
}

impl Serialize for Guid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Guid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Guid::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("not a GUID: {}", s.chars().take(64).collect::<String>())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_uses_mixed_endian_fields() {
        let g = Guid([0x33, 0x22, 0x11, 0x00, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        assert_eq!(g.to_string(), "{00112233-4455-6677-8899-AABBCCDDEEFF}");
        assert_eq!(Guid::parse("{00112233-4455-6677-8899-aabbccddeeff}"), Some(g));
        assert_eq!(Guid::parse("00112233-4455-6677-8899-AABBCCDDEEFF"), Some(g));
    }

    #[test]
    fn parse_rejects_junk() {
        for bad in [
            "",
            "{}",
            "0011-2233",
            "{00112233-4455-6677-8899-AABBCCDDEEFG}",
            "{00112233-4455-6677-8899AABBCCDDEEFF}",
            "{é0112233-4455-6677-8899-AABBCCDDEEFF}",
        ] {
            assert_eq!(Guid::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn new_guids_are_unique_v4() {
        let a = Guid::new();
        let b = Guid::new();
        assert_ne!(a, b);
        assert!(!a.is_nil());
        assert_eq!(a.0[7] >> 4, 4);
        assert_eq!(a.0[8] >> 6, 0b10);
        assert_eq!(Guid::parse(&a.to_string()), Some(a));
    }

    #[test]
    fn serde_round_trip() {
        let g = Guid::new();
        let j = serde_json::to_string(&g).unwrap();
        assert_eq!(j, format!("\"{g}\""));
        assert_eq!(serde_json::from_str::<Guid>(&j).unwrap(), g);
        assert!(serde_json::from_str::<Guid>("\"nope\"").is_err());
    }

    #[test]
    fn fallback_is_unique() {
        assert_ne!(fallback_bytes(), fallback_bytes());
    }
}
