//! Font files the user adds, as plain OpenType: TrueType, OpenType and collections pass through;
//! WOFF and WOFF2 are unwrapped with `wuff`, after checks that keep a small hostile file from
//! making the decoder allocate gigabytes.
//!
//! Ported from WordCraft (`crates/fonts/src/fontdb/user.rs`, storytold/wordcraft, MIT OR
//! Apache-2.0, the same licence as NoteCraft): the signature sniffing, the WOFF 1.0 size check, the
//! WOFF2 glyph point and collection directory checks, and the brotli wrapper.

use crate::FontError;

/// Largest font file accepted, bytes.
pub const MAX_FONT_FILE_BYTES: usize = 50 << 20;
/// Largest font a WOFF or WOFF2 file may unwrap to, bytes.
const MAX_DECODED_BYTES: usize = 128 << 20;

/// The kind of font file `bytes` holds, from its signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,
    OpenType,
    Collection,
    Woff,
    Woff2,
}

pub fn sniff(bytes: &[u8]) -> Option<FontFormat> {
    match bytes.get(..4)? {
        [0, 1, 0, 0] | b"true" | b"typ1" => Some(FontFormat::TrueType),
        b"OTTO" => Some(FontFormat::OpenType),
        b"ttcf" => Some(FontFormat::Collection),
        b"wOFF" => Some(FontFormat::Woff),
        b"wOF2" => Some(FontFormat::Woff2),
        _ => None,
    }
}

/// The font file in `bytes` as plain OpenType (TTF/OTF/TTC) bytes: WOFF and WOFF2 are unwrapped.
/// Checks the size and the signature only; the faces are checked when they are added.
pub fn decode_font_file(bytes: &[u8]) -> Result<Vec<u8>, FontError> {
    if bytes.is_empty() {
        return Err(FontError::Empty);
    }
    if bytes.len() > MAX_FONT_FILE_BYTES {
        return Err(FontError::TooLarge { bytes: bytes.len() });
    }
    let decoded = match sniff(bytes).ok_or(FontError::NotAFont)? {
        FontFormat::TrueType | FontFormat::OpenType | FontFormat::Collection => return Ok(bytes.to_vec()),
        FontFormat::Woff => {
            check_woff1_size(bytes)?;
            unwrap_web_font(|| wuff::decompress_woff1(bytes))?
        }
        FontFormat::Woff2 => {
            check_woff2_collection(bytes)?;
            let mut too_many_points = false;
            let unwrapped = unwrap_web_font(|| {
                wuff::decompress_woff2_with_custom_brotli(bytes, &mut |data, size| {
                    let stream = unbrotli(data, size)?;
                    too_many_points = !woff2_points_fit(bytes, &stream);
                    if too_many_points { Err(TOO_MANY_POINTS.into()) } else { Ok(stream) }
                })
            });
            if too_many_points {
                return Err(FontError::BadWebFont(TOO_MANY_POINTS.into()));
            }
            unwrapped?
        }
    };
    if decoded.len() > MAX_DECODED_BYTES {
        return Err(FontError::TooLarge { bytes: decoded.len() });
    }
    match sniff(&decoded) {
        Some(FontFormat::TrueType | FontFormat::OpenType | FontFormat::Collection) => Ok(decoded),
        _ => Err(FontError::BadWebFont("it doesn't hold a TrueType or OpenType font".into())),
    }
}

/// WOFF 1.0: the font its table directory describes must fit [`MAX_DECODED_BYTES`]. The decoder
/// sizes each table's buffer from the directory's `origLength` before inflating it. A short
/// header or directory is left to the decoder to report.
fn check_woff1_size(bytes: &[u8]) -> Result<(), FontError> {
    let u32_at = |at: usize| bytes.get(at..at.saturating_add(4)).and_then(|b| b.try_into().ok()).map(u32::from_be_bytes);
    let tables = bytes.get(12..14).and_then(|n| n.try_into().ok()).map_or(0, |n| usize::from(u16::from_be_bytes(n)));
    let mut total = 12 + 16 * tables;
    for i in 0..tables {
        let entry = 44 + 20 * i;
        let (Some(compressed), Some(orig)) = (u32_at(entry + 8), u32_at(entry + 12)) else { break };
        total = total.saturating_add((compressed.max(orig) as usize).saturating_add(3) & !3);
    }
    if total > MAX_DECODED_BYTES {
        return Err(FontError::TooLarge { bytes: total });
    }
    Ok(())
}

