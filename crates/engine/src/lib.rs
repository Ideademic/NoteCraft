//! NoteCraft engine: the editing session and its commands.
//!
//! **Everything is a command.** Each user-visible action is a [`CommandSpec`] with a stable id
//! (`page.new`, `draw.stroke`), a label, a default shortcut and a `run` function. The ribbon,
//! shortcuts, the CLI and tests all call [`Session::run`], which checkpoints undo for mutating
//! commands, marks the changed sections dirty for autosave, and never panics.
//!
//! - [`Session`]: the open notebooks, the current [`Location`], undo/redo, the dirty set,
//!   autosave ([`Session::tick`]).
//! - [`registry`]: every command by id. Ids are the constants in [`ids`].
//! - [`io`]: opening files by extension, the [`io::Storage`] trait that saves notebooks
//!   (desktop [`io::FolderStorage`], web [`io::MemoryStorage`]), crash [`io::recovery`], the
//!   [`io::recent`] list.
//! - [`text`]: caret positions and rich-text editing on the model (the canvas's selection
//!   model is described in [`cmd::text`]).
//!
//! Every command's parameters are listed in `COMMANDS.md` and in [`Registry::describe`].
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod cmd;
mod history;
pub mod ids;
pub mod io;
mod session;
pub mod text;

use std::collections::HashMap;
use std::sync::LazyLock;

use serde_json::Value;

pub use history::{MAX_UNDO, Snapshot};
pub use io::{IoError, Origin, SaveJob, SaveOutcome, Storage};
pub use notecraft_doc as doc;
pub use session::{AUTOSAVE_DELAY_SECS, AUTOSAVE_MAX_WAIT_SECS, Location, Session};
pub use text::TextPos;

/// Why a command did not run.
#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum CmdError {
    #[error("unknown command `{0}`")]
    Unknown(String),
    #[error("bad parameters: {0}")]
    Params(String),
    #[error("not available: {0}")]
    Disabled(String),
    #[error("`{0}` is not implemented yet")]
    NotImplemented(&'static str),
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Io(#[from] IoError),
}

/// What a command returns: a JSON value for callers (new ids, lists…) and whether it changed
/// anything. A mutating command that reports `changed: false` leaves no undo step.
#[derive(Debug, Clone, PartialEq)]
pub struct CmdOutput {
    pub value: Value,
    pub changed: bool,
}

impl CmdOutput {
    /// Changed, nothing to report.
    pub fn ok() -> CmdOutput {
        CmdOutput { value: Value::Null, changed: true }
    }
    /// Changed, with a value.
    pub fn value(value: Value) -> CmdOutput {
        CmdOutput { value, changed: true }
    }
    /// Nothing changed (a no-op, or a query).
    pub fn unchanged() -> CmdOutput {
        CmdOutput { value: Value::Null, changed: false }
    }
    /// Nothing changed, with a value.
    pub fn query(value: Value) -> CmdOutput {
        CmdOutput { value, changed: false }
    }
}

pub type CmdResult = Result<CmdOutput, CmdError>;

/// A registered command.
#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Default shortcut in a portable form (`Mod+B`, `Mod+Shift+N`). `Mod` = Ctrl, or ⌘ on macOS.
    pub shortcut: Option<&'static str>,
    /// Parameter description for scripts and agents (`{"id": guid, "title": string}`).
    pub params: &'static str,
    /// Changes notebooks: [`Session::run`] checkpoints undo and marks sections dirty.
    pub mutates: bool,
    /// Whether the command can run now (greys out its button otherwise).
    pub enabled: fn(&Session) -> bool,
    pub run: fn(&mut Session, &Value) -> CmdResult,
}

fn always(_: &Session) -> bool {
    true
}

/// The `run` of commands not implemented yet.
pub(crate) fn not_implemented(_: &mut Session, _: &Value) -> CmdResult {
    Err(CmdError::NotImplemented(""))
}

impl CommandSpec {
    /// A mutating command, always enabled, no shortcut.
    pub const fn new(id: &'static str, label: &'static str, run: fn(&mut Session, &Value) -> CmdResult) -> Self {
        CommandSpec { id, label, shortcut: None, params: "{}", mutates: true, enabled: always, run }
    }
    /// A registered command whose `run` returns [`CmdError::NotImplemented`].
    pub const fn stub(id: &'static str, label: &'static str) -> Self {
        CommandSpec::new(id, label, not_implemented)
    }
    pub const fn key(mut self, s: &'static str) -> Self {
        self.shortcut = Some(s);
        self
    }
    pub const fn params(mut self, p: &'static str) -> Self {
        self.params = p;
        self
    }
    /// Doesn't change notebooks (navigation, export, undo/redo themselves).
    pub const fn pure(mut self) -> Self {
        self.mutates = false;
        self
    }
    pub const fn when(mut self, f: fn(&Session) -> bool) -> Self {
        self.enabled = f;
        self
    }
}

