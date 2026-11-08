//! Links: Insert › Link (Ctrl+K) puts a link on text, Ctrl+click follows it, Copy Link to Page /
//! Paragraph make `notecraft:` links ([`notecraft_doc::link`]), and typing `[[Page title]]` links
//! to that page (creating it when the notebook has none).
//!
//! In a table cell, pass `"cell"` as the text commands take it ([`super::text`]).
//!
//! Addresses are checked before they are kept: web and mail links as the PDF export allows them
//! (`craft_pdf::safe_url`), `notecraft:` links when they parse, `file:` and `onenote:` links as
//! they are. `www.example.com` becomes `https://www.example.com` and `a@b.org` a `mailto:` link.
//! Nothing else (`javascript:`, `data:`…) is kept.

use notecraft_doc::link::{InternalLink, MAX_URL, scheme};
use notecraft_doc::{Guid, Page, Paragraph, Run};
use serde_json::{Value, json};

use super::has_page;
use super::text::{frame_param, outline_id, range, with_frame};
use crate::ids::{LINK_FOLLOW, LINK_GET, LINK_REMOVE, LINK_SET, LINK_TO_PAGE, LINK_TO_PARAGRAPH, LINK_WIKI};
use crate::text::{self, Flow, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Session, p};

/// Longest page name a `[[wiki link]]` takes, characters.
pub const MAX_WIKI_NAME: usize = 200;
/// Most pages a section can have for `[[wiki links]]` to add one.
const MAX_WIKI_PAGES: usize = 10_000;
/// Longest text Insert › Link puts in, characters.
const MAX_LINK_TEXT: usize = 2000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(LINK_SET, "Link", set).when(has_page).key("Mod+K").params(concat!(
            r##"{"outline": guid, "start"?/"end"?/"at"?: pos (default: the end), "url": string, "text"?: string (shown instead of"##,
            r##" the selected text, or inserted at a caret), "page"?: guid} -> {"caret", "start", "end", "url"}"##
        )),
        CommandSpec::new(LINK_REMOVE, "Remove Link", remove)
            .when(has_page)
            .params(r##"{"outline": guid, "start"?/"end"?/"at"?: pos (a caret: the link around it), "page"?: guid}"##),
        CommandSpec::new(LINK_GET, "Link at Caret", get)
            .pure()
            .when(has_page)
            .params(r##"{"outline": guid, "at"?: pos | "start"/"end": pos, "page"?: guid} -> null | {"url", "text", "start", "end"}"##),
        CommandSpec::new(LINK_FOLLOW, "Follow Link", follow).pure().params(concat!(
            r##"{"url": string} -> {"internal": true, "page", "outline"?, "para"?} (went there) | {"external": string}"##,
            r##" (the front end opens it)"##
        )),
        CommandSpec::new(LINK_TO_PAGE, "Copy Link to Page", to_page).pure().params(r##"{"id"?: page guid (default: the current page)} -> {"url"}"##),
        CommandSpec::new(LINK_TO_PARAGRAPH, "Copy Link to Paragraph", to_paragraph)
            .pure()
            .when(has_page)
            .params(r##"{"outline": guid, "para"?: number (default 0) | "at"?: pos, "page"?: guid} -> {"url"}"##),
        CommandSpec::new(LINK_WIKI, "Wiki Link", wiki).when(has_page).params(concat!(
            r##"{"outline": guid, "at": pos (just after `]]`), "page"?: guid} -> null (no [[name]] there) |"##,
            r##" {"caret", "page": guid, "created": bool}"##
        )),
    ]
}

/// `url` as NoteCraft keeps it, or why not.
pub fn clean_url(url: &str) -> Result<String, CmdError> {
    let u: String = url.trim().chars().filter(|c| !c.is_control()).take(MAX_URL + 1).collect();
    if u.is_empty() {
        return Err(CmdError::Params("the address is empty".into()));
    }
    if u.chars().count() > MAX_URL {
        return Err(CmdError::Params(format!("the address is longer than {MAX_URL} characters")));
    }
    let bad = |what: &str| Err(CmdError::Params(format!("\u{201c}{}\u{201d} {what}", u.chars().take(80).collect::<String>())));
    match scheme(&u).as_deref() {
        Some("http" | "https" | "mailto") => {
            if craft_pdf::safe_url(&u) {
                Ok(u)
            } else {
                bad("isn't a web or e-mail address NoteCraft can open")
            }
        }
        Some("notecraft") => match InternalLink::parse(&u) {
            Some(_) => Ok(u),
            None => bad("doesn't point to a page"),
        },
        Some("file" | "onenote") => Ok(u),
        // A Windows path (`C:\Notes\a.pdf`).
        Some(d) if d.len() == 1 && u.get(1..3).is_some_and(|s| s == ":\\" || s == ":/") => Ok(format!("file:///{}", u.replace('\\', "/"))),
        Some(_) if u.contains("://") => bad("uses a kind of link NoteCraft doesn't open"),
        Some(_) | None => {
            if u.contains(char::is_whitespace) {
                bad("isn't an address")
            } else if u.starts_with('/') {
                Ok(format!("file://{u}"))
            } else if u.contains('@') && !u.contains('/') {
                let m = format!("mailto:{u}");
                if craft_pdf::safe_url(&m) { Ok(m) } else { bad("isn't an e-mail address") }
            } else if u.contains('.') && !u.starts_with('.') {
                let w = format!("https://{u}");
                if craft_pdf::safe_url(&w) { Ok(w) } else { bad("isn't a web address") }
            } else {
                bad("uses a kind of link NoteCraft doesn't open")
            }
        }
    }
}

/// The characters `a..b` (start of the link run and its end) of the link around character `ch`
/// of `p` (a caret right after a link counts), and its address.
fn link_extent(p: &Paragraph, ch: usize) -> Option<(usize, usize, String)> {
    let mut spans: Vec<(usize, usize, Option<&str>)> = Vec::new();
    let mut seen = 0usize;
    for r in &p.runs {
        let n = r.text.chars().count();
        spans.push((seen, seen + n, r.link.as_deref()));
        seen += n;
    }
    let i = spans
        .iter()
        .position(|(a, b, l)| l.is_some() && *a <= ch && ch < *b)
        .or_else(|| spans.iter().position(|(a, b, l)| l.is_some() && *b == ch && a < b))?;
    let url = spans.get(i)?.2?.to_owned();
    let (mut lo, mut hi) = (i, i);
    while lo > 0 && spans.get(lo - 1).is_some_and(|s| s.2 == Some(url.as_str())) {
        lo -= 1;
    }
    while spans.get(hi + 1).is_some_and(|s| s.2 == Some(url.as_str())) {
        hi += 1;
    }
    Some((spans.get(lo)?.0, spans.get(hi)?.1, url))
}

/// The run holding character `ch` of `p` (no text), else the formatting typed there.
fn run_at_char(p: &Paragraph, ch: usize) -> Run {
    let mut seen = 0usize;
    for r in &p.runs {
        let n = r.text.chars().count();
        if ch < seen + n {
            return Run { text: String::new(), ..r.clone() };
        }
        seen += n;
    }
    text::format_at(p, ch)
}

/// The text between `a` and `b` of `o`, paragraphs joined by newlines.
fn text_between(o: &dyn Flow, a: TextPos, b: TextPos) -> String {
    let mut out = String::new();
    for i in a.para..=b.para.min(o.count().saturating_sub(1)) {
        let (x, y) = text::para_span(o, i, a, b);
        if i > a.para {
            out.push('\n');
        }
        if let Some(p) = o.para(i) {
            out.extend(p.text().chars().skip(x).take(y - x));
        }
    }
    out
}

fn pos_range(params: &Value, o: &dyn Flow) -> (TextPos, TextPos) {
    if let Some(at) = params.get("at").and_then(TextPos::from_json) {
        let at = text::clamp_ref(o, at);
        return (at, at);
    }
    range(params, o).unwrap_or_else(|| {
        let e = text::end_of(o);
        (e, e)
    })
}

/// What a link to `url` shows when it is inserted with no text: a page's title for a
/// `notecraft:` link, else the address.
fn default_text(url: &str) -> String {
    InternalLink::parse(url).and_then(|l| l.title).unwrap_or_else(|| url.to_owned())
}

fn set(s: &mut Session, params: &Value) -> CmdResult {
    let url = clean_url(p::req_str(params, "url")?)?;
    let shown: Option<String> = p::str(params, "text")
        .map(|t| t.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).filter(|c| !c.is_control()).take(MAX_LINK_TEXT).collect::<String>())
        .filter(|t| !t.trim().is_empty());
    with_frame(s, params, |o| {
        let (a, b) = pos_range(params, o);
        let current = text_between(o, a, b);
        let base = match o.para(a.para) {
            Some(p) if a < b => run_at_char(p, a.char),
            Some(p) => text::format_at(p, a.char),
            None => Run::default(),
        };
        let fmt = Run { link: Some(url.clone()), ..base };
        let end = if a == b || shown.as_ref().is_some_and(|t| *t != current) {
            let t = shown.clone().unwrap_or_else(|| default_text(&url));
            let at = text::delete(o, a, b);
            text::insert(o, at, &t, &fmt)
        } else {
            text::apply_range(o, a, b, &mut |r| r.link = Some(url.clone()));
            b
        };
        Ok(CmdOutput::value(json!({"caret": end.to_json(), "start": a.to_json(), "end": end.to_json(), "url": url})))
    })
}

fn remove(s: &mut Session, params: &Value) -> CmdResult {
    with_frame(s, params, |o| {
        let (mut a, mut b) = pos_range(params, o);
        if a == b {
            let Some((x, y, _)) = o.para(a.para).and_then(|p| link_extent(p, a.char)) else { return Ok(CmdOutput::unchanged()) };
            (a, b) = (TextPos::new(a.para, x), TextPos::new(a.para, y));
        }
        let had = text::runs_in(o, a, b).iter().any(|r| r.link.is_some());
        if !had {
            return Ok(CmdOutput::unchanged());
        }
        text::apply_range(o, a, b, &mut |r| r.link = None);
        Ok(CmdOutput::ok())
    })
}

fn get(s: &mut Session, params: &Value) -> CmdResult {
    let id = outline_id(params)?;
    let page = super::target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let o = text::flow(o, frame_param(params)?).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
    let (a, _) = pos_range(params, o);
    let Some(p) = o.para(a.para) else { return Ok(CmdOutput::query(Value::Null)) };
    let Some((x, y, url)) = link_extent(p, a.char) else { return Ok(CmdOutput::query(Value::Null)) };
    let text: String = p.text().chars().skip(x).take(y - x).collect();
    Ok(CmdOutput::query(json!({"url": url, "text": text, "start": TextPos::new(a.para, x).to_json(), "end": TextPos::new(a.para, y).to_json()})))
}

/// Where an internal link points among the open notebooks: (notebook, section, page).
pub fn resolve(s: &Session, l: &InternalLink) -> Option<(Guid, Guid, Option<Guid>)> {
    if let Some(pid) = l.page {
        for nb in &s.notebooks {
            if let Some(sec) = nb.section_of_page(pid) {
                return Some((nb.id, sec, Some(pid)));
            }
        }
    }
    // The page is gone (or the link came from elsewhere): a page of that title, in the linked
    // notebook first, then the current one, then any.
    if let Some(title) = l.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        let order: Vec<&notecraft_doc::Notebook> =
            l.notebook.and_then(|id| s.notebook(id)).into_iter().chain(s.current_notebook()).chain(s.notebooks.iter()).collect();
        for nb in order {
            if let Some((sec, page)) = find_titled(nb, title, l.section) {
                return Some((nb.id, sec, Some(page)));
            }
        }
    }
    let sid = l.section?;
    s.notebooks.iter().find(|n| n.section(sid).is_some()).map(|n| (n.id, sid, None))
}