/// Why a WOFF2 file whose glyph has more points than a glyph may is refused.
const TOO_MANY_POINTS: &str = "a glyph has more than 65536 points";

/// WOFF2's brotli stream, which must decompress to exactly `size` bytes (the decoder has checked
/// `size` against its own cap and the compression ratio).
fn unbrotli(data: &[u8], size: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut out = Vec::with_capacity(size.min(MAX_DECODED_BYTES));
    brotli_decompressor::Decompressor::new(data, 4096).take(size as u64 + 1).read_to_end(&mut out)?;
    if out.len() != size {
        return Err("the compressed data doesn't have the size the directory gives".into());
    }
    Ok(out)
}

/// Does every glyph in the WOFF2 file's transformed `glyf` tables have at most 65536 points, as
/// the decoder requires? It allocates a glyph's points from its counts before checking them, so a
/// small file could claim gigabytes. `stream` is the decompressed table data; anything that
/// doesn't parse is refused too (the decoder would refuse it).
fn woff2_points_fit(file: &[u8], stream: &[u8]) -> bool {
    woff2_transformed_glyf(file, stream).is_some_and(|tables| tables.iter().all(|t| glyf_points_fit(t).unwrap_or(false)))
}

/// The transformed `glyf` tables in `stream`, found from the WOFF2 table directory in `file`
/// (tables follow each other in directory order).
fn woff2_transformed_glyf<'a>(file: &[u8], stream: &'a [u8]) -> Option<Vec<&'a [u8]>> {
    let (tables, _) = woff2_directory(file)?;
    let (mut offset, mut glyf) = (0usize, Vec::new());
    for t in tables {
        let end = offset.checked_add(t.len)?;
        if t.transformed_glyf {
            glyf.push(stream.get(offset..end)?);
        }
        offset = end;
    }
    Some(glyf)
}

/// A WOFF2 table directory entry: whether it is a transformed `glyf` table, and its length in
/// the decompressed stream.
struct Woff2Table {
    transformed_glyf: bool,
    len: usize,
}

/// The WOFF2 table directory in `file`, and where it ends (the collection directory follows).
fn woff2_directory(file: &[u8]) -> Option<(Vec<Woff2Table>, usize)> {
    let tables = u16::from_be_bytes(file.get(12..14)?.try_into().ok()?);
    let (mut at, mut out) = (48usize, Vec::with_capacity(usize::from(tables)));
    for _ in 0..tables {
        let flags = *file.get(at)?;
        at += 1;
        let tag: Option<&[u8]> = if flags & 63 == 63 {
            at += 4;
            Some(file.get(at - 4..at)?)
        } else {
            None
        };
        let is = |index: u8, name: &[u8]| flags & 63 == index || tag == Some(name);
        let (is_glyf, is_loca) = (is(10, b"glyf"), is(11, b"loca"));
        let orig = base128(file, &mut at)?;
        // glyf and loca are transformed at version 0, other tables at any other version.
        let transformed = if is_glyf || is_loca { flags >> 6 == 0 } else { flags >> 6 != 0 };
        let len = if transformed { base128(file, &mut at)? } else { orig };
        out.push(Woff2Table { transformed_glyf: is_glyf && transformed, len });
    }
    Some((out, at))
}

/// Largest table directories a WOFF2 collection may rebuild, bytes. Real collections need
/// kilobytes; a collection listing the same tables over and over could rebuild hundreds of
/// megabytes before the decoder checks its output size.
const MAX_WOFF2_COLLECTION_HEADER: usize = 16 << 20;

/// Why a WOFF2 collection whose directories are too large, or list a table twice, is refused.
const BAD_COLLECTION: &str = "the collection's font directories are too large or list a table twice";

