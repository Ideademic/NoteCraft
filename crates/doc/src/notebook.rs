//! Notebooks, section groups and sections.
//!
//! A notebook holds an ordered tree of [`SectionEntry`]s: sections and section groups, which hold
//! more of the same. On disk (the `one` crate) a notebook is a folder with an `Open
//! Notebook.onetoc2` table of contents and one `.one` file per section; a section group is a
//! subfolder.

use serde::{Deserialize, Serialize};

use crate::page::{CONTENT_ORIGIN, ElementKind, Outline, PageElement};
use crate::{Color, Guid, InkPoint, InkStroke, InkTool, Page};

/// Deepest section-group nesting walked (hostile files): deeper entries are ignored by the
/// helpers here and should be refused by readers.
pub const MAX_GROUP_DEPTH: usize = 16;

/// A notebook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub id: Guid,
    pub name: String,
    pub color: Color,
    /// Sections and section groups, in tab order.
    pub sections: Vec<SectionEntry>,
}

/// A section or a section group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SectionEntry {
    Section(Section),
    #[serde(rename = "sectionGroup")]
    Group(SectionGroup),
}

impl SectionEntry {
    pub fn id(&self) -> Guid {
        match self {
            SectionEntry::Section(s) => s.id,
            SectionEntry::Group(g) => g.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            SectionEntry::Section(s) => &s.name,
            SectionEntry::Group(g) => &g.name,
        }
    }
}

/// A section group: a named folder of sections and more groups.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionGroup {
    pub id: Guid,
    pub name: String,
    pub entries: Vec<SectionEntry>,
}

impl SectionGroup {
    pub fn new(name: impl Into<String>) -> SectionGroup {
        SectionGroup { id: Guid::new(), name: name.into(), entries: Vec::new() }
    }
}

/// Where a section came from, for the save path's safety checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SourceInfo {
    /// The file was written by NoteCraft (or the section is new): overwriting it loses nothing.
    /// When false, the first save keeps a `.bak` of the original.
    pub authored_by_notecraft: bool,
    /// The reader met content it keeps only as [`ElementKind::Opaque`] or drops: the UI warns
    /// before saving over it.
    pub unsupported_content: bool,
    /// The `.one` file name it was read from (`Quick Notes.one`), if any.
    pub file_name: Option<String>,
}

/// A section: a tab of pages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: Guid,
    pub name: String,
    pub color: Color,
    /// In page-list order; subpages follow their parent page.
    pub pages: Vec<Page>,
    #[serde(default)]
    pub source: SourceInfo,
}

impl Section {
    /// An empty section authored by NoteCraft.
    pub fn new(name: impl Into<String>, color: Color) -> Section {
        Section {
            id: Guid::new(),
            name: name.into(),
            color,
            pages: Vec::new(),
            source: SourceInfo { authored_by_notecraft: true, ..SourceInfo::default() },
        }
    }

    pub fn page(&self, id: Guid) -> Option<&Page> {
        self.pages.iter().find(|p| p.id == id)
    }

    pub fn page_mut(&mut self, id: Guid) -> Option<&mut Page> {
        self.pages.iter_mut().find(|p| p.id == id)
    }

    pub fn page_index(&self, id: Guid) -> Option<usize> {
        self.pages.iter().position(|p| p.id == id)
    }

    /// The index range of page `i` and its subpages (those after it with a higher level).
    pub fn page_family(&self, i: usize) -> std::ops::Range<usize> {
        let Some(level) = self.pages.get(i).map(|p| p.level) else { return i..i };
        let end = self.pages.iter().skip(i + 1).position(|p| p.level <= level).map_or(self.pages.len(), |n| i + 1 + n);
        i..end
    }
}

/// Where a section group or section sits: the ids of the groups from the notebook root down.
pub type EntryPath = Vec<Guid>;

/// A search result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub section: Guid,
    pub page: Guid,
    pub page_title: String,
    /// The matching line, trimmed to about 80 characters around the match.
    pub snippet: String,
}

impl Notebook {
    /// An empty notebook.
    pub fn new(name: impl Into<String>) -> Notebook {
        Notebook { id: Guid::new(), name: name.into(), color: Color::NOTECRAFT, sections: Vec::new() }
    }

