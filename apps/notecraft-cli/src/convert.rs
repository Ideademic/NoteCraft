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
    /// A PDF (`notecraft-pdf`).
    Pdf,
    /// A notebook folder: `Open Notebook.onetoc2` plus one `.one` per section.
    Folder,
}

pub const OUTPUTS: &str = ".onepkg, .json, .one, .html, .md, .txt, .pdf, or a folder (a path ending in / or Open Notebook.onetoc2)";

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
            "pdf" => Ok(OutKind::Pdf),
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
        OutKind::Pdf => pdf_bytes(nb, &PdfArgs::default())?,
        OutKind::Folder => return Err(Fail::new("a folder isn't one file")),
    })
}

/// Writes `nb` to `out` by its extension (a folder for a path ending in `/` or `.onetoc2`, or an
/// existing folder). Returns what was written, for the message.
pub fn write(nb: &Notebook, out: &str) -> Result<String, Fail> {
    write_with(nb, out, &PdfArgs::default())
}

/// How `convert` writes a PDF (`--scope`, `--paper`, `--landscape`, `--no-header`,
/// `--bundled-fonts`).
#[derive(Clone, Debug, Default)]
pub struct PdfArgs {
    /// `page` (the first page), `section` (the first section) or `notebook` (default: every page).
    pub scope: Option<String>,
    pub options: notecraft_pdf::Options,
    pub bundled_fonts: bool,
}

impl PdfArgs {
    /// From `convert`'s options.
    pub fn parse(scope: Option<&str>, paper: Option<&str>, landscape: bool, no_header: bool, bundled: bool) -> Result<PdfArgs, Fail> {
        if let Some(s) = scope
            && !matches!(s, "page" | "section" | "notebook")
        {
            return Err(Fail::usage(format!("--scope is page, section or notebook, not `{}`", s.chars().take(20).collect::<String>())));
        }
        let mut options = notecraft_pdf::Options::default();
        if let Some(p) = paper {
            options.paper = notecraft_pdf::Paper::parse(p)
                .ok_or_else(|| Fail::usage(format!("--paper is fit, letter, a4, legal or a5, not `{}`", p.chars().take(20).collect::<String>())))?;
        }
        options.landscape = landscape;
        options.header = !no_header;
        Ok(PdfArgs { scope: scope.map(str::to_owned), options, bundled_fonts: bundled })
    }
}

/// `nb` as a PDF: every page with a bookmark per section and page, or only the first page or
/// section by `--scope`.
pub fn pdf_bytes(nb: &Notebook, args: &PdfArgs) -> Result<Vec<u8>, Fail> {
    let fonts: &dyn notecraft_pdf::FontSource = if args.bundled_fonts { &notecraft_pdf::BundledFonts } else { &notecraft_pdf::SystemFonts };
    let first_section = || nb.all_sections().into_iter().find(|s| !s.pages.is_empty());
    let doc = match args.scope.as_deref() {
        Some("page") => {
            notecraft_pdf::Doc::page(first_section().and_then(|s| s.pages.first()).ok_or_else(|| Fail::new("the notebook has no pages"))?)
        }
        Some("section") => notecraft_pdf::Doc::section(first_section().ok_or_else(|| Fail::new("the notebook has no pages"))?),
        _ => notecraft_pdf::Doc::notebook(nb),
    };
    notecraft_pdf::export(&doc, &args.options, fonts).map_err(|e| Fail::new(e.to_string()))
}

/// [`write`] with the PDF settings.
pub fn write_with(nb: &Notebook, out: &str, pdf: &PdfArgs) -> Result<String, Fail> {
    let kind = OutKind::of(out)?;
    if kind == OutKind::Folder {
        let dir = if out.to_ascii_lowercase().ends_with(".onetoc2") { Path::new(out).parent().unwrap_or(Path::new(".")) } else { Path::new(out) };
        notecraft_one::write_notebook_dir(nb, dir).map_err(|e| Fail::new(format!("{}: {e}", dir.display())))?;
        let n = nb.all_sections().len();
        return Ok(format!("{} ({n} section{})", dir.display(), if n == 1 { "" } else { "s" }));
    }
    let bytes = if kind == OutKind::Pdf { pdf_bytes(nb, pdf)? } else { to_bytes(nb, kind)? };
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
