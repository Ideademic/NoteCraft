//! NoteCraft desktop app.
//!
//! Usage: `notecraft [notebook folder | .onetoc2 | .onepkg | .one]…`; `--help` prints `USAGE`.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod services;

use std::io::Write as _;

use notecraft_engine::Session;
use notecraft_ui_egui::NoteApp;

const USAGE: &str = "\
NoteCraft — notebooks, sections and pages

USAGE:
  notecraft [PATH]…     open notebook folders, Open Notebook.onetoc2, .onepkg or .one files
  notecraft --version
  notecraft --help
";

/// The eframe app: [`NoteApp`] does everything.
struct App(NoteApp);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn on_exit(&mut self) {
        if let Err(e) = self.0.session.save_dirty() {
            log::error!("saving on exit failed: {e}");
        }
    }
}

fn main() -> eframe::Result {
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NoteCraft")
            .with_app_id("com.ideademic.notecraft")
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([760.0, 480.0])
            .with_drag_and_drop(true)
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "NoteCraft",
        options,
        Box::new(move |_cc| {
            let mut session = Session::new_default();
            if let Some(root) = services::notebooks_dir() {
                session.storage = Some(Box::new(notecraft_engine::io::FolderStorage { root }));
            }
            let mut app = NoteApp::new(session, services::desktop());
            app.integrated_titlebar = true;
            for f in files {
                let _ = app.run(notecraft_engine::ids::NOTEBOOK_OPEN, serde_json::json!({"path": f}));
            }
            Ok(Box::new(App(app)))
        }),
    )
}