    /// The notebook a first start opens: "My Notebook" ▸ "Quick Notes" ▸ "Welcome to NoteCraft",
    /// a page with a note container and a little ink.
    pub fn new_default() -> Notebook {
        let mut page = Page::new("Welcome to NoteCraft");
        let mut outline = Outline::with_text(
            CONTENT_ORIGIN,
            "NoteCraft keeps your notes in notebooks, sections and pages.\n\
             Click anywhere on a page to start typing.\n\
             Use the Draw tab to write or sketch with a pen, pencil or highlighter.\n\
             Everything is saved automatically.",
        );
        if let Some(first) = outline.paragraphs.first_mut() {
            first.style = crate::ParaStyle::Heading2;
        }
        if let Some(p) = outline.paragraphs.get_mut(1) {
            p.tag = Some(crate::Tag::ToDo { checked: false });
        }
        outline.width = 420.0;
        page.elements.push(PageElement::new(ElementKind::Outline(outline)));
        // An underline swoosh and a highlight over the first line.
        let swoosh = (0..=24).map(|i| {
            let t = i as f32 / 24.0;
            InkPoint::new(40.0 + t * 380.0, 236.0 + (t * std::f32::consts::PI).sin() * 6.0, 0.6 + 0.4 * (t * std::f32::consts::PI).sin())
        });
        let mut pen = InkStroke::new(InkTool::Pen, swoosh.collect());
        pen.color = Color::NOTECRAFT;
        pen.width = 2.5;
        page.ink.push(pen);
        page.ink.push(InkStroke::new(InkTool::Highlighter, vec![InkPoint::new(40.0, 116.0, 1.0), InkPoint::new(260.0, 116.0, 1.0)]));
        let mut quick = Section::new("Quick Notes", Color::section_color(0));
        quick.pages.push(page);
        let mut nb = Notebook::new("My Notebook");
        nb.sections.push(SectionEntry::Section(quick));
        nb
    }

    /// Every section, depth first in tab order.
    pub fn all_sections(&self) -> Vec<&Section> {
        let mut out = Vec::new();
        collect(&self.sections, 0, &mut out);
        out
    }

    /// Every section mutably, depth first in tab order.
    pub fn all_sections_mut(&mut self) -> Vec<&mut Section> {
        let mut out = Vec::new();
        collect_mut(&mut self.sections, 0, &mut out);
        out
    }

    pub fn section(&self, id: Guid) -> Option<&Section> {
        self.all_sections().into_iter().find(|s| s.id == id)
    }

    pub fn section_mut(&mut self, id: Guid) -> Option<&mut Section> {
        self.all_sections_mut().into_iter().find(|s| s.id == id)
    }

    /// The first section in tab order.
    pub fn first_section(&self) -> Option<&Section> {
        self.all_sections().into_iter().next()
    }

    pub fn group(&self, id: Guid) -> Option<&SectionGroup> {
        find_group(&self.sections, id, 0)
    }

    pub fn group_mut(&mut self, id: Guid) -> Option<&mut SectionGroup> {
        find_group_mut(&mut self.sections, id, 0)
    }

    /// The entry list of a group, or the notebook's own (`None`).
    pub fn entries_mut(&mut self, group: Option<Guid>) -> Option<&mut Vec<SectionEntry>> {
        match group {
            None => Some(&mut self.sections),
            Some(g) => self.group_mut(g).map(|g| &mut g.entries),
        }
    }

    /// The group path down to the entry with `id` (empty for a top-level entry).
    pub fn path_of(&self, id: Guid) -> Option<EntryPath> {
        path_in(&self.sections, id, 0)
    }

    /// Removes the section or group with `id` from wherever it is.
    pub fn remove_entry(&mut self, id: Guid) -> Option<SectionEntry> {
        remove_in(&mut self.sections, id, 0)
    }

    /// The section holding page `id`, and the page.
    pub fn find_page(&self, id: Guid) -> Option<(&Section, &Page)> {
        self.all_sections().into_iter().find_map(|s| s.page(id).map(|p| (s, p)))
    }

