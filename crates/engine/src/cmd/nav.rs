//! Navigation: go to a notebook, section or page. Not undoable.

use serde_json::Value;

use crate::ids::{NAV_NOTEBOOK, NAV_PAGE, NAV_SECTION};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Location, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(NAV_NOTEBOOK, "Go to Notebook", notebook).pure().params(r##"{"id": guid}"##),
        CommandSpec::new(NAV_SECTION, "Go to Section", section).pure().params(r##"{"id": guid}"##),
        CommandSpec::new(NAV_PAGE, "Go to Page", page).pure().params(r##"{"id": guid}"##),
    ]
}

fn notebook(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    s.notebook(id).ok_or_else(|| CmdError::Params(format!("no notebook {id}")))?;
    s.current = Location { notebook: Some(id), section: None, page: None };
    Ok(CmdOutput::unchanged())
}

fn section(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let nb = s.notebook_of_section(id).ok_or_else(|| CmdError::Params(format!("no section {id}")))?;
    s.current = Location { notebook: Some(nb.id), section: Some(id), page: None };
    Ok(CmdOutput::unchanged())
}

fn page(s: &mut Session, params: &Value) -> CmdResult {
    let id = p::req_guid(params, "id")?;
    let (nb, sec) =
        s.notebooks.iter().find_map(|n| n.section_of_page(id).map(|sec| (n.id, sec))).ok_or_else(|| CmdError::Params(format!("no page {id}")))?;
    s.current = Location { notebook: Some(nb), section: Some(sec), page: Some(id) };
    Ok(CmdOutput::unchanged())
}
