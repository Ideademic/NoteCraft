//! Files in and out: opening notebooks, importing, exporting, creating a notebook, inserting
//! pictures, printouts and attachments. Desktop goes through the host's dialogs and a path;
//! the web through the browser's picker (files arrive in the inbox) and downloads.

use base64::Engine as _;
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::NoteApp;
use crate::services::{FolderFor, OpenPurpose, PathRequest};
use crate::tl;

/// What File › Export saves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportScope {
    Page,
    Section,
    Notebook,
}

/// The format File › Export writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Html,
    Markdown,
    Text,
    Onepkg,
    Pdf,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Html => "html",
            ExportFormat::Markdown => "md",
            ExportFormat::Text => "txt",
            ExportFormat::Onepkg => "onepkg",
            ExportFormat::Pdf => "pdf",
        }
    }
}

/// A file name made from a title: no path separators or characters file systems refuse.
pub fn safe_stem(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).take(120).collect();
    let s = s.trim().trim_matches('.').to_owned();
    if s.is_empty() { tl!("Untitled").to_owned() } else { s }
}

/// The file name File › Export suggests.
pub fn export_name(app: &NoteApp, scope: ExportScope, format: ExportFormat) -> String {
    let stem = match scope {
        ExportScope::Page => app.session.current_page().map(|p| p.display_title().to_owned()),
        ExportScope::Section => app.session.current_section().map(|s| s.name.clone()),
        ExportScope::Notebook => app.session.current_notebook().map(|n| n.name.clone()),
    };
    format!("{}.{}", safe_stem(stem.as_deref().unwrap_or("")), format.extension())
}

/// File › Export: asks where (desktop) and runs the export command; on the web the bytes go to
/// the browser as a download.
pub fn export(app: &mut NoteApp, scope: ExportScope, format: ExportFormat) {
    let name = export_name(app, scope, format);
    let (id, params) = match (scope, format) {
        (_, ExportFormat::Onepkg) => (ids::FILE_EXPORT_ONEPKG, json!({ "id": app.session.current.notebook })),
        // One sheet per page, its content's size: the page as on screen (File › Print offers paper sizes).
        (scope, ExportFormat::Pdf) => (
            ids::FILE_EXPORT_PDF,
            json!({ "scope": crate::backstage::print::scope_name(scope), "paper": "fit", "utcOffset": app.canvas.utc_offset_minutes }),
        ),
        (ExportScope::Page, ExportFormat::Html) => (ids::FILE_EXPORT_HTML, json!({ "scope": "page" })),
        (_, ExportFormat::Html) => (ids::FILE_EXPORT_HTML, json!({ "scope": "section" })),
        (ExportScope::Page, ExportFormat::Markdown) => (ids::FILE_EXPORT_MARKDOWN, json!({ "scope": "page" })),
        (_, ExportFormat::Markdown) => (ids::FILE_EXPORT_MARKDOWN, json!({ "scope": "section" })),
        (ExportScope::Page, ExportFormat::Text) => ("file.exportText", json!({ "scope": "page" })),
        (_, ExportFormat::Text) => ("file.exportText", json!({ "scope": "section" })),
    };
    save_as(app, name, id, params);
}

/// Saves what command `id` writes: on the desktop the save dialog asks where (the command then
/// runs with `params` plus `path`, see [`answer_path`]); on the web the command hands back the
/// bytes and they download (or go to the hosting page).
pub fn save_as(app: &mut NoteApp, name: String, id: &str, params: Value) {
    if let Some(ask) = app.services.ask_path.as_ref() {
        ask(PathRequest::Save { name, id: id.to_owned(), params });
        return;
    }
    let Ok(out) = app.run(id, params) else { return };
    let name = out.value.get("fileName").and_then(Value::as_str).map_or(name, str::to_owned);
    match bytes_of(&out.value) {
        Some(bytes) => match app.services.download.as_ref().map(|d| d(&name, &bytes)) {
            Some(Ok(())) => app.set_status(crate::i18n::fmt_plain(tl!("Downloaded {name}"), &[("name", &name)]), false),
            Some(Err(e)) => app.set_status(e, true),
            None => app.set_status(tl!("This version can't save files here."), true),
        },
        None => app.set_status(tl!("The export produced no file."), true),
    }
}

