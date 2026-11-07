//! View tab: rule lines, page colour.

use notecraft_doc::RuleLines;
use serde_json::Value;

use super::{has_page, target_page};
use crate::ids::{VIEW_PAGE_COLOR, VIEW_RULE_LINES};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session, p};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new(VIEW_RULE_LINES, "Rule Lines", rule_lines)
            .when(has_page)
            .params(r##"{"rule": "none"|"narrow"|"college"|"standard"|"wide"|"gridSmall"|"gridMedium"|"gridLarge", "page"?: guid}"##),
        CommandSpec::new(VIEW_PAGE_COLOR, "Page Color", page_color).when(has_page).params(r##"{"color": "#rrggbb" | null, "page"?: guid}"##),
    ]
}

fn rule_lines(s: &mut Session, params: &Value) -> CmdResult {
    let name = p::req_str(params, "rule")?;
    let rule =
        RuleLines::parse(name).ok_or_else(|| CmdError::Params(format!("unknown rule lines `{}`", name.chars().take(20).collect::<String>())))?;
    let page = target_page(s, params)?;
    if page.background.rule == rule {
        return Ok(CmdOutput::unchanged());
    }
    page.background.rule = rule;
    Ok(CmdOutput::ok())
}

/// `color: null` goes back to no colour.
fn page_color(s: &mut Session, params: &Value) -> CmdResult {
    if params.get("color").is_none() {
        return Err(CmdError::Params("`color` (#rrggbb or null) is required".into()));
    }
    let color = p::color(params, "color")?;
    let page = target_page(s, params)?;
    if page.background.color == color {
        return Ok(CmdOutput::unchanged());
    }
    page.background.color = color;
    Ok(CmdOutput::ok())
}
