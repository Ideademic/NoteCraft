//! The icon written for embedded files (MS-ONE requires one, §2.2.32): a small document glyph
//! drawn in code and encoded as a PNG with stored (uncompressed) deflate blocks. Original
//! artwork, no assets.

use notecraft_onestore::crc::crc32;

const SIZE: u32 = 32;

/// The icon's RGBA pixels: a page with a folded corner and three text lines, NoteCraft purple.
fn pixels() -> Vec<[u8; 4]> {
    let mut px = vec![[0u8; 4]; (SIZE * SIZE) as usize];
    let ink = [0xC4, 0x4D, 0xF0, 0xFF];
    let paper = [0xFF, 0xFF, 0xFF, 0xFF];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (l, r, t, b) = (6, 25, 3, 28);
            let inside = x >= l && x <= r && y >= t && y <= b;
            let corner = x + t > r - 7 + y; // the folded corner cut
            let c = if !inside || (x > r - 7 && y < t + 7 && corner) {
                continue;
            } else if x == l
                || x == r
                || y == t
                || y == b
                || (x > r - 7 && y == t + 7)
                || (y < t + 7 && x == r - 7)
                || ((y == 12 || y == 17 || y == 22) && x > l + 3 && x < r - 3)
            {
                ink
            } else {
                paper
            };
            if let Some(p) = px.get_mut((y * SIZE + x) as usize) {
                *p = c;
            }
        }
    }
    px
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + u32::from(d)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// The PNG file.
pub fn file_icon_png() -> Vec<u8> {
    let mut raw = Vec::with_capacity(((SIZE * 4 + 1) * SIZE) as usize);
    let px = pixels();
    for row in px.chunks(SIZE as usize) {
        raw.push(0); // filter: none
        for p in row {
            raw.extend_from_slice(p);
        }
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&SIZE.to_be_bytes());
    ihdr.extend_from_slice(&SIZE.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_is_a_png() {
        let png = super::file_icon_png();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(crate::read::picture_size(&png), Some(notecraft_doc::Size::new(24.0, 24.0)));
        assert_eq!(super::adler32(b"Wikipedia"), 0x11E6_0398);
    }
}
