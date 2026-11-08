//! NoteCraft's notebook model.
//!
//! [`Notebook`] ▸ [`SectionEntry`] ([`Section`] or [`SectionGroup`]) ▸ [`Page`] ▸
//! [`PageElement`]s (outlines of rich text, pictures, printouts, embedded files, kept content)
//! plus page-level [`InkStroke`]s.
//!
//! Everything is plain data: `Clone` (binary payloads are `Arc`-shared, so undo snapshots are
//! cheap), `PartialEq`, `Debug` and serde (camelCase JSON; GUIDs and colours as strings, bytes as
//! base64). Positions are points from the page's top-left corner ([`notecraft_geom`]).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod b64;
pub mod color;
pub mod guid;
pub mod history;
pub mod ink;
pub mod link;
pub mod notebook;
pub mod page;

pub use color::Color;
pub use guid::Guid;
pub use history::{DeletedPage, DeletedSection, PageVersion, RecycleBin};
pub use ink::{InkPoint, InkStroke, InkTool};
pub use notebook::{EntryPath, Notebook, SearchHit, Section, SectionEntry, SectionGroup, SourceInfo};
pub use notecraft_geom as geom;
pub use notecraft_geom::{Point, Rect, Size};
pub use page::{
    Align, Crop, ElementKind, ImageElement, ImageFormat, ListKind, ListStyle, Outline, Page, PageBackground, PageElement, ParaStyle, Paragraph,
    PictureTransform, Printout, RuleLines, Run, Tag,
};

use chrono::{DateTime, Utc};

/// Model errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DocError {
    #[error("not a GUID: {0}")]
    BadGuid(String),
    #[error("not found: {0}")]
    NotFound(String),
}

/// The current time. Uses `web-time`, so it works in the browser too (`SystemTime::now()`
/// panics on wasm32-unknown-unknown); a clock before 1970 reads as the epoch.
///
/// Rounded down to whole 100 ns ticks, the resolution of the FILETIME stamps `.one` files store,
/// so a page's times survive a save and reopen unchanged on every platform (Linux clocks have
/// nanosecond resolution).
pub fn now() -> DateTime<Utc> {
    let since = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).unwrap_or_default();
    let secs = i64::try_from(since.as_secs()).unwrap_or(0);
    let nanos = since.subsec_nanos() / 100 * 100;
    DateTime::from_timestamp(secs, nanos).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_is_after_2020() {
        assert!(super::now().timestamp() > 1_577_836_800);
    }

    #[test]
    fn now_fits_filetime_ticks() {
        assert_eq!(super::now().timestamp_subsec_nanos() % 100, 0);
    }
}
