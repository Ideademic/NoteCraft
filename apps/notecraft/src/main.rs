//! NoteCraft desktop app.
//!
//! Usage: `notecraft [notebook folder | .onetoc2 | .onepkg | .one]…`; `--help` prints `USAGE`.
//!
//! At start it reads its settings (`prefs.json`, `recent.json`, `read.json` in the settings folder), offers to
//! restore notebooks a crash left unsaved, reopens the notebooks that were open last time (or the
//! default "My Notebook"), then the files on the command line. Changes autosave; on exit
//! everything is saved and the settings written.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod dialogs;
mod print;
mod services;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use notecraft_engine::Session;
use notecraft_engine::io::recent::RecentNotebooks;
use notecraft_ui_egui::app::WindowGeometry;
use notecraft_ui_egui::{NoteApp, Prefs};

const USAGE: &str = "\
NoteCraft — notebooks, sections and pages

USAGE:
  notecraft [PATH]…     open notebook folders, Open Notebook.onetoc2, .onepkg or .one files
  notecraft --version
  notecraft --help

  NOTECRAFT_TITLEBAR=app|system   draw the window frame ourselves, or leave it to the desktop
";

/// The app icon for the window and the Dock / task bar (generated from assets/app-icon).
#[cfg(target_os = "macos")]
const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/notecraft-macos-512.png");
#[cfg(not(target_os = "macos"))]
const ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/com.ideademic.notecraft.png");

/// What the settings files held when last written (to write them only when they change).
#[derive(Default)]
struct Saved {
    prefs: String,
    recent: String,
    /// `read.json`: the pages the user has seen (History ▸ Unread).
    read: String,
}

/// The eframe app: [`NoteApp`] does everything; this keeps its settings on disk.
struct App {
    app: NoteApp,
    /// File dialogs: asked during the frame, shown at its end, answered on a later frame.
    dialogs: dialogs::Dialogs,
    settings: Option<PathBuf>,
    saved: Saved,
    /// Recovery snapshots offered at start, waiting for the answer.
    recovery: Vec<PathBuf>,
    /// When the local UTC offset was last read (`egui` input time, seconds).
    offset_read_at: Option<f64>,
}

/// How often the local UTC offset is read again (daylight saving, a changed time zone).
const OFFSET_REFRESH_SECS: f64 = 60.0;

/// Minutes east of UTC on this computer now (chrono's `Local`, from the system's time zone).
fn local_utc_offset_minutes() -> i32 {
    use chrono::Offset as _;
    chrono::Local::now().offset().fix().local_minus_utc() / 60
}

impl App {
    /// Keeps the canvas's dates (title line, Insert Date/Time) in local time.
    fn refresh_utc_offset(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.offset_read_at.is_some_and(|t| now >= t && now - t < OFFSET_REFRESH_SECS) {
            return;
        }
        self.offset_read_at = Some(now);
        self.app.canvas.utc_offset_minutes = local_utc_offset_minutes();
    }

    /// Writes the settings that changed.
    fn store_settings(&mut self) {
        let Some(dir) = &self.settings else { return };
        let prefs = self.app.prefs().to_json();
        let recent = self.app.session.recent.to_json();
        let read = self.app.read_state_json();
        if prefs == self.saved.prefs && recent == self.saved.recent && read == self.saved.read {
            return;
        }
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("settings folder {}: {e}", dir.display());
            return;
        }
        if prefs != self.saved.prefs {
            match std::fs::write(dir.join("prefs.json"), &prefs) {
                Ok(()) => self.saved.prefs = prefs,
                Err(e) => log::warn!("writing prefs: {e}"),
            }
        }
        if recent != self.saved.recent {
            match std::fs::write(dir.join("recent.json"), &recent) {
                Ok(()) => self.saved.recent = recent,
                Err(e) => log::warn!("writing the recent list: {e}"),
            }
        }
        if read != self.saved.read {
            match std::fs::write(dir.join("read.json"), &read) {
                Ok(()) => self.saved.read = read,
                Err(e) => log::warn!("writing the read marks: {e}"),
            }
        }
    }

    /// Acts on the answer to the recovery prompt.
    fn answer_recovery(&mut self) {
        let Some(restore) = self.app.ui.recovery_answer.take() else { return };
        for path in std::mem::take(&mut self.recovery) {
            let result = if restore { self.app.session.restore_recovery(&path).map(|_| ()) } else { Session::discard_recovery(&path) };
            if let Err(e) = result {
                let text = notecraft_ui_egui::i18n::fmt_plain(
                    notecraft_ui_egui::tl!("Couldn't recover {path}: {error}"),
                    &[("path", &path.display().to_string()), ("error", &e.to_string())],
                );
                self.app.set_status(text, true);
            }
        }
    }

    /// Remembers the window's place for the next start.
    fn track_window(&mut self, ctx: &egui::Context) {
        let (rect, maximized, minimized, fullscreen) = ctx.input(|i| {
            (
                i.viewport().outer_rect,
                i.viewport().maximized.unwrap_or(false),
                i.viewport().minimized.unwrap_or(false),
                i.viewport().fullscreen.unwrap_or(false),
            )
        });
        if minimized || fullscreen {
            return;
        }
        let previous = self.app.ui.prefs.window;
        let next = match (rect, maximized) {
            // Maximised: keep the last normal size to restore to.
            (_, true) => previous.map(|w| WindowGeometry { maximized: true, ..w }),
            (Some(r), false) => WindowGeometry { x: r.min.x, y: r.min.y, width: r.width(), height: r.height(), maximized: false }.sanitized(),
            (None, false) => previous,
        };
        if next != previous && next.is_some() {
            self.app.ui.prefs.window = next;
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_utc_offset(ctx);
        self.dialogs.deliver(&mut self.app);
        self.app.logic(ctx);
        self.answer_recovery();
        self.track_window(ctx);
        self.store_settings();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui);
        // After the frame, outside any widget: the dialog never runs inside egui's pass.
        self.dialogs.show_pending(frame);
    }

    fn on_exit(&mut self) {
        match self.app.session.save_dirty() {
            // Everything is on disk: the recovery snapshot isn't needed.
            Ok(_) => self.app.session.end_recovery(),
            Err(e) => log::error!("saving on exit failed: {e}"),
        }
        self.store_settings();
        print::finish_pending_jobs();
    }
}

