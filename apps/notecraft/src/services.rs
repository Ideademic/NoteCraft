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
        open_external: Some(Box::new(|target| open::that_detached(target).map_err(|e| format!("{target}: {e}")))),
        open_file: Some(Box::new(open_attachment)),
        ..Services::default()
    }
}

/// Opens an attached file: written to a folder of its own under the system's temporary folder,
/// then opened with the app the system uses for its type.
fn open_attachment(name: &str, bytes: &[u8]) -> Result<(), String> {
    let clean: String = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(120)
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_owned();
    let clean = if clean.is_empty() { "Attachment".to_owned() } else { clean };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let dir = std::env::temp_dir().join("NoteCraft").join(format!("open-{stamp:x}"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(clean);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    open::that_detached(&path).map_err(|e| format!("{}: {e}", path.display()))
}
