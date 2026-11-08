//! Search across the open notebooks (the search box), with a scope: this page, section,
//! section group, notebook, or all notebooks.

use notecraft_doc::{Guid, SectionEntry};
use serde_json::{Value, json};

use crate::ids::SEARCH;
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

/// Most results returned by default.
pub const DEFAULT_LIMIT: usize = 200;
/// Most results returned at all.
pub const MAX_LIMIT: usize = 2_000;
/// Longest query, characters.
pub const MAX_QUERY: usize = 200;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(SEARCH, "Search", search).pure().key("Mod+E").params(
        r##"{"query": string, "scope"?: "all" (default)|"notebook"|"sectionGroup"|"section"|"page", "limit"?: number} -> {"hits": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "snippet"}], "truncated": bool}"##,
    )]
}

/// Where a search (or the Tags Summary) looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    /// Only this notebook.
    pub notebook: Option<Guid>,
    /// Only these sections (`None`: every section of the notebooks in scope).
    pub sections: Option<Vec<Guid>>,
    /// Only this page.
    pub page: Option<Guid>,
}

impl Scope {
    pub(crate) fn has_section(&self, id: Guid) -> bool {
        self.sections.as_ref().is_none_or(|s| s.contains(&id))
    }
}

/// The section ids under `entries` (depth-bounded).
fn sections_in(entries: &[SectionEntry], depth: usize, out: &mut Vec<Guid>) {
    if depth > notecraft_doc::notebook::MAX_GROUP_DEPTH {
        return;
    }
    for e in entries {
        match e {
            SectionEntry::Section(s) => out.push(s.id),
            SectionEntry::Group(g) => sections_in(&g.entries, depth + 1, out),
        }
    }
}

/// Parses a scope name against the current location.
pub(crate) fn scope(s: &Session, name: &str) -> Result<Scope, CmdError> {
    let nb = || s.current.notebook.ok_or_else(|| CmdError::Disabled("no notebook is open".into()));
    let sec = || s.current.section.ok_or_else(|| CmdError::Disabled("no section is open".into()));
    Ok(match name {
        "all" => Scope { notebook: None, sections: None, page: None },
        "notebook" => Scope { notebook: Some(nb()?), sections: None, page: None },
        "section" => Scope { notebook: Some(nb()?), sections: Some(vec![sec()?]), page: None },
        "sectionGroup" => {
            let (nb, sec) = (nb()?, sec()?);
            let group = s.notebook(nb).and_then(|n| n.path_of(sec)).and_then(|path| path.last().copied());
            let sections = match group.and_then(|g| s.notebook(nb).and_then(|n| n.group(g))) {
                Some(g) => {
                    let mut out = Vec::new();
                    sections_in(&g.entries, 0, &mut out);
                    Some(out)
                }
                // Not in a group: the notebook is its group.
                None => None,
            };
            Scope { notebook: Some(nb), sections, page: None }
        }
        "page" => Scope {
            notebook: Some(nb()?),
            sections: Some(vec![sec()?]),
            page: Some(s.current.page.ok_or_else(|| CmdError::Disabled("no page is open".into()))?),
        },
        other => return Err(CmdError::Params(format!("unknown scope `{}`", other.chars().take(20).collect::<String>()))),
    })
}

fn search(s: &mut Session, params: &Value) -> CmdResult {
    let query: String = p::req_str(params, "query")?.chars().take(MAX_QUERY).collect();
    let limit = p::index(params, "limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let sc = scope(s, p::str(params, "scope").unwrap_or("all"))?;
    let mut hits = Vec::new();
    let mut truncated = false;
    'outer: for nb in s.notebooks.iter().filter(|n| sc.notebook.is_none_or(|id| id == n.id)) {
        for h in nb.search(&query) {
            if !sc.has_section(h.section) || sc.page.is_some_and(|p| p != h.page) {
                continue;
            }
            if hits.len() >= limit {
                truncated = true;
                break 'outer;
            }
            let section_name = nb.section(h.section).map(|x| x.name.clone()).unwrap_or_default();
            hits.push(json!({
                "notebook": nb.id, "notebookName": nb.name,
                "section": h.section, "sectionName": section_name,
                "page": h.page, "pageTitle": h.page_title, "snippet": h.snippet,
            }));
        }
    }
    Ok(CmdOutput::query(json!({"hits": hits, "truncated": truncated})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn scopes_narrow_the_search() {
        let mut s = Session::new_default();
        let hits = |s: &mut Session, scope: &str| {
            s.run(ids::SEARCH, &json!({"query": "zebra", "scope": scope})).unwrap().value["hits"].as_array().unwrap().len()
        };
        let first = s.current.section.unwrap();
        s.run(ids::PAGE_NEW, &json!({"title": "zebra one"})).unwrap();
        s.run(ids::PAGE_NEW, &json!({"title": "zebra two"})).unwrap();
        let g = s.run(ids::SECTION_GROUP_NEW, &json!({"name": "G"})).unwrap().value["id"].clone();
        let inner = s.run(ids::SECTION_NEW, &json!({"name": "Inner"})).unwrap().value["id"].clone();
        s.run(ids::SECTION_MOVE, &json!({"id": inner, "group": g, "index": 0})).unwrap();
        s.run(ids::NAV_SECTION, &json!({"id": inner})).unwrap();
        let three = s.run(ids::PAGE_NEW, &json!({"title": "zebra three"})).unwrap().value["id"].clone();
        s.run(ids::NOTEBOOK_NEW, &json!({"name": "Other"})).unwrap();
        s.run(ids::PAGE_NEW, &json!({"title": "zebra four"})).unwrap();
        assert_eq!(hits(&mut s, "all"), 4);
        assert_eq!(hits(&mut s, "notebook"), 1);
        s.run(ids::NAV_NOTEBOOK, &json!({"id": s.notebooks[0].id})).unwrap();
        s.run(ids::NAV_SECTION, &json!({"id": inner})).unwrap();
        assert_eq!(hits(&mut s, "sectionGroup"), 1);
        assert_eq!(hits(&mut s, "section"), 1);
        s.run(ids::NAV_PAGE, &json!({"id": three})).unwrap();
        assert_eq!(hits(&mut s, "page"), 1);
        s.run(ids::NAV_SECTION, &json!({"id": first})).unwrap();
        assert_eq!(hits(&mut s, "sectionGroup"), 3, "outside a group: the notebook");
        assert_eq!(hits(&mut s, "section"), 2);
        assert!(s.run(ids::SEARCH, &json!({"query": "x", "scope": "galaxy"})).is_err());
    }
}