/// The bytes a command returned as `bytesBase64` (or `base64`).
fn bytes_of(v: &Value) -> Option<Vec<u8>> {
    let s = v.get("bytesBase64").or_else(|| v.get("base64")).and_then(Value::as_str)?;
    base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).ok()
}

/// File › New › Create Notebook: a new notebook named from the form, kept in the chosen folder.
pub fn create_notebook(app: &mut NoteApp) {
    let name = app.ui.new_notebook_name.trim().to_owned();
    let name = if name.is_empty() { tl!("My Notebook").to_owned() } else { name };
    let dir = app.ui.new_notebook_dir.clone().or_else(|| app.ui.prefs.notebooks_dir.clone());
    let mut params = json!({ "name": name });
    if let (Some(d), Some(m)) = (&dir, params.as_object_mut()) {
        m.insert("location".into(), json!(d));
    }
    if app.run(ids::NOTEBOOK_NEW, params).is_err() {
        return;
    }
    app.ui.new_notebook_name.clear();
    app.ui.new_notebook_dir = None;
    app.ui.backstage = false;
}

/// Opens a path (a notebook folder, `.onetoc2`, `.onepkg` or `.one`) and goes to it.
pub fn open_path(app: &mut NoteApp, path: &str) {
    if app.run(ids::NOTEBOOK_OPEN, json!({ "path": path })).is_ok() {
        app.ui.backstage = false;
    } else if !std::path::Path::new(path).exists() {
        // Gone from disk: drop it from the list.
        app.session.recent.remove(std::path::Path::new(path));
        app.ui.prefs.recent.retain(|r| r.path != path);
    }
}

/// Asks for a file (or folder) to open or import; it opens when the answer comes
/// ([`answer_path`], or the inbox on the web).
pub fn pick_and_open(app: &mut NoteApp, purpose: OpenPurpose) {
    if let Some(ask) = app.services.ask_path.as_ref() {
        ask(if purpose == OpenPurpose::NotebookFolder { PathRequest::Folder(FolderFor::OpenNotebook) } else { PathRequest::Open(purpose) });
        return;
    }
    match app.services.open_async.as_ref() {
        Some(open) => open(purpose),
        None => app.set_status(tl!("Opening files isn't available here."), true),
    }
}

/// Insert › Pictures / File Printout / File Attachment: asks for a file; it is inserted when the
/// answer comes.
pub fn pick_and_insert(app: &mut NoteApp, purpose: OpenPurpose) {
    if let Some(ask) = app.services.ask_path.as_ref() {
        ask(PathRequest::Open(purpose));
        return;
    }
    match app.services.open_async.as_ref() {
        Some(open) => open(purpose),
        None => app.set_status(tl!("Inserting files isn't available here."), true),
    }
}

/// Asks for a folder (desktop); it is used when the answer comes.
pub fn choose_folder(app: &mut NoteApp, purpose: FolderFor) {
    if let Some(ask) = app.services.ask_path.as_ref() {
        ask(PathRequest::Folder(purpose));
    }
}

