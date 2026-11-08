//! Host modes: a page that runs NoteCraft (in an `<iframe>`, or in its own document) sends it a
//! notebook to edit and stores what NoteCraft saves, instead of the browser keeping it. The
//! messages and both transports are `craft-web-host`'s (storytold/craft-libs), the contract the
//! other Craft apps' web builds speak, with the `notecraft:` prefix. This module is NoteCraft's
//! part of it, ported from WordCraft's bridge.
//!
//! - `?host=parent` (an `<iframe>`) talks to the same-origin parent over `postMessage`;
//!   `data-host="page"` on the canvas (the page's own document, no iframe) uses DOM `CustomEvent`s
//!   on the canvas. `?host=parent` wins when both are present. With no parent, or a parent on
//!   another origin, there is no bridge: the normal app, keeping notebooks in the browser.
//! - `notecraft:ready` once the app runs and listens. NoteCraft reads bytes (`file: false`), so a
//!   host sends `bytes`.
//! - `notecraft:open` (from the host): `{ name, bytes }`, a notebook as one `.onepkg` (a `.one`
//!   section or a NoteCraft `.json` opens too). Over [`MAX_OPEN_BYTES`] is refused before it is
//!   copied.
//! - `notecraft:save` (to the host): `{ name, saveAs, bytes }`. **A hosted notebook travels as one
//!   `.onepkg`**: every autosave of a notebook sends the whole package. A notebook the host sent as
//!   a `.onepkg` goes back under the host's name with `saveAs: false`; any other (a new notebook,
//!   one the host sent as `.one`) is a new file, `<name>.onepkg` with `saveAs: true` the first time
//!   and `false` after. File › Export's files (HTML, Markdown, a `.onepkg` copy) are sent with
//!   `saveAs: false` instead of downloading.
//! - `notecraft:dirty` whenever unsaved changes appear or go away; `notecraft:failed` when the app
//!   couldn't start.
//!
//! In both host modes the notebooks aren't kept in browser storage (the host keeps them), the app
//! starts with none until the host sends one, and the host guards leaving the page.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use notecraft_doc::{Guid, Notebook};
use notecraft_engine::{IoError, Origin, Session, Storage, ids};
use serde_json::json;

/// The app id in the host messages (`notecraft:ready`).
pub const APP: &str = "notecraft";

/// The name a file from the host gets when it sent none (the crate says `document` then).
pub const DEFAULT_NAME: &str = "Notebook.onepkg";

/// The largest file taken from the host page, refused before it is copied.
pub const MAX_OPEN_BYTES: u64 = crate::open::MAX_FILE_BYTES as u64;

/// Sends a saved file to the host: (name, bytes, save as a new file).
pub type Post = Rc<dyn Fn(&str, &[u8], bool) -> Result<(), String>>;

/// Files the host sent, waiting for the next frame: (name, bytes).
pub type Opened = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

/// Which host file each notebook is saved as.
#[derive(Debug, Default)]
pub struct HostFiles {
    names: HashMap<Guid, String>,
}

impl HostFiles {
    /// Notebook `id` came from the host as `name`: a `.onepkg` is saved back under that name.
    pub fn opened(&mut self, id: Guid, name: &str) {
        if name.to_ascii_lowercase().ends_with(".onepkg") {
            self.names.insert(id, name.to_owned());
        }
    }

    /// The file `nb` is saved as, and whether that is a new file for the host (`saveAs`).
    pub fn target(&self, nb: &Notebook) -> (String, bool) {
        match self.names.get(&nb.id) {
            Some(name) => (name.clone(), false),
            None => {
                let raw = format!("{}.onepkg", notecraft_one::safe_file_stem(&nb.name));
                (craft_web_host::file_name(Some(&raw), DEFAULT_NAME), true)
            }
        }
    }

    /// `nb` was saved as `name`: later saves go to the same file.
    pub fn saved(&mut self, id: Guid, name: String) {
        self.names.insert(id, name);
    }
}

/// The session's storage in host mode: each save sends the notebook to the host as one `.onepkg`.
pub struct HostStorage {
    pub files: Rc<RefCell<HostFiles>>,
    pub post: Post,
}