/// A page titled `title` (case-insensitive) in `nb`, in section `prefer` first.
fn find_titled(nb: &notecraft_doc::Notebook, title: &str, prefer: Option<Guid>) -> Option<(Guid, Guid)> {
    let sections = nb.all_sections();
    let same = |p: &Page| p.title.trim().eq_ignore_ascii_case(title.trim());
    let first = prefer.and_then(|id| sections.iter().find(|s| s.id == id)).and_then(|s| s.pages.iter().find(|p| same(p)).map(|p| (s.id, p.id)));
    first.or_else(|| sections.iter().find_map(|s| s.pages.iter().find(|p| same(p)).map(|p| (s.id, p.id))))
}

fn follow(s: &mut Session, params: &Value) -> CmdResult {
    let url = p::req_str(params, "url")?;
    if notecraft_doc::link::is_internal(url) {
        let l = InternalLink::parse(url).ok_or_else(|| CmdError::Params("that link doesn't point to a page".into()))?;
        let (nb, sec, page) = resolve(s, &l).ok_or_else(|| CmdError::Failed("The page this link points to isn't in an open notebook.".into()))?;
        s.current = Location { notebook: Some(nb), section: Some(sec), page };
        s.viewing_version = None;
        s.normalize_location();
        let mut out = json!({"internal": true, "page": s.current.page});
        if let (Some(o), Value::Object(m)) = (l.outline, &mut out)
            && page.is_some()
        {
            m.insert("outline".into(), json!(o));
            m.insert("para".into(), json!(l.para.unwrap_or(0)));
        }
        return Ok(CmdOutput::query(out));
    }
    Ok(CmdOutput::query(json!({"external": clean_url(url)?})))
}

