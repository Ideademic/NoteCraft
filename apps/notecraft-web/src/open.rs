//! Notebooks from files the browser handed over (the picker, drag and drop): a `.onepkg` or a
//! NoteCraft `.json` opens on its own; `.one` sections picked together (with their
//! `Open Notebook.onetoc2`, if picked too) become one notebook; a single `.one` opens as a
//! notebook of that section, as on the desktop.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use notecraft_doc::Notebook;
use notecraft_engine::{Origin, Session, ids};
use notecraft_one::{NotebookFile, TOC_FILE_NAME};
use notecraft_ui_egui::OpenPurpose;
use serde_json::json;

/// Most bytes of one file read from the browser (the engine's own cap on file input).
pub const MAX_FILE_BYTES: usize = 512 << 20;
/// Most bytes one pick or drop reads in all.
pub const MAX_BATCH_BYTES: usize = 1 << 30;
/// Most files one pick or drop takes.
pub const MAX_FILES: usize = 2000;

/// What the shell opens as a notebook (the rest goes to the app's inbox).
pub fn is_notebook_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".one", ".onetoc2", ".onepkg", ".json"].iter().any(|ext| lower.ends_with(ext))
}

/// Where a dropped file goes: notebooks open, PDFs become printouts, pictures pictures,
/// anything else an attachment.
pub fn purpose_for_drop(name: &str) -> OpenPurpose {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        _ if is_notebook_file(name) => OpenPurpose::Notebook,
        "pdf" => OpenPurpose::Printout,
        "png" | "jpg" | "jpeg" | "jfif" | "gif" | "bmp" | "webp" => OpenPurpose::Picture,
        _ => OpenPurpose::Attachment,
    }
}

/// The notebooks in `files` (name, bytes), and what couldn't be opened.
pub fn notebooks_from_files(files: Vec<(String, Vec<u8>)>) -> (Vec<Notebook>, Vec<String>) {
    let mut notebooks = Vec::new();
    let mut problems = Vec::new();
    let mut parts: Vec<NotebookFile> = Vec::new();
    for (name, bytes) in files {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".one") || lower.ends_with(".onetoc2") {
            // A table of contents picked from a notebook folder has OneNote's fixed name.
            let path = if lower.ends_with(".onetoc2") { TOC_FILE_NAME.to_owned() } else { name };
            parts.push(NotebookFile { path, bytes });
            continue;
        }
        match notecraft_engine::io::open_bytes(&name, &bytes) {
            Ok(nb) => notebooks.push(nb),
            Err(e) => problems.push(format!("Couldn't open {name}: {e}")),
        }
    }
    let sections = parts.iter().filter(|f| f.path.to_ascii_lowercase().ends_with(".one")).count();
    match (sections, parts.first()) {
        (0, Some(_)) => problems.push(format!("{TOC_FILE_NAME} has no sections on its own: choose it together with the notebook's .one files.")),
        (1, _) if parts.len() == 1 => {
            if let Some(f) = parts.first() {
                match notecraft_engine::io::open_bytes(&f.path, &f.bytes) {
                    Ok(nb) => notebooks.push(nb),
                    Err(e) => problems.push(format!("Couldn't open {}: {e}", f.path)),
                }
            }
        }
        (n, _) if n > 0 => match notecraft_one::notebook_from_files(&parts, "Notebook") {
            Ok((nb, skipped)) => {
                if skipped > 0 {
                    problems.push(format!("{skipped} of {n} sections couldn't be read."));
                }
                if nb.all_sections().is_empty() {
                    problems.push("None of the sections could be read.".to_owned());
                } else {
                    notebooks.push(nb);
                }
            }
            Err(e) => problems.push(format!("Couldn't open the notebook: {e}")),
        },
        _ => {}
    }
    (notebooks, problems)
}

