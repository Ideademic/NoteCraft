//! The browser shell: web [`Services`], keeping the session in browser storage (or, hosted,
//! handing it to the hosting page: [`crate::host`]), drag and drop, the leave-page guard and the
//! eframe web runner.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use notecraft_ui_egui::app::SaveState;
use notecraft_ui_egui::{Inbox, InboxFile, NoteApp, OpenPurpose, Services};
use wasm_bindgen::JsCast as _;

use crate::host::{HostFiles, HostStorage, Opened};
use crate::persist::{self, Backend, BrowserStorage, Debounce, IDB_MARKER, LOCATION_KEY, PREFS_KEY, SESSION_KEY};
use crate::{browser, host, open, prefs};

const CANVAS_ID: &str = "notecraft_canvas";
const LOADING_ID: &str = "notecraft_loading";

/// Files picked or dropped as notebooks, one batch per pick or drop (sections picked together
/// make one notebook).
type Batches = Rc<RefCell<Vec<Vec<(String, Vec<u8>)>>>>;
/// Text read from the clipboard for the ribbon's Paste, waiting to be pasted.
type Pastes = Rc<RefCell<Vec<String>>>;

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
        // Host modes (`craft-web-host`): `?host=parent`, a same-origin page framing NoteCraft, or
        // `data-host="page"` on the canvas, a page running it in its own document. The host sends
        // the notebook and keeps what is saved, and guards leaving the page; unframed,
        // `?host=parent` stays the normal app.
        let bridge = host::Bridge::detect(&canvas);
        // `?fresh` starts with a new notebook, leaving what is stored alone until the next save
        // replaces it (a way out should the stored session ever be the problem).
        let fresh = browser::query().contains("fresh");
        let blob = if fresh || bridge.is_some() { None } else { load_session().await };
        let location = if bridge.is_some() { None } else { browser::local_get(LOCATION_KEY) };
        let stored_prefs = browser::local_get(PREFS_KEY);
        let unsaved = Rc::new(Cell::new(false));
        if bridge.is_none()
            && let Err(e) = browser::guard_unload(unsaved.clone())
        {
            log::error!("no unsaved-changes guard: {e}");
        }
        let app_bridge = bridge.clone();
        let mut options = eframe::WebOptions::default();
        if browser::query().contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    if let Some(rs) = &cc.wgpu_render_state {
                        log::info!("notecraft-web: wgpu backend {:?}", rs.adapter.get_info().backend);
                    }
                    let wanted = Rc::new(Cell::new(false));
                    let (mut session, notice) = match &app_bridge {
                        // The host sends the notebook to edit; none until then.
                        Some(_) => (notecraft_engine::Session::empty(), None),
                        None => persist::restore(blob.as_deref(), location.as_deref()),
                    };
                    let hosted = app_bridge.as_ref().map(|b| {
                        let h = Hosted { opened: Opened::default(), files: Rc::new(RefCell::new(HostFiles::default())) };
                        session.storage = Some(Box::new(HostStorage { files: h.files.clone(), post: b.post() }));
                        h
                    });
                    if hosted.is_none() {
                        session.storage = Some(Box::new(BrowserStorage { wanted: wanted.clone() }));
                    }
                    let revision = session.revision;
                    let ctx = cc.egui_ctx.clone();
                    let inbox = Inbox::default();
                    let batches = Batches::default();
                    let pastes = Pastes::default();
                    let picture_inbox = inbox.clone();
                    let picture_ctx = ctx.clone();
                    if let Err(e) = browser::listen_for_pasted_pictures(move |name, bytes| {
                        push(&picture_inbox, OpenPurpose::Picture, name, bytes);
                        picture_ctx.request_repaint();
                    }) {
                        log::error!("pasting pictures is unavailable: {e}");
                    }
                    let mut services = services(&ctx, &inbox, &batches, &pastes);
                    if let (Some(b), Some(h)) = (&app_bridge, &hosted) {
                        b.connect(&mut services, h.opened.clone(), ctx.clone());
                    }
                    let mut app = NoteApp::new(session, services);
                    // Fonts added on earlier visits.
                    crate::font_store::load_all(ctx.clone());
                    prefs::restore(&mut app, stored_prefs.as_deref());
                    if let Some(n) = notice {
                        app.set_status(n, true);
                    }
                    Ok(Box::new(WebShell {
                        app,
                        inbox,
                        batches,
                        pastes,
                        pasted_now: false,
                        prefs: prefs::PrefsSync::new(stored_prefs),
                        wanted,
                        debounce: Debounce::new(revision),
                        writer: Writer::default(),
                        failed: None,
                        location: None,
                        unsaved,
                        title: String::new(),
                        hosted,
                    }))
                }),
            )
            .await;
        // Ready only once the runner is up: starting can still fail after the app is made.
        if let Some(bridge) = &bridge {
            match &result {
                Ok(()) => bridge.post_ready(),
                Err(e) => bridge.post_failed(&format!("{e:?}")),
            }
        }
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => {
                    log::error!("failed to start NoteCraft: {e:?}");
                    el.set_class_name("failed");
                    if let Some(msg) = document.get_element_by_id("notecraft_status") {
                        msg.set_text_content(Some("NoteCraft couldn't start in this browser: it needs WebGPU or WebGL2."));
                    }
                }
            }
        }
    });
}

