//! Desktop [`Services`]: native file dialogs (rfd), the clipboard (arboard), the file system.

use std::path::PathBuf;

use notecraft_ui_egui::{ClipboardImage, OpenPurpose, Services};

/// Where new notebooks are saved: `Documents/NoteCraft Notebooks` in the user's home.
pub fn notebooks_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join("Documents").join("NoteCraft Notebooks"))
}

fn filters(purpose: OpenPurpose) -> (&'static str, &'static [&'static str]) {
    match purpose {
        OpenPurpose::Notebook | OpenPurpose::NotebookFolder => ("OneNote notebooks", &["onetoc2", "onepkg", "one"]),
        OpenPurpose::Import => ("OneNote packages and sections", &["onepkg", "one"]),
        OpenPurpose::Picture => ("Pictures", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]),
        OpenPurpose::Printout => ("PDF documents", &["pdf"]),
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
        pick_save: Some(Box::new(|suggested| rfd::FileDialog::new().set_file_name(suggested).save_file())),
        pick_folder: Some(Box::new(|| rfd::FileDialog::new().pick_folder())),
        read_file: Some(Box::new(|p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())))),
        write_file: Some(Box::new(|p, b| std::fs::write(p, b).map_err(|e| format!("{}: {e}", p.display())))),
        clipboard_text: Some(Box::new(|| arboard::Clipboard::new().ok()?.get_text().ok())),
        set_clipboard_text: Some(Box::new(|t| arboard::Clipboard::new().and_then(|mut c| c.set_text(t.to_owned())).is_ok())),
        clipboard_image: Some(Box::new(|| {
            let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
            Some(ClipboardImage { width: img.width, height: img.height, rgba: img.bytes.into_owned() })
        })),
        ..Services::default()
    }
}