/// WOFF2 collections: the table directories the decoder rebuilds must fit
/// [`MAX_WOFF2_COLLECTION_HEADER`], and no font may list a table twice. A directory that doesn't
/// parse is left to the decoder to report.
fn check_woff2_collection(file: &[u8]) -> Result<(), FontError> {
    if file.get(4..8) != Some(b"ttcf".as_slice()) {
        return Ok(());
    }
    match woff2_collection_fits(file) {
        Some(false) => Err(FontError::BadWebFont(BAD_COLLECTION.into())),
        _ => Ok(()),
    }
}

/// Do the collection's rebuilt table directories fit, with no table listed twice in one font?
/// `None` when the directories don't parse.
fn woff2_collection_fits(file: &[u8]) -> Option<bool> {
    let (tables, mut at) = woff2_directory(file)?;
    let version = u32::from_be_bytes(file.get(at..at.checked_add(4)?)?.try_into().ok()?);
    at += 4;
    let fonts = read_255_u16(file, &mut at)?;
    // TTC header (with the DSIG fields of version 2) and one offset per font.
    let dsig = if version == 0x0002_0000 { 12 } else { 0 };
    let mut size = 12 + dsig + 4 * fonts;
    let mut indices: Vec<usize> = Vec::new();
    for _ in 0..fonts {
        let n = read_255_u16(file, &mut at)?;
        size = size.saturating_add(12 + 16 * n);
        if size > MAX_WOFF2_COLLECTION_HEADER {
            return Some(false);
        }
        at = at.checked_add(4)?; // flavor
        indices.clear();
        for _ in 0..n {
            let i = read_255_u16(file, &mut at)?;
            if i >= tables.len() {
                return None;
            }
            indices.push(i);
        }
        indices.sort_unstable();
        if indices.windows(2).any(|w| w.first() == w.get(1)) {
            return Some(false);
        }
    }
    Some(true)
}

/// WOFF2's 255UInt16.
fn read_255_u16(b: &[u8], at: &mut usize) -> Option<usize> {
    let mut byte = || {
        *at += 1;
        b.get(*at - 1).copied().map(usize::from)
    };
    Some(match byte()? {
        253 => (byte()? << 8) | byte()?,
        254 => byte()? + 253 * 2,
        255 => byte()? + 253,
        v => v,
    })
}

/// WOFF2's UIntBase128.
fn base128(b: &[u8], at: &mut usize) -> Option<usize> {
    let mut v: u32 = 0;
    for i in 0..5 {
        let byte = *b.get(*at)?;
        *at += 1;
        if (i == 0 && byte == 0x80) || v >> 25 != 0 {
            return None;
        }
        v = (v << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return usize::try_from(v).ok();
        }
    }
    None
}

/// Does every glyph of the transformed `glyf` table `t` have at most 65536 points?
fn glyf_points_fit(t: &[u8]) -> Option<bool> {
    let u32_at = |at: usize| t.get(at..at + 4).and_then(|b| b.try_into().ok()).map(u32::from_be_bytes).map(|v| v as usize);
    let glyphs = usize::from(u16::from_be_bytes(t.get(4..6)?.try_into().ok()?));
    let contours_end = 36usize.checked_add(u32_at(8)?)?;
    let contours = t.get(36..contours_end)?;
    let points = t.get(contours_end..contours_end.checked_add(u32_at(12)?)?)?;
    let mut at = 0;
    for g in 0..glyphs {
        // The decoder reads the count as unsigned and only 0xFFFF is a composite glyph (no points
        // here): 0x8000..=0xFFFE are that many contours, so they are counted, not read as negative.
        let n = u16::from_be_bytes(contours.get(2 * g..2 * g + 2)?.try_into().ok()?);
        let mut total = 0usize;
        for _ in 0..if n == 0xFFFF { 0 } else { n } {
            total += read_255_u16(points, &mut at)?;
            if total > 1 << 16 {
                return Some(false);
            }
        }
    }
    Some(true)
}

