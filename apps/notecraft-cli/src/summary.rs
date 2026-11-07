//! `info` (a JSON summary of a notebook) and `dump` (its pages as text), plus `--section` /
//! `--page` selection shared with `convert`.

use notecraft_doc::notebook::MAX_GROUP_DEPTH;

use notecraft_doc::{ElementKind, Guid, ListKind, Notebook, Page, Section, SectionEntry, Tag};
use serde_json::{Value, json};

use crate::Fail;

/// Counts of what a page holds.
#[derive(Default, Clone, Copy)]
struct Counts {
    outlines: usize,
    paragraphs: usize,
    words: usize,
    images: usize,
    printouts: usize,
    files: usize,
    opaque: usize,
    ink: usize,
}

impl Counts {
    fn of(page: &Page) -> Counts {
        let mut c = Counts { ink: page.ink.len(), words: page.title.split_whitespace().count(), ..Counts::default() };
        for e in &page.elements {
            match &e.kind {
                ElementKind::Outline(o) => {
                    c.outlines += 1;
                    c.paragraphs += o.paragraphs.len();
                    c.words += o.paragraphs.iter().map(|p| p.text().split_whitespace().count()).sum::<usize>();
                }
                ElementKind::Image(_) => c.images += 1,
                ElementKind::Printout(_) => c.printouts += 1,
                ElementKind::EmbeddedFile { .. } => c.files += 1,
                ElementKind::Opaque { .. } => c.opaque += 1,
            }
        }
        c
    }

    fn add(&mut self, o: Counts) {
        self.outlines += o.outlines;
        self.paragraphs += o.paragraphs;
        self.words += o.words;
        self.images += o.images;
        self.printouts += o.printouts;
        self.files += o.files;
        self.opaque += o.opaque;
        self.ink += o.ink;
    }

    fn json(self) -> Value {
        json!({
            "outlines": self.outlines, "paragraphs": self.paragraphs, "words": self.words, "images": self.images,
            "printouts": self.printouts, "files": self.files, "unsupportedElements": self.opaque, "inkStrokes": self.ink,
        })
    }
}

/// `info`: the notebook, its section tree and page list with counts, as JSON.
pub fn info(nb: &Notebook) -> Value {
    let mut total = Counts::default();
    let mut pages = 0usize;
    let mut groups = 0usize;
    let tree = entries_json(&nb.sections, 0, &mut total, &mut pages, &mut groups);
    let sections = nb.all_sections();
    json!({
        "name": nb.name,
        "id": nb.id,
        "color": nb.color,
        "sections": sections.len(),
        "sectionGroups": groups,
        "pages": pages,
        "totals": total.json(),
        "unsupportedContent": sections.iter().any(|s| s.source.unsupported_content),
        "entries": tree,
    })
}

fn entries_json(entries: &[SectionEntry], depth: usize, total: &mut Counts, pages: &mut usize, groups: &mut usize) -> Vec<Value> {
    if depth > MAX_GROUP_DEPTH {
        return Vec::new();
    }
    entries
        .iter()
        .map(|e| match e {
            SectionEntry::Section(s) => {
                let list: Vec<Value> = s
                    .pages
                    .iter()
                    .map(|p| {
                        let c = Counts::of(p);
                        total.add(c);
                        json!({"id": p.id, "title": p.title, "level": p.level, "created": p.created, "modified": p.modified, "counts": c.json()})
                    })
                    .collect();
                *pages += list.len();
                json!({
                    "type": "section", "id": s.id, "name": s.name, "color": s.color, "file": s.source.file_name,
                    "unsupportedContent": s.source.unsupported_content, "pages": list,
                })
            }
            SectionEntry::Group(g) => {
                *groups += 1;
                json!({"type": "group", "id": g.id, "name": g.name, "entries": entries_json(&g.entries, depth + 1, total, pages, groups)})
            }
        })
        .collect()
}

/// `dump`: every section and page with its text, readable.
pub fn dump(nb: &Notebook) -> String {
    let mut out = format!("Notebook: {} ({})\n", nb.name, nb.id);
    dump_entries(&nb.sections, "", 0, &mut out);
    out
}

fn dump_entries(entries: &[SectionEntry], prefix: &str, depth: usize, out: &mut String) {
    if depth > MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(s) => {
                let n = s.pages.len();
                out.push_str(&format!("\n== Section: {prefix}{} ({n} page{})\n", s.name, if n == 1 { "" } else { "s" }));
                for p in &s.pages {
                    dump_page(p, out);
                }
            }
            SectionEntry::Group(g) => {
                out.push_str(&format!("\n== Section group: {prefix}{}\n", g.name));
                dump_entries(&g.entries, &format!("{prefix}{} / ", g.name), depth + 1, out);
            }
        }
    }
}

