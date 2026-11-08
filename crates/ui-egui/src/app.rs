//! The app: [`NoteApp`] holds the session, the platform services and the UI state, and lays out
//! the window frame each frame ([`frame`]). Keyboard shortcuts are in [`keys`]; opening,
//! importing, exporting and inserting files in [`files`].

pub mod files;
pub mod frame;
pub mod keys;
pub mod shots;

use egui::Rect;
use notecraft_engine::{CmdError, CmdOutput, Location, Origin, Session};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::backstage::BackstagePage;
use crate::canvas::CanvasState;
use crate::keytips::Keytips;
use crate::ribbon::RibbonTab;
use crate::ribbon::display::{RibbonDisplay, RibbonLayout};
use crate::services::Services;
use crate::theme::{Appearance, Tokens};

/// Height of the title bar, logical pixels (macOS keeps AppKit's 28 so the traffic lights sit
/// centred in it).
pub const TITLE_BAR_HEIGHT: f32 = 30.0;
pub const MAC_TITLE_BAR_HEIGHT: f32 = 28.0;
/// Where title-bar content starts after macOS's traffic lights.
pub const MAC_CONTENT_LEFT: f32 = 78.0;
/// Default width of the notebook pane.
pub const NOTEBOOK_PANE_WIDTH: f32 = 220.0;
/// Width of the collapsed notebook pane (the notebook's name, turned on its side).
pub const NOTEBOOK_BAR_WIDTH: f32 = 30.0;
/// Default width of the page list.
pub const PAGE_LIST_WIDTH: f32 = 230.0;
/// Most notebooks the recent list keeps.
pub const MAX_RECENT: usize = 12;
/// Most fonts the font box lists as recently used.
pub const MAX_RECENT_FONTS: usize = 8;
/// Most steps the Back button remembers.
pub const MAX_BACK: usize = 50;

/// A notebook opened before (File › Open's list).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentNotebook {
    pub name: String,
    pub path: String,
    /// `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
}

/// The window's place on screen, for the next start (desktop).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowGeometry {
    /// Within sane bounds, or `None` (a broken settings file never makes an unusable window).
    pub fn sanitized(self) -> Option<WindowGeometry> {
        let ok = [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite());
        if !ok || self.width < 400.0 || self.height < 300.0 || self.width > 20_000.0 || self.height > 20_000.0 {
            return None;
        }
        Some(WindowGeometry { x: self.x.clamp(-10_000.0, 10_000.0), y: self.y.clamp(-10_000.0, 10_000.0), ..self })
    }
}

/// Quick Access Toolbar entries (in order) the customise menu can show or hide.
pub const QAT_CHOICES: [&str; 5] =
    [QAT_BACK, notecraft_engine::ids::EDIT_UNDO, notecraft_engine::ids::EDIT_REDO, notecraft_engine::ids::PAGE_NEW, QAT_FULL_PAGE];
/// The Back button (not an engine command: the UI keeps the navigation history).
pub const QAT_BACK: &str = "ui.back";
/// Full Page View, from the Quick Access Toolbar.
pub const QAT_FULL_PAGE: &str = "ui.fullPageView";

