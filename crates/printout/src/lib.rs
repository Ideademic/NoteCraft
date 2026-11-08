//! File printouts: a PDF's pages rendered to PNG pictures (craft-pdf's `raster` feature) so they
//! can be placed on a page as a [`Printout`] and inked over. A picture file becomes a one-page
//! printout.
//!
//! Bounded: the input size is checked by `craft_pdf::open_slice`; at most [`MAX_PAGES`] pages
//! are rendered, each at most [`MAX_PAGE_PIXELS`] pixels (the resolution drops for huge pages),
//! and rendering stops after [`MAX_TOTAL_PNG_BYTES`] of pictures. Encrypted or broken files
//! return an error, never panic; a single page that fails to render becomes a blank page.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;

use craft_pdf::interpret::InterpreterSettings;
use craft_pdf::raster::{self, Options, RenderCache};
use notecraft_doc::{ImageElement, ImageFormat, Point, Printout, Size};

/// Most pages rendered from one file.
pub const MAX_PAGES: usize = 200;
/// Largest rendered page, pixels (width × height).
pub const MAX_PAGE_PIXELS: u64 = 40_000_000;
/// Longest rendered page side, pixels (craft-pdf's own bound).
pub const MAX_PAGE_SIDE: u32 = raster::MAX_SIDE;
/// Rendering stops once the pictures add up to this many bytes.
pub const MAX_TOTAL_PNG_BYTES: usize = 256 * 1024 * 1024;
/// The resolution printouts are rendered at by default.
pub const DEFAULT_DPI: f32 = 150.0;
/// Lowest and highest resolution accepted.
pub const DPI_RANGE: (f32, f32) = (24.0, 600.0);
/// Vertical gap between stacked printout pages, points.
pub const PAGE_GAP: f32 = 12.0;
/// Largest page placed, points (a PDF page can be up to 200 in on a side): bigger pages are
/// shown scaled down to this width.
pub const MAX_PLACED_PAGE_WIDTH: f32 = 1_440.0;

/// Errors rendering a printout.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("not a PDF NoteCraft can open: {0}")]
    BadPdf(String),
    #[error("too large: {0}")]
    Limit(String),
    #[error("NoteCraft can print PDF files and pictures, not {0}")]
    Unsupported(String),
    #[error(transparent)]
    Picture(#[from] notecraft_formats::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One rendered page.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedPage {
    pub width_px: u32,
    pub height_px: u32,
    /// The page's size in PDF points (1/72 in), rotation applied.
    pub width_pt: f32,
    pub height_pt: f32,
    /// The page as a PNG file.
    pub png: Vec<u8>,
}

/// Whether `bytes` look like a PDF: `%PDF-` within the first kilobyte (some files have junk
/// before it).
pub fn is_pdf(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(1024)).unwrap_or(bytes);
    head.windows(5).any(|w| w == b"%PDF-")
}

fn clamp_dpi(dpi: f32) -> f32 {
    if dpi.is_finite() { dpi.clamp(DPI_RANGE.0, DPI_RANGE.1) } else { DEFAULT_DPI }
}

/// The scale (pixels per point) for a `w` × `h` point page at `dpi`, lowered to keep within
/// [`MAX_PAGE_PIXELS`] and [`MAX_PAGE_SIDE`].
fn page_scale(w: f32, h: f32, dpi: f32) -> f32 {
    let mut s = clamp_dpi(dpi) / 72.0;
    let side = MAX_PAGE_SIDE as f32 - 1.0;
    s = s.min(side / w).min(side / h);
    let px = f64::from(w * s) * f64::from(h * s);
    if px > MAX_PAGE_PIXELS as f64 {
        s *= ((MAX_PAGE_PIXELS as f64 / px).sqrt() * 0.999) as f32;
    }
    s
}