    pub fn page_mut(&mut self, id: Guid) -> Option<&mut Page> {
        self.all_sections_mut().into_iter().find_map(|s| s.page_mut(id))
    }

    /// The id of the section holding page `id`.
    pub fn section_of_page(&self, id: Guid) -> Option<Guid> {
        self.find_page(id).map(|(s, _)| s.id)
    }

    /// Pages whose title or text contains `query` (case-insensitive), in tab and page order.
    /// An empty query finds nothing.
    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for s in self.all_sections() {
            for p in &s.pages {
                let text = p.plain_text();
                if let Some(line) = text.lines().find(|l| l.to_lowercase().contains(&q)) {
                    hits.push(SearchHit { section: s.id, page: p.id, page_title: p.title.clone(), snippet: snippet(line, &q) });
                }
            }
        }
        hits
    }
}

/// About 80 characters of `line` around the first match of `q` (already lower case).
fn snippet(line: &str, q: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= 80 {
        return line.trim().to_owned();
    }
    let lower: Vec<char> = line.to_lowercase().chars().collect();
    let qc: Vec<char> = q.chars().collect();
    // Lower-casing can change the length; fall back to the start when it did.
    let at = if lower.len() == chars.len() { lower.windows(qc.len().max(1)).position(|w| w == qc.as_slice()).unwrap_or(0) } else { 0 };
    let start = at.saturating_sub(30);
    let end = (start + 80).min(chars.len());
    let mut s: String = chars.get(start..end).map(|c| c.iter().collect()).unwrap_or_default();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

fn collect<'a>(entries: &'a [SectionEntry], depth: usize, out: &mut Vec<&'a Section>) {
    if depth > MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(s) => out.push(s),
            SectionEntry::Group(g) => collect(&g.entries, depth + 1, out),
        }
    }
}

fn collect_mut<'a>(entries: &'a mut [SectionEntry], depth: usize, out: &mut Vec<&'a mut Section>) {
    if depth > MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(s) => out.push(s),
            SectionEntry::Group(g) => collect_mut(&mut g.entries, depth + 1, out),
        }
    }
}

fn find_group(entries: &[SectionEntry], id: Guid, depth: usize) -> Option<&SectionGroup> {
    if depth > MAX_GROUP_DEPTH {
        return None;
    }
    entries.iter().find_map(|e| match e {
        SectionEntry::Group(g) if g.id == id => Some(g),
        SectionEntry::Group(g) => find_group(&g.entries, id, depth + 1),
        SectionEntry::Section(_) => None,
    })
}