/// Settings kept between runs (the desktop app stores them as JSON in the settings folder; the
/// web app in browser storage).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    pub appearance: Appearance,
    /// Most recent first.
    pub recent: Vec<RecentNotebook>,
    /// Where new notebooks go (`None`: the host's default, `Documents/NoteCraft Notebooks`).
    pub notebooks_dir: Option<String>,
    /// Save changes automatically (on by default; off, changes are saved on exit and on close).
    pub autosave: bool,
    pub notebook_pane_open: bool,
    pub page_list_open: bool,
    /// Classic or Simplified ribbon.
    pub ribbon_layout: RibbonLayout,
    /// Always show the ribbon, show tabs only, or full-screen mode. (Older settings files say
    /// `ribbonCollapsed: true` for Show tabs only; [`Prefs::from_json`] reads that too.)
    pub ribbon_display: RibbonDisplay,
    /// Quick Access Toolbar entries shown, in [`QAT_CHOICES`] order.
    pub qat: Vec<String>,
    pub window: Option<WindowGeometry>,
    /// The font box's Recently Used Fonts, most recent first.
    pub recent_fonts: Vec<String>,
    /// Notebooks open when the app last closed (paths), reopened at the next start.
    pub open_notebooks: Vec<String>,
    /// Interface language: [`crate::i18n::AUTO`] (follow the system) or a language code.
    pub language: String,
    /// Desktop: look for a newer release when the app starts (File › Options).
    pub check_for_updates: bool,
    /// The page zoom (1.0 = 100 %), kept for the next start.
    pub zoom: f32,
    /// Review › Spelling: red wavy lines under misspelled words as you type (File › Options).
    pub check_spelling: bool,
    /// Words added to the dictionary (Add to Dictionary), lower case.
    pub user_words: Vec<String>,
    /// The proofing language of text without one (a BCP 47 tag); empty follows the system.
    pub proofing_language: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            appearance: Appearance::System,
            recent: Vec::new(),
            notebooks_dir: None,
            autosave: true,
            notebook_pane_open: true,
            page_list_open: true,
            ribbon_layout: RibbonLayout::Classic,
            ribbon_display: RibbonDisplay::Always,
            qat: vec![QAT_BACK.to_owned(), notecraft_engine::ids::EDIT_UNDO.to_owned(), notecraft_engine::ids::EDIT_REDO.to_owned()],
            window: None,
            open_notebooks: Vec::new(),
            recent_fonts: Vec::new(),
            language: crate::i18n::AUTO.to_owned(),
            check_for_updates: true,
            zoom: 1.0,
            check_spelling: true,
            user_words: Vec::new(),
            proofing_language: String::new(),
        }
    }
}

impl Prefs {
    /// Reads prefs from JSON; anything unreadable falls back to the defaults.
    pub fn from_json(s: &str) -> Prefs {
        let mut p: Prefs = serde_json::from_str(s).unwrap_or_default();
        // Settings from before the display options: a collapsed ribbon is Show tabs only.
        let legacy =
            serde_json::from_str::<Value>(s).ok().map(|v| (v.get("ribbonCollapsed").and_then(Value::as_bool), v.get("ribbonDisplay").is_some()));
        if let Some((Some(true), false)) = legacy {
            p.ribbon_display = RibbonDisplay::TabsOnly;
        }
        p.recent.truncate(MAX_RECENT);
        p.open_notebooks.truncate(64);
        p.recent_fonts.retain(|f| !f.trim().is_empty() && f.chars().count() <= 128);
        p.recent_fonts.truncate(MAX_RECENT_FONTS);
        p.qat.retain(|id| QAT_CHOICES.contains(&id.as_str()));
        p.window = p.window.and_then(WindowGeometry::sanitized);
        p.zoom = if p.zoom.is_finite() { notecraft_geom::zoom::clamp(p.zoom) } else { 1.0 };
        p.user_words.retain(|w| !w.trim().is_empty() && w.chars().count() <= 64);
        p.user_words.truncate(100_000);
        p.proofing_language = notecraft_doc::page::clean_lang(&p.proofing_language).unwrap_or_default();
        // Junk follows the system. A well-formed code this build has no catalog for is kept (it
        // follows the system meanwhile), so a build with that language shows it again.
        let tag_like = |s: &str| !s.is_empty() && s.len() <= 35 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !tag_like(&p.language) {
            p.language = crate::i18n::AUTO.to_owned();
        }
        p
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Puts a font first in the font box's recently used fonts.
    pub fn remember_font(&mut self, font: &str) {
        let font = font.trim();
        if font.is_empty() {
            return;
        }
        self.recent_fonts.retain(|f| !f.eq_ignore_ascii_case(font));
        self.recent_fonts.insert(0, font.chars().take(128).collect());
        self.recent_fonts.truncate(MAX_RECENT_FONTS);
    }

    /// Puts a notebook first in the recent list.
    pub fn remember(&mut self, name: &str, path: &str, color: Option<String>) {
        self.recent.retain(|r| r.path != path);
        self.recent.insert(0, RecentNotebook { name: name.to_owned(), path: path.to_owned(), color });
        self.recent.truncate(MAX_RECENT);
    }
}

/// What the status bar says about saving.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SaveState {
    /// Everything is saved.
    #[default]
    Saved,
    /// Changes wait for autosave.
    Saving,
    /// Changes that nothing will save (no storage, or autosave off).
    NotSaved,
    /// The last save failed.
    Failed(String),
}