impl Storage for HostStorage {
    fn save(&mut self, notebook: &Notebook, _origin: &Origin, _dirty: &HashSet<Guid>, _toc_dirty: bool) -> Result<Option<Origin>, IoError> {
        let bytes = notecraft_onepkg::write(notebook)?;
        let (name, save_as) = self.files.try_borrow().map_err(|e| IoError::Io(e.to_string()))?.target(notebook);
        (self.post)(&name, &bytes, save_as).map_err(IoError::Io)?;
        if let Ok(mut files) = self.files.try_borrow_mut() {
            files.saved(notebook.id, name);
        }
        Ok(None)
    }
}

/// Opens a file the host sent: its notebook joins the session (or, already open, is gone to) and
/// is remembered as that host file. Nothing is marked unsaved: the host has it as it is. Returns
/// the status line to show, if any, and whether it is an error.
pub fn open_hosted(session: &mut Session, files: &mut HostFiles, name: &str, bytes: Vec<u8>) -> Option<(String, bool)> {
    let (notebooks, problems) = crate::open::notebooks_from_files(vec![(name.to_owned(), bytes)]);
    if !problems.is_empty() {
        return Some((problems.join(" "), notebooks.is_empty()));
    }
    let mut opened = None;
    for nb in notebooks {
        let id = nb.id;
        if session.notebook(id).is_none() {
            opened = Some(nb.name.clone());
            session.add_notebook(nb, Origin::Scratch);
        }
        let _ = session.run(ids::NAV_NOTEBOOK, &json!({ "id": id }));
        files.opened(id, name);
    }
    opened.map(|n| (format!("Opened \u{201c}{n}\u{201d}."), false))
}