/// The session blob kept by an earlier visit: in `localStorage`, or in IndexedDB when the marker
/// says so.
async fn load_session() -> Option<Vec<u8>> {
    match browser::local_get(SESSION_KEY) {
        Some(m) if m == IDB_MARKER => match browser::idb_get().await {
            Ok(b) => b,
            Err(e) => {
                log::error!("reading the stored notebooks: {e}");
                None
            }
        },
        Some(json) => Some(json.into_bytes()),
        None => None,
    }
}

fn push(inbox: &Inbox, purpose: OpenPurpose, name: String, bytes: Vec<u8>) {
    match inbox.lock() {
        Ok(mut v) => v.push(InboxFile { purpose, name, bytes }),
        Err(poisoned) => poisoned.into_inner().push(InboxFile { purpose, name, bytes }),
    }
}

/// The picker's `accept` list for a purpose.
fn accept(purpose: OpenPurpose) -> &'static str {
    match purpose {
        OpenPurpose::Notebook | OpenPurpose::NotebookFolder => ".one,.onetoc2,.onepkg,.json",
        OpenPurpose::Import => ".onepkg,.one,.json",
        OpenPurpose::Picture => ".png,.jpg,.jpeg,.jfif,.gif,.bmp,.webp,image/*",
        OpenPurpose::Printout => ".pdf,.png,.jpg,.jpeg",
        OpenPurpose::Attachment => "",
        OpenPurpose::Font => ".ttf,.otf,.ttc,.woff,.woff2",
    }
}

/// Reads picked or dropped files, then routes them: notebooks to `batches` (one batch), the
/// rest to the app's inbox for `purpose` (`None`: by each file's extension).
fn deliver(files: Vec<web_sys::File>, purpose: Option<OpenPurpose>, inbox: Inbox, batches: Batches, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut batch = Vec::new();
        let mut total = 0usize;
        for f in files.into_iter().take(open::MAX_FILES) {
            let name = f.name();
            let bytes = match browser::read_file(&f, open::MAX_FILE_BYTES).await {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("{e}");
                    continue;
                }
            };
            total = total.saturating_add(bytes.len());
            if total > open::MAX_BATCH_BYTES {
                log::warn!("stopped reading at {name}: more than 1 GB at once");
                break;
            }
            let p = purpose.unwrap_or_else(|| open::purpose_for_drop(&name));
            // Import keeps adding `.one`/`.onepkg` to the current notebook; a NoteCraft `.json`
            // is a notebook of its own.
            let as_notebook = matches!(p, OpenPurpose::Notebook | OpenPurpose::NotebookFolder)
                || (p == OpenPurpose::Import && name.to_ascii_lowercase().ends_with(".json"));
            if as_notebook {
                batch.push((name, bytes));
            } else {
                push(&inbox, p, name, bytes);
            }
        }
        if !batch.is_empty() {
            batches.borrow_mut().push(batch);
        }
        ctx.request_repaint();
    });
}

