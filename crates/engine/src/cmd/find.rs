//! Find and Replace on the page (Ctrl+F): the title and every note container's text, table
//! cells included (a match in a cell names it: `"cell": {"block", "row", "col"}`).

use notecraft_doc::{ElementKind, Guid, Paragraph};
use serde_json::{Value, json};

use super::{has_page, target_page};
use crate::ids::{TEXT_FIND, TEXT_REPLACE};
use crate::text::{self, Frame};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Longest search text, characters.
pub const MAX_FIND: usize = 1000;
/// Most matches reported.
pub const MAX_MATCHES: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(TEXT_FIND, "Find on Page", find).pure().key("Mod+F").when(has_page).params(
            r##"{"query": string, "matchCase"?: bool, "wholeWord"?: bool, "page"?: guid} -> {"matches": [{"outline": guid|null (null: the title), "para", "start", "end"}]} (characters, in page order)"##,
        ),
        CommandSpec::new(TEXT_REPLACE, "Replace", replace).when(has_page).params(
            r##"{"query": string, "with": string, "matchCase"?: bool, "wholeWord"?: bool, "at"?: a text.find match (only that one; default: all), "page"?: guid} -> {"replaced": number}"##,
        ),
    ]
}

struct Opts {
    pat: Vec<char>,
    case: bool,
    whole: bool,
}

impl Opts {
    fn from(params: &Value) -> Result<Opts, CmdError> {
        let q = p::str(params, "query").or_else(|| p::str(params, "text")).unwrap_or("");
        let pat: Vec<char> = q.chars().take(MAX_FIND).collect();
        if pat.is_empty() {
            return Err(CmdError::Params("`query` (the text to find) is required".into()));
        }
        Ok(Opts { pat, case: p::bool(params, "matchCase").unwrap_or(false), whole: p::bool(params, "wholeWord").unwrap_or(false) })
    }

    fn eq(&self, a: char, b: char) -> bool {
        a == b || (!self.case && a.to_lowercase().eq(b.to_lowercase()))
    }

    /// Non-overlapping matches in `text`, as char ranges.
    fn matches(&self, text: &[char]) -> Vec<(usize, usize)> {
        let n = self.pat.len();
        let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
        let mut out = Vec::new();
        let mut i = 0;
        while i + n <= text.len() && out.len() < MAX_MATCHES {
            let hit = text.get(i..i + n).is_some_and(|w| w.iter().zip(&self.pat).all(|(a, b)| self.eq(*a, *b)))
                && (!self.whole || (!word(i.checked_sub(1).and_then(|j| text.get(j))) && !word(text.get(i + n))));
            if hit {
                out.push((i, i + n));
                i += n;
            } else {
                i += 1;
            }
        }
        out
    }
}

/// One match: the outline (`None`: the title), the flow, paragraph, char range.
type Hit = (Option<Guid>, Frame, usize, usize, usize);

fn hits(page: &notecraft_doc::Page, o: &Opts) -> Vec<Hit> {
    let mut out = Vec::new();
    let title: Vec<char> = page.title.chars().collect();
    out.extend(o.matches(&title).into_iter().map(|(a, b)| (None, Frame::Top, 0, a, b)));
    for el in &page.elements {
        let ElementKind::Outline(ol) = &el.kind else { continue };
        for (frame, pi, para) in text::all_paragraphs(ol) {
            let chars: Vec<char> = para.text().chars().collect();
            out.extend(o.matches(&chars).into_iter().map(|(a, b)| (Some(el.id), frame, pi, a, b)));
            if out.len() >= MAX_MATCHES {
                out.truncate(MAX_MATCHES);
                return out;
            }
        }
    }
    out
}

fn find(s: &mut Session, params: &Value) -> CmdResult {
    let o = Opts::from(params)?;
    let page = target_page(s, params)?;
    let matches: Vec<Value> = hits(page, &o)
        .into_iter()
        .map(|(ol, f, pi, a, b)| {
            let mut m = json!({"outline": ol, "para": pi, "start": a, "end": b});
            if let (Some(c), Value::Object(map)) = (f.cell(), &mut m) {
                map.insert("cell".into(), c.to_json());
            }
            m
        })
        .collect();
    Ok(CmdOutput::query(json!({ "matches": matches })))
}

/// Replaces chars `a..b` of the paragraph with `with`, in the formatting of the first replaced
/// character.
fn splice(p: &mut Paragraph, a: usize, b: usize, with: &str) {
    let fmt = text::format_at(p, a.saturating_add(1).min(b.max(1)));
    let fmt = notecraft_doc::Run { link: p_link_at(p, a), ..fmt };
    let i = text::split_at(p, a);
    let j = text::split_at(p, b);
    let j = j.max(i).min(p.runs.len());
    p.runs.drain(i..j);
    p.runs.insert(i.min(p.runs.len()), notecraft_doc::Run { text: with.to_owned(), ..fmt });
    text::normalize(p);
}

