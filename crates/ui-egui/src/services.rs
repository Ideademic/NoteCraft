//! Platform services the host app injects (desktop: rfd dialogs, arboard, the file system; web:
//! the browser's file picker, downloads, storage). Every hook is optional: without one the
//! feature is hidden or disabled. Programmatic commands never open dialogs; only the UI calls
//! these.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What a file is being picked for (picks the dialog's filters and the inbox route).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpenPurpose {
    /// A notebook: a folder, `Open Notebook.onetoc2`, `.onepkg` or a `.one` section.
    Notebook,
    /// A notebook folder (desktop).
    NotebookFolder,
    /// Insert › Pictures.
    Picture,
    /// Insert › File Printout (PDF).
    Printout,
    /// Insert › File Attachment.
    Attachment,
    /// File › Import a `.onepkg` or `.one`.
    Import,
}

/// A file the host delivered asynchronously (web picker, drag and drop).
#[derive(Clone, Debug, PartialEq)]
pub struct InboxFile {
    pub purpose: OpenPurpose,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Files delivered asynchronously; the app drains it every frame.
pub type Inbox = Arc<Mutex<Vec<InboxFile>>>;

/// A picture on the system clipboard, as RGBA8 pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Platform hooks.
#[derive(Default)]
pub struct Services {
    /// Desktop: a native open dialog (blocking is acceptable for v0.1); `None` when cancelled.
    pub pick_open: Option<Box<dyn Fn(OpenPurpose) -> Option<PathBuf>>>,
    /// Desktop: a native save dialog with a suggested file name; `None` when cancelled.
    pub pick_save: Option<Box<dyn Fn(&str) -> Option<PathBuf>>>,
    /// Desktop: choose a folder (new notebook location, open a notebook folder).
    pub pick_folder: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    /// Web: open the browser's file picker; the file arrives later in `inbox`.
    pub open_async: Option<Box<dyn Fn(OpenPurpose)>>,
    /// Web: files delivered asynchronously (picker, drag and drop).
    pub inbox: Option<Inbox>,
    /// Web: hand bytes to the browser as a download (file name, bytes).
    pub download: Option<Box<dyn Fn(&str, &[u8]) -> Result<(), String>>>,
    /// Desktop: read a file (the app never touches the file system without a host hook).
    pub read_file: Option<Box<dyn Fn(&Path) -> Result<Vec<u8>, String>>>,
    /// Desktop: write a file (exports).
    pub write_file: Option<Box<dyn Fn(&Path, &[u8]) -> Result<(), String>>>,
    /// Desktop: text on the system clipboard (egui's paste event covers Ctrl+V).
    pub clipboard_text: Option<Box<dyn Fn() -> Option<String>>>,
    /// Desktop: put text on the system clipboard; `true` on success.
    pub set_clipboard_text: Option<Box<dyn Fn(&str) -> bool>>,
    /// Desktop: the picture on the system clipboard, if any (pasting screenshots).
    pub clipboard_image: Option<Box<dyn Fn() -> Option<ClipboardImage>>>,
    /// Web: told whether anything is unsaved after each frame (the leave-page guard).
    pub on_dirty: Option<Box<dyn Fn(bool)>>,
    /// Whether the system prefers a dark theme, if it says.
    pub prefers_dark: Option<Box<dyn Fn() -> Option<bool>>>,
}