/// The host's answer to a [`PathRequest`]: the chosen path, acted on as the request says.
pub fn answer_path(app: &mut NoteApp, request: PathRequest, path: &std::path::Path) {
    let text = path.to_string_lossy().into_owned();
    match request {
        PathRequest::Open(OpenPurpose::Import) => {
            let id = if text.to_ascii_lowercase().ends_with(".onepkg") { ids::FILE_IMPORT_ONEPKG } else { ids::FILE_IMPORT_ONE };
            if app.run(id, json!({ "path": text })).is_ok() {
                app.ui.backstage = false;
            }
        }
        PathRequest::Open(OpenPurpose::Notebook | OpenPurpose::NotebookFolder) | PathRequest::Folder(FolderFor::OpenNotebook) => {
            open_path(app, &text);
        }
        PathRequest::Open(purpose) => {
            let Some(id) = insert_command(purpose) else { return };
            let params = crate::widgets::selection_params(app, json!({ "path": text }));
            app.act(id, params);
        }
        PathRequest::Folder(FolderFor::NewNotebook) => app.ui.new_notebook_dir = Some(text),
        PathRequest::Folder(FolderFor::DefaultLocation) => app.ui.prefs.notebooks_dir = Some(text),
        PathRequest::Save { id, mut params, .. } => {
            if let Some(m) = params.as_object_mut() {
                m.insert("path".into(), json!(text));
            }
            if app.run(&id, params).is_ok() {
                app.set_status(crate::i18n::fmt_plain(tl!("Exported to {path}"), &[("path", &text)]), false);
            }
        }
    }
}

fn insert_command(purpose: OpenPurpose) -> Option<&'static str> {
    match purpose {
        OpenPurpose::Picture => Some(ids::INSERT_IMAGE),
        OpenPurpose::Printout => Some(ids::INSERT_PRINTOUT),
        OpenPurpose::Attachment => Some(ids::INSERT_FILE),
        _ => None,
    }
}

/// Inserts file bytes (a pasted picture, a file from the browser).
pub fn insert_bytes(app: &mut NoteApp, purpose: OpenPurpose, name: &str, bytes: &[u8]) {
    let Some(id) = insert_command(purpose) else { return };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let params = crate::widgets::selection_params(app, json!({ "name": name, "bytesBase64": b64 }));
    app.act(id, params);
}

/// Files the host delivered (web picker, drag and drop), each to where it was asked for.
pub fn drain_inbox(app: &mut NoteApp) {
    let files = match app.services.inbox.as_ref().and_then(|i| i.lock().ok().map(|mut v| std::mem::take(&mut *v))) {
        Some(f) if !f.is_empty() => f,
        _ => return,
    };
    for f in files {
        match f.purpose {
            OpenPurpose::Picture | OpenPurpose::Printout | OpenPurpose::Attachment => insert_bytes(app, f.purpose, &f.name, &f.bytes),
            OpenPurpose::Notebook | OpenPurpose::NotebookFolder | OpenPurpose::Import => {
                let lower = f.name.to_ascii_lowercase();
                let id = if lower.ends_with(".onepkg") { ids::FILE_IMPORT_ONEPKG } else { ids::FILE_IMPORT_ONE };
                let b64 = base64::engine::general_purpose::STANDARD.encode(&f.bytes);
                if app.run(id, json!({ "name": f.name, "bytesBase64": b64 })).is_ok() {
                    app.ui.backstage = false;
                }
            }
        }
    }
}

