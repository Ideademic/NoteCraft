//! Picture import: decode just enough to know the format and size, bounded.

use std::io::Cursor;
use std::sync::Arc;

use image::{ImageReader, Limits};
use notecraft_doc::{ImageElement, ImageFormat, Point, Size};

use crate::{Error, Result};

/// Largest picture decoded, in pixels (width × height).
pub const MAX_IMAGE_PIXELS: u64 = 100_000_000;
/// Largest picture side, pixels.
pub const MAX_IMAGE_SIDE: u32 = 30_000;
/// Largest picture file read, bytes.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Widest a new picture is placed on the page, points (about 7.5 in); larger ones are scaled
/// down, keeping their aspect.
pub const MAX_PLACED_WIDTH: f32 = 540.0;
/// Tallest a new picture is placed, points.
pub const MAX_PLACED_HEIGHT: f32 = 720.0;

/// What [`probe_image`] learns about a picture file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub width_px: u32,
    pub height_px: u32,
    /// The model's format; `None` for formats NoteCraft stores re-encoded as PNG (WebP, …).
    pub format: Option<ImageFormat>,
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>> {
    if bytes.is_empty() {
        return Err(Error::BadImage("empty file".into()));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(Error::Limit(format!("{} MB is more than {} MB", bytes.len() / 1_048_576, MAX_IMAGE_BYTES / 1_048_576)));
    }
    let mut r = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| Error::BadImage(e.to_string()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_PIXELS.saturating_mul(4));
    r.limits(limits);
    Ok(r)
}

/// The format and pixel size of a picture file, without decoding its pixels.
pub fn probe_image(bytes: &[u8]) -> Result<ImageInfo> {
    let r = reader(bytes)?;
    let format = match r.format() {
        Some(image::ImageFormat::Png) => Some(ImageFormat::Png),
        Some(image::ImageFormat::Jpeg) => Some(ImageFormat::Jpeg),
        Some(image::ImageFormat::Gif) => Some(ImageFormat::Gif),
        Some(image::ImageFormat::Bmp | image::ImageFormat::WebP) => None,
        Some(f) => return Err(Error::BadImage(format!("{f:?} pictures aren't supported"))),
        None => return Err(Error::BadImage("unknown picture format".into())),
    };
    let (w, h) = r.into_dimensions().map_err(|e| Error::BadImage(e.to_string()))?;
    check_pixels(w, h)?;
    Ok(ImageInfo { width_px: w, height_px: h, format })
}

fn check_pixels(w: u32, h: u32) -> Result<()> {
    if w == 0 || h == 0 {
        return Err(Error::BadImage("the picture has no pixels".into()));
    }
    if u64::from(w) * u64::from(h) > MAX_IMAGE_PIXELS {
        return Err(Error::Limit(format!("{w} × {h} pixels")));
    }
    Ok(())
}

/// The size a `w` × `h` pixel picture is placed at: 96 pixels per inch, scaled down to fit
/// [`MAX_PLACED_WIDTH`] × [`MAX_PLACED_HEIGHT`].
pub fn placed_size(w: u32, h: u32) -> Size {
    let natural = Size::new(w as f32 * 0.75, h as f32 * 0.75);
    let fit = natural.fit_within(Size::new(MAX_PLACED_WIDTH, MAX_PLACED_HEIGHT));
    Size::new(fit.w.max(1.0), fit.h.max(1.0)).sanitized()
}

/// A picture file as an image element at the page origin, sized by [`placed_size`]. PNG, JPEG
/// and GIF are kept as they are; BMP and WebP are decoded and stored as PNG.
pub fn import_image(bytes: &[u8]) -> Result<ImageElement> {
    let info = probe_image(bytes)?;
    let (data, format) = match info.format {
        Some(f) => (bytes.to_vec(), f),
        None => {
            let img = reader(bytes)?.decode().map_err(|e| Error::BadImage(e.to_string()))?;
            let mut out = Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| Error::BadImage(e.to_string()))?;
            (out.into_inner(), ImageFormat::Png)
        }
    };
    Ok(ImageElement { pos: Point::ZERO, size: placed_size(info.width_px, info.height_px), data: Arc::new(data), format, alt: String::new() })
}

/// Encodes straight-alpha RGBA8 pixels as a PNG file.
pub fn encode_png(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>> {
    check_pixels(width, height)?;
    let img = image::RgbaImage::from_raw(width, height, rgba).ok_or_else(|| Error::BadImage("pixel buffer size doesn't match".into()))?;
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| Error::BadImage(e.to_string()))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        encode_png(w, h, vec![200; (w * h * 4) as usize]).unwrap()
    }

    #[test]
    fn imports_png_and_sizes_it() {
        let el = import_image(&png(40, 20)).unwrap();
        assert_eq!(el.format, ImageFormat::Png);
        assert_eq!((el.size.w, el.size.h), (30.0, 15.0));
        let big = import_image(&png(2000, 100)).unwrap();
        assert!((big.size.w - MAX_PLACED_WIDTH).abs() < 0.01);
        assert!((big.size.h - 27.0).abs() < 0.01, "{:?}", big.size);
    }

    #[test]
    fn bmp_is_stored_as_png() {
        let img = image::RgbaImage::from_raw(3, 2, vec![10; 24]).unwrap();
        let mut bmp = Cursor::new(Vec::new());
        img.write_to(&mut bmp, image::ImageFormat::Bmp).unwrap();
        let el = import_image(bmp.get_ref()).unwrap();
        assert_eq!(el.format, ImageFormat::Png);
        assert_eq!(ImageFormat::sniff(&el.data), ImageFormat::Png);
    }

    #[test]
    fn junk_is_refused() {
        assert!(import_image(&[]).is_err());
        assert!(import_image(b"hello world").is_err());
        // A PNG header claiming a huge size.
        let mut huge = png(1, 1);
        huge[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        huge[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        assert!(import_image(&huge).is_err());
        let mut truncated = png(8, 8);
        truncated.truncate(30);
        assert!(probe_image(&truncated).is_err() || import_image(&truncated).is_ok());
        assert!(encode_png(2, 2, vec![0; 3]).is_err());
    }
}