/// A file the host sent, queued for the next frame like a dropped one. A host's file is untrusted
/// input: an oversized one is refused before it is copied (its own length, not the host's claim).
/// A blank name (the crate's `document`) becomes [`DEFAULT_NAME`].
pub fn queue_open(opened: &Opened, name: &str, len: u64, bytes: impl FnOnce() -> Vec<u8>) -> bool {
    if len > MAX_OPEN_BYTES {
        log::warn!("host file is {len} bytes, over the {} MiB host open limit; not opened", MAX_OPEN_BYTES >> 20);
        return false;
    }
    let name = if name == "document" { DEFAULT_NAME } else { name };
    match opened.try_borrow_mut() {
        Ok(mut q) => {
            q.push((name.to_owned(), bytes()));
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub use bridge::Bridge;

#[cfg(target_arch = "wasm32")]
mod bridge {
    use std::rc::Rc;

    use craft_web_host::web::{FileData, Host, Message};
    use notecraft_ui_egui::Services;

    use super::{APP, Opened, Post};

    /// The connection to the hosting page.
    #[derive(Clone)]
    pub struct Bridge {
        host: Rc<Host>,
    }

    impl Bridge {
        /// The bridge for this page, or `None` when NoteCraft isn't hosted: no `?host=parent` and
        /// no `data-host="page"` on the canvas, an unframed `?host=parent` page, or a parent on
        /// another origin.
        pub fn detect(canvas: &web_sys::HtmlCanvasElement) -> Option<Self> {
            Some(Self { host: Rc::new(Host::connect(APP, canvas)?) })
        }

        /// Sends what would download to the host, reports unsaved changes to it, and hears the
        /// files it sends (queued in `opened`).
        pub fn connect(&self, services: &mut Services, opened: Opened, ctx: egui::Context) {
            self.host.listen(move |message| {
                if let Message::Open { name, data } = message {
                    match &data {
                        FileData::Bytes(array) => {
                            let len = data.size().max(0.0) as u64;
                            if super::queue_open(&opened, &name, len, || array.to_vec()) {
                                ctx.request_repaint();
                            }
                        }
                        FileData::Blob(_) => log::warn!("host file {name} came as a File; NoteCraft reads bytes only; not opened"),
                    }
                }
            });
            let post = self.post();
            services.download = Some(Box::new(move |name: &str, bytes: &[u8]| post(name, bytes, false)));
            // The crate doesn't repeat a state it already reported.
            let host = self.host.clone();
            services.on_dirty = Some(Box::new(move |dirty: bool| host.post_dirty(dirty)));
        }

        /// Sends a file to the host: (name, bytes, save as a new file).
        pub fn post(&self) -> Post {
            let host = self.host.clone();
            Rc::new(move |name: &str, bytes: &[u8], save_as: bool| host.post_save(name, bytes, None, save_as))
        }

        pub fn post_ready(&self) {
            let _ = self.host.post_ready(env!("CARGO_PKG_VERSION"), false);
        }

        /// The app couldn't start (no WebGPU or WebGL2): let the host say so.
        pub fn post_failed(&self, error: &str) {
            let _ = self.host.post_failed(error, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notebook(name: &str) -> Notebook {
        let mut nb = Notebook::new_default();
        nb.name = name.into();
        nb
    }

    type Sent = Rc<RefCell<Vec<(String, usize, bool)>>>;

    fn storage() -> (HostStorage, Sent) {
        let sent: Sent = Rc::default();
        let log = sent.clone();
        let post: Post = Rc::new(move |name: &str, bytes: &[u8], save_as: bool| {
            log.borrow_mut().push((name.to_owned(), bytes.len(), save_as));
            Ok(())
        });
        (HostStorage { files: Rc::default(), post }, sent)
    }

    #[test]
    fn a_notebook_from_the_host_goes_back_under_its_name() {
        let work = notebook("Work");
        let pkg = notecraft_onepkg::write(&work).unwrap();
        let mut session = Session::empty();
        let (store, sent) = storage();
        let files = store.files.clone();
        let msg = open_hosted(&mut session, &mut files.borrow_mut(), "Team Work.onepkg", pkg.clone());
        assert_eq!(msg, Some(("Opened \u{201c}Work\u{201d}.".into(), false)));
        assert_eq!(session.notebooks.len(), 1);
        assert!(!session.is_dirty(), "the host has it as it is");
        assert_eq!(session.current_notebook().map(|n| n.id), Some(work.id));
        // Sent again: gone to, not opened twice.
        assert_eq!(open_hosted(&mut session, &mut files.borrow_mut(), "Team Work.onepkg", pkg), None);
        assert_eq!(session.notebooks.len(), 1);

        session.storage = Some(Box::new(store));
        let page = session.current_page().map(|p| p.id);
        assert!(session.run(ids::PAGE_RENAME, &json!({ "id": page, "title": "Edited" })).is_ok());
        assert!(session.is_dirty());
        assert!(session.save_dirty().is_ok());
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].0.as_str(), sent[0].2), ("Team Work.onepkg", false));
        assert!(sent[0].1 > 0);
    }

    #[test]
    fn new_notebooks_and_sections_become_new_packages() {
        let (mut store, sent) = storage();
        let nb = notebook("Ideas / 2026");
        store.files.borrow_mut().opened(nb.id, "Ideas.one");
        assert!(store.save(&nb, &Origin::Scratch, &HashSet::new(), true).is_ok());
        assert!(store.save(&nb, &Origin::Scratch, &HashSet::new(), true).is_ok());
        let sent = sent.borrow();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].0.ends_with(".onepkg") && !sent[0].0.contains('/'), "{:?}", sent[0]);
        assert!(sent[0].2, "the first save makes a new file");
        assert_eq!(sent[1].0, sent[0].0);
        assert!(!sent[1].2, "later saves write the same file");
    }

    #[test]
    fn a_failed_post_is_a_failed_save_and_is_tried_as_new_again() {
        let attempts: Rc<RefCell<Vec<bool>>> = Rc::default();
        let log = attempts.clone();
        let mut store = HostStorage {
            files: Rc::default(),
            post: Rc::new(move |_: &str, _: &[u8], save_as: bool| {
                log.borrow_mut().push(save_as);
                Err("host gone".into())
            }),
        };
        let nb = notebook("Lost");
        assert!(matches!(store.save(&nb, &Origin::Scratch, &HashSet::new(), true), Err(IoError::Io(e)) if e == "host gone"));
        assert!(store.save(&nb, &Origin::Scratch, &HashSet::new(), true).is_err());
        assert_eq!(*attempts.borrow(), [true, true]);
    }

    #[test]
    fn host_files_are_checked() {
        let opened: Opened = Rc::default();
        assert!(!queue_open(&opened, "huge.onepkg", MAX_OPEN_BYTES + 1, || panic!("not copied")));
        assert!(queue_open(&opened, "document", 3, || vec![1, 2, 3]));
        assert_eq!(opened.borrow().first().map(|(n, b)| (n.as_str(), b.len())), Some((DEFAULT_NAME, 3)));
        let mut session = Session::empty();
        let mut files = HostFiles::default();
        let bad = open_hosted(&mut session, &mut files, "bad.onepkg", b"junk".to_vec());
        assert!(bad.is_some_and(|(_, error)| error));
        assert!(session.notebooks.is_empty());
    }
}