fn dump_page(p: &Page, out: &mut String) {
    let indent = "  ".repeat(usize::from(p.level.min(8)));
    let c = Counts::of(p);
    let mut extras = Vec::new();
    for (n, what) in [(c.images, "picture"), (c.printouts, "printout"), (c.files, "file"), (c.ink, "ink stroke"), (c.opaque, "unsupported element")] {
        if n > 0 {
            extras.push(format!("{n} {what}{}", if n == 1 { "" } else { "s" }));
        }
    }
    let extras = if extras.is_empty() { String::new() } else { format!(" [{}]", extras.join(", ")) };
    out.push_str(&format!("\n{indent}-- {}{extras}\n", p.display_title()));
    for e in &p.elements {
        match &e.kind {
            ElementKind::Outline(o) => {
                for para in &o.paragraphs {
                    let mark = match (para.tag, para.list) {
                        (Some(Tag::ToDo { checked: true }), _) => "[x] ",
                        (Some(Tag::ToDo { checked: false }), _) => "[ ] ",
                        (_, Some(ListKind::Bullet)) => "\u{2022} ",
                        (_, Some(ListKind::Numbered)) => "# ",
                        _ => "",
                    };
                    let pad = "  ".repeat(usize::from(para.indent.min(8)));
                    out.push_str(&format!("{indent}   {pad}{mark}{}\n", para.text()));
                }
                out.push('\n');
            }
            ElementKind::Image(i) if !i.alt.is_empty() => out.push_str(&format!("{indent}   (picture: {})\n", i.alt)),
            ElementKind::Printout(pr) => out.push_str(&format!("{indent}   (printout: {}, {} pages)\n", pr.file_name, pr.pages.len())),
            ElementKind::EmbeddedFile { name, bytes, .. } => out.push_str(&format!("{indent}   (file: {name}, {} bytes)\n", bytes.len())),
            _ => {}
        }
    }
}

/// Matches a section, group or page by GUID, else by name (case-insensitive).
fn matches(id: Guid, name: &str, want: &str) -> bool {
    Guid::parse(want).map_or_else(|| name.trim().eq_ignore_ascii_case(want.trim()), |g| g == id)
}

/// Keeps only the sections named `section` (a section group keeps all its sections) and the
/// pages titled `page`. Errors when nothing is left.
pub fn select(nb: &Notebook, section: Option<&str>, page: Option<&str>) -> Result<Notebook, Fail> {
    if section.is_none() && page.is_none() {
        return Ok(nb.clone());
    }
    let mut out = nb.clone();
    out.sections = filter_entries(&nb.sections, section, page, 0);
    if out.all_sections().is_empty() {
        let what = match (section, page) {
            (Some(s), Some(p)) => format!("no page `{p}` in a section `{s}`"),
            (Some(s), None) => format!("no section `{s}`"),
            (None, Some(p)) => format!("no page `{p}`"),
            (None, None) => "nothing selected".into(),
        };
        return Err(Fail::new(format!("{what} in {}", nb.name)));
    }
    Ok(out)
}

fn filter_entries(entries: &[SectionEntry], section: Option<&str>, page: Option<&str>, depth: usize) -> Vec<SectionEntry> {
    if depth > MAX_GROUP_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for e in entries {
        match e {
            SectionEntry::Section(s) => {
                if section.is_some_and(|want| !matches(s.id, &s.name, want)) {
                    continue;
                }
                if let Some(s) = filter_pages(s, page) {
                    out.push(SectionEntry::Section(s));
                }
            }
            SectionEntry::Group(g) => {
                // A selected group keeps every section inside it.
                let inner = if section.is_some_and(|want| matches(g.id, &g.name, want)) { None } else { section };
                let kept = filter_entries(&g.entries, inner, page, depth + 1);
                if !kept.is_empty() {
                    let mut g = g.clone();
                    g.entries = kept;
                    out.push(SectionEntry::Group(g));
                }
            }
        }
    }
    out
}

fn filter_pages(s: &Section, page: Option<&str>) -> Option<Section> {
    let Some(want) = page else { return Some(s.clone()) };
    let mut s = s.clone();
    s.pages.retain(|p| matches(p.id, &p.title, want));
    if s.pages.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_counts_the_default_notebook() {
        let nb = Notebook::new_default();
        let v = info(&nb);
        assert_eq!(v["name"], "My Notebook");
        assert_eq!(v["entries"][0]["name"], "Quick Notes");
        assert!(v["pages"].as_u64().unwrap() >= 1);
        assert!(v["totals"]["words"].as_u64().unwrap() > 0);
    }

    #[test]
    fn dump_has_titles_and_text() {
        let nb = Notebook::new_default();
        let d = dump(&nb);
        assert!(d.contains("== Section: Quick Notes"));
        assert!(d.contains("-- Welcome to NoteCraft"));
    }

    #[test]
    fn selection() {
        let nb = Notebook::new_default();
        assert!(select(&nb, Some("quick notes"), None).is_ok());
        assert!(select(&nb, Some("nope"), None).is_err());
        let one = select(&nb, None, Some("Welcome to NoteCraft")).unwrap();
        assert_eq!(one.all_sections().iter().map(|s| s.pages.len()).sum::<usize>(), 1);
        let id = nb.first_section().unwrap().id.to_string();
        assert!(select(&nb, Some(&id), None).is_ok(), "by GUID");
    }
}
