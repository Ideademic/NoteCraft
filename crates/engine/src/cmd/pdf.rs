//! File › Export › PDF and the PDF the Print page sends to a printer (`notecraft-pdf`).
//!
//! Like the other exports it hands the file back as base64, or writes it to `path` (desktop),
//! plus `sheets`, the number of PDF pages written.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use notecraft_pdf::{BundledFonts, Doc, FontSource, Options, Paper, SystemFonts};
use serde_json::{Value, json};

use super::has_page;
use crate::ids::FILE_EXPORT_PDF;
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

const PARAMS: &str = concat!(
    r##"{"scope"?: "page" (default)|"section"|"notebook", "id"?: page, section or notebook guid (default: the current one), "##,
    r##""paper"?: "fit" (default: one sheet per page, its content's size)|"letter"|"a4"|"legal"|"a5", "landscape"?: bool, "##,
    r##""fitWidth"?: bool (default true: shrink wide pages to the paper's width), "header"?: bool (default true: title and date), "##,
    r##""pageNumbers"?: bool (default true on paper), "pages"?: "1-3, 5" (sheets to keep), "utcOffset"?: minutes east of UTC for dates, "##,
    r##""fonts"?: "system" (default)|"bundled", "path"?: string} -> {"fileName", "mime", "bytesBase64", "size", "sheets"} | {"path", "size", "sheets"}"##
);

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(FILE_EXPORT_PDF, "Export as PDF", export_pdf).pure().when(has_page).params(PARAMS)]
}

/// The print options in `params`.
pub fn options(params: &Value) -> Result<Options, CmdError> {
    let mut o = Options::default();
    if let Some(paper) = p::str(params, "paper") {
        o.paper = Paper::parse(paper).ok_or_else(|| CmdError::Params(format!("unknown paper `{}`", paper.chars().take(20).collect::<String>())))?;
    }
    o.landscape = p::bool(params, "landscape").unwrap_or(false);
    o.fit_width = p::bool(params, "fitWidth").unwrap_or(true);
    o.header = p::bool(params, "header").unwrap_or(true);
    o.page_numbers = p::bool(params, "pageNumbers").unwrap_or(true);
    if let Some(r) = p::str(params, "pages")
        && !r.trim().is_empty()
    {
        o.sheets = Some(notecraft_pdf::parse_ranges(r).ok_or_else(|| CmdError::Params("`pages` is not a page range like 1-3, 5".into()))?);
    }
    if let Some(m) = p::f32(params, "utcOffset") {
        o.utc_offset_minutes = m.clamp(-14.0 * 60.0, 14.0 * 60.0) as i32;
    }
    Ok(o)
}

fn fonts(params: &Value) -> Result<&'static dyn FontSource, CmdError> {
    match p::str(params, "fonts").unwrap_or("system") {
        "system" => Ok(&SystemFonts),
        "bundled" => Ok(&BundledFonts),
        other => Err(CmdError::Params(format!("unknown fonts `{}`", other.chars().take(20).collect::<String>()))),
    }
}

fn export_pdf(s: &mut Session, params: &Value) -> CmdResult {
    let opts = options(params)?;
    let fonts = fonts(params)?;
    let id = p::guid(params, "id")?;
    let (name, bytes) = match p::str(params, "scope").unwrap_or("page") {
        "page" => {
            let pid = id.or(s.current.page).ok_or_else(|| CmdError::Disabled("no page is open".into()))?;
            let page =
                s.notebooks.iter().find_map(|n| n.find_page(pid)).map(|(_, pg)| pg).ok_or_else(|| CmdError::Params(format!("no page {pid}")))?;
            (page.display_title().to_owned(), notecraft_pdf::export(&Doc::page(page), &opts, fonts))
        }
        "section" => {
            let sid = id.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
            let sec = s.notebooks.iter().find_map(|n| n.section(sid)).ok_or_else(|| CmdError::Params(format!("no section {sid}")))?;
            (sec.name.clone(), notecraft_pdf::export(&Doc::section(sec), &opts, fonts))
        }
        "notebook" => {
            let nid = id.or(s.current.notebook).ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?;
            let nb = s.notebook(nid).ok_or_else(|| CmdError::Params(format!("no notebook {nid}")))?;
            (nb.name.clone(), notecraft_pdf::export(&Doc::notebook(nb), &opts, fonts))
        }
        other => return Err(CmdError::Params(format!("unknown scope `{}`", other.chars().take(20).collect::<String>()))),
    };
    let bytes = bytes.map_err(|e| match e {
        notecraft_pdf::PdfError::NoPages => CmdError::Failed("nothing to export: no pages (or none in the page range)".into()),
        e => CmdError::Failed(e.to_string()),
    })?;
    let sheets = notecraft_pdf::thumbnails::sheet_count(&bytes);
    let size = bytes.len();
    if let Some(path) = p::str(params, "path") {
        super::file::write_to(path, &bytes)?;
        return Ok(CmdOutput::query(json!({"path": path, "size": size, "sheets": sheets})));
    }
    let file_name = format!("{}.pdf", notecraft_one::safe_file_stem(&name));
    Ok(CmdOutput::query(
        json!({"fileName": file_name, "mime": "application/pdf", "bytesBase64": STANDARD.encode(&bytes), "size": size, "sheets": sheets}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(v: &Value) -> Vec<u8> {
        STANDARD.decode(v["bytesBase64"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn every_scope_exports_a_pdf() {
        let mut s = Session::new_default();
        for scope in ["page", "section", "notebook"] {
            let out = s.run(FILE_EXPORT_PDF, &json!({"scope": scope, "fonts": "bundled", "paper": "letter"})).unwrap();
            assert!(!out.changed);
            let pdf = bytes(&out.value);
            assert!(pdf.starts_with(b"%PDF-"), "{scope}");
            assert!(out.value["fileName"].as_str().unwrap().ends_with(".pdf"));
            assert!(out.value["sheets"].as_u64().unwrap() >= 1);
        }
        assert!(matches!(s.run(FILE_EXPORT_PDF, &json!({"paper": "tabloid"})), Err(CmdError::Params(_))));
        assert!(matches!(s.run(FILE_EXPORT_PDF, &json!({"pages": "9-1"})), Err(CmdError::Params(_))));
        assert!(matches!(s.run(FILE_EXPORT_PDF, &json!({"fonts": "nope"})), Err(CmdError::Params(_))));
        assert!(matches!(s.run(FILE_EXPORT_PDF, &json!({"pages": "500", "fonts": "bundled"})), Err(CmdError::Failed(_))));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn writes_to_a_path() {
        let dir = std::env::temp_dir().join(format!("nc-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page.pdf");
        let mut s = Session::new_default();
        let out = s.run(FILE_EXPORT_PDF, &json!({"path": path.to_string_lossy(), "fonts": "bundled"})).unwrap();
        assert_eq!(out.value["sheets"], 1);
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
