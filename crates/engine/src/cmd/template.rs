//! Insert › Page Templates: list templates, add a page from one, save the current page as one,
//! and choose the template new pages in a section start from.

use notecraft_doc::Guid;
use serde_json::{Value, json};

use super::{has_page, has_section, target_page};
use crate::ids::{PAGE_NEW_FROM_TEMPLATE, SECTION_DEFAULT_TEMPLATE, TEMPLATE_DELETE, TEMPLATE_LIST, TEMPLATE_SAVE_CURRENT};
use crate::templates::{self, MAX_TEMPLATE_BYTES, MAX_USER_TEMPLATES, Template, USER_CATEGORY};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(TEMPLATE_LIST, "Page Templates", list)
            .pure()
            .params(r##"{} -> {"templates": [{"id", "name", "category", "builtin": bool}]} (built-in first, then "My Templates")"##),
        CommandSpec::new(TEMPLATE_SAVE_CURRENT, "Save Current Page as a Template", save_current)
            .pure()
            .when(has_page)
            .params(r##"{"name": string, "page"?: guid} -> {"template": Template} (kept in Session::templates; the app stores them)"##),
        CommandSpec::new(TEMPLATE_DELETE, "Delete Template", delete).pure().params(r##"{"id": string (a user template)}"##),
        CommandSpec::new(PAGE_NEW_FROM_TEMPLATE, "New Page from Template", new_from_template)
            .when(has_section)
            .params(r##"{"template": string (id), "title"?: string, "section"?: guid} -> {"id": guid}"##),
        CommandSpec::new(SECTION_DEFAULT_TEMPLATE, "Default Template", default_template)
            .when(has_section)
            .params(r##"{"template": string (id) | null (none), "section"?: guid}"##),
    ]
}

/// The template `id`: built-in, else one of the user's.
pub(crate) fn find(s: &Session, id: &str) -> Option<Template> {
    templates::builtin(id).or_else(|| s.templates.iter().find(|t| t.id == id).cloned())
}

fn list(s: &mut Session, _: &Value) -> CmdResult {
    let row = |t: &Template| json!({"id": t.id, "name": t.name, "category": t.category, "builtin": t.is_builtin()});
    let rows: Vec<Value> = templates::builtins().iter().chain(s.templates.iter()).map(row).collect();
    Ok(CmdOutput::query(json!({ "templates": rows })))
}

fn save_current(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::name(params, "name")?.ok_or_else(|| CmdError::Params("`name` is required".into()))?;
    let page = target_page(s, params)?;
    let mut copy = page.clone();
    copy.versions.clear();
    copy.level = 0;
    let size = serde_json::to_vec(&copy).map(|b| b.len()).unwrap_or(usize::MAX);
    if size > MAX_TEMPLATE_BYTES {
        return Err(CmdError::Failed("the page is too large to keep as a template".into()));
    }
    let existing = s.templates.iter().position(|t| t.name.eq_ignore_ascii_case(&name));
    if existing.is_none() && s.templates.len() >= MAX_USER_TEMPLATES {
        return Err(CmdError::Failed(format!("at most {MAX_USER_TEMPLATES} templates are kept")));
    }
    let id = existing.and_then(|i| s.templates.get(i)).map_or_else(|| format!("user:{}", Guid::new()), |t| t.id.clone());
    let t = Template { id, name, category: USER_CATEGORY.to_owned(), page: copy };
    match existing.and_then(|i| s.templates.get_mut(i)) {
        Some(slot) => *slot = t.clone(),
        None => s.templates.push(t.clone()),
    }
    Ok(CmdOutput::query(json!({ "template": t })))
}

fn delete(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_str(params, "id")?;
    let before = s.templates.len();
    s.templates.retain(|t| t.id != id);
    if s.templates.len() == before {
        return Err(CmdError::Params("no such user template".into()));
    }
    Ok(CmdOutput::unchanged())
}

fn new_from_template(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_str(params, "template")?;
    let t = find(s, id).ok_or_else(|| CmdError::Params(format!("no template `{}`", id.chars().take(64).collect::<String>())))?;
    let mut page = t.instantiate();
    if let Some(title) = p::str(params, "title") {
        page.title = super::page::clean_title(title);
    }
    super::page::insert_built_page(s, p::guid(params, "section")?, page, false)
}

fn default_template(s: &mut Session, params: &Value) -> CmdResult {
    let template = match params.get("template") {
        Some(Value::Null) => None,
        Some(Value::String(id)) => {
            if find(s, id).is_none() {
                return Err(CmdError::Params(format!("no template `{}`", id.chars().take(64).collect::<String>())));
            }
            Some(id.clone())
        }
        _ => return Err(CmdError::Params("`template` (an id, or null) is required".into())),
    };
    let sec_id = p::guid(params, "section")?.or(s.current.section).ok_or_else(|| CmdError::Disabled("no section is open".into()))?;
    let sec = s.section_mut(sec_id).ok_or_else(|| CmdError::Params(format!("no section {sec_id}")))?;
    if sec.default_template == template {
        return Ok(CmdOutput::unchanged());
    }
    sec.default_template = template;
    Ok(CmdOutput::ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids;

    #[test]
    fn templates_make_pages_and_section_defaults() {
        let mut s = Session::new_default();
        let v = s.run(ids::TEMPLATE_LIST, &json!({})).unwrap().value;
        assert!(v["templates"].as_array().unwrap().len() >= 7);
        let id = s.run(ids::PAGE_NEW_FROM_TEMPLATE, &json!({"template": "nc:meeting"})).unwrap().value["id"].clone();
        let page = s.current_page().unwrap();
        assert_eq!(page.id.to_string(), id.as_str().unwrap());
        assert_eq!(page.title, "Meeting");
        assert!(!page.elements.is_empty());
        assert!(s.run(ids::PAGE_NEW_FROM_TEMPLATE, &json!({"template": "nc:nope"})).is_err());
        // Save it as mine, then make it the section's default.
        s.run(ids::PAGE_RENAME, &json!({"title": "Standup"})).unwrap();
        let t = s.run(ids::TEMPLATE_SAVE_CURRENT, &json!({"name": "Standup"})).unwrap().value["template"].clone();
        let tid = t["id"].as_str().unwrap().to_owned();
        assert!(tid.starts_with("user:"));
        assert_eq!(s.templates.len(), 1);
        s.run(ids::TEMPLATE_SAVE_CURRENT, &json!({"name": "standup"})).unwrap();
        assert_eq!(s.templates.len(), 1, "the same name replaces it");
        s.run(ids::SECTION_DEFAULT_TEMPLATE, &json!({"template": tid})).unwrap();
        s.run(ids::PAGE_NEW, &json!({})).unwrap();
        assert_eq!(s.current_page().unwrap().title, "Standup", "new pages follow the default");
        s.run(ids::PAGE_NEW, &json!({"blank": true})).unwrap();
        assert!(s.current_page().unwrap().elements.is_empty());
        s.run(ids::PAGE_NEW, &json!({"title": "Named"})).unwrap();
        assert_eq!(s.current_page().unwrap().title, "Named");
        assert!(!s.current_page().unwrap().elements.is_empty());
        s.run(ids::SECTION_DEFAULT_TEMPLATE, &json!({"template": null})).unwrap();
        assert!(s.current_section().unwrap().default_template.is_none());
        assert!(s.run(ids::SECTION_DEFAULT_TEMPLATE, &json!({"template": "user:gone"})).is_err());
        s.run(ids::TEMPLATE_DELETE, &json!({"id": tid})).unwrap();
        assert!(s.templates.is_empty());
    }
}