fn find_group_mut(entries: &mut [SectionEntry], id: Guid, depth: usize) -> Option<&mut SectionGroup> {
    if depth > MAX_GROUP_DEPTH {
        return None;
    }
    for e in entries.iter_mut() {
        if let SectionEntry::Group(g) = e {
            if g.id == id {
                return Some(g);
            }
            if let Some(found) = find_group_mut(&mut g.entries, id, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn path_in(entries: &[SectionEntry], id: Guid, depth: usize) -> Option<EntryPath> {
    if depth > MAX_GROUP_DEPTH {
        return None;
    }
    for e in entries {
        if e.id() == id {
            return Some(Vec::new());
        }
        if let SectionEntry::Group(g) = e
            && let Some(mut p) = path_in(&g.entries, id, depth + 1)
        {
            p.insert(0, g.id);
            return Some(p);
        }
    }
    None
}

fn remove_in(entries: &mut Vec<SectionEntry>, id: Guid, depth: usize) -> Option<SectionEntry> {
    if depth > MAX_GROUP_DEPTH {
        return None;
    }
    if let Some(i) = entries.iter().position(|e| e.id() == id) {
        return Some(entries.remove(i));
    }
    for e in entries.iter_mut() {
        if let SectionEntry::Group(g) = e
            && let Some(found) = remove_in(&mut g.entries, id, depth + 1)
        {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested() -> (Notebook, Guid, Guid, Guid) {
        let mut nb = Notebook::new_default();
        let mut g = SectionGroup::new("Work");
        let mut inner = SectionGroup::new("Projects");
        let mut s = Section::new("Plans", Color::section_color(1));
        let mut p = Page::new("Roadmap");
        p.elements.push(PageElement::new(ElementKind::Outline(Outline::with_text(CONTENT_ORIGIN, "Ship the Ink Tools"))));
        let pid = p.id;
        s.pages.push(p);
        let sid = s.id;
        inner.entries.push(SectionEntry::Section(s));
        let gid = inner.id;
        g.entries.push(SectionEntry::Group(inner));
        nb.sections.push(SectionEntry::Group(g));
        (nb, sid, pid, gid)
    }

    #[test]
    fn default_notebook_shape() {
        let nb = Notebook::new_default();
        assert_eq!(nb.name, "My Notebook");
        let s = nb.first_section().unwrap();
        assert_eq!(s.name, "Quick Notes");
        assert!(s.source.authored_by_notecraft);
        let p = &s.pages[0];
        assert_eq!(p.title, "Welcome to NoteCraft");
        assert_eq!(p.elements.len(), 1);
        assert_eq!(p.ink.len(), 2);
        assert!(p.ink.iter().any(|s| s.tool == InkTool::Highlighter));
    }

    #[test]
    fn lookups_through_groups() {
        let (mut nb, sid, pid, gid) = nested();
        assert_eq!(nb.all_sections().len(), 2);
        assert_eq!(nb.section(sid).map(|s| s.name.as_str()), Some("Plans"));
        assert_eq!(nb.section_of_page(pid), Some(sid));
        assert_eq!(nb.find_page(pid).map(|(_, p)| p.title.as_str()), Some("Roadmap"));
        assert_eq!(nb.path_of(sid).map(|p| p.len()), Some(2));
        assert_eq!(nb.path_of(sid).and_then(|p| p.last().copied()), Some(gid));
        assert!(nb.group(gid).is_some());
        nb.page_mut(pid).unwrap().title = "Plan".into();
        assert_eq!(nb.find_page(pid).unwrap().1.title, "Plan");
        nb.entries_mut(Some(gid)).unwrap().push(SectionEntry::Section(Section::new("More", Color::WHITE)));
        assert_eq!(nb.all_sections().len(), 3);
        assert!(matches!(nb.remove_entry(sid), Some(SectionEntry::Section(_))));
        assert_eq!(nb.section(sid), None);
        assert!(nb.remove_entry(Guid::new()).is_none());
    }

    #[test]
    fn search_finds_text_and_titles() {
        let (nb, sid, pid, _) = nested();
        let hits = nb.search("ink tools");
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].section, hits[0].page), (sid, pid));
        assert_eq!(hits[0].snippet, "Ship the Ink Tools");
        assert_eq!(nb.search("welcome").len(), 1);
        assert!(nb.search("   ").is_empty());
        let long = format!("{}needle{}", "a".repeat(100), "b".repeat(100));
        let s = snippet(&long, "needle");
        assert!(s.contains("needle") && s.starts_with('…') && s.ends_with('…'));
    }

    #[test]
    fn page_families() {
        let mut s = Section::new("S", Color::WHITE);
        for (i, level) in [0u8, 1, 2, 0, 1].into_iter().enumerate() {
            let mut p = Page::new(format!("p{i}"));
            p.level = level;
            s.pages.push(p);
        }
        assert_eq!(s.page_family(0), 0..3);
        assert_eq!(s.page_family(1), 1..3);
        assert_eq!(s.page_family(3), 3..5);
        assert_eq!(s.page_family(9), 9..9);
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut entries = vec![SectionEntry::Section(Section::new("deep", Color::WHITE))];
        for _ in 0..(MAX_GROUP_DEPTH + 5) {
            let mut g = SectionGroup::new("g");
            g.entries = entries;
            entries = vec![SectionEntry::Group(g)];
        }
        let nb = Notebook { sections: entries, ..Notebook::new("x") };
        assert!(nb.all_sections().is_empty());
    }

    #[test]
    fn json_round_trip() {
        let (nb, ..) = nested();
        let j = serde_json::to_string(&nb).unwrap();
        assert!(j.contains("\"type\":\"sectionGroup\""));
        assert_eq!(serde_json::from_str::<Notebook>(&j).unwrap(), nb);
    }
}