fn services(ctx: &egui::Context, inbox: &Inbox, batches: &Batches, pastes: &Pastes) -> Services {
    let (open_ctx, open_inbox, open_batches) = (ctx.clone(), inbox.clone(), batches.clone());
    let (paste_ctx, paste_queue) = (ctx.clone(), pastes.clone());
    let (pic_ctx, pic_inbox) = (ctx.clone(), inbox.clone());
    Services {
        open_async: Some(Box::new(move |purpose| {
            let (ctx, inbox, batches) = (open_ctx.clone(), open_inbox.clone(), open_batches.clone());
            let many = matches!(purpose, OpenPurpose::Notebook | OpenPurpose::NotebookFolder | OpenPurpose::Import | OpenPurpose::Font);
            let picked = browser::pick_files(accept(purpose), many, move |files| deliver(files, Some(purpose), inbox, batches, ctx));
            if let Err(e) = picked {
                log::error!("file picker: {e}");
            }
        })),
        inbox: Some(inbox.clone()),
        download: Some(Box::new(browser::download)),
        print: Some(notecraft_ui_egui::Printing::Browser(Box::new(crate::print::print_pdf))),
        // Reading the clipboard is asynchronous in a browser: the text arrives a frame later
        // (`WebShell::raw_input_hook`). Ctrl+V needs none of this: eframe pastes it.
        clipboard_text: Some(Box::new(move || {
            let (ctx, queue) = (paste_ctx.clone(), paste_queue.clone());
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(text) = browser::clipboard_text().await {
                    queue.borrow_mut().push(text);
                    ctx.request_repaint();
                }
            });
            None
        })),
        clipboard_image: Some(Box::new(move || {
            let (ctx, inbox) = (pic_ctx.clone(), pic_inbox.clone());
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(bytes) = browser::clipboard_picture().await {
                    push(&inbox, OpenPurpose::Picture, "Pasted picture.png".to_owned(), bytes);
                    ctx.request_repaint();
                }
            });
            None
        })),
        set_clipboard_text: Some(Box::new(browser::set_clipboard_text)),
        prefers_dark: Some(browser::prefers_dark()),
        keep_font: Some(Box::new(crate::font_store::store)),
        ..Services::default()
    }
}

/// Writes session blobs. IndexedDB writes finish later; a newer write supersedes an older one
/// still on its way (`generation`).
#[derive(Default)]
struct Writer {
    generation: Rc<Cell<u64>>,
    in_flight: Rc<Cell<u32>>,
    /// The latest finished IndexedDB write's result, for the status bar.
    finished: Rc<RefCell<Option<Result<(), String>>>>,
}

