//! The app: [`NoteApp`] holds the session, the platform services and the UI state, and lays out
//! the window frame each frame.

use egui::{CentralPanel, Frame, Panel};
use notecraft_engine::{CmdError, CmdOutput, Session};
use serde_json::Value;

use crate::canvas::{self, CanvasState};
use crate::ribbon::RibbonTab;
use crate::services::Services;
use crate::theme::Tokens;
use crate::{backstage, chrome, panes, ribbon};

/// Height of the title bar, logical pixels.
pub const TITLE_BAR_HEIGHT: f32 = 32.0;
/// Default width of the notebook pane.
pub const NOTEBOOK_PANE_WIDTH: f32 = 220.0;
/// Default width of the page list.
pub const PAGE_LIST_WIDTH: f32 = 240.0;

/// UI state that isn't part of the notebooks.
#[derive(Clone, Debug, PartialEq)]
pub struct UiState {
    pub ribbon_tab: RibbonTab,
    /// File ▸ (Backstage) is showing instead of the body.
    pub backstage: bool,
    /// The ribbon shows only its tabs until one is clicked.
    pub ribbon_collapsed: bool,
    pub notebook_pane_open: bool,
    pub page_list_open: bool,
    pub dark: bool,
    /// The search box's text.
    pub search: String,
    /// One-line message in the status bar (the last command's error, a notice).
    pub status: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            ribbon_tab: RibbonTab::Home,
            backstage: false,
            ribbon_collapsed: false,
            notebook_pane_open: true,
            page_list_open: true,
            dark: false,
            search: String::new(),
            status: None,
        }
    }
}

/// The NoteCraft window.
pub struct NoteApp {
    pub session: Session,
    pub services: Services,
    pub ui: UiState,
    pub canvas: CanvasState,
    pub tokens: Tokens,
    /// Whether the title bar is drawn by us (custom, no system title bar).
    pub integrated_titlebar: bool,
    applied_dark: Option<bool>,
}

impl NoteApp {
    pub fn new(session: Session, services: Services) -> NoteApp {
        NoteApp {
            session,
            services,
            ui: UiState::default(),
            canvas: CanvasState::default(),
            tokens: Tokens::light(),
            integrated_titlebar: true,
            applied_dark: None,
        }
    }

    /// Runs a command; an error is also shown in the status bar.
    pub fn run(&mut self, id: &str, params: Value) -> Result<CmdOutput, CmdError> {
        let r = self.session.run(id, &params);
        match &r {
            Ok(_) => self.ui.status = None,
            Err(e) => {
                log::info!("{id}: {e}");
                self.ui.status = Some(e.to_string());
            }
        }
        r
    }

    /// Per-frame work that isn't drawing: theme, autosave, delivered files, the dirty flag.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if self.applied_dark != Some(self.ui.dark) {
            self.tokens = Tokens::for_dark(self.ui.dark);
            self.tokens.apply(ctx);
            self.applied_dark = Some(self.ui.dark);
        }
        let now = ctx.input(|i| i.time);
        if self.session.tick(now).is_some() {
            ctx.request_repaint();
        }
        if self.session.is_dirty() {
            // Wake up for the autosave deadline even without input.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if let Some(notice) = self.session.notices.pop() {
            self.ui.status = Some(notice);
        }
        if let Some(hook) = &self.services.on_dirty {
            hook(self.session.is_dirty());
        }
    }

    /// Draws the whole window into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let tokens = self.tokens.clone();
        Panel::top("nc_title_bar").exact_size(TITLE_BAR_HEIGHT).frame(Frame::new().fill(tokens.title_bar)).show(ui, |ui| chrome::title_bar(ui, self));
        if self.ui.backstage {
            CentralPanel::default().frame(Frame::new().fill(tokens.panel)).show(ui, |ui| backstage::show(ui, self));
            return;
        }
        Panel::top("nc_ribbon").frame(Frame::new().fill(tokens.ribbon)).show(ui, |ui| ribbon::show(ui, self));
        Panel::bottom("nc_status").frame(Frame::new().fill(tokens.status_bar)).show(ui, |ui| chrome::status_bar(ui, self));
        if self.ui.notebook_pane_open {
            Panel::left("nc_notebooks")
                .default_size(NOTEBOOK_PANE_WIDTH)
                .frame(Frame::new().fill(tokens.notebook_pane))
                .show(ui, |ui| panes::notebook_pane(ui, self));
        }
        if self.ui.page_list_open {
            Panel::right("nc_pages")
                .default_size(PAGE_LIST_WIDTH)
                .frame(Frame::new().fill(tokens.page_list))
                .show(ui, |ui| panes::page_list(ui, self));
        }
        CentralPanel::default().frame(Frame::new().fill(tokens.canvas)).show(ui, |ui| {
            panes::section_tabs(ui, self);
            canvas::show(ui, &mut self.canvas, &mut self.session, &self.tokens, &self.services);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_draws_headless() {
        let mut app = NoteApp::new(Session::new_default(), Services::default());
        let mut harness = egui_kittest::Harness::new_ui(|ui| {
            app.logic(ui.ctx());
            app.ui(ui);
        });
        harness.run();
        drop(harness);
        assert!(app.session.current_page().is_some());
    }

    #[test]
    fn errors_reach_the_status_bar() {
        let mut app = NoteApp::new(Session::new_default(), Services::default());
        assert!(app.run("format.bold", Value::Null).is_err());
        assert!(app.ui.status.is_some());
        assert!(app.run("page.new", Value::Null).is_ok());
        assert!(app.ui.status.is_none());
    }
}