/// Renders every page of a PDF (up to [`MAX_PAGES`]) at `dpi` on a white background.
pub fn render_pdf(pdf: &[u8], dpi: f32) -> Result<Vec<RenderedPage>> {
    if !is_pdf(pdf) {
        return Err(Error::BadPdf("no PDF header".into()));
    }
    let doc = craft_pdf::open_slice(pdf, None).map_err(open_error)?;
    let pages = doc.pages();
    if pages.is_empty() {
        return Err(Error::BadPdf("the file has no pages".into()));
    }
    if pages.len() > MAX_PAGES {
        log::warn!("printout: rendering the first {MAX_PAGES} of {} pages", pages.len());
    }
    let settings = InterpreterSettings::default();
    let mut out = Vec::new();
    let mut total = 0usize;
    for page in pages.iter().take(MAX_PAGES) {
        let (w, h) = page.render_dimensions();
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            out.push(blank_page(612.0, 792.0)?);
            continue;
        }
        let s = page_scale(w, h, dpi);
        let options = Options { scale: [s, s], background: [255, 255, 255, 255], ..Options::default() };
        // A fresh cache per page bounds what one file can hold on to.
        let rendered = match raster::render(page, &RenderCache::new(), &settings, &options) {
            Ok(r) => {
                // An opaque white background: premultiplied and straight alpha agree.
                let png = notecraft_formats::encode_png(r.width, r.height, r.bytes().to_vec())?;
                RenderedPage { width_px: r.width, height_px: r.height, width_pt: w, height_pt: h, png }
            }
            Err(e) => {
                log::warn!("printout: a page failed to render: {e}");
                blank_page(w, h)?
            }
        };
        total = total.saturating_add(rendered.png.len());
        out.push(rendered);
        if total > MAX_TOTAL_PNG_BYTES {
            log::warn!("printout: stopped after {} pages ({total} bytes of pictures)", out.len());
            break;
        }
    }
    Ok(out)
}

fn open_error(e: craft_pdf::OpenError) -> Error {
    match e {
        craft_pdf::OpenError::InputLimit { limit } => Error::Limit(format!("the file is larger than {} MB", limit / 1_048_576)),
        craft_pdf::OpenError::Load(craft_pdf::syntax::LoadPdfError::Decryption(_)) => Error::BadPdf("the file is password-protected".into()),
        other => Error::BadPdf(other.to_string()),
    }
}

/// A white stand-in for a page that couldn't be rendered.
fn blank_page(w: f32, h: f32) -> Result<RenderedPage> {
    let png = notecraft_formats::encode_png(1, 1, vec![255; 4])?;
    Ok(RenderedPage { width_px: 1, height_px: 1, width_pt: w.clamp(1.0, 14_400.0), height_pt: h.clamp(1.0, 14_400.0), png })
}

/// A printout of `file_name` with its first page's top-left corner at `pos`: the pages rendered
/// at `dpi`, sized in points (the PDF page size, scaled down past [`MAX_PLACED_PAGE_WIDTH`]),
/// stacked down with [`PAGE_GAP`].
pub fn build_printout(file_name: &str, bytes: Arc<Vec<u8>>, pos: Point, dpi: f32) -> Result<Printout> {
    let rendered = render_pdf(&bytes, dpi)?;
    let pos = pos.sanitized();
    let mut y = pos.y;
    let mut pages = Vec::with_capacity(rendered.len());
    for (i, r) in rendered.into_iter().enumerate() {
        let size = Size::new(r.width_pt, r.height_pt).fit_within(Size::new(MAX_PLACED_PAGE_WIDTH, f32::MAX)).sanitized();
        pages.push(ImageElement {
            pos: Point::new(pos.x, y).sanitized(),
            size,
            data: Arc::new(r.png),
            format: ImageFormat::Png,
            alt: format!("{}, page {}", file_name, i + 1),
            transform: Default::default(),
        });
        y += size.h + PAGE_GAP;
    }
    Ok(Printout { file_name: file_name.to_owned(), file_bytes: bytes, pos, pages })
}

/// A picture file as a one-page printout at `pos` (the picture placed at its import size).
pub fn printout_from_image(file_name: &str, bytes: Arc<Vec<u8>>, pos: Point) -> Result<Printout> {
    let mut img = notecraft_formats::import_image(&bytes)?;
    let pos = pos.sanitized();
    img.pos = pos;
    img.alt = file_name.to_owned();
    Ok(Printout { file_name: file_name.to_owned(), file_bytes: bytes, pos, pages: vec![img] })
}

