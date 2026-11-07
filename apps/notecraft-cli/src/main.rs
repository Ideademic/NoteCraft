//! `notecraft-cli`: NoteCraft from the command line.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use notecraft_doc::{Notebook, Section};
use serde_json::{Value, json};

const USAGE: &str = "\
notecraft-cli — NoteCraft from the command line

USAGE:
  notecraft-cli info <path>             notebook name, sections, page counts (JSON)
  notecraft-cli dump <path>             the whole notebook as JSON
  notecraft-cli convert <in> <out>      convert by the output's extension: .onepkg, .one (first
                                        section), .html / .md / .txt (every page), .json
  notecraft-cli commands [--json]       list every command
  notecraft-cli -h | --help             this help
  notecraft-cli --version

<path> is a notebook folder, Open Notebook.onetoc2, a .onepkg or a .one file.
";

/// A failure with its exit code (1: failed, 2: usage).
struct Fail(u8, String);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&refs) {
        Ok(out) => {
            let mut stdout = std::io::stdout();
            if stdout.write_all(out.as_bytes()).and_then(|()| stdout.flush()).is_err() {
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(Fail(code, msg)) => {
            let _ = writeln!(std::io::stderr(), "notecraft-cli: {msg}");
            ExitCode::from(code)
        }
    }
}

fn run(args: &[&str]) -> Result<String, Fail> {
    match args {
        [] | ["-h" | "--help" | "help"] => Ok(USAGE.to_owned()),
        ["--version" | "-V"] => Ok(format!("notecraft-cli {}\n", env!("CARGO_PKG_VERSION"))),
        ["info", path] => Ok(pretty(&info(&open(path)?))),
        ["dump", path] => serde_json::to_string_pretty(&open(path)?).map(|s| s + "\n").map_err(|e| Fail(1, e.to_string())),
        ["convert", input, output] => convert(&open(input)?, output).map(|()| String::new()),
        ["commands"] => Ok(notecraft_engine::registry().all().iter().map(|s| format!("{:<28} {}\n", s.id, s.label)).collect()),
        ["commands", "--json"] => Ok(pretty(&notecraft_engine::registry().describe())),
        _ => Err(Fail(2, format!("unrecognised arguments\n\n{USAGE}"))),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).map(|s| s + "\n").unwrap_or_default()
}

fn open(path: &str) -> Result<Notebook, Fail> {
    notecraft_engine::io::open_path(Path::new(path)).map_err(|e| match e {
        // Already names the file.
        notecraft_engine::IoError::Io(m) => Fail(1, m),
        e => Fail(1, format!("{path}: {e}")),
    })
}

fn info(nb: &Notebook) -> Value {
    let sections: Vec<Value> = nb
        .all_sections()
        .iter()
        .map(|s| json!({"name": s.name, "color": s.color, "pages": s.pages.len(), "unsupportedContent": s.source.unsupported_content}))
        .collect();
    json!({"name": nb.name, "id": nb.id, "color": nb.color, "sections": sections, "pages": nb.all_sections().iter().map(|s| s.pages.len()).sum::<usize>()})
}

fn convert(nb: &Notebook, out: &str) -> Result<(), Fail> {
    let ext = out.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let fail = |e: &dyn std::fmt::Display| Fail(1, e.to_string());
    let bytes: Vec<u8> = match ext.as_str() {
        "onepkg" => notecraft_onepkg::write(nb).map_err(|e| fail(&e))?,
        "one" => {
            let s: &Section = nb.first_section().ok_or_else(|| Fail(1, "the notebook has no sections".into()))?;
            notecraft_one::write_section(s).map_err(|e| fail(&e))?
        }
        "json" => serde_json::to_vec_pretty(nb).map_err(|e| fail(&e))?,
        "html" | "htm" => {
            let mut html = String::new();
            for s in nb.all_sections() {
                html.push_str(&notecraft_formats::section_to_html(s).map_err(|e| fail(&e))?);
            }
            html.into_bytes()
        }
        "md" | "txt" => {
            let mut text = String::new();
            for p in nb.all_sections().iter().flat_map(|s| s.pages.iter()) {
                let one = if ext == "md" { notecraft_formats::page_to_markdown(p) } else { notecraft_formats::page_to_text(p) };
                text.push_str(&one.map_err(|e| fail(&e))?);
                text.push_str("\n\n");
            }
            text.into_bytes()
        }
        _ => return Err(Fail(2, format!("can't convert to `.{ext}` (onepkg, one, html, md, txt, json)"))),
    };
    std::fs::write(out, bytes).map_err(|e| Fail(1, format!("{out}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_and_commands() {
        assert!(run(&[]).is_ok_and(|s| s.contains("USAGE")));
        assert!(run(&["commands"]).is_ok_and(|s| s.contains("page.new")));
        assert!(matches!(run(&["bogus"]), Err(Fail(2, _))));
        assert!(matches!(run(&["info", "/no/such/file.one"]), Err(Fail(1, _))));
    }

    #[test]
    fn info_and_json_convert() {
        let nb = Notebook::new_default();
        let v = info(&nb);
        assert_eq!(v["sections"][0]["name"], "Quick Notes");
        let out = std::env::temp_dir().join(format!("notecraft-cli-{}.json", std::process::id()));
        let out_s = out.to_string_lossy().into_owned();
        convert(&nb, &out_s).map_err(|Fail(_, m)| m).unwrap();
        let back: Notebook = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(back, nb);
        assert!(matches!(convert(&nb, "x.docx"), Err(Fail(2, _))));
    }
}
