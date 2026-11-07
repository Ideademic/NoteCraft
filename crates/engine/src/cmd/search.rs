//! Search across the open notebooks (the search box).

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
        r##"{"query": string, "scope"?: "all" (default)|"notebook"|"section", "limit"?: number} -> {"hits": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "snippet"}], "truncated": bool}"##,
    )]
}

fn search(s: &mut Session, params: &Value) -> CmdResult {
    let query: String = p::req_str(params, "query")?.chars().take(MAX_QUERY).collect();
    let limit = p::index(params, "limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let scope = p::str(params, "scope").unwrap_or("all");
    let (only_nb, only_sec) = match scope {
        "all" => (None, None),
        "notebook" => (Some(s.current.notebook.ok_or_else(|| CmdError::Disabled("no notebook is open".into()))?), None),
        "section" => (s.current.notebook, Some(s.current.section.ok_or_else(|| CmdError::Disabled("no section is open".into()))?)),
        other => return Err(CmdError::Params(format!("unknown scope `{}`", other.chars().take(20).collect::<String>()))),
    };
    let mut hits = Vec::new();
    let mut truncated = false;
    'outer: for nb in s.notebooks.iter().filter(|n| only_nb.is_none_or(|id| id == n.id)) {
        for h in nb.search(&query) {
            if only_sec.is_some_and(|id| id != h.section) {
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
