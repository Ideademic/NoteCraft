//! Insert tab: pictures, file printouts, attached files, the date and time.
//!
//! Files arrive as base64 bytes with a name (web, scripts, drag and drop) or as a path
//! (desktop). Without `x`/`y` the new item goes at the left margin below the page's content.

use std::sync::Arc;

use chrono::{FixedOffset, Offset as _, Utc};
use notecraft_doc::page::MAX_ELEMENTS_PER_PAGE;
use notecraft_doc::{ElementKind, Outline, Page, PageElement};
use serde_json::{Value, json};

use super::text::{outline_id, range};
use super::{MAX_FILE_BYTES, file_input, has_page, insert_pos, target_page};
use crate::ids::{INSERT_DATE_TIME, INSERT_FILE, INSERT_IMAGE, INSERT_PRINTOUT};
use crate::text::{self, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Largest attached file, bytes.
pub const MAX_ATTACHMENT_BYTES: usize = 100 * 1024 * 1024;

const FILE: &str = r##"{"bytesBase64": string, "name"?: string} | {"path": string}, "x"?: number, "y"?: number, "page"?: guid"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(INSERT_IMAGE, "Pictures", image).when(has_page).params(concat!(
            r##"{"bytesBase64": string, "name"?: string} | {"path": string}, "x"?: number, "y"?: number, "page"?: guid"##,
            r##", "alt"?: string -> {"id": guid}"##
        )),
        CommandSpec::new(INSERT_PRINTOUT, "File Printout", printout).when(has_page).params(concat!(
            r##"{"bytesBase64": string, "name"?: string} | {"path": string}, "x"?: number, "y"?: number, "page"?: guid"##,
            r##", "dpi"?: number (24-600, default 150) -> {"id": guid, "pages": number}"##
        )),
        CommandSpec::new(INSERT_FILE, "Attach File", attach).when(has_page).params(FILE),
        CommandSpec::new(INSERT_DATE_TIME, "Date & Time", date_time).key("Alt+Shift+F").when(has_page).params(
            r##"{"what"?: "date"|"time"|"dateTime" (default), "format"?: strftime string, "utcOffsetMinutes"?: number, "outline"?: guid, "at"?: {"para","char"} (insert at the caret), "x"?: number, "y"?: number (else a new note container), "page"?: guid} -> {"id"?: guid, "caret"?: pos, "text": string}"##,
        ),
    ]
}

/// Adds `kind` to the page (bounded); returns its id.
fn push(page: &mut Page, kind: ElementKind) -> Result<notecraft_doc::Guid, CmdError> {
    if page.elements.len() >= MAX_ELEMENTS_PER_PAGE {
        return Err(CmdError::Failed("this page holds as many items as it can".into()));
    }
    let el = PageElement::new(kind);
    let id = el.id;
    page.elements.push(el);
    page.touch();
    Ok(id)
}

pub(crate) fn image(s: &mut Session, params: &Value) -> CmdResult {
    let file = file_input(params, notecraft_formats::MAX_IMAGE_BYTES)?;
    let mut img = notecraft_formats::import_image(&file.bytes).map_err(|e| CmdError::Failed(e.to_string()))?;
    img.alt = p::str(params, "alt").map(|a| a.chars().filter(|c| !c.is_control()).take(1000).collect()).unwrap_or_default();
    let page = target_page(s, params)?;
    img.pos = insert_pos(params, page);
    let id = push(page, ElementKind::Image(img))?;
    Ok(CmdOutput::value(json!({"id": id})))
}

fn printout(s: &mut Session, params: &Value) -> CmdResult {
    let file = file_input(params, MAX_FILE_BYTES)?;
    let dpi = p::f32(params, "dpi").unwrap_or(notecraft_printout::DEFAULT_DPI);
    let pos = insert_pos(params, target_page(s, params)?);
    let pr = notecraft_printout::printout_for_file(&file.name, Arc::new(file.bytes), pos, dpi).map_err(|e| CmdError::Failed(e.to_string()))?;
    let pages = pr.pages.len();
    let page = target_page(s, params)?;
    let id = push(page, ElementKind::Printout(pr))?;
    Ok(CmdOutput::value(json!({"id": id, "pages": pages})))
}

fn attach(s: &mut Session, params: &Value) -> CmdResult {
    let file = file_input(params, MAX_ATTACHMENT_BYTES)?;
    let page = target_page(s, params)?;
    let pos = insert_pos(params, page);
    let id = push(page, ElementKind::EmbeddedFile { name: file.name, bytes: Arc::new(file.bytes), pos })?;
    Ok(CmdOutput::value(json!({"id": id})))
}

/// The date and/or time as text: `what` picks the default format, `format` overrides it (a
/// chrono strftime string, checked), `utcOffsetMinutes` shifts from UTC to local time.
pub(crate) fn date_time_text(params: &Value) -> Result<String, CmdError> {
    use std::fmt::Write as _;
    let what = p::str(params, "what").unwrap_or("dateTime");
    let default = match what {
        "date" => "%A, %-d %B %Y",
        "time" => "%-I:%M %p",
        "dateTime" => "%A, %-d %B %Y %-I:%M %p",
        other => return Err(CmdError::Params(format!("unknown `what` `{}`", other.chars().take(20).collect::<String>()))),
    };
    let fmt = match p::str(params, "format") {
        Some(f) => {
            let f: String = f.chars().take(64).collect();
            if chrono::format::StrftimeItems::new(&f).any(|i| matches!(i, chrono::format::Item::Error)) {
                return Err(CmdError::Params("`format` is not a valid date format".into()));
            }
            f
        }
        None => default.to_owned(),
    };
    let minutes = p::f32(params, "utcOffsetMinutes").unwrap_or(0.0).clamp(-14.0 * 60.0, 14.0 * 60.0);
    let offset = FixedOffset::east_opt((minutes as i32) * 60).unwrap_or_else(|| Utc.fix());
    let now = notecraft_doc::now().with_timezone(&offset);
    let mut out = String::new();
    // `write!` reports a formatting error instead of panicking like `to_string` would.
    write!(out, "{}", now.format(&fmt)).map_err(|_| CmdError::Params("`format` can't be used with this date".into()))?;
    Ok(out)
}

fn date_time(s: &mut Session, params: &Value) -> CmdResult {
    let txt = date_time_text(params)?;
    if params.get("outline").is_some() || params.get("id").is_some() {
        let id = outline_id(params)?;
        let page = target_page(s, params)?;
        let o = page.outline_mut(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
        let (a, b) = range(params, o).unwrap_or_else(|| {
            let e = text::end_of(o);
            (e, e)
        });
        let at: TextPos = text::delete(o, a, b);
        let fmt = o.paragraphs.get(at.para).map(|pg| text::format_at(pg, at.char)).unwrap_or_default();
        let caret = text::insert(o, at, &txt, &fmt);
        page.touch();
        return Ok(CmdOutput::value(json!({"caret": caret.to_json(), "text": txt})));
    }
    let page = target_page(s, params)?;
    let pos = insert_pos(params, page);
    let id = push(page, ElementKind::Outline(Outline::with_text(pos, &txt)))?;
    Ok(CmdOutput::value(json!({"id": id, "text": txt})))
}