/// The link of the character at `a` (format_at drops a link at its end).
fn p_link_at(p: &Paragraph, a: usize) -> Option<String> {
    let mut seen = 0usize;
    for r in &p.runs {
        let n = r.text.chars().count();
        if a < seen + n {
            return r.link.clone();
        }
        seen += n;
    }
    None
}

fn replace(s: &mut Session, params: &Value) -> CmdResult {
    let o = Opts::from(params)?;
    let with: String =
        p::str(params, "with").unwrap_or("").chars().filter(|c| *c == '\t' || !c.is_control()).take(text::MAX_PARA_CHARS / 4).collect();
    let at = match params.get("at").filter(|v| v.is_object()) {
        Some(v) => {
            let outline = p::guid(v, "outline")?;
            let n = |k: &str| p::index(v, k).ok_or_else(|| CmdError::Params(format!("`at` needs `{k}`")));
            let frame = Frame::from_json(v.get("cell")).ok_or_else(|| CmdError::Params("`at.cell` must be {\"block\", \"row\", \"col\"}".into()))?;
            Some((outline, frame, n("para")?, n("start")?))
        }
        None => None,
    };
    let page = target_page(s, params)?;
    let mut todo = hits(page, &o);
    if let Some((ol, f, pi, start)) = at {
        todo.retain(|h| h.0 == ol && h.1 == f && h.2 == pi && h.3 == start);
    }
    if todo.is_empty() {
        return Ok(CmdOutput { value: json!({"replaced": 0}), changed: false });
    }
    let n = todo.len();
    // Back to front, so earlier offsets stay right.
    for (ol, frame, pi, a, b) in todo.into_iter().rev() {
        match ol {
            None => {
                let mut t: Vec<char> = page.title.chars().collect();
                if b <= t.len() && a <= b {
                    t.splice(a..b, with.chars());
                    page.title = t.into_iter().take(1000).collect();
                }
            }
            Some(id) => {
                if let Some(para) = page.outline_mut(id).and_then(|outline| text::flow_mut(outline, frame)).and_then(|fl| fl.para_mut(pi)) {
                    splice(para, a, b, &with);
                }
            }
        }
    }
    page.touch();
    Ok(CmdOutput::value(json!({ "replaced": n })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn find_and_replace_on_the_page() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({"title": "Cat notes"})).unwrap();
        let v = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "cat Cat category\nthe cat sat"})).unwrap().value;
        let ol = Guid::parse(v["id"].as_str().unwrap()).unwrap();
        // Bold "Cat" in the first paragraph.
        s.run(ids::FORMAT_BOLD, &json!({"outline": ol, "start": [0, 4], "end": [0, 7]})).unwrap();
        let m = s.run(ids::TEXT_FIND, &json!({"query": "cat"})).unwrap().value;
        assert_eq!(m["matches"].as_array().unwrap().len(), 5);
        assert_eq!(m["matches"][0], json!({"outline": null, "para": 0, "start": 0, "end": 3}));
        let m = s.run(ids::TEXT_FIND, &json!({"query": "cat", "matchCase": true, "wholeWord": true})).unwrap().value;
        assert_eq!(m["matches"].as_array().unwrap().len(), 2);
        assert!(s.run(ids::TEXT_FIND, &json!({"query": ""})).is_err());
        // Replace one, then all in one undo step.
        let depth = s.undo_depth();
        let one = json!({"outline": ol, "para": 1, "start": 4});
        s.run(ids::TEXT_REPLACE, &json!({"query": "cat", "with": "dog", "at": one})).unwrap();
        assert_eq!(s.current_page().unwrap().outline(ol).unwrap().para(1).unwrap().text(), "the dog sat");
        let r = s.run(ids::TEXT_REPLACE, &json!({"query": "cat", "with": "dog", "wholeWord": true})).unwrap();
        assert_eq!(r.value["replaced"], 3);
        assert_eq!(s.undo_depth(), depth + 2);
        let page = s.current_page().unwrap();
        assert_eq!(page.title, "dog notes");
        let o = page.outline(ol).unwrap();
        assert_eq!(o.para(0).unwrap().text(), "dog dog category");
        assert!(o.para(0).unwrap().runs.iter().any(|r| r.bold && r.text == "dog"), "the replacement keeps the formatting");
        assert!(!s.run(ids::TEXT_REPLACE, &json!({"query": "zebra", "with": "x"})).unwrap().changed);
        assert!(s.undo());
        assert_eq!(s.current_page().unwrap().title, "Cat notes");
    }
}
