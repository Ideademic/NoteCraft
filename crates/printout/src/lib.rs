//! File printouts: a PDF's pages rendered to PNG pictures (craft-pdf's `raster` feature) so they
//! can be placed on a page as a [`Printout`] and inked over.
//!
//! Scope (phase 2, engine agent): open with `craft_pdf::open_slice` (input size checked),
//! render each page with `craft_pdf::raster` at `dpi`, encode PNG with `image`; at most
//! [`MAX_PAGES`] pages and [`MAX_PAGE_PIXELS`] pixels per page; encrypted or broken files return
//! an error, never panic.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;

use notecraft_doc::{Point, Printout};

/// Most pages rendered from one file.
pub const MAX_PAGES: usize = 200;
/// Largest rendered page, pixels (width × height).
pub const MAX_PAGE_PIXELS: u64 = 40_000_000;
/// The resolution printouts are rendered at by default.
pub const DEFAULT_DPI: f32 = 150.0;
/// Vertical gap between stacked printout pages, points.
pub const PAGE_GAP: f32 = 12.0;

/// Errors rendering a printout.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("not a PDF NoteCraft can open: {0}")]
    BadPdf(String),
    #[error("too large: {0}")]
    Limit(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One rendered page.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedPage {
    pub width_px: u32,
    pub height_px: u32,
    /// The page as a PNG file.
    pub png: Vec<u8>,
}

/// Renders every page of a PDF (up to [`MAX_PAGES`]) at `dpi`.
pub fn render_pdf(pdf: &[u8], dpi: f32) -> Result<Vec<RenderedPage>> {
    let _ = dpi;
    if !pdf.starts_with(b"%PDF") {
        return Err(Error::BadPdf("no PDF header".into()));
    }
    Err(Error::NotImplemented("notecraft_printout::render_pdf"))
}

/// A printout of `file_name` with its first page's top-left corner at `pos`: the pages rendered
/// at `dpi`, sized in points (page size, not pixels), stacked down with [`PAGE_GAP`].
pub fn build_printout(file_name: &str, bytes: Arc<Vec<u8>>, pos: Point, dpi: f32) -> Result<Printout> {
    let _pages = render_pdf(&bytes, dpi)?;
    let _ = (file_name, pos);
    Err(Error::NotImplemented("notecraft_printout::build_printout"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_pdf_is_refused() {
        assert!(matches!(super::render_pdf(b"hello", 96.0), Err(super::Error::BadPdf(_))));
    }
}