/// The `notecraft:` link to page `id` of an open notebook.
pub fn page_link(s: &Session, id: Guid) -> Option<InternalLink> {
    s.notebooks.iter().find_map(|nb| {
        let sec = nb.section_of_page(id)?;
        let (_, page) = nb.find_page(id)?;
        Some(InternalLink::to_page(nb.id, sec, id, page.display_title()))
    })
}

fn to_page(s: &mut Session, params: &Value) -> CmdResult {
    let id = super::id_or(params, s.current.page, "page")?;
    let l = page_link(s, id).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    Ok(CmdOutput::query(json!({"url": l.to_url()})))
}

fn to_paragraph(s: &mut Session, params: &Value) -> CmdResult {
    let oid = outline_id(params)?;
    let pid = match p::guid(params, "page")? {
        Some(id) => id,
        None => s.current.page.ok_or_else(|| CmdError::Disabled("no page is open".into()))?,
    };
    let page = super::target_page(s, params)?;
    let o = page.outline(oid).ok_or_else(|| CmdError::Params(format!("no outline {oid}")))?;
    // In a cell: the link goes to the table's block.
    let para = match frame_param(params)?.cell() {
        Some(c) => c.block,
        None => params.get("at").and_then(TextPos::from_json).map(|t| t.para).or_else(|| p::index(params, "para")).unwrap_or(0),
    };
    let para = para.min(o.blocks.len().saturating_sub(1));
    let mut l = page_link(s, pid).ok_or_else(|| CmdError::Params(format!("no page {pid}")))?;
    l.outline = Some(oid);
    l.para = Some(para);
    Ok(CmdOutput::query(json!({"url": l.to_url()})))
}