/// What is being renamed in place (a section tab, a page in the list, a notebook).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameTarget {
    Notebook(notecraft_doc::Guid),
    Section(notecraft_doc::Guid),
    Page(notecraft_doc::Guid),
}

/// A modal question.
#[derive(Clone, Debug, PartialEq)]
pub enum Prompt {
    /// Delete a section (or section group) and its pages?
    DeleteSection { id: notecraft_doc::Guid, name: String },
    /// Close a notebook?
    CloseNotebook { id: notecraft_doc::Guid, name: String },
    /// Notebooks recovered after a crash: restore them? (The engine's recovery hook; see
    /// `NoteApp::offer_recovery`.)
    Recovery { names: Vec<String> },
    /// A message with an OK button.
    Message { title: String, body: String },
    /// A yes / no question that runs `command` with `params` on yes (Empty Recycle Bin, Delete
    /// All Versions).
    Confirm { title: String, body: String, yes: String, command: String, params: Value },
}

/// UI state that isn't part of the notebooks.
#[derive(Clone, Debug, PartialEq)]
pub struct UiState {
    pub ribbon_tab: RibbonTab,
    /// File › (Backstage) is showing instead of the body.
    pub backstage: bool,
    pub backstage_page: BackstagePage,
    /// Classic or Simplified ribbon ([`crate::ribbon::display`]).
    pub ribbon_layout: RibbonLayout,
    /// Always show the ribbon, show tabs only, or full-screen mode.
    pub ribbon_display: RibbonDisplay,
    /// Full-screen mode: the tabs and ribbon are revealed for one use (the strip at the top was
    /// clicked). Show tabs only never peeks: a click on a tab shows the ribbon for good.
    pub ribbon_peek: bool,
    pub notebook_pane_open: bool,
    pub page_list_open: bool,
    /// View › Full Page View: the page alone (no ribbon, panes or tabs).
    pub full_page_view: bool,
    /// View › Hide Page Title. The canvas should skip the title block while this is on.
    pub hide_page_title: bool,
    /// Resolved from the appearance and the system's preference.
    pub dark: bool,
    /// The search box's text.
    pub search: String,
    /// Give the search box keyboard focus next frame (Ctrl+E).
    pub search_focus: bool,
    /// One-line message in the status bar (the last command's error, a notice).
    pub status: Option<String>,
    /// The status message is an error (drawn with a red mark).
    pub status_error: bool,
    pub save_state: SaveState,
    pub keytips: Keytips,
    /// In-place rename and its text.
    pub rename: Option<(RenameTarget, String)>,
    pub prompt: Option<Prompt>,
    /// The answer to [`Prompt::Recovery`] (`true`: restore), for the host to act on.
    pub recovery_answer: Option<bool>,
    /// Back: where the user was, most recent last.
    pub back: Vec<Location>,
    pub last_location: Location,
    /// File › New's form.
    pub new_notebook_name: String,
    pub new_notebook_dir: Option<String>,
    /// File › Info: the notebook whose settings are open.
    pub info_notebook: Option<notecraft_doc::Guid>,
    /// History ▸ Notebook Recycle Bin is open in the page area.
    pub bin_view: Option<crate::panes::history::BinView>,
    /// Where the user was when the recycle bin opened (going elsewhere closes it).
    pub bin_opened_at: Location,
    /// History ▸ Recent Edits is open, for this period (`history.recentEdits`'s `since`).
    pub recent_edits: Option<String>,
    /// Find on the page (Ctrl+F) is open.
    pub find: Option<crate::dialogs::find::FindBar>,
    /// Home › Find Tags: the Tags Summary pane is open.
    pub tags_pane: Option<crate::panes::tags::TagsPane>,
    /// Insert › Page Templates: the Templates pane is open.
    pub templates_pane: Option<crate::panes::templates::TemplatesPane>,
    /// The Alt Text dialog is open for this picture, with the text so far.
    pub alt_text: Option<(notecraft_doc::Guid, String)>,
    /// Where the search box looks (`search`'s scope: `section`, `sectionGroup`, `notebook`,
    /// `all`).
    pub search_scope: String,
    /// Bumped when the user's page templates change (the host stores them, see
    /// [`NoteApp::templates_json`]).
    pub templates_revision: u64,
    /// Review: the Spelling pane, the link and word count dialogs ([`crate::review`]).
    pub review: crate::review::ReviewUi,
    pub prefs: Prefs,
}