/// A printout of any file NoteCraft can print: a PDF ([`build_printout`]) or a picture
/// ([`printout_from_image`]).
pub fn printout_for_file(file_name: &str, bytes: Arc<Vec<u8>>, pos: Point, dpi: f32) -> Result<Printout> {
    if is_pdf(&bytes) {
        return build_printout(file_name, bytes, pos, dpi);
    }
    if notecraft_formats::probe_image(&bytes).is_ok() {
        return printout_from_image(file_name, bytes, pos);
    }
    let ext = file_name.rsplit_once('.').map(|(_, e)| format!(".{}", e.chars().take(10).collect::<String>())).unwrap_or_else(|| "this file".into());
    Err(Error::Unsupported(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small PDF with one 200 × 100 pt page per content stream.
    pub(crate) fn tiny_pdf(contents: &[&str]) -> Vec<u8> {
        let n = contents.len();
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
        let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" "))];
        for (i, c) in contents.iter().enumerate() {
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents {} 0 R >>", 4 + i * 2));
            objects.push(format!("<< /Length {} >>\nstream\n{c}\nendstream", c.len()));
        }
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut offsets = vec![];
        for (i, body) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let start = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
        for o in &offsets {
            bytes.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", offsets.len() + 1).as_bytes());
        bytes
    }

    #[test]
    fn non_pdf_is_refused() {
        assert!(matches!(render_pdf(b"hello", 96.0), Err(Error::BadPdf(_))));
        assert!(matches!(render_pdf(b"%PDF-1.7\ngarbage", 96.0), Err(Error::BadPdf(_))));
        assert!(matches!(printout_for_file("a.docx", Arc::new(b"PK".to_vec()), Point::ZERO, 96.0), Err(Error::Unsupported(e)) if e == ".docx"));
    }

    #[test]
    fn renders_and_stacks_pages() {
        let pdf = tiny_pdf(&["1 0 0 rg 0 0 100 100 re f", "0 0 1 rg 100 0 100 100 re f"]);
        let pages = render_pdf(&pdf, 72.0).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!((pages[0].width_px, pages[0].height_px), (200, 100));
        assert_eq!((pages[0].width_pt, pages[0].height_pt), (200.0, 100.0));
        let img = image::load_from_memory(&pages[0].png).unwrap().to_rgba8();
        // Left half red, right half white (bottom-left origin in the PDF; full height here).
        assert_eq!(img.get_pixel(10, 50).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(190, 50).0, [255, 255, 255, 255]);
        let p = build_printout("two.pdf", Arc::new(pdf), Point::new(36.0, 100.0), 150.0).unwrap();
        assert_eq!(p.pages.len(), 2);
        assert_eq!(p.pages[0].pos, Point::new(36.0, 100.0));
        assert_eq!(p.pages[1].pos, Point::new(36.0, 100.0 + 100.0 + PAGE_GAP));
        assert_eq!(p.pages[0].size, Size::new(200.0, 100.0));
        assert_eq!(p.pages[1].alt, "two.pdf, page 2");
        assert_eq!(p.file_name, "two.pdf");
    }

    #[test]
    fn huge_dpi_is_bounded() {
        let s = page_scale(14_400.0, 14_400.0, 600.0);
        let px = (14_400.0 * s) as u64 * (14_400.0 * s) as u64;
        assert!(px <= MAX_PAGE_PIXELS, "{px}");
        assert_eq!(clamp_dpi(f32::NAN), DEFAULT_DPI);
        assert_eq!(clamp_dpi(1e9), DPI_RANGE.1);
    }

    #[test]
    fn picture_becomes_one_page() {
        let png = notecraft_formats::encode_png(8, 4, vec![0; 128]).unwrap();
        let p = printout_for_file("photo.png", Arc::new(png), Point::new(5.0, 6.0), DEFAULT_DPI).unwrap();
        assert_eq!(p.pages.len(), 1);
        assert_eq!(p.pages[0].pos, Point::new(5.0, 6.0));
        assert_eq!(p.pages[0].size, Size::new(6.0, 3.0));
    }

    #[test]
    fn truncated_pdfs_never_panic() {
        let pdf = tiny_pdf(&["0 0 0 rg 0 0 10 10 re f"]);
        for cut in (0..pdf.len()).step_by(7) {
            let _ = render_pdf(&pdf[..cut], 72.0);
        }
    }
}
