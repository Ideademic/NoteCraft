//! Files in and out: opening notebooks, importing, exporting, creating a notebook, inserting
//! pictures, printouts and attachments. Desktop goes through the host's dialogs and a path;
//! the web through the browser's picker (files arrive in the inbox) and downloads.

use base64::Engine as _;
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::NoteApp;
use crate::services::OpenPurpose;
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
    export_with(app, name, id, params);
}

/// Runs export command `id` with `params`, saving its file as `name`: where the save dialog
/// says (desktop, as `path`) or as a download (web). File › Export and Print › Save as PDF.
pub fn export_with(app: &mut NoteApp, name: String, id: &str, mut params: Value) {
    if let Some(pick) = app.services.pick_save.as_ref() {
        let Some(path) = pick(&name) else { return };
        if let Some(m) = params.as_object_mut() {
            m.insert("path".into(), json!(path.to_string_lossy()));
        }
        if app.run(id, params).is_ok() {
            app.set_status(crate::i18n::fmt_plain(tl!("Exported to {path}"), &[("path", &path.to_string_lossy())]), false);
        }
        return;
    }
    // No save dialog (web): the command hands back the bytes.
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

/// Asks for a file (or folder) to open or import, then opens it.
pub fn pick_and_open(app: &mut NoteApp, purpose: OpenPurpose) {
    if purpose == OpenPurpose::NotebookFolder {
        if let Some(p) = app.services.pick_folder.as_ref().and_then(|f| f()) {
            open_path(app, &p.to_string_lossy());
        }
        return;
    }
    if let Some(pick) = app.services.pick_open.as_ref() {
        let Some(p) = pick(purpose) else { return };
        let path = p.to_string_lossy().into_owned();
        if purpose == OpenPurpose::Import {
            let lower = path.to_ascii_lowercase();
            let id = if lower.ends_with(".onepkg") { ids::FILE_IMPORT_ONEPKG } else { ids::FILE_IMPORT_ONE };
            if app.run(id, json!({ "path": path })).is_ok() {
                app.ui.backstage = false;
            }
        } else {
            open_path(app, &path);
        }
        return;
    }
    match app.services.open_async.as_ref() {
        Some(open) => open(purpose),
        None => app.set_status(tl!("Opening files isn't available here."), true),
    }
}

/// Insert › Pictures / File Printout / File Attachment: asks for a file and inserts it.
pub fn pick_and_insert(app: &mut NoteApp, purpose: OpenPurpose) {
    if let Some(pick) = app.services.pick_open.as_ref() {
        let Some(p) = pick(purpose) else { return };
        let Some(id) = insert_command(purpose) else { return };
        let params = crate::widgets::selection_params(app, json!({ "path": p.to_string_lossy() }));
        app.act(id, params);
        return;
    }
    match app.services.open_async.as_ref() {
        Some(open) => open(purpose),
        None => app.set_status(tl!("Inserting files isn't available here."), true),
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
