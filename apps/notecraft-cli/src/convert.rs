//! Opening any notebook input and writing a notebook by the output's extension.

use std::path::Path;

use notecraft_doc::{Notebook, Section};

use crate::Fail;

/// What `convert` (and `run --save`) can write, chosen by the output path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutKind {
    Onepkg,
    Json,
    One,
    Html,
    Markdown,
    Text,
    /// A notebook folder: `Open Notebook.onetoc2` plus one `.one` per section.
    Folder,
}

pub const OUTPUTS: &str = ".onepkg, .json, .one, .html, .md, .txt, or a folder (a path ending in / or Open Notebook.onetoc2)";

impl OutKind {
    pub fn of(out: &str) -> Result<OutKind, Fail> {
        if out.ends_with('/') || out.ends_with('\\') || Path::new(out).is_dir() {
            return Ok(OutKind::Folder);
        }
        let file = out.rsplit(['/', '\\']).next().unwrap_or(out);
        let Some((_, ext)) = file.rsplit_once('.') else {
            return Err(Fail::usage(format!("`{out}` has no extension: write {OUTPUTS}")));
        };
        match ext.to_ascii_lowercase().as_str() {
            "onepkg" => Ok(OutKind::Onepkg),
            "json" => Ok(OutKind::Json),
            "one" => Ok(OutKind::One),
            "html" | "htm" => Ok(OutKind::Html),
            "md" | "markdown" => Ok(OutKind::Markdown),
            "txt" => Ok(OutKind::Text),
            "onetoc2" => Ok(OutKind::Folder),
            e => Err(Fail::usage(format!("can't write `.{}` files: write {OUTPUTS}", e.chars().take(20).collect::<String>()))),
        }
    }
}

/// Opens a notebook folder, `Open Notebook.onetoc2`, `.onepkg`, `.one` or NoteCraft `.json`.
pub fn open_input(path: &str) -> Result<Notebook, Fail> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(Fail::new(format!("{path}: no such file or folder")));
    }
    if p.is_file() && path.to_ascii_lowercase().ends_with(".json") {
        let bytes = std::fs::read(p).map_err(|e| Fail::new(format!("{path}: {e}")))?;
        return notecraft_engine::io::open_bytes(path, &bytes).map_err(|e| Fail::new(format!("{path}: {e}")));
    }
    notecraft_engine::io::open_path(p).map_err(|e| match e {
        // Already names the file.
        notecraft_engine::IoError::Io(m) => Fail::new(m),
        e => Fail::new(format!("{path}: {e}")),
    })
}

/// Every page of the notebook, as one section named after it (one HTML file with a table of
/// contents, one Markdown or text file).
fn all_pages(nb: &Notebook) -> Section {
    let sections = nb.all_sections();
    if let [only] = sections.as_slice() {
        return (*only).clone();
    }
    let mut s = Section::new(nb.name.clone(), nb.color);
    s.pages = sections.iter().flat_map(|sec| sec.pages.iter().cloned()).collect();
    s
}

fn join_pages(nb: &Notebook, sep: &str, one: fn(&notecraft_doc::Page) -> notecraft_formats::Result<String>) -> Result<String, Fail> {
    let parts = all_pages(nb).pages.iter().map(one).collect::<Result<Vec<_>, _>>().map_err(|e| Fail::new(e.to_string()))?;
    let mut text = parts.join(sep);
    text.push('\n');
    Ok(text)
}

/// The bytes of `nb` as `kind` (not a folder).
pub fn to_bytes(nb: &Notebook, kind: OutKind) -> Result<Vec<u8>, Fail> {
    let fail = |e: &dyn std::fmt::Display| Fail::new(e.to_string());
    Ok(match kind {
        OutKind::Onepkg => notecraft_onepkg::write(nb).map_err(|e| fail(&e))?,
        OutKind::Json => {
            let mut v = serde_json::to_vec_pretty(nb).map_err(|e| fail(&e))?;
            v.push(b'\n');
            v
        }
        OutKind::One => {
            let sections = nb.all_sections();
            let section = match sections.as_slice() {
                [only] => *only,
                [] => return Err(Fail::new("the notebook has no sections")),
                many => {
                    let names: Vec<&str> = many.iter().map(|s| s.name.as_str()).collect();
                    return Err(Fail::usage(format!(
                        "a .one file holds one section; choose one with --section (or write a folder): {}",
                        names.join(", ")
                    )));
                }
            };
            notecraft_one::write_section(section).map_err(|e| fail(&e))?
        }
        OutKind::Html => notecraft_formats::section_to_html(&all_pages(nb)).map_err(|e| fail(&e))?.into_bytes(),
        OutKind::Markdown => join_pages(nb, "\n\n---\n\n", notecraft_formats::page_to_markdown)?.into_bytes(),
        OutKind::Text => join_pages(nb, "\n\n", notecraft_formats::page_to_text)?.into_bytes(),
        OutKind::Folder => return Err(Fail::new("a folder isn't one file")),
    })
}

/// Writes `nb` to `out` by its extension (a folder for a path ending in `/` or `.onetoc2`, or an
/// existing folder). Returns what was written, for the message.
pub fn write(nb: &Notebook, out: &str) -> Result<String, Fail> {
    let kind = OutKind::of(out)?;
    if kind == OutKind::Folder {
        let dir = if out.to_ascii_lowercase().ends_with(".onetoc2") { Path::new(out).parent().unwrap_or(Path::new(".")) } else { Path::new(out) };
        notecraft_one::write_notebook_dir(nb, dir).map_err(|e| Fail::new(format!("{}: {e}", dir.display())))?;
        let n = nb.all_sections().len();
        return Ok(format!("{} ({n} section{})", dir.display(), if n == 1 { "" } else { "s" }));
    }
    let bytes = to_bytes(nb, kind)?;
    if let Some(parent) = Path::new(out).parent().filter(|p| !p.as_os_str().is_empty() && !p.exists()) {
        std::fs::create_dir_all(parent).map_err(|e| Fail::new(format!("{}: {e}", parent.display())))?;
    }
    std::fs::write(out, &bytes).map_err(|e| Fail::new(format!("{out}: {e}")))?;
    Ok(format!("{out} ({} bytes)", bytes.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(OutKind::of("a/b.ONEPKG").unwrap(), OutKind::Onepkg);
        assert_eq!(OutKind::of("x.md").unwrap(), OutKind::Markdown);
        assert_eq!(OutKind::of("out/").unwrap(), OutKind::Folder);
        assert_eq!(OutKind::of("nb/Open Notebook.onetoc2").unwrap(), OutKind::Folder);
        assert!(matches!(OutKind::of("x.docx"), Err(Fail { code: 2, .. })));
        assert!(matches!(OutKind::of("noext"), Err(Fail { code: 2, .. })));
    }

    #[test]
    fn bytes_of_every_kind() {
        let nb = Notebook::new_default();
        for k in [OutKind::Onepkg, OutKind::Json, OutKind::One, OutKind::Html, OutKind::Markdown, OutKind::Text] {
            assert!(!to_bytes(&nb, k).unwrap().is_empty(), "{k:?}");
        }
        let mut two = nb.clone();
        two.sections.push(notecraft_doc::SectionEntry::Section(Section::new("B", notecraft_doc::Color::NOTECRAFT)));
        assert!(matches!(to_bytes(&two, OutKind::One), Err(Fail { code: 2, .. })));
        let html = String::from_utf8(to_bytes(&two, OutKind::Html).unwrap()).unwrap();
        assert!(html.contains("Welcome to NoteCraft"));
    }
}