/// RGBA pixels (a clipboard picture) as a PNG file; `None` when the sizes don't add up.
pub fn png_from_rgba(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (u32::try_from(width).ok()?, u32::try_from(height).ok()?);
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 || rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(safe_stem("a/b:c?"), "a_b_c_");
        assert_eq!(safe_stem("  ..  "), "Untitled");
        assert_eq!(safe_stem(&"x".repeat(500)).len(), 120);
        let app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        assert_eq!(export_name(&app, ExportScope::Page, ExportFormat::Html), "Welcome to NoteCraft.html");
        assert_eq!(export_name(&app, ExportScope::Notebook, ExportFormat::Onepkg), "My Notebook.onepkg");
    }

    #[test]
    fn clipboard_pixels_become_png() {
        let png = png_from_rgba(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        assert_eq!(png.get(1..4), Some(&b"PNG"[..]));
        assert!(png_from_rgba(2, 2, &[0; 4]).is_none(), "wrong length");
        assert!(png_from_rgba(0, 0, &[]).is_none());
        assert!(png_from_rgba(usize::MAX, 2, &[]).is_none());
    }

    /// An app whose dialogs record what they were asked (the host's side of [`PathRequest`]).
    fn with_dialogs() -> (NoteApp, std::rc::Rc<std::cell::RefCell<Vec<PathRequest>>>) {
        let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = asked.clone();
        let services = crate::Services { ask_path: Some(Box::new(move |r| log.borrow_mut().push(r))), ..crate::Services::default() };
        (NoteApp::new(notecraft_engine::Session::new_default(), services), asked)
    }

    #[test]
    fn dialogs_ask_and_the_answer_comes_later() {
        let (mut app, asked) = with_dialogs();
        assert!(app.services.has_dialogs());
        pick_and_open(&mut app, OpenPurpose::NotebookFolder);
        pick_and_insert(&mut app, OpenPurpose::Picture);
        choose_folder(&mut app, FolderFor::DefaultLocation);
        export(&mut app, ExportScope::Page, ExportFormat::Markdown);
        let asked = std::mem::take(&mut *asked.borrow_mut());
        assert_eq!(asked.len(), 4, "{asked:?}");
        assert_eq!(asked[0], PathRequest::Folder(FolderFor::OpenNotebook));
        assert_eq!(asked[1], PathRequest::Open(OpenPurpose::Picture));
        assert_eq!(asked[1].filter().map(|(_, e)| e.contains(&"png".to_owned())), Some(true));
        assert!(matches!(&asked[3], PathRequest::Save { name, id, .. } if name == "Welcome to NoteCraft.md" && id == ids::FILE_EXPORT_MARKDOWN));
        assert_eq!(asked[3].filter().map(|(_, e)| e), Some(vec!["md".to_owned()]));
        // Nothing happened yet: the answers do it.
        assert_eq!(app.ui.prefs.notebooks_dir, None);
        let dir = std::env::temp_dir().join(format!("nc-dialogs-{}", std::process::id()));
        answer_path(&mut app, asked[2].clone(), &dir);
        assert_eq!(app.ui.prefs.notebooks_dir.as_deref(), Some(dir.to_string_lossy().as_ref()));
        answer_path(&mut app, PathRequest::Folder(FolderFor::NewNotebook), &dir);
        assert_eq!(app.ui.new_notebook_dir.as_deref(), Some(dir.to_string_lossy().as_ref()));
        let _ = std::fs::create_dir_all(&dir);
        let out = dir.join("page.md");
        answer_path(&mut app, asked[3].clone(), &out);
        assert!(std::fs::read_to_string(&out).is_ok_and(|t| t.contains("Welcome")), "the export ran with the chosen path");
        assert!(app.ui.status.as_deref().is_some_and(|s| s.starts_with("Exported to")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_dialogs_or_a_picker_nothing_is_asked() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        assert!(!app.services.has_dialogs());
        pick_and_open(&mut app, OpenPurpose::Notebook);
        assert!(app.ui.status_error);
        choose_folder(&mut app, FolderFor::NewNotebook);
        assert_eq!(app.ui.new_notebook_dir, None);
        assert_eq!(PathRequest::Save { name: "noext".into(), id: String::new(), params: json!({}) }.filter(), None);
        assert_eq!(PathRequest::Open(OpenPurpose::Attachment).filter(), None);
    }

    #[test]
    fn new_notebook_from_the_form() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        app.ui.new_notebook_name = "Work".into();
        app.ui.new_notebook_dir = Some("/tmp/nc-test".into());
        app.ui.backstage = true;
        create_notebook(&mut app);
        assert_eq!(app.session.current_notebook().map(|n| n.name.as_str()), Some("Work"));
        assert!(!app.ui.backstage);
    }
}
