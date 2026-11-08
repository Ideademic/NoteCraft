//! The desktop's file dialogs, through craft-dialogs (craft-libs): the UI asks
//! ([`notecraft_ui_egui::Services::ask_path`]), the request is queued and shown at the end of the
//! frame, and the answer comes back on a later frame ([`Dialogs::deliver`]).
//!
//! Never a blocking `rfd` dialog: on macOS that runs AppKit's modal loop inside winit's event
//! handler, and an event arriving meanwhile (a file dragged over the window, the panel resized)
//! re-enters the handler and aborts the app. On macOS, Open and Folder show as free-standing
//! panels and Save as a sheet; elsewhere each is a modal owned by the window.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use craft_dialogs::{Answer, FileDialog};
use notecraft_ui_egui::{FolderFor, NoteApp, OpenPurpose, PathRequest};

/// The queue the UI's requests go into and the answers come out of.
#[derive(Clone)]
pub struct Dialogs {
    inner: Rc<RefCell<craft_dialogs::Dialogs<PathRequest>>>,
}

impl Dialogs {
    /// Dialogs that wake `ctx` when one answers.
    pub fn new(ctx: &egui::Context) -> Dialogs {
        let ctx = ctx.clone();
        Dialogs { inner: Rc::new(RefCell::new(craft_dialogs::Dialogs::new(Arc::new(move || ctx.request_repaint())))) }
    }

    /// The hook for [`notecraft_ui_egui::Services::ask_path`]: queues the request.
    pub fn hook(&self) -> Box<dyn Fn(PathRequest)> {
        let inner = self.inner.clone();
        Box::new(move |request| match inner.try_borrow_mut() {
            Ok(mut d) => {
                let dialog = file_dialog(&request);
                d.ask(dialog, request);
            }
            Err(_) => log::warn!("a file dialog was asked for while the dialogs were busy; ignored"),
        })
    }

    /// Hands the answers that came in to the app: a chosen path is acted on; a dialog that couldn't
    /// be shown is reported in the status bar; Cancel does nothing.
    pub fn deliver(&self, app: &mut NoteApp) {
        let answers = match self.inner.try_borrow_mut() {
            Ok(mut d) => d.take_answers(),
            Err(_) => return,
        };
        for (request, answer) in answers {
            match answer {
                Answer::Paths(paths) => {
                    if let Some(path) = paths.first() {
                        notecraft_ui_egui::app::files::answer_path(app, request, path);
                    }
                }
                Answer::Unavailable(why) => {
                    let text =
                        notecraft_ui_egui::i18n::fmt_plain(notecraft_ui_egui::tl!("The file dialog couldn't be shown: {why}"), &[("why", &why)]);
                    app.set_status(text, true);
                }
                Answer::Cancelled | Answer::Message(_) => {}
            }
        }
    }

    /// Shows the next queued dialog, with the window as its parent. Call at the end of the frame.
    pub fn show_pending(&self, frame: &eframe::Frame) {
        if let Ok(mut d) = self.inner.try_borrow_mut() {
            d.show_pending(Some(frame));
        }
    }

    /// Answers every request with `answer_with` instead of a native dialog (tests).
    #[cfg(test)]
    fn set_override(&self, answer_with: craft_dialogs::Override) {
        if let Ok(mut d) = self.inner.try_borrow_mut() {
            d.set_override(Some(answer_with));
        }
    }

    /// Shows what is queued without a window (tests, with an override).
    #[cfg(test)]
    fn show_pending_headless(&self) {
        if let Ok(mut d) = self.inner.try_borrow_mut() {
            d.show_pending::<eframe::Frame>(None);
        }
    }
}

/// The native dialog for a request: kind, title, file types.
pub fn file_dialog(request: &PathRequest) -> FileDialog {
    use notecraft_ui_egui::i18n::t;
    let mut dialog = match request {
        PathRequest::Open(purpose) => FileDialog::open().title(t(match purpose {
            OpenPurpose::Notebook | OpenPurpose::NotebookFolder => "Open Notebook",
            OpenPurpose::Import => "Import",
            OpenPurpose::Picture => "Insert Pictures",
            OpenPurpose::Printout => "Insert File Printout",
            OpenPurpose::Attachment => "Insert File Attachment",
            OpenPurpose::Font => "Add Fonts",
        })),
        PathRequest::Folder(purpose) => FileDialog::folder().title(t(match purpose {
            FolderFor::OpenNotebook => "Open Notebook Folder",
            FolderFor::NewNotebook | FolderFor::DefaultLocation => "Choose a Folder for Notebooks",
        })),
        PathRequest::Save { name, .. } => FileDialog::save().title(t("Export")).file_name(name.clone()),
    };
    if let Some((name, exts)) = request.filter() {
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        dialog = dialog.filter(t(name), &exts);
    }
    dialog
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use craft_dialogs::Kind;

    use super::*;

    #[test]
    fn requests_become_dialogs() {
        let open = file_dialog(&PathRequest::Open(OpenPurpose::Notebook));
        assert_eq!(open.kind, Kind::Open { multiple: false });
        assert_eq!(open.filters.len(), 1);
        let folder = file_dialog(&PathRequest::Folder(FolderFor::OpenNotebook));
        assert_eq!(folder.kind, Kind::Folder);
        assert!(folder.filters.is_empty());
        let save = file_dialog(&PathRequest::Save { name: "Page.html".into(), id: "file.exportHtml".into(), params: serde_json::json!({}) });
        assert_eq!(save.kind, Kind::Save);
        assert_eq!(save.file_name.as_deref(), Some("Page.html"));
        assert_eq!(save.filters.len(), 1);
        assert!(file_dialog(&PathRequest::Open(OpenPurpose::Attachment)).filters.is_empty(), "any file");
    }

    #[test]
    fn answers_reach_the_app_on_a_later_frame() {
        let ctx = egui::Context::default();
        let dialogs = Dialogs::new(&ctx);
        let dir = std::env::temp_dir().join(format!("nc-desktop-dialogs-{}", std::process::id()));
        let chosen = dir.clone();
        dialogs.set_override(Box::new(move |request| match request {
            craft_dialogs::Request::File(f) if f.kind == Kind::Folder => Answer::Paths(vec![chosen.clone()]),
            craft_dialogs::Request::File(_) => Answer::Cancelled,
            craft_dialogs::Request::Message(_) => Answer::Unavailable("no".into()),
        }));
        let services = notecraft_ui_egui::Services { ask_path: Some(dialogs.hook()), ..notecraft_ui_egui::Services::default() };
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), services);
        notecraft_ui_egui::app::files::choose_folder(&mut app, FolderFor::DefaultLocation);
        notecraft_ui_egui::app::files::pick_and_open(&mut app, OpenPurpose::Notebook);
        assert_eq!(app.ui.prefs.notebooks_dir, None, "nothing until the dialog answers");
        // One dialog at a time: the folder first, then the open (cancelled).
        for _ in 0..2 {
            dialogs.show_pending_headless();
            dialogs.deliver(&mut app);
        }
        assert_eq!(app.ui.prefs.notebooks_dir.map(PathBuf::from), Some(dir));
        assert!(!app.ui.status_error, "Cancel says nothing");
    }
}