impl Default for UiState {
    fn default() -> Self {
        let prefs = Prefs::default();
        UiState {
            ribbon_tab: RibbonTab::Home,
            backstage: false,
            backstage_page: BackstagePage::Info,
            ribbon_layout: prefs.ribbon_layout,
            ribbon_display: prefs.ribbon_display,
            ribbon_peek: false,
            notebook_pane_open: prefs.notebook_pane_open,
            page_list_open: prefs.page_list_open,
            full_page_view: false,
            hide_page_title: false,
            dark: false,
            search: String::new(),
            search_focus: false,
            status: None,
            status_error: false,
            save_state: SaveState::Saved,
            keytips: Keytips::default(),
            rename: None,
            prompt: None,
            recovery_answer: None,
            back: Vec::new(),
            last_location: Location::default(),
            new_notebook_name: String::new(),
            new_notebook_dir: None,
            info_notebook: None,
            bin_view: None,
            bin_opened_at: Location::default(),
            recent_edits: None,
            find: None,
            tags_pane: None,
            templates_pane: None,
            alt_text: None,
            search_scope: "all".to_owned(),
            templates_revision: 0,
            review: crate::review::ReviewUi::default(),
            prefs,
        }
    }
}

/// The NoteCraft window.
pub struct NoteApp {
    pub session: Session,
    pub services: Services,
    pub ui: UiState,
    pub canvas: CanvasState,
    /// The canvas state of read-only pages (a page version, a page in the recycle bin).
    pub preview_canvas: CanvasState,
    /// File › Print: its settings, preview and the job being sent.
    pub print: crate::backstage::print::PrintPage,
    pub tokens: Tokens,
    /// macOS: the title bar is drawn by us under AppKit's traffic lights (hidden system title bar,
    /// full-size content view).
    pub integrated_titlebar: bool,
    /// The app draws the window frame (GNOME on Wayland, `NOTECRAFT_TITLEBAR=app`): the title bar
    /// gets minimise / maximise / close buttons in the desktop's layout.
    pub window_buttons: Option<craft_window::Layout>,
    /// The page area's rect last frame (Zoom › Page Width).
    pub canvas_rect: Option<Rect>,
    /// What the system says about dark mode (set by the host; `None`: ask egui).
    pub system_dark: Option<bool>,
    /// The update check and prompt ([`crate::updates`]; desktop only).
    pub updates: crate::updates::Updates,
    applied_dark: Option<bool>,
    /// macOS: the `movable` flag last applied to the window (craft-window).
    #[cfg(target_os = "macos")]
    movable: Option<bool>,
}

impl NoteApp {
    pub fn new(session: Session, services: Services) -> NoteApp {
        let ui = UiState { last_location: session.current, ..UiState::default() };
        NoteApp {
            session,
            services,
            ui,
            canvas: CanvasState::default(),
            preview_canvas: CanvasState::default(),
            print: crate::backstage::print::PrintPage::default(),
            tokens: Tokens::light(),
            integrated_titlebar: false,
            window_buttons: None,
            canvas_rect: None,
            system_dark: None,
            updates: crate::updates::Updates::default(),
            applied_dark: None,
            #[cfg(target_os = "macos")]
            movable: None,
        }
    }

    /// Takes saved settings (panes, ribbon, appearance, recent notebooks).
    pub fn load_prefs(&mut self, prefs: Prefs) {
        self.ui.notebook_pane_open = prefs.notebook_pane_open;
        self.ui.page_list_open = prefs.page_list_open;
        self.ui.ribbon_layout = prefs.ribbon_layout;
        self.ui.ribbon_display = prefs.ribbon_display;
        self.ui.ribbon_peek = false;
        self.canvas.set_zoom(prefs.zoom);
        crate::review::load_prefs(&mut self.session, &prefs);
        self.ui.prefs = prefs;
        self.applied_dark = None;
    }

    /// Takes the user's page templates (Insert › Page Templates › Save current page as a
    /// template), as [`NoteApp::templates_json`] gave them: the host keeps them beside its
    /// settings (`templates.json`, or browser storage). Junk is ignored.
    pub fn load_templates(&mut self, json: &str) {
        let list: Vec<notecraft_engine::templates::Template> = serde_json::from_str(json).unwrap_or_default();
        self.session.templates =
            list.into_iter().filter(|t| !t.is_builtin() && !t.id.trim().is_empty()).take(notecraft_engine::templates::MAX_USER_TEMPLATES).collect();
    }

