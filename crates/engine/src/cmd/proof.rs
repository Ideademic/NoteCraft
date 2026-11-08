//! Review › Spelling, Language and Word Count.
//!
//! Misspellings are named by their note container, paragraph and character range
//! (`{"outline", "para", "start", "end", "word", "lang"}`; `start`/`end` are characters in the
//! paragraph, as the selection model's `char`). See [`crate::proof`] for how paragraphs are
//! checked and cached.

use notecraft_doc::page::clean_lang;
use notecraft_doc::{ElementKind, Guid, Outline, Page, Paragraph, Run};
use serde_json::{Value, json};

use super::text::{outline_id, range, with_outline};
use super::{has_page, target_page};
use crate::ids::{
    PROOF_ADD_TO_DICTIONARY, PROOF_APPLY, PROOF_CHANGE_ALL, PROOF_CHECK, PROOF_DICTIONARY, PROOF_IGNORE, PROOF_IGNORE_ALL, PROOF_LANGUAGES,
    PROOF_REMOVE_FROM_DICTIONARY, PROOF_SET_DEFAULT_LANGUAGE, PROOF_SET_LANGUAGE, PROOF_SUGGESTIONS, REVIEW_WORD_COUNT,
};
use crate::proof::{self, Misspelling, ProofState};
use crate::text::{self, TextPos};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Most misspellings `proof.check` lists by default, and at most.
const DEFAULT_LIMIT: usize = 500;
const MAX_LIMIT: usize = 5000;
/// Longest replacement `proof.apply` takes, characters.
const MAX_REPLACEMENT: usize = 200;

