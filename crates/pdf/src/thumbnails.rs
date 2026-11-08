//! Small pictures of a PDF's sheets (the Print page's preview), rendered by craft-pdf.

use craft_pdf::interpret::InterpreterSettings;
use craft_pdf::raster::{self, Options, RenderCache};

/// One sheet as straight RGBA8 pixels on white.
#[derive(Clone, Debug, PartialEq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The first `max_pages` sheets of `pdf`, each fitted into `max_side` pixels (at most 2048).
/// Sheets that fail to render are left out; an unreadable file gives none.
pub fn thumbnails(pdf: &[u8], max_side: u32, max_pages: usize) -> Vec<Thumbnail> {
    let Ok(doc) = craft_pdf::open_slice(pdf, None) else { return Vec::new() };
    let side = max_side.clamp(16, 2048) as f32;
    let settings = InterpreterSettings::default();
    let mut out = Vec::new();
    for page in doc.pages().iter().take(max_pages.min(500)) {
        let (w, h) = page.render_dimensions();
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            continue;
        }
        let s = (side / w).min(side / h);
        let options = Options { scale: [s, s], background: [255, 255, 255, 255], ..Options::default() };
        match raster::render(page, &RenderCache::new(), &settings, &options) {
            // An opaque background: premultiplied and straight alpha agree.
            Ok(r) => out.push(Thumbnail { width: r.width, height: r.height, rgba: r.bytes().to_vec() }),
            Err(e) => log::warn!("pdf preview: a sheet failed to render: {e}"),
        }
    }
    out
}

/// How many sheets `pdf` has (0 when it can't be read).
pub fn sheet_count(pdf: &[u8]) -> usize {
    craft_pdf::open_slice(pdf, None).map_or(0, |d| d.pages().len())
}
