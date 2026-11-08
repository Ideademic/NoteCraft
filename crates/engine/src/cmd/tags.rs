//! Home › Find Tags: the Tags Summary, every tagged paragraph in a scope.

use notecraft_doc::{ElementKind, Tag};
use serde_json::{Value, json};

use super::search::scope;
use crate::ids::TAGS_SUMMARY;
use crate::{CmdOutput, CmdResult, CommandSpec, Session, p};

/// Most items listed by default.
pub const DEFAULT_LIMIT: usize = 2_000;
/// Most items listed at all.
pub const MAX_LIMIT: usize = 10_000;
/// Longest paragraph text in an item, characters.
const MAX_TEXT: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new(TAGS_SUMMARY, "Find Tags", summary).pure().params(
        r##"{"scope"?: "page"|"section"|"sectionGroup"|"notebook" (default)|"all", "uncheckedOnly"?: bool (only To Do items not ticked), "limit"?: number} -> {"items": [{"notebook", "notebookName", "section", "sectionName", "page", "pageTitle", "outline", "para", "tag", "label", "checked": bool|null, "text", "modified"}], "truncated": bool}"##,
    )]
}

fn summary(s: &mut Session, params: &Value) -> CmdResult {
    let sc = scope(s, p::str(params, "scope").unwrap_or("notebook"))?;
    let unchecked_only = p::bool(params, "uncheckedOnly").unwrap_or(false);
    let limit = p::index(params, "limit").unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut items = Vec::new();
    let mut truncated = false;
    'all: for nb in s.notebooks.iter().filter(|n| sc.notebook.is_none_or(|id| id == n.id)) {
        for sec in nb.all_sections().into_iter().filter(|x| sc.has_section(x.id)) {
            for page in sec.pages.iter().filter(|pg| sc.page.is_none_or(|id| id == pg.id)) {
                for el in &page.elements {
                    let ElementKind::Outline(o) = &el.kind else { continue };
                    for (frame, pi, para) in crate::text::all_paragraphs(o) {
                        let Some(tag) = para.tag else { continue };
                        let checked = match tag {
                            Tag::ToDo { checked } => Some(checked),
                            _ => None,
                        };
                        if unchecked_only && checked != Some(false) {
                            continue;
                        }
                        if items.len() >= limit {
                            truncated = true;
                            break 'all;
                        }
                        let text: String = para.text().chars().take(MAX_TEXT).collect();
                        let mut item = json!({
                            "notebook": nb.id, "notebookName": nb.name,
                            "section": sec.id, "sectionName": sec.name,
                            "page": page.id, "pageTitle": page.title,
                            "outline": el.id, "para": pi,
                            "tag": tag.name(), "label": tag.label(), "checked": checked,
                            "text": text, "modified": page.modified.to_rfc3339(),
                        });
                        // A paragraph in a table cell names the cell.
                        if let (Some(c), Value::Object(m)) = (frame.cell(), &mut item) {
                            m.insert("cell".into(), c.to_json());
                        }
                        items.push(item);
                    }
                }
            }
        }
    }
    Ok(CmdOutput::query(json!({"items": items, "truncated": truncated})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn the_summary_lists_tagged_paragraphs() {
        let mut s = Session::new_default();
        s.run(ids::PAGE_NEW, &json!({"title": "Tasks"})).unwrap();
        let o = s.run(ids::OUTLINE_NEW, &json!({"x": 40, "y": 100, "text": "buy milk\ncall Ana\nplain\nbig idea"})).unwrap().value["id"].clone();
        s.run(ids::FORMAT_TAG_TODO, &json!({"outline": o, "paras": [0, 1]})).unwrap();
        s.run(ids::FORMAT_TOGGLE_TODO, &json!({"outline": o, "para": 1})).unwrap();
        s.run(ids::FORMAT_TAG, &json!({"outline": o, "para": 3, "tag": "idea"})).unwrap();
        let v = s.run(ids::TAGS_SUMMARY, &json!({})).unwrap().value;
        let items = v["items"].as_array().unwrap();
        let mine: Vec<&Value> = items.iter().filter(|i| i["pageTitle"] == "Tasks").collect();
        assert_eq!(mine.len(), 3);
        assert_eq!(mine[0]["text"], "buy milk");
        assert_eq!(mine[0]["checked"], false);
        assert_eq!(mine[1]["checked"], true);
        assert_eq!(mine[2]["tag"], "idea");
        assert_eq!(mine[2]["checked"], Value::Null);
        let v = s.run(ids::TAGS_SUMMARY, &json!({"scope": "page", "uncheckedOnly": true})).unwrap().value;
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        let v = s.run(ids::TAGS_SUMMARY, &json!({"scope": "all", "limit": 1})).unwrap().value;
        assert_eq!(v["truncated"], true);
    }
}
