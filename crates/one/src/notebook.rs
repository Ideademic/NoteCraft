//! Notebooks as sets of files (relative paths with `/`): what a notebook folder and a `.onepkg`
//! hold. `dir.rs` and the onepkg crate move these files to and from disk or a cabinet.

use std::collections::HashSet;

use notecraft_doc::notebook::MAX_GROUP_DEPTH;
use notecraft_doc::{Guid, Notebook, SectionEntry, SectionGroup};

use crate::{
    Error, NotebookToc, Result, SECTION_EXT, TOC_FILE_NAME, TocEntry, read_section_named, read_toc_ids, safe_file_stem, section_file_name, toc,
    write_section_with,
};

/// A file of a notebook: its path relative to the notebook folder, `/`-separated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotebookFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Folder names OneNote keeps for itself in a notebook folder (its recycle bin).
fn is_reserved_folder(name: &str) -> bool {
    name.eq_ignore_ascii_case("OneNote_RecycleBin")
}

/// Every file a notebook is saved as: `Open Notebook.onetoc2`, a `.one` per section and a
/// subfolder per section group.
pub fn notebook_files(notebook: &Notebook) -> Result<Vec<NotebookFile>> {
    let mut out = Vec::new();
    let top = Level { prefix: String::new(), toc_id: notebook.id, name: notebook.name.clone(), ancestor: Guid::NIL, depth: 0 };
    folder_files(&notebook.sections, &top, notebook.color, notebook.history_disabled, &mut out)?;
    out.extend(crate::recycle_bin_files(&notebook.recycle_bin)?);
    Ok(out)
}

/// One folder level being written: its path prefix, table of contents id and name, and the
/// parent table of contents.
struct Level {
    prefix: String,
    toc_id: Guid,
    name: String,
    ancestor: Guid,
    depth: usize,
}

fn folder_files(entries: &[SectionEntry], lv: &Level, color: notecraft_doc::Color, no_history: bool, out: &mut Vec<NotebookFile>) -> Result<()> {
    if lv.depth > MAX_GROUP_DEPTH {
        return Err(Error::Format("section groups nested too deeply".into()));
    }
    let mut used = HashSet::new();
    let mut toc_entries = Vec::with_capacity(entries.len());
    let mut ids = Vec::with_capacity(entries.len());
    for e in entries {
        match e {
            SectionEntry::Section(s) => {
                let file = unique(&section_file_name(s), &mut used);
                out.push(NotebookFile { path: format!("{}{file}", lv.prefix), bytes: write_section_with(s, &file, lv.toc_id)? });
                toc_entries.push(TocEntry::Section { file_name: file, color: Some(s.color) });
                ids.push(s.id);
            }
            SectionEntry::Group(g) => {
                let folder = unique(&safe_file_stem(&g.name), &mut used);
                let gid = if g.id.is_nil() { Guid::new() } else { g.id };
                let inner =
                    Level { prefix: format!("{}{folder}/", lv.prefix), toc_id: gid, name: g.name.clone(), ancestor: lv.toc_id, depth: lv.depth + 1 };
                folder_files(&g.entries, &inner, color, false, out)?;
                toc_entries.push(TocEntry::Group { folder_name: folder });
                ids.push(gid);
            }
        }
    }
    let t = NotebookToc { id: lv.toc_id, name: lv.name.clone(), color, entries: toc_entries, history_disabled: no_history && lv.depth == 0 };
    out.push(NotebookFile { path: format!("{}{TOC_FILE_NAME}", lv.prefix), bytes: toc::write(&t, &ids, lv.ancestor)? });
    Ok(())
}