/// The `[[name]]` just before character `at` of `p`: (where `[[` starts, the name).
pub fn wiki_before(p: &Paragraph, at: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = p.text().chars().collect();
    let at = at.min(chars.len());
    let before = chars.get(..at)?;
    if before.len() < 5 || before.get(at - 2..at) != Some(&[']', ']']) {
        return None;
    }
    let inner_end = at - 2;
    let mut i = inner_end;
    while i >= 2 {
        if before.get(i - 2..i) == Some(&['[', '[']) {
            break;
        }
        if inner_end - i > MAX_WIKI_NAME || matches!(before.get(i - 1), Some('[' | ']')) {
            return None;
        }
        i -= 1;
    }
    if i < 2 || before.get(i - 2..i) != Some(&['[', '[']) {
        return None;
    }
    let name: String = before.get(i..inner_end)?.iter().collect();
    let name = name.trim().to_owned();
    (!name.is_empty() && name.chars().count() <= MAX_WIKI_NAME && !name.contains(['[', ']'])).then_some((i - 2, name))
}

fn wiki(s: &mut Session, params: &Value) -> CmdResult {
    let oid = outline_id(params)?;
    let at = params.get("at").and_then(TextPos::from_json).ok_or_else(|| CmdError::Params("`at` must be {\"para\", \"char\"}".into()))?;
    let host = match p::guid(params, "page")? {
        Some(id) => id,
        None => s.current.page.ok_or_else(|| CmdError::Disabled("no page is open".into()))?,
    };
    let frame = frame_param(params)?;
    let found = {
        let page = super::target_page(s, params)?;
        let o = page.outline(oid).ok_or_else(|| CmdError::Params(format!("no outline {oid}")))?;
        let o = text::flow(o, frame).ok_or_else(|| CmdError::Params("no such table cell".into()))?;
        let at = text::clamp_ref(o, at);
        o.para(at.para).and_then(|p| wiki_before(p, at.char)).map(|(start, name)| (at, start, name))
    };
    let Some((at, start, name)) = found else { return Ok(CmdOutput::query(Value::Null)) };
    let nb = s.notebooks.iter().position(|n| n.find_page(host).is_some()).ok_or_else(|| CmdError::Params(format!("no page {host}")))?;
    let sec = s.notebooks.get(nb).and_then(|n| n.section_of_page(host)).ok_or_else(|| CmdError::Params(format!("no page {host}")))?;
    let existing = s.notebooks.get(nb).and_then(|n| find_titled(n, &name, Some(sec))).map(|(_, p)| p).filter(|p| *p != host);
    let (target, created) = match existing {
        Some(p) => (p, false),
        None => {
            let section = s.notebooks.get_mut(nb).and_then(|n| n.section_mut(sec)).ok_or_else(|| CmdError::Params(format!("no section {sec}")))?;
            if section.pages.len() >= MAX_WIKI_PAGES {
                return Err(CmdError::Failed("this section holds as many pages as it can".into()));
            }
            let page = Page::new(name.clone());
            let id = page.id;
            section.pages.push(page);
            (id, true)
        }
    };
    let url = page_link(s, target).map(|l| l.to_url()).ok_or_else(|| CmdError::Failed("the page couldn't be linked".into()))?;
    let out = with_frame(s, params, |o| {
        let fmt = o.para(at.para).map(|p| run_at_char(p, start + 2)).unwrap_or_default();
        let from = text::delete(o, TextPos::new(at.para, start), at);
        let caret = text::insert(o, from, &name, &Run { link: Some(url.clone()), ..fmt });
        Ok(CmdOutput::value(json!({"caret": caret.to_json(), "page": target, "created": created})))
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(clean_url(" https://example.com/a?b ").unwrap(), "https://example.com/a?b");
        assert_eq!(clean_url("www.example.com").unwrap(), "https://www.example.com");
        assert_eq!(clean_url("me@example.org").unwrap(), "mailto:me@example.org");
        assert_eq!(clean_url("/Users/me/a.pdf").unwrap(), "file:///Users/me/a.pdf");
        assert_eq!(clean_url("C:\\Notes\\a.pdf").unwrap(), "file:///C:/Notes/a.pdf");
        assert!(clean_url("notecraft:page?id=00000000-0000-0000-0000-000000000001").is_ok());
        for bad in
            ["", "javascript:alert(1)", "data:text/html,x", "http://", "notecraft:nope", "two words", "ftp://x.org/a", "x".repeat(3000).as_str()]
        {
            assert!(clean_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn wiki_names() {
        let p = |t: &str| Paragraph::plain(t);
        assert_eq!(wiki_before(&p("see [[Plans]]"), 13), Some((4, "Plans".into())));
        assert_eq!(wiki_before(&p("[[ Big plans ]] x"), 15), Some((0, "Big plans".into())));
        assert_eq!(wiki_before(&p("[[]]"), 4), None);
        assert_eq!(wiki_before(&p("a]]"), 3), None);
        assert_eq!(wiki_before(&p("[[a[b]]"), 7), None);
        assert_eq!(wiki_before(&p("[[x]] tail"), 99), None);
        let long = format!("[[{}]]", "y".repeat(300));
        assert_eq!(wiki_before(&p(&long), 304), None);
    }

    #[test]
    fn link_runs() {
        let para = Paragraph {
            runs: vec![
                Run::plain("a "),
                Run { text: "lin".into(), link: Some("u".into()), ..Run::default() },
                Run { text: "k".into(), link: Some("u".into()), bold: true, ..Run::default() },
                Run::plain(" b"),
            ],
            ..Paragraph::default()
        };
        assert_eq!(link_extent(&para, 3), Some((2, 6, "u".into())));
        assert_eq!(link_extent(&para, 6), Some((2, 6, "u".into())), "a caret right after the link");
        assert_eq!(link_extent(&para, 0), None);
    }
}
