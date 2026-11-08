//! `notecraft-cli`: NoteCraft from the command line: inspect, dump and convert notebooks, run
//! engine commands headlessly, list the commands.
//!
//! Exit codes: 0 success, 1 a failure (a file that can't be read or written, a command that
//! failed), 2 a usage error.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod args;
mod convert;
mod run;
mod store;
mod summary;

use std::io::Write;
use std::process::ExitCode;

use args::{flag, valued};

const USAGE: &str = "\
notecraft-cli — NoteCraft from the command line

USAGE:
  notecraft-cli info <path> [--section S] [--page P]
        the notebook, its section tree and pages with counts (JSON)
  notecraft-cli dump <path> [--section S] [--page P] [--json]
        every page with its text; --json: the whole model as JSON
  notecraft-cli dump --store <file.one | file.onetoc2 | file.onepkg>
        the low-level revision store objects (MS-ONESTORE) as JSON
  notecraft-cli convert <in> <out> [--section S] [--page P]
        convert by <out>'s extension: .onepkg, .json, .one (one section), .html (one file,
        every page), .md, .txt, .pdf, or a notebook folder (<out> ending in / or
        Open Notebook.onetoc2, or an existing folder)
        PDF: [--scope page|section|notebook] [--paper fit|letter|a4|legal|a5] [--landscape]
        [--no-header] [--bundled-fonts] (default: every page, each one sheet its size)
  notecraft-cli run [--notebook <in>] [--script FILE|-] [--cmd 'id={json}']… [--save OUT]…
        run engine commands headlessly (on <in>, else on a new \"My Notebook\"), print each
        result as a JSON line, then save the current notebook (by extension, as convert).
        A script is JSON: {\"notebook\"?, \"commands\": [{\"command\": id, \"params\"?} | id, …],
        \"save\"?: [OUT, …]}; it runs before the --cmd commands
  notecraft-cli commands [--json]
        every command id with its label and shortcut; --json adds the params
  notecraft-cli [<command>] -h | --help
  notecraft-cli -V | --version

<path>/<in> is a notebook folder, Open Notebook.onetoc2, a .onepkg, a .one section or a
NoteCraft .json notebook. --section and --page take a name (case-insensitive) or a GUID; a
selected section group keeps every section in it.

Exit codes: 0 success, 1 failure, 2 usage error.
";

const SUBCOMMANDS: &[&str] = &["info", "dump", "convert", "run", "commands"];

/// A failure with its exit code (1: failed, 2: usage).
#[derive(Debug)]
pub struct Fail {
    pub code: u8,
    pub msg: String,
}

impl Fail {
    pub fn new(msg: impl Into<String>) -> Fail {
        Fail { code: 1, msg: msg.into() }
    }

    pub fn usage(msg: impl Into<String>) -> Fail {
        Fail { code: 2, msg: msg.into() }
    }
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for (i, a) in std::env::args_os().skip(1).enumerate() {
        match a.into_string() {
            Ok(s) => args.push(s),
            Err(a) => {
                let _ = writeln!(std::io::stderr(), "notecraft-cli: argument {} is not valid Unicode: {}", i + 1, a.to_string_lossy());
                return ExitCode::from(2);
            }
        }
    }
    match run(&args) {
        Ok(out) => {
            let mut stdout = std::io::stdout();
            // A closed pipe (`| head`) is not a crash, just the end.
            if stdout.write_all(out.as_bytes()).and_then(|()| stdout.flush()).is_err() {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(Fail { code, msg }) => {
            let _ = writeln!(std::io::stderr(), "notecraft-cli: {msg}");
            ExitCode::from(code)
        }
    }
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).map(|s| s + "\n").unwrap_or_default()
}

/// The single positional argument a subcommand needs.
fn one_path<'a>(cmd: &str, p: &'a args::Parsed) -> Result<&'a str, Fail> {
    match p.positionals.as_slice() {
        [path] => Ok(path.as_str()),
        [] => Err(Fail::usage(format!("{cmd}: missing <path>\n\n{USAGE}"))),
        _ => Err(Fail::usage(format!("{cmd}: one <path> only\n\n{USAGE}"))),
    }
}

fn run(args: &[String]) -> Result<String, Fail> {
    let Some((cmd, rest)) = args.split_first() else { return Ok(USAGE.to_owned()) };
    let cmd = cmd.as_str();
    if matches!(cmd, "-h" | "--help" | "help") || (SUBCOMMANDS.contains(&cmd) && rest.iter().any(|a| a == "-h" || a == "--help")) {
        return Ok(USAGE.to_owned());
    }
    match cmd {
        "-V" | "--version" | "version" => Ok(format!("notecraft-cli {}\n", env!("CARGO_PKG_VERSION"))),
        "info" => {
            let p = args::parse(cmd, rest, &[valued("--section"), valued("--page")])?;
            let nb = convert::open_input(one_path(cmd, &p)?)?;
            let nb = summary::select(&nb, p.value("--section"), p.value("--page"))?;
            Ok(pretty(&summary::info(&nb)))
        }
        "dump" => {
            let p = args::parse(cmd, rest, &[valued("--section"), valued("--page"), flag("--json"), flag("--store")])?;
            let path = one_path(cmd, &p)?;
            if p.has("--store") {
                let bytes = std::fs::read(path).map_err(|e| Fail::new(format!("{path}: {e}")))?;
                return store::dump_store(path, &bytes).map(|v| pretty(&v)).map_err(Fail::new);
            }
            let nb = summary::select(&convert::open_input(path)?, p.value("--section"), p.value("--page"))?;
            if p.has("--json") {
                return serde_json::to_string_pretty(&nb).map(|s| s + "\n").map_err(|e| Fail::new(e.to_string()));
            }
            Ok(summary::dump(&nb))
        }
        "convert" => {
            let p = args::parse(
                cmd,
                rest,
                &[
                    valued("--section"),
                    valued("--page"),
                    valued("--scope"),
                    valued("--paper"),
                    flag("--landscape"),
                    flag("--no-header"),
                    flag("--bundled-fonts"),
                ],
            )?;
            let [input, output] = p.positionals.as_slice() else {
                return Err(Fail::usage(format!("convert: needs <in> and <out>\n\n{USAGE}")));
            };
            // Check the output kind before reading a large input.
            convert::OutKind::of(output)?;
            let pdf = convert::PdfArgs::parse(
                p.value("--scope"),
                p.value("--paper"),
                p.has("--landscape"),
                p.has("--no-header"),
                p.has("--bundled-fonts"),
            )?;
            let nb = summary::select(&convert::open_input(input)?, p.value("--section"), p.value("--page"))?;
            let what = convert::write_with(&nb, output, &pdf)?;
            Ok(format!("wrote {what}\n"))
        }
        "run" => {
            let p = args::parse(cmd, rest, &[valued("--notebook"), valued("--script"), valued("--cmd"), valued("--save")])?;
            if let Some(extra) = p.positionals.first() {
                return Err(Fail::usage(format!("run: unexpected `{extra}` (commands go in --cmd)")));
            }
            let mut plan = run::Plan::default();
            if let Some(script) = p.value("--script") {
                run::add_script(&mut plan, script)?;
            }
            if let Some(nb) = p.value("--notebook") {
                plan.notebook = Some(nb.to_owned());
            }
            for c in p.values("--cmd") {
                plan.commands.push(run::parse_cmd(c)?);
            }
            plan.saves.extend(p.values("--save").into_iter().map(str::to_owned));
            for s in &plan.saves {
                convert::OutKind::of(s)?;
            }
            run::run(&plan)
        }
        "commands" => {
            let p = args::parse(cmd, rest, &[flag("--json")])?;
            if !p.positionals.is_empty() {
                return Err(Fail::usage("commands: takes no arguments"));
            }
            if p.has("--json") {
                return Ok(pretty(&notecraft_engine::registry().describe()));
            }
            Ok(notecraft_engine::registry()
                .all()
                .iter()
                .map(|s| format!("{:<26} {:<36} {}\n", s.id, s.label, s.shortcut.unwrap_or("")).trim_end().to_owned() + "\n")
                .collect())
        }
        other => Err(Fail::usage(format!("unknown command `{}`\n\n{USAGE}", other.chars().take(60).collect::<String>()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn usage_and_commands() {
        assert!(run(&[]).is_ok_and(|s| s.contains("USAGE")));
        assert!(run(&s(&["convert", "--help"])).is_ok_and(|s| s.contains("USAGE")));
        assert!(run(&s(&["--version"])).is_ok_and(|s| s.starts_with("notecraft-cli ")));
        assert!(run(&s(&["commands"])).is_ok_and(|s| s.contains("page.new")));
        assert!(run(&s(&["commands", "--json"])).is_ok_and(|s| s.contains("\"params\"")));
        assert!(matches!(run(&s(&["bogus"])), Err(Fail { code: 2, .. })));
        assert!(matches!(run(&s(&["info"])), Err(Fail { code: 2, .. })));
        assert!(matches!(run(&s(&["info", "/no/such/file.one"])), Err(Fail { code: 1, .. })));
        assert!(matches!(run(&s(&["convert", "a.one", "b.docx"])), Err(Fail { code: 2, .. })));
        assert!(matches!(run(&s(&["run", "page.new"])), Err(Fail { code: 2, .. })));
    }
}
