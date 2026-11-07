//! Checksums: the CRC of [RFC 3309] that `.one` files use, and MsoCrc32 ([MS-OSHARED] §2.4.3.2)
//! that `.onetoc2` files use ([MS-ONESTORE] §2.1.2 / §2.3.3.2). Hand-rolled from those specs:
//! no crate dependency.
//!
//! [MS-ONESTORE] §2.1.2 gives the `.one` CRC as polynomial 0x04C11DB7 with the least significant
//! bit as the x³¹ coefficient (the reflected form, 0xEDB88320), the register starting at all ones
//! and inverted at the end. The spec's own example (§3.1) is checked in the tests: the CRC of the
//! file name `Example.one` (UTF-16 plus a terminating null) is 0xCEBE8422.

/// The reflected form of polynomial 0x04C11DB7.
const POLY_REFLECTED: u32 = 0xEDB8_8320;

const fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ POLY_REFLECTED } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static CRC32: [u32; 256] = crc32_table();

/// The `.one` CRC of `data`.
pub fn crc32(data: &[u8]) -> u32 {
    crc32_update(0, data)
}

/// Continues a `.one` CRC: `crc32_update(crc32(a), b) == crc32(a ++ b)`, and a running value
/// "initialised to zero" (as [MS-ONESTORE] §2.6.10 describes) starts from `0`.
pub fn crc32_update(crc: u32, data: &[u8]) -> u32 {
    let mut c = !crc;
    for &b in data {
        let idx = usize::from((c as u8) ^ b);
        c = (c >> 8) ^ CRC32.get(idx).copied().unwrap_or(0);
    }
    !c
}

/// MsoCrc32 polynomial, normal representation ([MS-OSHARED] §2.4.3).
const MSO_POLY: u32 = 0xAF;

const fn mso_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut v = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            v = if v & 0x8000_0000 != 0 { (v << 1) ^ MSO_POLY } else { v << 1 };
            bit += 1;
        }
        // [MS-OSHARED] §2.4.3.1 masks each cache entry with 0xFFFF; followed as written.
        table[i] = v & 0xFFFF;
        i += 1;
    }
    table
}

static MSO: [u32; 256] = mso_table();

/// MsoCrc32Compute ([MS-OSHARED] §2.4.3.2): `crc` is the caller's initial value (or the result
/// for a previous part of the stream).
pub fn mso_crc32(crc: u32, data: &[u8]) -> u32 {
    let mut c = crc;
    for &b in data {
        let idx = usize::from(((c >> 24) as u8) ^ b);
        c = (c << 8) ^ MSO.get(idx).copied().unwrap_or(0);
    }
    c
}

/// The `crcName` of a file name ([MS-ONESTORE] §2.3.1): the `.one` CRC of the UTF-16 name with
/// its extension and a terminating null, whatever the file type.
pub fn name_crc(file_name: &str) -> u32 {
    let mut bytes = crate::bytes::utf16le(file_name);
    bytes.extend_from_slice(&[0, 0]);
    crc32(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_name_crc() {
        // [MS-ONESTORE] §3.1: crcName 0xCEBE8422 for "Example.one".
        assert_eq!(name_crc("Example.one"), 0xCEBE_8422);
    }

    #[test]
    fn crc32_check_value() {
        // The standard check value of this CRC (CRC-32 of "123456789").
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32_update(crc32(b"1234"), b"56789"), crc32(b"123456789"));
    }

    #[test]
    fn mso_crc32_vectors() {
        // Cache entries straight from the §2.4.3.1 algorithm.
        assert_eq!(MSO[0], 0);
        assert_eq!(MSO[1], 0xAF);
        assert_eq!(MSO[2], 0x15E);
        assert_eq!(MSO[255], 0x6565);
        // Computed independently from the §2.4.3.2 pseudo-code.
        assert_eq!(mso_crc32(0, b"123456789"), 0xBD0B_E338);
        assert_eq!(mso_crc32(0, b"Example.one"), 0x8C3E_44EE);
        assert_eq!(mso_crc32(mso_crc32(0, b"Example"), b".one"), 0x8C3E_44EE);
    }
}