impl Writer {
    /// Starts writing `blob`. `Err` when it can't be kept at all.
    fn write(&self, blob: Vec<u8>, ctx: &egui::Context) -> Result<(), String> {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let backend = Backend::for_len(blob.len());
        if backend == Backend::TooLarge {
            return Err(persist::TOO_LARGE.to_owned());
        }
        if backend == Backend::Local {
            // The blob is JSON, so UTF-8.
            let text = String::from_utf8(blob).map_err(|e| e.to_string())?;
            match browser::local_set(SESSION_KEY, &text) {
                Ok(()) => {
                    let current = self.generation.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Err(e) = browser::idb_delete(move || current.get() == generation).await {
                            log::debug!("IndexedDB cleanup: {e}");
                        }
                    });
                    return Ok(());
                }
                Err(e) => {
                    log::info!("localStorage refused the notebooks ({e}); using IndexedDB");
                    return self.write_idb(text.into_bytes(), generation, ctx);
                }
            }
        }
        self.write_idb(blob, generation, ctx)
    }

    fn write_idb(&self, blob: Vec<u8>, generation: u64, ctx: &egui::Context) -> Result<(), String> {
        let (current, in_flight, finished, ctx) = (self.generation.clone(), self.in_flight.clone(), self.finished.clone(), ctx.clone());
        in_flight.set(in_flight.get().saturating_add(1));
        wasm_bindgen_futures::spawn_local(async move {
            let check = current.clone();
            let result = match browser::idb_put(blob, move || check.get() == generation).await {
                Ok(true) if current.get() == generation => {
                    browser::local_set(SESSION_KEY, IDB_MARKER).map_err(|e| format!("{}: {e}", persist::TOO_LARGE))
                }
                Ok(_) => Ok(()),
                Err(e) => {
                    log::error!("IndexedDB: {e}");
                    Err(persist::TOO_LARGE.to_owned())
                }
            };
            in_flight.set(in_flight.get().saturating_sub(1));
            *finished.borrow_mut() = Some(result);
            ctx.request_repaint();
        });
        Ok(())
    }

    fn busy(&self) -> bool {
        self.in_flight.get() > 0
    }
}

/// Host mode's state: files the host sent, and which host file each notebook is saved as.
struct Hosted {
    opened: Opened,
    files: Rc<RefCell<HostFiles>>,
}

/// Wraps the app: opens picked and dropped notebooks, pastes clipboard text the ribbon asked for,
/// and keeps the session, where the user is and the preferences in browser storage (hosted, the
/// host keeps the notebooks).
struct WebShell {
    app: NoteApp,
    inbox: Inbox,
    batches: Batches,
    pastes: Pastes,
    /// Clipboard text was pasted this frame (clear the "nothing to paste" hint).
    pasted_now: bool,
    prefs: prefs::PrefsSync,
    /// Raised by the session's storage when autosave saved.
    wanted: Rc<Cell<bool>>,
    debounce: Debounce,
    writer: Writer,
    /// Why the notebooks couldn't be kept, while that lasts.
    failed: Option<String>,
    /// The location last stored.
    location: Option<notecraft_engine::Location>,
    /// Read by the leave-page guard.
    unsaved: Rc<Cell<bool>>,
    /// The tab's title last set.
    title: String,
    /// Hosted by a page ([`crate::host`]).
    hosted: Option<Hosted>,
}

impl WebShell {
    /// Opens the files the hosting page sent.
    fn open_hosted(&mut self) {
        let Some(h) = &self.hosted else { return };
        let files = std::mem::take(&mut *h.opened.borrow_mut());
        for (name, bytes) in files {
            let msg = match h.files.try_borrow_mut() {
                Ok(mut f) => host::open_hosted(&mut self.app.session, &mut f, &name, bytes),
                Err(_) => continue,
            };
            if let Some((msg, error)) = msg {
                self.app.set_status(msg, error);
            }
            self.app.ui.backstage = false;
        }
    }

    fn open_batches(&mut self) {
        let batches = std::mem::take(&mut *self.batches.borrow_mut());
        for batch in batches {
            if let Some((msg, error)) = open::open_into(&mut self.app.session, batch) {
                self.app.set_status(msg, error);
            }
            self.app.ui.backstage = false;
        }
    }