/// Opens `files` in the session (after the open notebooks) and goes to the last one. A notebook
/// already open is gone to instead. Returns the status line to show, if any, and whether it is
/// an error.
pub fn open_into(session: &mut Session, files: Vec<(String, Vec<u8>)>) -> Option<(String, bool)> {
    let (notebooks, mut problems) = notebooks_from_files(files);
    let mut opened = Vec::new();
    for nb in notebooks {
        let name = nb.name.clone();
        if session.notebook(nb.id).is_some() {
            let _ = session.run(ids::NAV_NOTEBOOK, &json!({ "id": nb.id }));
            problems.push(format!("\u{201c}{name}\u{201d} is already open."));
            continue;
        }
        let id = session.add_notebook(nb, Origin::Scratch);
        // Kept in the browser from the next autosave on.
        session.mark_notebook_dirty(id);
        opened.push(name);
    }
    match (opened.as_slice(), problems.is_empty()) {
        (_, false) => Some((problems.join(" "), opened.is_empty())),
        ([], true) => None,
        ([one], true) => Some((format!("Opened \u{201c}{one}\u{201d}."), false)),
        (many, true) => Some((format!("Opened {} notebooks.", many.len()), false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section_file(name: &str) -> Vec<u8> {
        let s = notecraft_doc::Section::new(name, notecraft_doc::Color::NOTECRAFT);
        notecraft_one::write_section(&s).unwrap()
    }

    #[test]
    fn routes() {
        assert_eq!(purpose_for_drop("A.ONE"), OpenPurpose::Notebook);
        assert_eq!(purpose_for_drop("nb.json"), OpenPurpose::Notebook);
        assert_eq!(purpose_for_drop("scan.pdf"), OpenPurpose::Printout);
        assert_eq!(purpose_for_drop("cat.webp"), OpenPurpose::Picture);
        assert_eq!(purpose_for_drop("notes.docx"), OpenPurpose::Attachment);
        assert_eq!(purpose_for_drop("noext"), OpenPurpose::Attachment);
    }

    #[test]
    fn sections_picked_together_are_one_notebook() {
        let nb = Notebook::new_default();
        let mut files: Vec<(String, Vec<u8>)> =
            notecraft_one::notebook_files(&nb).unwrap().into_iter().map(|f| (f.path.rsplit('/').next().unwrap_or("").to_owned(), f.bytes)).collect();
        files.push(("Extra.one".into(), section_file("Extra")));
        let (got, problems) = notebooks_from_files(files);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "My Notebook", "named by its table of contents");
        assert_eq!(got[0].all_sections().len(), 2);
    }

    #[test]
    fn single_files_and_failures() {
        let (got, _) = notebooks_from_files(vec![("Ideas.one".into(), section_file("Ideas"))]);
        assert_eq!(got.len(), 1);
        let pkg = notecraft_onepkg::write(&Notebook::new_default()).unwrap();
        let json = serde_json::to_vec(&Notebook::new_default()).unwrap();
        let (got, problems) = notebooks_from_files(vec![
            ("a.onepkg".into(), pkg),
            ("b.json".into(), json),
            ("bad.onepkg".into(), b"junk".to_vec()),
            ("Open Notebook.onetoc2".into(), b"junk".to_vec()),
        ]);
        assert_eq!(got.len(), 2);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn opening_into_a_session() {
        let mut s = Session::new_default();
        let mut work = Notebook::new_default();
        work.name = "Work".into();
        let pkg = notecraft_onepkg::write(&work).unwrap();
        let msg = open_into(&mut s, vec![("Work.onepkg".into(), pkg.clone())]);
        assert_eq!(msg, Some(("Opened \u{201c}Work\u{201d}.".into(), false)));
        assert_eq!(s.notebooks.len(), 2);
        assert_eq!(s.current_notebook().map(|n| n.name.as_str()), Some("Work"));
        assert!(s.is_dirty(), "kept in the browser at the next autosave");
        let again = open_into(&mut s, vec![("Work.onepkg".into(), pkg)]);
        assert!(again.is_some_and(|(m, err)| m.contains("already open") && err));
        assert_eq!(s.notebooks.len(), 2);
        assert_eq!(open_into(&mut s, Vec::new()), None);
    }
}
