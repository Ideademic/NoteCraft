//! NoteCraft's egui front end: a OneNote 2016/2019-style window.
//!
//! Layout (top to bottom): the accent-coloured title bar with the quick access toolbar
//! ([`chrome`]), the ribbon ([`ribbon`]: File opens the [`backstage`]; Home, Insert, Draw, View),
//! the body, the status bar. The body is the notebook pane on the left, the coloured section tabs
//! with the search box across the top, the page list on the right ([`panes`]) and the page
//! [`canvas`] in the middle ([`app::frame`]).
//!
//! The UI is thin: it reads [`notecraft_engine::Session`] to draw and changes it only through
//! [`NoteApp::run`] (commands). Colours come from [`theme::Tokens`]; icons are drawn in code
//! ([`icons`]); platform services (file dialogs, clipboard, downloads) come from [`Services`].
//! Strings go through [`tl!`].
//!
//! Ownership for parallel work: the shell (theme, app, chrome, ribbon*, backstage, panes, icons,
//! widgets, keytips) and the canvas (canvas*) meet only at [`canvas::CanvasState`] and
//! [`canvas::show`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod app;
pub mod backstage;
pub mod canvas;
pub mod chrome;
pub mod i18n;
pub mod icons;
pub mod keytips;
pub mod panes;
pub mod ribbon;
pub mod ribbon_fit;
pub mod services;
pub mod theme;
pub mod updates;
pub mod widgets;

pub use app::{NoteApp, Prefs, UiState};
pub use canvas::{CanvasState, Tool};
pub use services::{ClipboardImage, FolderFor, Inbox, InboxFile, OpenPurpose, PathRequest, Services};
pub use theme::{APP_COLOR, Appearance, Tokens};