    /// The user's page templates, for the host to store when [`UiState::templates_revision`]
    /// changes.
    pub fn templates_json(&self) -> String {
        serde_json::to_string(&self.session.templates).unwrap_or_default()
    }

    /// The settings to keep, with the panes, ribbon and open notebooks as they are now.
    pub fn prefs(&self) -> Prefs {
        let mut p = self.ui.prefs.clone();
        p.open_notebooks = self
            .session
            .notebooks
            .iter()
            .filter_map(|nb| match self.session.origins.get(&nb.id) {
                Some(Origin::Folder(p) | Origin::Package(p) | Origin::SectionFile(p)) => Some(p.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        p.notebook_pane_open = self.ui.notebook_pane_open;
        p.page_list_open = self.ui.page_list_open;
        p.ribbon_layout = self.ui.ribbon_layout;
        p.ribbon_display = self.ui.ribbon_display;
        p.zoom = self.canvas.zoom;
        p
    }

    /// Takes the pages the user had seen (History ▸ Unread), as [`NoteApp::read_state_json`]
    /// gave them: the host stores them beside its settings (`read.json`). Open notebooks the
    /// state doesn't know count as read through.
    pub fn load_read_state(&mut self, json: &str) {
        self.session.read_state = notecraft_engine::unread::ReadState::from_json(json);
        for nb in &self.session.notebooks {
            self.session.read_state.adopt(nb);
        }
    }

    /// The pages the user has seen, for the host to store.
    pub fn read_state_json(&self) -> String {
        self.session.read_state.to_json()
    }

    /// Whether command `id` exists (some land in later versions; their buttons stay greyed out).
    pub fn has_command(&self, id: &str) -> bool {
        notecraft_engine::registry().get(id).is_some()
    }

    /// Whether command `id` exists and can run now.
    pub fn can(&self, id: &str) -> bool {
        self.session.is_enabled(id)
    }

    /// Runs a command; an error is also shown in the status bar.
    pub fn run(&mut self, id: &str, params: Value) -> Result<CmdOutput, CmdError> {
        let r = self.session.run(id, &params);
        match &r {
            Ok(_) => {
                if self.ui.status_error {
                    self.ui.status = None;
                    self.ui.status_error = false;
                }
            }
            Err(e) => {
                log::info!("{id}: {e}");
                let text = match e {
                    CmdError::NotImplemented(_) | CmdError::Unknown(_) => {
                        let label = notecraft_engine::registry().get(id).map_or(id, |s| s.label);
                        crate::i18n::fmt_plain(crate::tl!("\u{201c}{name}\u{201d} isn't available in this version yet."), &[("name", label)])
                    }
                    e => e.to_string(),
                };
                self.set_status(text, true);
            }
        }
        r
    }

    /// Runs a command for a button: the result shows in the status bar on error.
    pub fn act(&mut self, id: &str, params: Value) {
        let _ = self.run(id, params);
    }

    /// A message in the status bar.
    pub fn set_status(&mut self, text: impl Into<String>, error: bool) {
        self.ui.status = Some(text.into());
        self.ui.status_error = error;
    }

    /// The command's label from the registry (its id when unknown).
    pub fn label(&self, id: &str) -> &'static str {
        notecraft_engine::registry().get(id).map_or("", |s| s.label)
    }

    /// Offers to restore notebooks the engine recovered after a crash.
    ///
    /// Hook for the engine's crash recovery: the host calls this at start-up with the names of
    /// recoverable notebooks; the answer arrives in [`UiState::recovery_answer`]. Until the engine
    /// lands a recovery API this only shows the prompt.
    pub fn offer_recovery(&mut self, names: Vec<String>) {
        if !names.is_empty() {
            self.ui.prompt = Some(Prompt::Recovery { names });
        }
    }

    /// Goes back to where the user was before (the Quick Access Toolbar's Back).
    pub fn go_back(&mut self) {
        while let Some(loc) = self.ui.back.pop() {
            let Some(page) = loc.page else { continue };
            if self.session.notebooks.iter().any(|n| n.find_page(page).is_some()) {
                // Don't record the step back as a new place to go back to.
                self.ui.last_location = loc;
                self.act(notecraft_engine::ids::NAV_PAGE, serde_json::json!({ "id": page }));
                self.ui.last_location = self.session.current;
                return;
            }
        }
    }

    /// Whether Back has somewhere to go.
    pub fn can_go_back(&self) -> bool {
        !self.ui.back.is_empty()
    }

    /// Per-frame work that isn't drawing: theme, autosave, delivered files, the dirty flag.
    pub fn logic(&mut self, ctx: &egui::Context) {
        crate::fonts::sync(ctx);
        crate::i18n::set_current(crate::i18n::Lang::from_pref(&self.ui.prefs.language));
        let system = self
            .system_dark
            .or_else(|| self.services.prefers_dark.as_ref().and_then(|f| f()))
            .or_else(|| ctx.system_theme().map(|t| t == egui::Theme::Dark));
        self.ui.dark = self.ui.prefs.appearance.is_dark(system);
        if self.applied_dark != Some(self.ui.dark) {
            self.tokens = Tokens::for_dark(self.ui.dark);
            self.tokens.apply(ctx);
            self.applied_dark = Some(self.ui.dark);
        }
        self.track_location();
        let now = ctx.input(|i| i.time);
        if self.ui.prefs.autosave {
            match self.session.tick(now) {
                Some(Ok(_)) => {
                    self.ui.save_state = SaveState::Saved;
                    ctx.request_repaint();
                }
                Some(Err(e)) => self.ui.save_state = SaveState::Failed(e.to_string()),
                None => {}
            }
        }
        if self.session.is_dirty() {
            self.ui.save_state = if self.session.storage.is_some() && self.ui.prefs.autosave { SaveState::Saving } else { SaveState::NotSaved };
            // Wake up for the autosave deadline even without input.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else if self.ui.save_state != SaveState::Saved && !matches!(self.ui.save_state, SaveState::Failed(_)) {
            self.ui.save_state = SaveState::Saved;
        }
        if !self.session.notices.is_empty() {
            let notice = self.session.notices.remove(0);
            let error = notice.starts_with("Couldn't");
            self.set_status(notice, error);
        }
        self.drain_canvas_messages();
        self.remember_open_notebooks();
        files::drain_inbox(self);
        crate::updates::tick(self, ctx);
        keys::handle(self, ctx);
        crate::backstage::print::poll(self, ctx);
        crate::review::tick(self, ctx);
        if let Some(hook) = &self.services.on_dirty {
            hook(self.session.is_dirty());
        }
    }

    /// Moves the canvas's command errors to the status bar (the last one shows). `true` when there
    /// was one.
    pub fn drain_canvas_messages(&mut self) -> bool {
        let Some(last) = std::mem::take(&mut self.canvas.messages).pop() else { return false };
        self.set_status(last, true);
        true
    }

    /// Records the previous place for Back whenever the current page changes.
    fn track_location(&mut self) {
        let now = self.session.current;
        if now.page != self.ui.last_location.page {
            if self.ui.last_location.page.is_some() {
                self.ui.back.push(self.ui.last_location);
                if self.ui.back.len() > MAX_BACK {
                    self.ui.back.remove(0);
                }
            }
            self.ui.last_location = now;
        }
    }

    /// Keeps every open notebook that lives somewhere on the recent list.
    fn remember_open_notebooks(&mut self) {
        for nb in &self.session.notebooks {
            let path = match self.session.origins.get(&nb.id) {
                Some(Origin::Folder(p) | Origin::Package(p) | Origin::SectionFile(p)) => p.to_string_lossy().into_owned(),
                _ => continue,
            };
            let color = Some(nb.color.to_hex());
            let known = self.ui.prefs.recent.iter().any(|r| r.path == path && r.name == nb.name && r.color == color);
            if !known {
                // Updating an entry in place keeps the order; a new one goes first.
                if let Some(r) = self.ui.prefs.recent.iter_mut().find(|r| r.path == path) {
                    r.name = nb.name.clone();
                    r.color = color;
                } else {
                    self.ui.prefs.remember(&nb.name, &path, color);
                }
            }
        }
    }

    /// Draws the whole window into `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        frame::show(self, ui);
        if self.drain_canvas_messages() {
            ui.ctx().request_repaint();
        }
        #[cfg(target_os = "macos")]
        if self.integrated_titlebar {
            craft_window::title_bar::sync_movable(ui.ctx(), &mut self.movable);
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_editing;
#[cfg(test)]
mod tests_history;
#[cfg(test)]
mod tests_review;