/// All commands by id.
pub struct Registry {
    specs: Vec<CommandSpec>,
    by_id: HashMap<&'static str, usize>,
}

impl Registry {
    pub fn new(specs: Vec<CommandSpec>) -> Self {
        let by_id = specs.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        Registry { specs, by_id }
    }
    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.by_id.get(id).and_then(|i| self.specs.get(*i))
    }
    pub fn all(&self) -> &[CommandSpec] {
        &self.specs
    }
    /// The command list as JSON (`notecraft-cli commands --json`).
    pub fn describe(&self) -> Value {
        Value::Array(
            self.specs
                .iter()
                .map(|s| serde_json::json!({"id": s.id, "label": s.label, "shortcut": s.shortcut, "params": s.params, "mutates": s.mutates}))
                .collect(),
        )
    }
    /// The command bound to a shortcut (`Mod+Z`), if any.
    pub fn by_shortcut(&self, key: &str) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| s.shortcut.is_some_and(|k| k.split(" / ").any(|k| k.eq_ignore_ascii_case(key))))
    }
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry::new(cmd::all()));

/// Every command.
pub fn registry() -> &'static Registry {
    &REGISTRY
}

/// Parameter helpers for command implementations.
pub mod p {
    use notecraft_doc::{Color, Guid, Point};
    use serde_json::Value;

    use crate::CmdError;

    pub fn str<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
        v.get(k).and_then(Value::as_str)
    }
    pub fn req_str<'a>(v: &'a Value, k: &str) -> Result<&'a str, CmdError> {
        str(v, k).ok_or_else(|| CmdError::Params(format!("`{k}` (string) is required")))
    }
    pub fn f32(v: &Value, k: &str) -> Option<f32> {
        v.get(k).and_then(Value::as_f64).map(|x| x as f32).filter(|x| x.is_finite())
    }
    pub fn req_f32(v: &Value, k: &str) -> Result<f32, CmdError> {
        f32(v, k).ok_or_else(|| CmdError::Params(format!("`{k}` (number) is required")))
    }
    pub fn u64(v: &Value, k: &str) -> Option<u64> {
        v.get(k).and_then(|x| x.as_u64().or_else(|| x.as_f64().filter(|f| *f >= 0.0 && f.is_finite()).map(|f| f as u64)))
    }
    /// A non-negative index, saturating on 32-bit targets.
    pub fn index(v: &Value, k: &str) -> Option<usize> {
        u64(v, k).map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    }
    pub fn bool(v: &Value, k: &str) -> Option<bool> {
        v.get(k).and_then(Value::as_bool)
    }
    /// A GUID string (`{…}` or bare). A present but malformed value is an error.
    pub fn guid(v: &Value, k: &str) -> Result<Option<Guid>, CmdError> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Guid::parse(s).map(Some).ok_or_else(|| CmdError::Params(format!("`{k}` is not a GUID"))),
            Some(_) => Err(CmdError::Params(format!("`{k}` must be a GUID string"))),
        }
    }
    pub fn req_guid(v: &Value, k: &str) -> Result<Guid, CmdError> {
        guid(v, k)?.ok_or_else(|| CmdError::Params(format!("`{k}` (GUID) is required")))
    }
    /// A `#rrggbb` colour. A present but malformed value is an error.
    pub fn color(v: &Value, k: &str) -> Result<Option<Color>, CmdError> {
        match v.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Color::parse_hex(s).map(Some).ok_or_else(|| CmdError::Params(format!("`{k}` is not a #rrggbb colour"))),
            Some(_) => Err(CmdError::Params(format!("`{k}` must be a #rrggbb string"))),
        }
    }
    pub fn req_color(v: &Value, k: &str) -> Result<Color, CmdError> {
        color(v, k)?.ok_or_else(|| CmdError::Params(format!("`{k}` (#rrggbb) is required")))
    }
    /// `x` and `y` as a page point.
    pub fn req_point(v: &Value) -> Result<Point, CmdError> {
        Ok(Point::new(req_f32(v, "x")?, req_f32(v, "y")?).sanitized())
    }
    /// A non-empty name, trimmed and cut to 255 characters.
    pub fn name(v: &Value, k: &str) -> Result<Option<String>, CmdError> {
        match str(v, k) {
            None => Ok(None),
            Some(s) => {
                let s: String = s.trim().chars().filter(|c| !c.is_control()).take(255).collect();
                if s.is_empty() { Err(CmdError::Params(format!("`{k}` must not be empty"))) } else { Ok(Some(s)) }
            }
        }
    }
}

#[cfg(test)]
mod tests;