/// `name`, made unique among `used` (case-insensitively) with " (2)", " (3)"… before the
/// extension.
fn unique(name: &str, used: &mut HashSet<String>) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_owned(), format!(".{e}")),
        _ => (name.to_owned(), String::new()),
    };
    let mut candidate = name.to_owned();
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) || candidate.eq_ignore_ascii_case(TOC_FILE_NAME) || is_reserved_folder(&candidate) {
        candidate = format!("{stem} ({n}){ext}");
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

/// Builds a notebook from its files. `name` is used when the table of contents doesn't name
/// it (the folder or package name). Unreadable sections are skipped with a warning in the log;
/// the result says how many through `skipped`.
pub fn notebook_from_files(files: &[NotebookFile], name: &str) -> Result<(Notebook, usize)> {
    let paths: Vec<Vec<&str>> = files.iter().map(|f| f.path.split(['/', '\\']).filter(|p| !p.is_empty()).collect()).collect();
    // Packages may wrap the notebook in a single top folder.
    let prefix = common_top_folder(&paths);
    let mut items: Vec<(Vec<&str>, &[u8])> =
        paths.iter().zip(files).map(|(p, f)| (p.get(prefix..).unwrap_or_default().to_vec(), f.bytes.as_slice())).collect();
    items.retain(|(p, _)| !p.is_empty() && !p.iter().any(|c| *c == ".." || *c == "."));
    let mut skipped = 0;
    let (entries, toc) = folder(&items, 0, &mut skipped)?;
    // The recycle bin: the `.one` files directly in its folder.
    let bin_files: Vec<(&str, &[u8])> = items
        .iter()
        .filter_map(|(p, b)| match p.as_slice() {
            [dir, file] if is_reserved_folder(dir) => Some((*file, *b)),
            _ => None,
        })
        .collect();
    let notebook = Notebook {
        id: toc.as_ref().map_or_else(Guid::new, |t| t.id),
        name: toc.as_ref().map(|t| t.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| name.to_owned()),
        color: toc.as_ref().map_or(notecraft_doc::Color::NOTECRAFT, |t| t.color),
        sections: entries,
        recycle_bin: crate::read_recycle_bin(&bin_files),
        history_disabled: toc.as_ref().is_some_and(|t| t.history_disabled),
    };
    Ok((notebook, skipped))
}

fn common_top_folder(paths: &[Vec<&str>]) -> usize {
    let has_root_toc = paths.iter().any(|p| p.len() == 1 && p.first().is_some_and(|n| n.eq_ignore_ascii_case(TOC_FILE_NAME)));
    if has_root_toc || paths.is_empty() {
        return 0;
    }
    let first = paths.first().and_then(|p| p.first());
    let same = paths.iter().all(|p| p.len() > 1 && p.first() == first);
    usize::from(same)
}

type Items<'a> = [(Vec<&'a str>, &'a [u8])];

/// The entries of one folder level (`items` paths are relative to it).
fn folder(items: &Items<'_>, depth: usize, skipped: &mut usize) -> Result<(Vec<SectionEntry>, Option<NotebookToc>)> {
    if depth > MAX_GROUP_DEPTH {
        return Ok((Vec::new(), None));
    }
    let toc = items
        .iter()
        .find(|(p, _)| p.len() == 1 && p.first().is_some_and(|n| n.eq_ignore_ascii_case(TOC_FILE_NAME)))
        .and_then(|(_, b)| read_toc_ids(b).map_err(|e| log::warn!("table of contents unreadable: {e}")).ok());
    let mut sections: Vec<(String, SectionEntry)> = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    for (p, bytes) in items {
        match p.as_slice() {
            [file] if file.starts_with('.') || file.to_ascii_lowercase().ends_with(".bak") => {}
            [file] if file.to_ascii_lowercase().ends_with(&format!(".{SECTION_EXT}")) => {
                let stem = file.get(..file.len() - SECTION_EXT.len() - 1).unwrap_or(file);
                match read_section_named(bytes, stem) {
                    Ok(mut s) => {
                        // In a notebook folder a section is named after its file.
                        s.name = stem.to_owned();
                        s.source.file_name = Some((*file).to_owned());
                        sections.push(((*file).to_owned(), SectionEntry::Section(s)));
                    }
                    Err(e) => {
                        log::warn!("section {file} skipped: {e}");
                        *skipped += 1;
                    }
                }
            }
            [dir, _, ..] if !is_reserved_folder(dir) && !folders.iter().any(|f| f == dir) => folders.push((*dir).to_owned()),
            _ => {}
        }
    }
    for dir in folders {
        let inner: Vec<(Vec<&str>, &[u8])> = items
            .iter()
            .filter(|(p, _)| p.len() > 1 && p.first() == Some(&dir.as_str()))
            .map(|(p, b)| (p.get(1..).unwrap_or_default().to_vec(), *b))
            .collect();
        let (entries, gtoc) = folder(&inner, depth + 1, skipped)?;
        let id = gtoc.as_ref().map_or_else(Guid::new, |t| t.id);
        sections.push((dir.clone(), SectionEntry::Group(SectionGroup { id, name: dir, entries })));
    }
    // Table-of-contents order first, then the rest by name.
    let (toc, ids) = match toc {
        Some((t, ids)) => (Some(t), ids),
        None => (None, Vec::new()),
    };
    let rank = |name: &str, entry: &SectionEntry| -> usize {
        let Some(t) = &toc else { return usize::MAX };
        t.entries
            .iter()
            .enumerate()
            .position(|(i, e)| {
                let by_name = match e {
                    TocEntry::Section { file_name, .. } => file_name.eq_ignore_ascii_case(name),
                    TocEntry::Group { folder_name } => folder_name.eq_ignore_ascii_case(name),
                };
                by_name || ids.get(i).copied().flatten().is_some_and(|g| g == entry.id() && matches!(entry, SectionEntry::Section(_)))
            })
            .unwrap_or(usize::MAX)
    };
    sections.sort_by(|(a, ea), (b, eb)| rank(a, ea).cmp(&rank(b, eb)).then_with(|| a.to_lowercase().cmp(&b.to_lowercase())));
    Ok((sections.into_iter().map(|(_, e)| e).collect(), toc))
}
