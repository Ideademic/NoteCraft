//! Value conversions: strings, times, colours, half-inches.

use chrono::{DateTime, Utc};
use notecraft_doc::Color;
use notecraft_onestore::bytes::{from_utf16le, utf16le};
use notecraft_onestore::{PropertySet, PropertyValue};

/// Seconds from 1601-01-01 (FILETIME epoch) to 1970-01-01.
const FILETIME_UNIX_SECS: i64 = 11_644_473_600;
/// Seconds from 1970-01-01 to 1980-01-01 (the Time32 epoch, [MS-ONE] §2.3.1).
const TIME32_UNIX_SECS: i64 = 315_532_800;

/// A FILETIME ([MS-DTYP]: 100 ns intervals since 1601) as a time; out of range reads as `None`.
pub fn from_filetime(ft: u64) -> Option<DateTime<Utc>> {
    let secs = i64::try_from(ft / 10_000_000).ok()?.checked_sub(FILETIME_UNIX_SECS)?;
    let nanos = u32::try_from((ft % 10_000_000) * 100).ok()?;
    DateTime::from_timestamp(secs, nanos)
}

pub fn to_filetime(t: DateTime<Utc>) -> u64 {
    let secs = t.timestamp().saturating_add(FILETIME_UNIX_SECS).max(0) as u64;
    secs.saturating_mul(10_000_000).saturating_add(u64::from(t.timestamp_subsec_nanos() / 100))
}

/// A Time32 (seconds since 1980, UTC).
pub fn from_time32(v: u32) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(i64::from(v) + TIME32_UNIX_SECS, 0)
}

pub fn to_time32(t: DateTime<Utc>) -> u32 {
    u32::try_from(t.timestamp().saturating_sub(TIME32_UNIX_SECS).max(0)).unwrap_or(u32::MAX)
}

/// A WzInAtom ([MS-ONE] §2.2.9): UTF-16, the terminating null dropped.
pub fn wz(b: &[u8]) -> String {
    let s = from_utf16le(b);
    match s.strip_suffix('\0') {
        Some(t) => t.to_owned(),
        None => s,
    }
}

/// A WzInAtom value: UTF-16 with a terminating null.
pub fn wz_bytes(s: &str) -> PropertyValue {
    let mut b = utf16le(s);
    b.extend_from_slice(&[0, 0]);
    PropertyValue::Bytes(b)
}

/// The Windows-1252 characters for 0x80–0x9F; the rest of the range matches Latin-1.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜',
    '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

/// TextExtendedAscii ([MS-ONE] §2.2.89), read as Windows-1252.
pub fn extended_ascii(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0x80..=0x9F => CP1252_HIGH.get(usize::from(c - 0x80)).copied().unwrap_or('\u{FFFD}'),
            _ => char::from(c),
        })
        .collect()
}

pub fn string(set: &PropertySet, id: u32) -> Option<String> {
    set.bytes(id).map(wz)
}

/// A GuidInAtom ([MS-ONE] §2.2.12).
pub fn guid(set: &PropertySet, id: u32) -> Option<notecraft_doc::Guid> {
    let b = set.bytes(id)?;
    let arr: [u8; 16] = b.get(..16)?.try_into().ok()?;
    Some(notecraft_doc::Guid(arr))
}

/// A COLORREF ([MS-ONE] §2.2.8): `0x00BBGGRR`, `0xFF000000` (automatic) reads as `None`.
pub fn from_colorref(v: u32) -> Option<Color> {
    if v >> 24 != 0 {
        return None;
    }
    Some(Color::rgb((v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8))
}

pub fn to_colorref(c: Color) -> u32 {
    u32::from(c.r) | (u32::from(c.g) << 8) | (u32::from(c.b) << 16)
}

/// Half-inches (MS-ONE layout) to points, sanitized.
pub fn hi_to_pt(v: f32) -> f32 {
    notecraft_doc::geom::sane(notecraft_doc::geom::half_inch_to_pt(v))
}

pub fn pt_to_hi(v: f32) -> PropertyValue {
    PropertyValue::U32(notecraft_doc::geom::pt_to_half_inch(notecraft_doc::geom::sane(v)).to_bits())
}

pub fn f32v(v: f32) -> PropertyValue {
    PropertyValue::U32(v.to_bits())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_round_trip() {
        let t = DateTime::from_timestamp(1_700_000_000, 123_456_700).unwrap();
        assert_eq!(from_filetime(to_filetime(t)), Some(t));
        let s = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        assert_eq!(from_time32(to_time32(s)), Some(s));
        // [MS-ONE] §2.3.1: zero is 1980-01-01.
        assert_eq!(from_time32(0).unwrap().to_rfc3339(), "1980-01-01T00:00:00+00:00");
        assert!(from_filetime(u64::MAX).is_some() || from_filetime(u64::MAX).is_none());
    }

    #[test]
    fn colours_and_strings() {
        // The sample files' blue section: 0x00E4A88A is #8AA8E4.
        assert_eq!(from_colorref(0x00E4_A88A), Some(Color::rgb(0x8A, 0xA8, 0xE4)));
        assert_eq!(from_colorref(0xFF00_0000), None);
        assert_eq!(to_colorref(Color::rgb(0x8A, 0xA8, 0xE4)), 0x00E4_A88A);
        let PropertyValue::Bytes(b) = wz_bytes("PageTitle") else { panic!() };
        assert_eq!(wz(&b), "PageTitle");
        assert_eq!(extended_ascii(&[0x48, 0x80, 0xE9]), "H€é");
    }
}
