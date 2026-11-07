//! Bounds-checked little-endian reading and writing. Every read returns an [`Error`] instead of
//! panicking when the input is short (hostile files).

use crate::{Error, Result};

/// A cursor over a byte slice.
#[derive(Clone, Copy, Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    /// Bytes not read yet.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// The next `n` bytes.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or_else(|| short("length overflow"))?;
        let out = self.data.get(self.pos..end).ok_or_else(|| short("unexpected end of data"))?;
        self.pos = end;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self.bytes(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(b);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn guid(&mut self) -> Result<[u8; 16]> {
        self.array()
    }

    /// The rest of the data.
    pub fn rest(&mut self) -> &'a [u8] {
        let out = self.data.get(self.pos..).unwrap_or_default();
        self.pos = self.data.len();
        out
    }
}

fn short(what: &str) -> Error {
    Error::Corrupt(what.to_owned())
}

/// `data[offset..offset + len]`, or an error naming `what`.
pub fn slice<'a>(data: &'a [u8], offset: u64, len: u64, what: &str) -> Result<&'a [u8]> {
    let start = usize::try_from(offset).map_err(|_| Error::Corrupt(format!("{what}: offset out of range")))?;
    let len = usize::try_from(len).map_err(|_| Error::Corrupt(format!("{what}: size out of range")))?;
    let end = start.checked_add(len).ok_or_else(|| Error::Corrupt(format!("{what}: size overflow")))?;
    data.get(start..end).ok_or_else(|| Error::Corrupt(format!("{what}: reference past the end of the file")))
}

/// Little-endian output helpers.
pub trait WriteLe {
    fn put_u8(&mut self, v: u8);
    fn put_u16(&mut self, v: u16);
    fn put_u32(&mut self, v: u32);
    fn put_u64(&mut self, v: u64);
    fn put(&mut self, b: &[u8]);
}

impl WriteLe for Vec<u8> {
    fn put_u8(&mut self, v: u8) {
        self.push(v);
    }
    fn put_u16(&mut self, v: u16) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u64(&mut self, v: u64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put(&mut self, b: &[u8]) {
        self.extend_from_slice(b);
    }
}

/// UTF-16LE code units of `s`.
pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// Decodes UTF-16LE bytes (an odd trailing byte is dropped, unpaired surrogates become U+FFFD).
pub fn from_utf16le(b: &[u8]) -> String {
    let units = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c));
    char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_bounds_checked() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.u16().unwrap(), 0x0201);
        assert!(r.u16().is_err());
        assert_eq!(r.u8().unwrap(), 3);
        assert!(r.u8().is_err());
        assert!(r.bytes(usize::MAX).is_err());
        assert!(slice(&[0; 4], u64::MAX, 1, "x").is_err());
        assert!(slice(&[0; 4], 2, 3, "x").is_err());
        assert_eq!(slice(&[0, 1, 2, 3], 1, 2, "x").unwrap(), &[1, 2]);
    }

    #[test]
    fn utf16_round_trip() {
        let s = "Notes ✓ 𝄞";
        assert_eq!(from_utf16le(&utf16le(s)), s);
        assert_eq!(from_utf16le(&[0x41, 0x00, 0x42]), "A");
    }
}