fn read(dir: Option<&Path>, name: &str) -> String {
    dir.and_then(|d| std::fs::read_to_string(d.join(name)).ok()).unwrap_or_default()
}

/// The session at start: the notebooks open last time, else the default notebook folder's "My
/// Notebook" if it exists, else a new one (saved there on the first change).
fn start_session(prefs: &Prefs, root: Option<&Path>) -> Session {
    let mut session = Session::empty();
    for path in &prefs.open_notebooks {
        let p = Path::new(path);
        if p.exists() {
            match notecraft_engine::io::open_path(p) {
                Ok(nb) => {
                    session.add_notebook(nb, notecraft_engine::io::origin_of(p));
                }
                Err(e) => log::warn!("reopening {path}: {e}"),
            }
        }
    }
    if session.notebooks.is_empty() {
        let mine = root.map(|r| r.join("My Notebook")).filter(|p| p.is_dir());
        match mine.as_deref().map(|p| (p, notecraft_engine::io::open_path(p))) {
            Some((p, Ok(nb))) => {
                session.add_notebook(nb, notecraft_engine::io::origin_of(p));
            }
            _ => session = Session::new_default(),
        }
    }
    session
}

fn main() -> eframe::Result {
    // Windows: the update helper is a copy of this program started with `--finish-update`.
    #[cfg(windows)]
    if let Some(result) = craft_update::self_update::finish(&notecraft_ui_egui::updates::UPDATE_APP, &std::env::args_os().skip(1).collect::<Vec<_>>())
    {
        std::process::exit(i32::from(result.is_err()));
    }
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--version" | "-V" => {
                let _ = writeln!(std::io::stdout(), "NoteCraft {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                let _ = std::io::stdout().write_all(USAGE.as_bytes());
                return Ok(());
            }
            _ => files.push(a),
        }
    }
    let settings = services::settings_dir();
    let prefs = Prefs::from_json(&read(settings.as_deref(), "prefs.json"));
    let recent_json = read(settings.as_deref(), "recent.json");
    let read_json = read(settings.as_deref(), "read.json");
    let mac = cfg!(target_os = "macos");
    let frame = craft_window::Choice::probe(craft_window::Prefix::new("NOTECRAFT")).into_frame();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("NoteCraft")
        .with_app_id("com.ideademic.notecraft")
        .with_inner_size([1440.0, 920.0])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true);
    if mac {
        // The title bar is ours, under AppKit's traffic lights.
        viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    } else {
        viewport = craft_window::decorate(viewport, &frame);
    }
    match eframe::icon_data::from_png_bytes(ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(std::sync::Arc::new(icon)),
        Err(e) => log::warn!("app icon: {e}"),
    }
    if let Some(w) = prefs.window.and_then(WindowGeometry::sanitized) {
        viewport = viewport.with_inner_size([w.width, w.height]).with_position([w.x, w.y]).with_maximized(w.maximized);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "NoteCraft",
        options,
        Box::new(move |cc| {
            let root = prefs.notebooks_dir.clone().map(PathBuf::from).or_else(services::notebooks_dir);
            let mut session = start_session(&prefs, root.as_deref());
            session.recent = RecentNotebooks::from_json(&recent_json);
            if let Some(root) = &root {
                session.storage = Some(Box::new(notecraft_engine::io::FolderStorage::new(root.clone())));
                session.default_location = Some(root.clone());
            }
            let mut recovery = Vec::new();
            let mut names = Vec::new();
            if let Some(dir) = &settings {
                match session.enable_recovery(dir.join("recovery")) {
                    Ok(()) => {
                        for c in session.recovery_candidates() {
                            names.extend(c.notebooks.iter().cloned());
                            recovery.push(c.path);
                        }
                    }
                    Err(e) => log::warn!("crash recovery unavailable: {e}"),
                }
            }
            if !recovery.is_empty() && names.is_empty() {
                names.push(notecraft_ui_egui::tl!("your notebooks").to_owned());
            }
            let dialogs = dialogs::Dialogs::new(&cc.egui_ctx);
            let mut app = NoteApp::new(session, services::desktop(dialogs.hook()));
            app.load_prefs(prefs);
            app.load_read_state(&read_json);
            // The check at start, unless File › Options turned it off.
            app.updates.enable_startup_check();
            app.integrated_titlebar = mac;
            app.window_buttons = if mac { None } else { frame.layout().cloned() };
            app.offer_recovery(names);
            for f in files {
                notecraft_ui_egui::app::files::open_path(&mut app, &f);
            }
            app.canvas.utc_offset_minutes = local_utc_offset_minutes();
            Ok(Box::new(App { app, dialogs, settings, saved: Saved::default(), recovery, offset_read_at: None }))
        }),
    )
}
