//! The browser shell: web `Services` and the eframe web runner.

use notecraft_engine::Session;
use notecraft_engine::io::MemoryStorage;
use notecraft_ui_egui::{Inbox, NoteApp, Services};
use wasm_bindgen::JsCast as _;

const CANVAS_ID: &str = "notecraft_canvas";
const LOADING_ID: &str = "notecraft_loading";

struct App(NoteApp);

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_cc| {
                    let mut session = Session::new_default();
                    session.storage = Some(Box::new(MemoryStorage::default()));
                    let services = Services { inbox: Some(Inbox::default()), ..Services::default() };
                    Ok(Box::new(App(NoteApp::new(session, services))))
                }),
            )
            .await;
        if let Some(loading) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => {
                    log::error!("failed to start NoteCraft: {e:?}");
                    loading.set_text_content(Some("NoteCraft couldn't start in this browser."));
                }
            }
        }
    });
}
