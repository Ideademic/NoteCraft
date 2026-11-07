//! Export a page or section as HTML, Markdown or plain text, and import pictures.
//!
//! - [`page_to_html`] / [`section_to_html`]: semantic HTML that keeps runs' formatting, lists,
//!   To Do tags, headings, pictures (inline `data:` URIs) and printout pages; ink is inline SVG
//!   ([`ink_svg`]). Elements keep their page positions by default ([`HtmlOptions`]).
//! - [`page_to_markdown`]: what Markdown can keep (headings, emphasis, lists, task lists,
//!   links, pictures).
//! - [`import_image`]: decodes a picture file with the `image` crate (PNG, JPEG, GIF, BMP, WebP;
//!   WebP and BMP are re-encoded as PNG), bounded in bytes and pixels, sized at 96 dpi and capped
//!   to a sensible size on the page.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod html;
mod markdown;
mod picture;
mod svg;

use notecraft_doc::Page;

pub use html::{HtmlOptions, MAX_INLINE_FILE, esc as escape_html, page_to_html, page_to_html_with, section_to_html, section_to_html_with};
pub use markdown::page_to_markdown;
pub use picture::{
    ImageInfo, MAX_IMAGE_BYTES, MAX_IMAGE_PIXELS, MAX_IMAGE_SIDE, MAX_PLACED_HEIGHT, MAX_PLACED_WIDTH, encode_png, import_image, placed_size,
    probe_image,
};
pub use svg::{ink_bounds, ink_svg};

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

/// One page as plain text: the title, then each outline's paragraphs, one per line.
pub fn page_to_text(page: &Page) -> Result<String> {
    Ok(page.plain_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_export() {
        let p = Page::new("Hello");
        assert_eq!(page_to_text(&p).unwrap(), "Hello");
        assert!(import_image(&[]).is_err());
        assert!(page_to_html(&p).unwrap().contains("<h1 class=\"page-title\">Hello</h1>"));
        assert!(page_to_markdown(&p).unwrap().starts_with("# Hello"));
    }
}
