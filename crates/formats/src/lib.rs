//! Export a page or section as HTML, Markdown or plain text, and import pictures.
//!
//! Scope (phase 2, engine agent): HTML keeps runs' formatting, lists, To Do tags, headings,
//! pictures (inline `data:` URIs) and printout pages; ink is drawn as inline SVG. Markdown keeps
//! what Markdown can (headings, emphasis, lists, task lists, links). Picture import decodes the
//! file with the `image` crate (PNG, JPEG, GIF, BMP, WebP; WebP and BMP are re-encoded as PNG),
//! bounded in pixels, and sizes it at 96 dpi, capped to a sensible width on the page.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use notecraft_doc::{ImageElement, Page, Section};

/// Largest picture decoded, in pixels (width × height).
pub const MAX_IMAGE_PIXELS: u64 = 100_000_000;

/// Errors exporting or importing.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error("not a picture NoteCraft can read: {0}")]
    BadImage(String),
    #[error("picture too large: {0}")]
    Limit(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One page as a complete HTML document.
pub fn page_to_html(page: &Page) -> Result<String> {
    let _ = page;
    Err(Error::NotImplemented("notecraft_formats::page_to_html"))
}

/// A section as one HTML document: each page in order, titled, separated.
pub fn section_to_html(section: &Section) -> Result<String> {
    let _ = section;
    Err(Error::NotImplemented("notecraft_formats::section_to_html"))
}

/// One page as Markdown (title as `#` heading).
pub fn page_to_markdown(page: &Page) -> Result<String> {
    let _ = page;
    Err(Error::NotImplemented("notecraft_formats::page_to_markdown"))
}

/// One page as plain text: the title, then each outline's paragraphs, one per line.
pub fn page_to_text(page: &Page) -> Result<String> {
    Ok(page.plain_text())
}

/// A picture file as an image element at the page origin, sized at 96 dpi.
pub fn import_image(bytes: &[u8]) -> Result<ImageElement> {
    if bytes.is_empty() {
        return Err(Error::BadImage("empty file".into()));
    }
    Err(Error::NotImplemented("notecraft_formats::import_image"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_export_works_and_stubs_error() {
        let p = Page::new("Hello");
        assert_eq!(page_to_text(&p).unwrap(), "Hello");
        assert!(import_image(&[]).is_err());
        assert!(page_to_html(&p).is_err());
    }
}
