//! Undo and redo.

use serde_json::Value;

use crate::ids::{EDIT_REDO, EDIT_UNDO};
use crate::{CmdError, CmdOutput, CmdResult, CommandSpec, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        // `pure`: they restore snapshots themselves rather than recording one.
        CommandSpec::new(EDIT_UNDO, "Undo", undo).pure().key("Mod+Z").when(Session::can_undo),
        CommandSpec::new(EDIT_REDO, "Redo", redo).pure().key("Mod+Y / Mod+Shift+Z").when(Session::can_redo),
    ]
}

fn undo(s: &mut Session, _: &Value) -> CmdResult {
    if s.undo() { Ok(CmdOutput::ok()) } else { Err(CmdError::Disabled("nothing to undo".into())) }
}

fn redo(s: &mut Session, _: &Value) -> CmdResult {
    if s.redo() { Ok(CmdOutput::ok()) } else { Err(CmdError::Disabled("nothing to redo".into())) }
}
