//! Platform services the host app injects (desktop: craft-dialogs file dialogs, arboard, the file
//! system; web: the browser's file picker, downloads, storage, a hosting page). Every hook is
//! optional: without one the feature is hidden or disabled. Programmatic commands never open
//! dialogs; only the UI calls these.
//!
//! File dialogs never block: the UI asks with [`Services::ask_path`] and the host answers on a
//! later frame through [`crate::app::files::answer_path`] (a blocking dialog runs AppKit's modal
//! loop inside winit's event handler on macOS, and an event arriving meanwhile aborts the app).

use std::path::Path;
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
    /// The font box's Add Fonts (web): TTF, OTF, TTC, WOFF, WOFF2.
    Font,
    /// File › Import a `.onepkg` or `.one`.
    Import,
}

/// Which folder a folder dialog chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FolderFor {
    /// File › Open › Open Notebook Folder.
    OpenNotebook,
    /// File › New: where the new notebook goes.
    NewNotebook,
    /// File › Options: where new notebooks go by default.
    DefaultLocation,
}

/// A file dialog the UI asks the host for, and what to do with the answer.
#[derive(Clone, Debug, PartialEq)]
pub enum PathRequest {
    /// Choose a file to open, import or insert.
    Open(OpenPurpose),
    /// Choose a folder.
    Folder(FolderFor),
    /// Choose where to save `name`, then run command `id` with `params` and the chosen `path`.
    Save { name: String, id: String, params: serde_json::Value },
}

impl PathRequest {
    /// The file type a dialog for this request offers: (name, extensions); `None` for any file.
    pub fn filter(&self) -> Option<(&'static str, Vec<String>)> {
        let fixed = |name: &'static str, exts: &[&str]| Some((name, exts.iter().map(|e| (*e).to_owned()).collect()));
        match self {
            PathRequest::Open(OpenPurpose::Notebook | OpenPurpose::NotebookFolder) => fixed("OneNote notebooks", &["onetoc2", "onepkg", "one"]),
            PathRequest::Open(OpenPurpose::Import) => fixed("OneNote packages and sections", &["onepkg", "one"]),
            PathRequest::Open(OpenPurpose::Picture) => fixed("Pictures", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]),
            PathRequest::Open(OpenPurpose::Printout) => fixed("PDF documents and pictures", &["pdf", "png", "jpg", "jpeg"]),
            PathRequest::Open(OpenPurpose::Font) => fixed("Fonts", &["ttf", "otf", "ttc", "woff", "woff2"]),
            PathRequest::Open(OpenPurpose::Attachment) | PathRequest::Folder(_) => None,
            PathRequest::Save { name, .. } => {
                let ext = name.rsplit_once('.').map(|(_, e)| e).filter(|e| !e.is_empty() && e.len() <= 10)?;
                Some(("File", vec![ext.to_owned()]))
            }
        }
    }
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
    /// Desktop: show a native file, folder or save dialog without blocking; the host hands the
    /// chosen path to [`crate::app::files::answer_path`] on a later frame (nothing on Cancel).
    pub ask_path: Option<Box<dyn Fn(PathRequest)>>,
    /// Web: open the browser's file picker; the file arrives later in `inbox`.
    pub open_async: Option<Box<dyn Fn(OpenPurpose)>>,
    /// Web: files delivered asynchronously (picker, drag and drop).
    pub inbox: Option<Inbox>,
    /// Web: hand bytes to the browser as a download (file name, bytes), or to the page hosting
    /// NoteCraft.
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
    /// The HTML on the system clipboard with the text being pasted, if any (pasting from a
    /// browser or a word processor keeps basic formatting). Desktop: read now; web: what the
    /// last paste event carried.
    pub clipboard_html: Option<Box<dyn Fn() -> Option<String>>>,
    /// Put HTML (a whole document) with its plain-text version on the system clipboard (Copy
    /// and Cut); `true` on success. Without it only the text goes, through egui.
    pub set_clipboard_rich: Option<Box<dyn Fn(&str, &str) -> bool>>,
    /// Put a picture on the system clipboard (copying a picture); `true` on success.
    pub set_clipboard_image: Option<Box<dyn Fn(&ClipboardImage) -> bool>>,
    /// Web: told whether anything is unsaved after each frame (the leave-page guard, a hosting
    /// page).
    pub on_dirty: Option<Box<dyn Fn(bool)>>,
    /// Web: keep a font file the user added (name, bytes) for the next visit.
    pub keep_font: Option<Box<dyn Fn(&str, &[u8])>>,
    /// Whether the system prefers a dark theme, if it says.
    pub prefers_dark: Option<Box<dyn Fn() -> Option<bool>>>,
    /// Printing: the host's printers and spooler, or the browser's print dialog.
    pub print: Option<Printing>,
}

/// How the host prints File › Print's PDF.
pub enum Printing {
    /// Desktop: find printers and send jobs to the spooler (craft-print), off the UI thread.
    Spooler { discover: craft_ui::printing::Discover, submit: craft_ui::printing::Submit },
    /// Web: hand the PDF to the browser's print dialog (the printer, copies and range are chosen
    /// there).
    Browser(Box<dyn Fn(&[u8]) -> Result<(), String>>),
}

impl Services {
    /// Whether the host has file and folder dialogs (the desktop app).
    pub fn has_dialogs(&self) -> bool {
        self.ask_path.is_some()
    }
}