/// Run a WOFF/WOFF2 decoder. It checks its input, but it is third-party code reading hostile
/// bytes: a panic in it is caught (on the desktop; the web aborts) and reported as a damaged file.
fn unwrap_web_font(decode: impl FnOnce() -> Result<Vec<u8>, wuff::WuffErr>) -> Result<Vec<u8>, FontError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(FontError::BadWebFont(format!("{e:?}"))),
        Err(_) => Err(FontError::BadWebFont("damaged file".into())),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `sfnt` wrapped as WOFF 1.0 with every table stored uncompressed (allowed by the format).
    pub(crate) fn woff1(sfnt: &[u8]) -> Vec<u8> {
        let be16 = |b: &[u8], at: usize| u16::from_be_bytes([b[at], b[at + 1]]);
        let be32 = |b: &[u8], at: usize| u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
        let n = usize::from(be16(sfnt, 4));
        let mut tables: Vec<([u8; 4], u32, &[u8])> = (0..n)
            .map(|i| {
                let e = 12 + 16 * i;
                let tag = [sfnt[e], sfnt[e + 1], sfnt[e + 2], sfnt[e + 3]];
                let (off, len) = (be32(sfnt, e + 8) as usize, be32(sfnt, e + 12) as usize);
                (tag, be32(sfnt, e + 4), &sfnt[off..off + len])
            })
            .collect();
        tables.sort_by_key(|t| t.0);
        let mut out = Vec::new();
        out.extend_from_slice(b"wOFF");
        out.extend_from_slice(&sfnt[0..4]);
        out.extend_from_slice(&0u32.to_be_bytes()); // length, set below
        out.extend_from_slice(&(n as u16).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(sfnt.len() as u32).to_be_bytes()); // totalSfntSize (approximate)
        out.extend_from_slice(&[0, 1, 0, 0]); // version
        out.extend_from_slice(&[0; 20]); // meta and private blocks
        let mut offset = 44 + 20 * n;
        let mut data = Vec::new();
        for (tag, checksum, bytes) in &tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(&checksum.to_be_bytes());
            data.extend_from_slice(bytes);
            while data.len() % 4 != 0 {
                data.push(0);
            }
            offset = 44 + 20 * n + data.len();
        }
        out.extend_from_slice(&data);
        let len = out.len() as u32;
        out[8..12].copy_from_slice(&len.to_be_bytes());
        out
    }

    #[test]
    fn plain_fonts_pass_and_junk_is_refused() {
        let ttf = crate::tests::test_font();
        assert_eq!(decode_font_file(ttf).unwrap(), ttf);
        assert!(matches!(decode_font_file(b""), Err(FontError::Empty)));
        assert!(matches!(decode_font_file(b"hello world"), Err(FontError::NotAFont)));
        assert!(matches!(decode_font_file(b"wOFFxxxxxxxxxxxxxxxx"), Err(FontError::BadWebFont(_))));
        assert!(matches!(decode_font_file(b"wOF2xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"), Err(FontError::BadWebFont(_))));
        let big = vec![0u8; MAX_FONT_FILE_BYTES + 1];
        assert!(matches!(decode_font_file(&big), Err(FontError::TooLarge { .. })));
    }

    #[test]
    fn woff_unwraps_to_the_font() {
        let ttf = crate::tests::test_font();
        let woff = woff1(ttf);
        let sfnt = decode_font_file(&woff).unwrap();
        assert_eq!(sniff(&sfnt), Some(FontFormat::TrueType));
        assert!(crate::face_parses(&sfnt, 0), "the unwrapped font parses");
        // A directory claiming huge tables is refused before anything is allocated.
        let mut bomb = woff.clone();
        bomb[44 + 12..44 + 16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(decode_font_file(&bomb), Err(FontError::TooLarge { .. })));
        // Truncations and flipped bytes never panic.
        for cut in (0..woff.len()).step_by(97) {
            let _ = decode_font_file(&woff[..cut]);
        }
        for i in (0..woff.len()).step_by(53) {
            let mut w = woff.clone();
            w[i] ^= 0xA5;
            let _ = decode_font_file(&w);
        }
    }

    #[test]
    fn woff2_helpers_read_hostile_bytes_safely() {
        assert_eq!(base128(&[0x80, 1], &mut 0), None, "a leading zero byte");
        assert_eq!(base128(&[0x3F], &mut 0), Some(63));
        assert_eq!(read_255_u16(&[253, 1, 2], &mut 0), Some(258));
        assert_eq!(read_255_u16(&[], &mut 0), None);
        assert!(woff2_directory(&[0; 13]).is_none());
        assert_eq!(glyf_points_fit(&[0; 3]), None);
    }
}
