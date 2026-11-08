//! Desktop [`Services`]: native file dialogs (craft-dialogs, see [`crate::dialogs`]), the
//! clipboard (arboard), the file system, and where NoteCraft keeps its settings and notebooks.

use std::path::PathBuf;

use notecraft_ui_egui::{ClipboardImage, PathRequest, Services};

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

/// The desktop services; `ask_path` queues file dialogs (see [`crate::dialogs::Dialogs::hook`]).
pub fn desktop(ask_path: Box<dyn Fn(PathRequest)>) -> Services {
    Services {
        ask_path: Some(ask_path),
        read_file: Some(Box::new(|p| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())))),
        write_file: Some(Box::new(|p, b| std::fs::write(p, b).map_err(|e| format!("{}: {e}", p.display())))),
        clipboard_text: Some(Box::new(|| arboard::Clipboard::new().ok()?.get_text().ok().filter(|t| !t.is_empty()))),
        set_clipboard_text: Some(Box::new(|t| arboard::Clipboard::new().and_then(|mut c| c.set_text(t.to_owned())).is_ok())),
        clipboard_image: Some(Box::new(|| {
            let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
            Some(ClipboardImage { width: img.width, height: img.height, rgba: img.bytes.into_owned() })
        })),
        print: Some(crate::print::printing()),
        ..Services::default()
    }
}
