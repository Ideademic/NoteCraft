//! Where each section of a notebook lives inside its folder: `Quick Notes.one`,
//! `Work/Plans.one` (a section group is a subfolder). Names follow the section and group names
//! (OneNote shows a section by its file name), made safe for every file system and unique
//! within each folder (`Notes (2).one`). A previous layout keeps its numbering stable when
//! sections are reordered.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use notecraft_doc::notebook::MAX_GROUP_DEPTH;
use notecraft_doc::{Guid, Notebook, Section, SectionEntry};

/// Section id → its file, relative to the notebook folder.
pub type Layout = BTreeMap<Guid, PathBuf>;

/// One folder level of a laid-out notebook: its relative path, its table of contents' entries
/// in order.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderPlan {
    /// Relative to the notebook folder (empty for the notebook folder itself).
    pub dir: PathBuf,
    /// The group (`None` for the notebook itself).
    pub group: Option<(Guid, String)>,
    pub entries: Vec<notecraft_one::TocEntry>,
}

/// A section's file name before de-duplication: the file it was read from, else one made from
/// its name ([`notecraft_one::section_file_name`]). Renaming a section forgets the old file
/// name, so the file follows the new name.
pub fn base_file_name(sec: &Section) -> String {
    notecraft_one::section_file_name(sec)
}

/// `name` → `name (n)` keeping the extension: `Notes.one` → `Notes (2).one`.
fn numbered(name: &str, n: usize, ext: bool) -> String {
    match (ext, name.rsplit_once('.')) {
        (true, Some((s, e))) => format!("{s} ({n}).{e}"),
        _ => format!("{name} ({n})"),
    }
}

/// Whether `candidate` is `base` or one of its numbered forms.
fn is_form_of(candidate: &str, base: &str, ext: bool) -> bool {
    if candidate.eq_ignore_ascii_case(base) {
        return true;
    }
    (2..1000).any(|n| candidate.eq_ignore_ascii_case(&numbered(base, n, ext)))
}

/// The first of `base`, `base (2)`… not in `used` (case-insensitive).
fn free(base: &str, used: &HashSet<String>, ext: bool) -> String {
    if !used.contains(&base.to_lowercase()) {
        return base.to_owned();
    }
    (2..100_000).map(|n| numbered(base, n, ext)).find(|c| !used.contains(&c.to_lowercase())).unwrap_or_else(|| format!("{base}-{}", Guid::new()))
}

/// Lays out `nb`: every section's file and every folder's table of contents. `previous` keeps
/// existing numbered names; `pin` forces one section's file name (a single `.one` opened on its
/// own keeps its name).
pub fn plan(nb: &Notebook, previous: Option<&Layout>, pin: Option<(Guid, &str)>) -> (Layout, Vec<FolderPlan>) {
    let mut w = Walker { previous, pin, layout: Layout::new(), folders: Vec::new() };
    w.walk(&nb.sections, Path::new(""), None, 0);
    (w.layout, w.folders)
}

struct Walker<'a> {
    previous: Option<&'a Layout>,
    pin: Option<(Guid, &'a str)>,
    layout: Layout,
    folders: Vec<FolderPlan>,
}

impl Walker<'_> {
    fn walk(&mut self, entries: &[SectionEntry], dir: &Path, group: Option<(Guid, String)>, depth: usize) {
        let (previous, pin) = (self.previous, self.pin);
        if depth > MAX_GROUP_DEPTH {
            return;
        }
        let mut used: HashSet<String> = HashSet::new();
        used.insert(notecraft_one::TOC_FILE_NAME.to_lowercase());
        used.insert(super::RECYCLE_BIN.to_lowercase());
        let mut names: Vec<Option<String>> = vec![None; entries.len()];
        // Pass 1: pinned names, and earlier names that still fit, are kept.
        for (i, e) in entries.iter().enumerate() {
            let SectionEntry::Section(sec) = e else { continue };
            let base = base_file_name(sec);
            let keep = match pin {
                Some((id, name)) if id == sec.id => Some(name.to_owned()),
                _ => previous
                    .and_then(|p| p.get(&sec.id))
                    .filter(|p| p.parent().unwrap_or(Path::new("")) == dir)
                    .and_then(|p| p.file_name())
                    .map(|f| f.to_string_lossy().into_owned())
                    .filter(|f| is_form_of(f, &base, true)),
            };
            if let Some(k) = keep
                && used.insert(k.to_lowercase())
                && let Some(slot) = names.get_mut(i)
            {
                *slot = Some(k);
            }
        }
        // Pass 2: everything else gets the first free name.
        let mut toc = Vec::new();
        let mut subgroups = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            match e {
                SectionEntry::Section(sec) => {
                    let name = match names.get(i).cloned().flatten() {
                        Some(n) => n,
                        None => {
                            let n = free(&base_file_name(sec), &used, true);
                            used.insert(n.to_lowercase());
                            n
                        }
                    };
                    self.layout.insert(sec.id, dir.join(&name));
                    toc.push(notecraft_one::TocEntry::Section { file_name: name, color: Some(sec.color) });
                }
                SectionEntry::Group(g) => {
                    let n = free(&notecraft_one::safe_file_stem(&g.name), &used, false);
                    used.insert(n.to_lowercase());
                    toc.push(notecraft_one::TocEntry::Group { folder_name: n.clone() });
                    subgroups.push((g, n));
                }
            }
        }
        self.folders.push(FolderPlan { dir: dir.to_path_buf(), group, entries: toc });
        for (g, n) in subgroups {
            self.walk(&g.entries, &dir.join(&n), Some((g.id, g.name.clone())), depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use notecraft_doc::{Color, SectionGroup};

    use super::*;

    #[test]
    fn names_are_unique_and_stable() {
        let mut nb = Notebook::new("N");
        let a = Section::new("Notes", Color::WHITE);
        let b = Section::new("notes", Color::WHITE);
        let mut c = Section::new("Renamed", Color::WHITE);
        c.source.file_name = Some("Old Name.one".into());
        let mut g = SectionGroup::new("Work/Stuff");
        let inner = Section::new("Plans", Color::WHITE);
        let (ia, ib, ic, ii) = (a.id, b.id, c.id, inner.id);
        g.entries.push(SectionEntry::Section(inner));
        nb.sections = vec![SectionEntry::Section(a), SectionEntry::Section(b), SectionEntry::Section(c), SectionEntry::Group(g)];
        let (l, folders) = plan(&nb, None, None);
        assert_eq!(l[&ia], PathBuf::from("Notes.one"));
        assert_eq!(l[&ib], PathBuf::from("notes (2).one"));
        assert_eq!(l[&ic], PathBuf::from("Old Name.one"), "a section keeps the file it was read from");
        assert_eq!(l[&ii], PathBuf::from("Work_Stuff").join("Plans.one"));
        assert_eq!(folders.len(), 2);
        assert!(matches!(&folders[0].entries[3], notecraft_one::TocEntry::Group { folder_name } if folder_name == "Work_Stuff"));
        // Swapping the two "Notes" keeps their files.
        nb.sections.swap(0, 1);
        let (l2, _) = plan(&nb, Some(&l), None);
        assert_eq!(l2[&ia], l[&ia]);
        assert_eq!(l2[&ib], l[&ib]);
        let (l3, _) = plan(&nb, None, Some((ic, "keep.one")));
        assert_eq!(l3[&ic], PathBuf::from("keep.one"));
    }
}