    fn report(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                if self.failed.take().is_some() && matches!(self.app.ui.save_state, SaveState::Failed(_)) {
                    self.app.ui.save_state = SaveState::Saved;
                }
            }
            Err(e) => {
                if self.failed.as_deref() != Some(e.as_str()) {
                    self.app.set_status(e.clone(), true);
                }
                self.app.ui.save_state = SaveState::Failed(e.clone());
                self.failed = Some(e);
            }
        }
    }

    /// Writes the session when autosave saved, or a while after a change that left nothing to
    /// autosave.
    fn keep(&mut self, ctx: &egui::Context) {
        let done = self.writer.finished.borrow_mut().take();
        if let Some(done) = done {
            self.report(done);
        }
        let now = ctx.input(|i| i.time);
        let (revision, dirty) = (self.app.session.revision, self.app.session.is_dirty());
        // Hosted, autosave hands each notebook to the host; nothing is kept in the browser.
        let hosted = self.hosted.is_some();
        if !hosted && self.debounce.due(revision, dirty, self.wanted.replace(false), now) {
            let result = self.app.session.to_blob().map_err(|e| e.to_string()).and_then(|blob| self.writer.write(blob, ctx));
            self.report(result);
        } else if !hosted && self.debounce.pending() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(persist::QUIET_SECS));
        }
        let s = &self.app.session;
        if !hosted && self.location != Some(s.current) {
            self.location = Some(s.current);
            if let Ok(json) = serde_json::to_string(&s.current)
                && let Err(e) = browser::local_set(LOCATION_KEY, &json)
            {
                log::debug!("location not stored: {e}");
            }
        }
        if let Some(json) = self.prefs.changed(&self.app)
            && let Err(e) = browser::local_set(PREFS_KEY, &json)
        {
            log::warn!("preferences not stored: {e}");
        }
        // eframe's web runner doesn't apply the window title; the tab shows the page, as the
        // desktop title bar does.
        let title = match self.app.session.current_page() {
            Some(p) => format!("{} - NoteCraft", p.display_title()),
            None => "NoteCraft".to_owned(),
        };
        if title != self.title {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                doc.set_title(&title);
            }
            self.title = title;
        }
        let s = &self.app.session;
        self.unsaved.set(s.is_dirty() || self.debounce.pending() || self.writer.busy() || self.failed.is_some());
    }
}

impl eframe::App for WebShell {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let pastes = std::mem::take(&mut *self.pastes.borrow_mut());
        self.pasted_now = !pastes.is_empty();
        raw.events.extend(pastes.into_iter().map(egui::Event::Paste));
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.open_hosted();
        self.open_batches();
        if std::mem::take(&mut self.pasted_now) && !self.app.ui.status_error {
            self.app.ui.status = None;
        }
        // Dates under page titles and Insert Date/Time in the browser's time zone. JavaScript's
        // offset is UTC minus local, so its sign is flipped.
        let offset = -js_sys::Date::new_0().get_timezone_offset();
        self.app.canvas.utc_offset_minutes = if offset.is_finite() { offset.clamp(-14.0 * 60.0, 14.0 * 60.0) as i32 } else { 0 };
        self.app.logic(ctx);
        self.keep(ctx);
    }

    /// Takes the browser's dropped files on the first pass of a shown frame (egui only offers
    /// their bytes asynchronously), then draws the app.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.ctx().current_pass_index() == 0 {
            let ctx = ui.ctx().clone();
            let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
            if !dropped.is_empty() {
                let (inbox, batches) = (self.inbox.clone(), self.batches.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let mut batch = Vec::new();
                    let mut total = 0usize;
                    for f in dropped.into_iter().take(open::MAX_FILES) {
                        let name = f.path().file_name().map_or_else(|| "Dropped file".to_owned(), |n| n.to_string_lossy().into_owned());
                        let bytes = match f.bytes_async().await {
                            Ok(b) if b.len() <= open::MAX_FILE_BYTES => b,
                            Ok(_) => {
                                log::warn!("{name} is larger than 512 MB");
                                continue;
                            }
                            Err(e) => {
                                log::warn!("couldn't read dropped file {name}: {e}");
                                continue;
                            }
                        };
                        total = total.saturating_add(bytes.len());
                        if total > open::MAX_BATCH_BYTES {
                            break;
                        }
                        match open::purpose_for_drop(&name) {
                            OpenPurpose::Notebook => batch.push((name, bytes)),
                            p => push(&inbox, p, name, bytes),
                        }
                    }
                    if !batch.is_empty() {
                        batches.borrow_mut().push(batch);
                    }
                    ctx.request_repaint();
                });
            }
        }
        self.app.ui(ui);
    }
}
