//! Desktop [`Services`]: native file dialogs (rfd), the clipboard (arboard), the file system,
//! and where NoteCraft keeps its settings and notebooks.

use std::path::PathBuf;

use notecraft_ui_egui::{ClipboardImage, OpenPurpose, Services};

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// The user's documents folder (`~/Documents`; Windows: the profile's Documents).
pub fn documents_dir() -> Option<PathBuf> {
    home().map(|h| h.join("Documents"))
}

/// Where new notebooks are saved by default: `Documents/NoteCraft Notebooks`.
pub fn notebooks_dir() -> Option<PathBuf> {
    documents_dir().map(|d| notecraft_engine::io::default_notebook_root(&d))
}

/// NoteCraft's settings folder: macOS `~/Library/Application Support/NoteCraft`, Windows
/// `%APPDATA%\NoteCraft`, elsewhere `$XDG_CONFIG_HOME/notecraft` (or `~/.config/notecraft`).
pub fn settings_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return home().map(|h| h.join("Library").join("Application Support").join("NoteCraft"));
    }
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("NoteCraft")).or_else(|| home().map(|h| h.join("NoteCraft")));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|h| h.join(".config")))
        .map(|c| c.join("notecraft"))
}

fn filters(purpose: OpenPurpose) -> (&'static str, &'static [&'static str]) {
    match purpose {
        OpenPurpose::Notebook | OpenPurpose::NotebookFolder => ("OneNote notebooks", &["onetoc2", "onepkg", "one"]),
        OpenPurpose::Import => ("OneNote packages and sections", &["onepkg", "one"]),
        OpenPurpose::Picture => ("Pictures", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]),
        OpenPurpose::Printout => ("PDF documents and pictures", &["pdf", "png", "jpg", "jpeg"]),
        OpenPurpose::Attachment => ("All files", &["*"]),
    }
}

pub fn desktop() -> Services {
    Services {
        pick_open: Some(Box::new(|purpose| {
            let (name, exts) = filters(purpose);
            let dialog = rfd::FileDialog::new();
            let dialog = if exts == ["*"] { dialog } else { dialog.add_filter(name, exts) };
            dialog.pick_file()
        })),
        pick_save: Some(Box::new(|suggested| {
            let ext = suggested.rsplit_once('.').map(|(_, e)| e.to_owned());
            let dialog = rfd::FileDialog::new().set_file_name(suggested);
            let dialog = match &ext {
                Some(e) => dialog.add_filter(e.to_uppercase(), &[e.as_str()]),
                None => dialog,
            };
            dialog.save_file()
        })),
        pick_folder: Some(Box::new(|| rfd::FileDialog::new().pick_folder())),
        read_file: Some(Box::new(|p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())))),
        write_file: Some(Box::new(|p, b| std::fs::write(p, b).map_err(|e| format!("{}: {e}", p.display())))),
        clipboard_text: Some(Box::new(|| arboard::Clipboard::new().ok()?.get_text().ok().filter(|t| !t.is_empty()))),
        set_clipboard_text: Some(Box::new(|t| arboard::Clipboard::new().and_then(|mut c| c.set_text(t.to_owned())).is_ok())),
        clipboard_image: Some(Box::new(|| {
            let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
            Some(ClipboardImage { width: img.width, height: img.height, rgba: img.bytes.into_owned() })
        })),
        ..Services::default()
    }
}