const ISSUE: &str = r##"{"outline": guid, "para": number, "start": number, "end": number, "word": string, "lang": string}"##;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(PROOF_CHECK, "Check Spelling", check)
            .pure()
            .when(has_page)
            .key("F7")
            .params(concat!(r##"{"page"?: guid, "outline"?: guid, "limit"?: number} -> {"issues": ["##, r##"{"outline", "para", "start", "end", "word", "lang"}], "more": bool}"##)),
        CommandSpec::new(PROOF_SUGGESTIONS, "Spelling Suggestions", suggestions)
            .pure()
            .when(has_page)
            .params(r##"{"outline": guid, "at": {"para", "char"}, "page"?: guid, "max"?: number} -> null | {"outline", "para", "start", "end", "word", "lang", "suggestions": [string]}"##),
        CommandSpec::new(PROOF_APPLY, "Change Spelling", apply)
            .when(has_page)
            .params(r##"{"outline": guid, "para": number, "start": number, "end": number, "text": string, "word"?: string (the text there now, checked), "page"?: guid} -> {"caret"}"##),
        CommandSpec::new(PROOF_CHANGE_ALL, "Change All", change_all)
            .when(has_page)
            .params(r##"{"word": string, "text": string, "page"?: guid} -> {"changed": number}"##),
        CommandSpec::new(PROOF_IGNORE, "Ignore", ignore).pure().when(has_page).params(ISSUE),
        CommandSpec::new(PROOF_IGNORE_ALL, "Ignore All", ignore_all).pure().params(r##"{"word": string}"##),
        CommandSpec::new(PROOF_ADD_TO_DICTIONARY, "Add to Dictionary", add).pure().params(r##"{"word": string} -> {"word", "words": [string]}"##),
        CommandSpec::new(PROOF_REMOVE_FROM_DICTIONARY, "Remove from Dictionary", remove)
            .pure()
            .params(r##"{"word": string} -> {"removed": bool, "words": [string]}"##),
        CommandSpec::new(PROOF_DICTIONARY, "User Dictionary", dictionary).pure().params(r##"{} -> {"words": [string]}"##),
        CommandSpec::new(PROOF_SET_LANGUAGE, "Set Proofing Language", set_language).when(has_page).params(concat!(
            r##"{"lang": string|null (a BCP 47 tag; "zxx": don't check; null: the default), "outline"?: guid, "start"?/"end"?: pos"##,
            r##" (default: the whole container; no outline: the whole page), "page"?: guid} -> {"paragraphs": number}"##
        )),
        CommandSpec::new(PROOF_SET_DEFAULT_LANGUAGE, "Default Proofing Language", set_default)
            .pure()
            .params(r##"{"lang": string} -> {"lang"}"##),
        CommandSpec::new(PROOF_LANGUAGES, "Proofing Languages", languages)
            .pure()
            .params(r##"{} -> {"default": string, "languages": [{"tag", "name", "dictionary": bool}]}"##),
        CommandSpec::new(REVIEW_WORD_COUNT, "Word Count", word_count).pure().when(has_page).params(concat!(
            r##"{"page"?: guid, "outline"?: guid, "start"?/"end"?: pos (a selection; default: the whole page)}"##,
            r##" -> {"scope": "page"|"selection", "words", "characters", "charactersNoSpaces", "paragraphs"}"##
        )),
    ]
}

fn issue_json(outline: Guid, para: usize, m: &Misspelling) -> Value {
    json!({"outline": outline, "para": para, "start": m.start, "end": m.end, "word": m.word, "lang": m.lang})
}

fn outlines(page: &Page) -> impl Iterator<Item = (Guid, &Outline)> {
    page.elements.iter().filter_map(|e| match &e.kind {
        ElementKind::Outline(o) => Some((e.id, o)),
        _ => None,
    })
}

/// Every misspelling on `page` (of one container when `only` names it), in reading order.
fn page_issues(state: &ProofState, page: &Page, only: Option<Guid>, limit: usize) -> (Vec<(Guid, usize, Misspelling)>, bool) {
    let mut out = Vec::new();
    for (id, o) in outlines(page).filter(|(id, _)| only.is_none_or(|w| w == *id)) {
        for (i, para) in o.paragraphs.iter().enumerate() {
            for m in proof::issues(state, para) {
                if out.len() >= limit {
                    return (out, true);
                }
                out.push((id, i, m));
            }
        }
    }
    (out, false)
}

fn check(s: &mut Session, params: &Value) -> CmdResult {
    let only = p::guid(params, "outline")?;
    let limit = p::index(params, "limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let state = s.proof.clone();
    let page = target_page(s, params)?;
    let (found, more) = page_issues(&state, page, only, limit);
    let list: Vec<Value> = found.iter().map(|(o, i, m)| issue_json(*o, *i, m)).collect();
    Ok(CmdOutput::query(json!({"issues": list, "more": more})))
}

/// The misspelling covering `at` in paragraph `para` of `o`.
fn issue_at(state: &ProofState, o: &Outline, at: TextPos) -> Option<Misspelling> {
    let para = o.paragraphs.get(at.para)?;
    proof::issues(state, para).into_iter().find(|m| m.start <= at.char && at.char <= m.end)
}

fn suggestions(s: &mut Session, params: &Value) -> CmdResult {
    let id = outline_id(params)?;
    let max = p::index(params, "max").unwrap_or(6).clamp(1, 20);
    let state = s.proof.clone();
    let page = target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let at = params.get("at").and_then(TextPos::from_json).or_else(|| range(params, o).map(|(a, _)| a));
    let at = at.ok_or_else(|| CmdError::Params("`at` must be {\"para\", \"char\"}".into()))?;
    let Some(m) = issue_at(&state, o, text::clamp_ref(o, at)) else { return Ok(CmdOutput::query(Value::Null)) };
    let mut v = issue_json(id, at.para, &m);
    if let Value::Object(map) = &mut v {
        map.insert("suggestions".into(), json!(proof::suggestions(&m.word, &m.lang, max)));
    }
    Ok(CmdOutput::query(v))
}

/// The run holding character `ch` of `p` (its formatting, no text).
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

/// The text of characters `a..b` of `p`.
fn chars_of(p: &Paragraph, a: usize, b: usize) -> String {
    p.text().chars().skip(a).take(b.saturating_sub(a)).collect()
}

fn clean_replacement(t: &str) -> Result<String, CmdError> {
    let t: String = t.chars().map(|c| if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c }).filter(|c| !c.is_control()).collect();
    if t.chars().count() > MAX_REPLACEMENT {
        return Err(CmdError::Params(format!("`text` is longer than {MAX_REPLACEMENT} characters")));
    }
    Ok(t)
}

/// Replaces characters `a..b` of paragraph `para` with `with`, in the formatting of the first
/// replaced character. Returns the caret after it.
fn replace(o: &mut Outline, para: usize, a: usize, b: usize, with: &str) -> Option<TextPos> {
    let p = o.paragraphs.get(para)?;
    let len = text::para_len(p);
    if a >= b || b > len {
        return None;
    }
    let fmt = run_at_char(p, a);
    let at = text::delete(o, TextPos::new(para, a), TextPos::new(para, b));
    Some(text::insert(o, at, with, &fmt))
}

fn apply(s: &mut Session, params: &Value) -> CmdResult {
    let para = p::index(params, "para").ok_or_else(|| CmdError::Params("`para` (number) is required".into()))?;
    let a = p::index(params, "start").ok_or_else(|| CmdError::Params("`start` (number) is required".into()))?;
    let b = p::index(params, "end").ok_or_else(|| CmdError::Params("`end` (number) is required".into()))?;
    let with = clean_replacement(p::req_str(params, "text")?)?;
    let word = p::str(params, "word").map(str::to_owned);
    with_outline(s, params, |o| {
        let p = o.paragraphs.get(para).ok_or_else(|| CmdError::Params(format!("no paragraph {para}")))?;
        if let Some(w) = &word
            && chars_of(p, a, b) != *w
        {
            return Err(CmdError::Failed("the text has changed since it was checked".into()));
        }
        let caret = replace(o, para, a, b, &with).ok_or_else(|| CmdError::Params("`start`..`end` isn't text in that paragraph".into()))?;
        Ok(CmdOutput::value(json!({"caret": caret.to_json()})))
    })
}

fn change_all(s: &mut Session, params: &Value) -> CmdResult {
    let word = p::req_str(params, "word")?.to_owned();
    let with = clean_replacement(p::req_str(params, "text")?)?;
    if word.is_empty() || word == with {
        return Ok(CmdOutput::unchanged());
    }
    let state = s.proof.clone();
    let page = target_page(s, params)?;
    let mut changed = 0usize;
    for el in &mut page.elements {
        let ElementKind::Outline(o) = &mut el.kind else { continue };
        for i in 0..o.paragraphs.len() {
            let Some(para) = o.paragraphs.get(i) else { continue };
            let found: Vec<Misspelling> = proof::issues(&state, para).into_iter().filter(|m| m.word == word).collect();
            // Last first: the earlier ranges stay where they are.
            for m in found.iter().rev() {
                if replace(o, i, m.start, m.end, &with).is_some() {
                    changed += 1;
                }
            }
        }
    }
    if changed == 0 {
        return Ok(CmdOutput::query(json!({"changed": 0})));
    }
    page.touch();
    Ok(CmdOutput::value(json!({"changed": changed})))
}

fn ignore(s: &mut Session, params: &Value) -> CmdResult {
    let id = outline_id(params)?;
    let para = p::index(params, "para").ok_or_else(|| CmdError::Params("`para` (number) is required".into()))?;
    let start = p::index(params, "start").ok_or_else(|| CmdError::Params("`start` (number) is required".into()))?;
    let state = s.proof.clone();
    let page = target_page(s, params)?;
    let o = page.outline(id).ok_or_else(|| CmdError::Params(format!("no outline {id}")))?;
    let Some(pg) = o.paragraphs.get(para) else { return Err(CmdError::Params(format!("no paragraph {para}"))) };
    let text = pg.text();
    let found = proof::issues(&state, pg).into_iter().find(|m| m.start == start);
    match found {
        Some(m) => {
            s.proof.ignore_once(&text, &m);
            Ok(CmdOutput::query(json!({"ignored": true})))
        }
        None => Ok(CmdOutput::query(json!({"ignored": false}))),
    }
}

fn word_param(params: &Value) -> Result<String, CmdError> {
    craft_proof::custom_word(p::req_str(params, "word")?).ok_or_else(|| CmdError::Params("`word` must be one word of at most 64 characters".into()))
}

fn ignore_all(_: &mut Session, params: &Value) -> CmdResult {
    let w = word_param(params)?;
    craft_proof::ignore_word(&w);
    Ok(CmdOutput::query(json!({"word": w})))
}

fn add(_: &mut Session, params: &Value) -> CmdResult {
    let w = word_param(params)?;
    craft_proof::add_word(&w);
    Ok(CmdOutput::query(json!({"word": w, "words": proof::user_words()})))
}

fn remove(_: &mut Session, params: &Value) -> CmdResult {
    let w = word_param(params)?;
    let removed = craft_proof::remove_word(&w);
    Ok(CmdOutput::query(json!({"removed": removed, "words": proof::user_words()})))
}

fn dictionary(_: &mut Session, _: &Value) -> CmdResult {
    Ok(CmdOutput::query(json!({"words": proof::user_words()})))
}

/// `params.lang` as a paragraph keeps it: `None` for the default (null or empty).
fn lang_param(params: &Value) -> Result<Option<String>, CmdError> {
    match params.get("lang") {
        None => Err(CmdError::Params("`lang` (a language tag or null) is required".into())),
        Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if t.trim().is_empty() => Ok(None),
        Some(Value::String(t)) => clean_lang(t).map(Some).ok_or_else(|| CmdError::Params("`lang` isn't a language tag".into())),
        Some(_) => Err(CmdError::Params("`lang` must be a string or null".into())),
    }
}

fn set_language(s: &mut Session, params: &Value) -> CmdResult {
    let lang = lang_param(params)?;
    let set = |o: &mut Outline, a: usize, b: usize| {
        let mut n = 0;
        for p in o.paragraphs.iter_mut().skip(a).take(b.saturating_sub(a) + 1) {
            if p.lang != lang {
                p.lang = lang.clone();
                n += 1;
            }
        }
        n
    };
    let n = if params.get("outline").is_some() || params.get("id").is_some() {
        let mut n = 0;
        with_outline(s, params, |o| {
            let (a, b) = range(params, o).map_or((0, o.paragraphs.len().saturating_sub(1)), |(a, b)| (a.para, b.para));
            n = set(o, a, b);
            Ok(if n > 0 { CmdOutput::ok() } else { CmdOutput::unchanged() })
        })?;
        n
    } else {
        let page = target_page(s, params)?;
        let mut n = 0;
        for el in &mut page.elements {
            if let ElementKind::Outline(o) = &mut el.kind {
                let last = o.paragraphs.len().saturating_sub(1);
                n += set(o, 0, last);
            }
        }
        if n > 0 {
            page.touch();
        }
        n
    };
    let out = json!({"paragraphs": n});
    Ok(if n > 0 { CmdOutput::value(out) } else { CmdOutput::query(out) })
}

fn set_default(s: &mut Session, params: &Value) -> CmdResult {
    let lang = lang_param(params)?.unwrap_or_else(|| "en-US".to_owned());
    s.proof.default_lang = lang.clone();
    Ok(CmdOutput::query(json!({"lang": lang})))
}

fn languages(s: &mut Session, _: &Value) -> CmdResult {
    let list: Vec<Value> =
        proof::LANGUAGES.iter().map(|(tag, name)| json!({"tag": tag, "name": name, "dictionary": proof::has_dictionary(tag)})).collect();
    Ok(CmdOutput::query(json!({"default": s.proof.default_lang, "languages": list})))
}

/// Words in `text`: runs of non-space characters with a letter or digit in them.
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
}

fn word_count(s: &mut Session, params: &Value) -> CmdResult {
    let only = p::guid(params, "outline")?;
    let page = target_page(s, params)?;
    let mut paras: Vec<String> = Vec::new();
    let mut scope = "page";
    match only.and_then(|id| page.outline(id)) {
        Some(o) if range(params, o).is_some_and(|(a, b)| a != b) => {
            scope = "selection";
            let (a, b) = range(params, o).unwrap_or_default();
            for i in a.para..=b.para.min(o.paragraphs.len().saturating_sub(1)) {
                let (x, y) = text::para_span(o, i, a, b);
                if let Some(p) = o.paragraphs.get(i) {
                    paras.push(chars_of(p, x, y));
                }
            }
        }
        _ => {
            if !page.title.trim().is_empty() {
                paras.push(page.title.clone());
            }
            for (_, o) in outlines(page) {
                paras.extend(o.paragraphs.iter().map(Paragraph::text));
            }
        }
    }
    let words: usize = paras.iter().map(|t| count_words(t)).sum();
    let characters: usize = paras.iter().map(|t| t.chars().count()).sum();
    let no_spaces: usize = paras.iter().map(|t| t.chars().filter(|c| !c.is_whitespace()).count()).sum();
    let paragraphs = paras.iter().filter(|t| !t.trim().is_empty()).count();
    Ok(CmdOutput::query(json!({"scope": scope, "words": words, "characters": characters, "charactersNoSpaces": no_spaces, "paragraphs": paragraphs})))
}
