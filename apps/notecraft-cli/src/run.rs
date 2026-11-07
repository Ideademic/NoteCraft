//! `run`: engine commands headlessly, then save.
//!
//! `--cmd 'id={json}'` (or a bare `id`) runs one command; `--script FILE` (`-`: stdin) runs a JSON
//! script first: `{"notebook"?: path, "commands": [{"command": id, "params"?: {…}} | id, …],
//! "save"?: [path, …]}`. Each command's output is printed as one JSON line
//! (`{"command", "changed", "value"}`); the first error stops the run.

use std::io::Read as _;

use notecraft_engine::Session;
use notecraft_engine::io::Origin;
use serde_json::{Value, json};

use crate::Fail;

/// Largest script read.
const MAX_SCRIPT_BYTES: u64 = 64 << 20;
/// Most commands one run takes.
const MAX_COMMANDS: usize = 100_000;

/// What to run.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    pub notebook: Option<String>,
    pub commands: Vec<(String, Value)>,
    pub saves: Vec<String>,
}

/// `id={json}` or `id`.
pub fn parse_cmd(s: &str) -> Result<(String, Value), Fail> {
    let (id, params) = match s.split_once('=') {
        Some((id, json)) => {
            let v: Value = serde_json::from_str(json).map_err(|e| Fail::usage(format!("--cmd {}: params aren't JSON: {e}", id.trim())))?;
            (id.trim(), v)
        }
        None => (s.trim(), json!({})),
    };
    if id.is_empty() {
        return Err(Fail::usage("--cmd needs a command id (`notecraft-cli commands` lists them)"));
    }
    Ok((id.to_owned(), params))
}

/// Adds a script's notebook, commands and saves to `plan`.
pub fn add_script(plan: &mut Plan, source: &str) -> Result<(), Fail> {
    let mut text = String::new();
    let read = if source == "-" {
        std::io::stdin().take(MAX_SCRIPT_BYTES).read_to_string(&mut text)
    } else {
        std::fs::File::open(source).and_then(|f| f.take(MAX_SCRIPT_BYTES).read_to_string(&mut text))
    };
    read.map_err(|e| Fail::new(format!("--script {source}: {e}")))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| Fail::usage(format!("--script {source}: not JSON: {e}")))?;
    let bad = |what: &str| Fail::usage(format!("--script {source}: {what}"));
    if let Some(nb) = v.get("notebook") {
        plan.notebook = Some(nb.as_str().ok_or_else(|| bad("`notebook` must be a path"))?.to_owned());
    }
    for c in v.get("commands").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        let entry = match c {
            Value::String(id) => (id.clone(), json!({})),
            Value::Object(o) => {
                let id = o.get("command").and_then(Value::as_str).ok_or_else(|| bad("each command needs a `command` id"))?;
                (id.to_owned(), o.get("params").cloned().unwrap_or_else(|| json!({})))
            }
            _ => return Err(bad("`commands` holds ids or {\"command\", \"params\"} objects")),
        };
        plan.commands.push(entry);
    }
    match v.get("save") {
        None => {}
        Some(Value::String(s)) => plan.saves.push(s.clone()),
        Some(Value::Array(a)) => {
            for s in a {
                plan.saves.push(s.as_str().ok_or_else(|| bad("`save` holds paths"))?.to_owned());
            }
        }
        Some(_) => return Err(bad("`save` is a path or a list of paths")),
    }
    Ok(())
}

/// Runs the plan; returns the output lines.
pub fn run(plan: &Plan) -> Result<String, Fail> {
    if plan.commands.len() > MAX_COMMANDS {
        return Err(Fail::usage(format!("more than {MAX_COMMANDS} commands")));
    }
    let mut session = match &plan.notebook {
        Some(path) => {
            let nb = crate::convert::open_input(path)?;
            let origin = if path.to_ascii_lowercase().ends_with(".json") {
                Origin::Scratch
            } else {
                notecraft_engine::io::origin_of(std::path::Path::new(path))
            };
            Session::new(nb, origin)
        }
        None => Session::new_default(),
    };
    let mut out = String::new();
    for (id, params) in &plan.commands {
        if notecraft_engine::registry().get(id).is_none() {
            return Err(Fail::usage(format!("unknown command `{}` (`notecraft-cli commands` lists them)", id.chars().take(60).collect::<String>())));
        }
        let r = session.run(id, params).map_err(|e| Fail::new(format!("{id}: {e}")))?;
        out.push_str(&json!({"command": id, "changed": r.changed, "value": r.value}).to_string());
        out.push('\n');
    }
    for save in &plan.saves {
        let nb = session.current_notebook().ok_or_else(|| Fail::new("no notebook is open to save"))?;
        let what = crate::convert::write(nb, save)?;
        out.push_str(&json!({"saved": what}).to_string());
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_forms() {
        assert_eq!(parse_cmd("page.new").unwrap(), ("page.new".into(), json!({})));
        assert_eq!(parse_cmd("page.new={\"title\":\"A=B\"}").unwrap(), ("page.new".into(), json!({"title": "A=B"})));
        assert!(matches!(parse_cmd("page.new={bad"), Err(Fail { code: 2, .. })));
        assert!(matches!(parse_cmd("={}"), Err(Fail { code: 2, .. })));
    }

    #[test]
    fn runs_commands() {
        let plan = Plan { commands: vec![parse_cmd("page.new={\"title\":\"Plans\"}").unwrap()], ..Plan::default() };
        let out = run(&plan).unwrap();
        let line: Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
        assert_eq!(line["command"], "page.new");
        assert_eq!(line["changed"], true);
        let bad = Plan { commands: vec![("no.such".into(), json!({}))], ..Plan::default() };
        assert!(matches!(run(&bad), Err(Fail { code: 2, .. })));
        let failing = Plan { commands: vec![("nav.page".into(), json!({"id": "nope"}))], ..Plan::default() };
        assert!(matches!(run(&failing), Err(Fail { code: 1, .. })));
    }
}
