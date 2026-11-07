//! NoteCraft's egui front end: a OneNote 2016/2019-style window.
//!
//! Layout (top to bottom): title bar with the quick access toolbar ([`chrome`]), the ribbon
//! ([`ribbon`]: File opens the [`backstage`]; Home, Insert, Draw, View), the body, the status
//! bar. The body is the notebook pane on the left, the section tabs across the top, the page
//! list on the right ([`panes`]) and the page [`canvas`] in the middle.
//!
//! The UI is thin: it reads [`notecraft_engine::Session`] to draw and changes it only through
//! [`NoteApp::run`] (commands). Colours come from [`theme::Tokens`]; platform services (file
//! dialogs, clipboard, downloads) from [`Services`]. Strings go through [`tl!`].
//!
//! Ownership for parallel work: the shell (theme, app, chrome, ribbon*, backstage, panes, icons,
//! widgets) and the canvas (canvas*) meet only at [`canvas::CanvasState`] and [`canvas::show`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod app;
pub mod backstage;
pub mod canvas;
pub mod chrome;
pub mod i18n;
pub mod icons;
pub mod panes;
pub mod ribbon;
pub mod services;
pub mod theme;
pub mod widgets;

pub use app::{NoteApp, UiState};
pub use canvas::{CanvasState, Tool};
pub use services::{ClipboardImage, Inbox, InboxFile, OpenPurpose, Services};
pub use theme::{APP_COLOR, Tokens};
