//! End-to-end tests of the `notecraft-cli` binary. Every input is made here (through `run`), so
//! the tests don't need the sample corpus.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_notecraft-cli");

/// A scratch folder removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("notecraft-cli-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = cli(args);
    assert!(out.status.success(), "{args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn code(args: &[&str]) -> i32 {
    cli(args).status.code().unwrap_or(-1)
}

fn info(path: &str) -> Value {
    serde_json::from_str(&ok(&["info", path])).unwrap()
}

/// What two notebooks must agree on after a conversion.
fn shape(v: &Value) -> Value {
    serde_json::json!({"name": v["name"], "sections": v["sections"], "pages": v["pages"], "totals": v["totals"]})
}

/// A notebook with two sections, typed text and ink, saved as `.onepkg` and `.json`.
fn make_notebook(s: &Scratch) -> (String, String) {
    let pkg = s.path("made.onepkg");
    let json = s.path("made.json");
    let out = ok(&[
        "run",
        "--cmd",
        r#"notebook.rename={"name":"Field Notes"}"#,
        "--cmd",
        r#"page.new={"title":"Birds"}"#,
        "--cmd",
        r#"outline.new={"x":36,"y":100,"text":"Heron by the canal\nTwo kingfishers"}"#,
        "--cmd",
        r##"draw.stroke={"points":[[10,10,0.4],[60,40,0.9],[90,20]],"tool":"pen","color":"#c44df0"}"##,
        "--cmd",
        r#"section.new={"name":"Plants"}"#,
        "--cmd",
        r#"page.rename={"title":"Ferns"}"#,
        "--cmd",
        r#"outline.new={"x":36,"y":100,"text":"Hart's-tongue fern"}"#,
        "--save",
        &pkg,
        "--save",
        &json,
    ]);
    let lines: Vec<Value> = out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 9, "{out}");
    assert_eq!(lines[1]["command"], "page.new");
    assert!(lines[8]["saved"].as_str().unwrap().contains("made.json"));
    (pkg, json)
}

#[test]
fn help_version_and_usage_errors() {
    assert!(ok(&[]).contains("USAGE"));
    assert!(ok(&["--help"]).contains("notecraft-cli convert"));
    assert!(ok(&["dump", "--help"]).contains("USAGE"));
    assert!(ok(&["--version"]).starts_with("notecraft-cli "));
    assert_eq!(code(&["frobnicate"]), 2);
    assert_eq!(code(&["info"]), 2);
    assert_eq!(code(&["info", "a", "b"]), 2);
    assert_eq!(code(&["info", "x.one", "--bogus"]), 2);
    assert_eq!(code(&["convert", "x.one"]), 2);
    assert_eq!(code(&["convert", "x.one", "y.docx"]), 2, "the output kind is checked first");
    assert_eq!(code(&["run", "--cmd", "page.new={oops"]), 2);
    assert_eq!(code(&["run", "--cmd", "no.such.command"]), 2);
    assert_eq!(code(&["run", "--save", "out.pdf"]), 2);
}

#[test]
fn missing_and_hostile_files_fail_cleanly() {
    let s = Scratch::new("hostile");
    assert_eq!(code(&["info", &s.path("missing.one")]), 1);
    let junk = s.path("junk.one");
    std::fs::write(&junk, b"\xE4\x52\x5C\x7B\x8C\xD8\xA7\x4D\xAE\xB1\x53\x78\xD0\x29\x96\xD3 not really a section").unwrap();
    for args in [vec!["info", junk.as_str()], vec!["dump", junk.as_str()], vec!["dump", "--store", junk.as_str()]] {
        let out = cli(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("notecraft-cli: "));
    }
    let pkg = s.path("junk.onepkg");
    std::fs::write(&pkg, b"MSCF\0\0\0\0garbage").unwrap();
    assert_eq!(code(&["info", &pkg]), 1);
    let json = s.path("junk.json");
    std::fs::write(&json, b"{\"name\": 3}").unwrap();
    assert_eq!(code(&["info", &json]), 1);
}

#[test]
fn run_saves_and_info_reads_back() {
    let s = Scratch::new("run");
    let (pkg, json) = make_notebook(&s);
    let a = info(&pkg);
    let b = info(&json);
    assert_eq!(shape(&a), shape(&b));
    assert_eq!(a["name"], "Field Notes");
    assert_eq!(a["sections"], 2);
    assert_eq!(a["totals"]["inkStrokes"].as_u64(), Some(3), "the default page's two strokes and ours");
    let titles: Vec<&str> = a["entries"][0]["pages"].as_array().unwrap().iter().filter_map(|p| p["title"].as_str()).collect();
    assert!(titles.contains(&"Birds"), "{titles:?}");
}

#[test]
fn conversions_round_trip() {
    let s = Scratch::new("convert");
    let (pkg, _) = make_notebook(&s);
    let want = shape(&info(&pkg));
    // .onepkg → folder → .json → .onepkg, and the folder's table of contents.
    let folder = s.path("Field Notes") + "/";
    assert!(ok(&["convert", &pkg, &folder]).contains("2 sections"));
    assert!(Path::new(&folder).join("Open Notebook.onetoc2").is_file());
    assert_eq!(shape(&info(&folder)), want);
    assert_eq!(shape(&info(&(folder.clone() + "Open Notebook.onetoc2"))), want);
    let json = s.path("again.json");
    ok(&["convert", &folder, &json]);
    assert_eq!(shape(&info(&json)), want);
    let pkg2 = s.path("again.onepkg");
    ok(&["convert", &json, &pkg2]);
    assert_eq!(shape(&info(&pkg2)), want);
    // A folder named by its table of contents.
    let toc_out = s.path("toc-out/Open Notebook.onetoc2");
    ok(&["convert", &pkg, &toc_out]);
    assert_eq!(shape(&info(&toc_out)), want);

    // One section as a .one; a notebook with two needs --section.
    let one = s.path("plants.one");
    assert_eq!(code(&["convert", &pkg, &one]), 2);
    ok(&["convert", &pkg, &one, "--section", "plants"]);
    let v = info(&one);
    assert_eq!(v["sections"], 1);
    assert!(ok(&["dump", &one]).contains("Hart's-tongue fern"));
}

#[test]
fn text_exports_and_selection() {
    let s = Scratch::new("text");
    let (pkg, _) = make_notebook(&s);
    let html = s.path("all.html");
    let md = s.path("all.md");
    let txt = s.path("birds.txt");
    ok(&["convert", &pkg, &html]);
    ok(&["convert", &pkg, &md]);
    ok(&["convert", &pkg, &txt, "--page", "BIRDS"]);
    let html = std::fs::read_to_string(&html).unwrap();
    assert!(html.contains("Heron by the canal") && html.contains("fern"), "every page in one HTML file");
    assert_eq!(html.matches("<html").count(), 1);
    let md = std::fs::read_to_string(&md).unwrap();
    assert!(md.contains("# Birds") && md.contains("# Ferns") && md.contains("---"));
    let txt = std::fs::read_to_string(&txt).unwrap();
    assert!(txt.starts_with("Birds\nHeron by the canal"), "{txt}");
    assert!(!txt.contains("fern"));
    assert_eq!(code(&["convert", &pkg, &s.path("x.txt"), "--page", "Nope"]), 1);

    let dump = ok(&["dump", &pkg, "--section", "Plants"]);
    assert!(dump.contains("== Section: Plants") && dump.contains("Hart's-tongue fern") && !dump.contains("Heron"), "{dump}");
    let full = ok(&["dump", &pkg]);
    assert!(full.contains("-- Birds [1 ink stroke]"), "{full}");
    let model: Value = serde_json::from_str(&ok(&["dump", &pkg, "--json"])).unwrap();
    assert_eq!(model["name"], "Field Notes");
}

#[test]
fn store_dump() {
    let s = Scratch::new("store");
    let (pkg, _) = make_notebook(&s);
    let v: Value = serde_json::from_str(&ok(&["dump", "--store", &pkg])).unwrap();
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 3, "two sections and the table of contents");
    let one = s.path("birds.one");
    ok(&["convert", &pkg, &one, "--section", "Quick Notes"]);
    let v: Value = serde_json::from_str(&ok(&["dump", "--store", &one])).unwrap();
    assert_eq!(v["kind"], "Section");
    let text = v.to_string();
    assert!(text.contains("jcidPageNode") && text.contains("Heron by the canal"), "the text is decoded from UTF-16");
}

#[test]
fn scripts() {
    let s = Scratch::new("script");
    let script = s.path("script.json");
    let out = s.path("scripted.json");
    let body = serde_json::json!({"commands": ["page.new", {"command": "page.rename", "params": {"title": "Scripted"}}], "save": [out]});
    std::fs::write(&script, body.to_string()).unwrap();
    let printed = ok(&["run", "--script", &script, "--cmd", r#"outline.new={"x":1,"y":2,"text":"after"}"#]);
    assert_eq!(printed.lines().count(), 4, "{printed}");
    let dump = ok(&["dump", &out, "--page", "Scripted"]);
    assert!(dump.contains("after"), "--cmd runs after the script: {dump}");
    // The notebook a script names is opened; a failing command stops the run with 1.
    let again = s.path("again.json");
    std::fs::write(&script, serde_json::json!({"notebook": out, "commands": [{"command": "nav.page", "params": {"id": "{00000000-0000-0000-0000-000000000001}"}}], "save": again}).to_string()).unwrap();
    assert_eq!(code(&["run", "--script", &script]), 1);
    assert!(!Path::new(&again).exists(), "nothing is saved after a failure");
}

#[test]
fn commands_list() {
    let text = ok(&["commands"]);
    assert!(text.lines().any(|l| l.starts_with("page.new") && l.contains("Mod+N")));
    let v: Value = serde_json::from_str(&ok(&["commands", "--json"])).unwrap();
    let list = v.as_array().unwrap();
    assert!(list.len() > 40);
    assert!(list.iter().all(|c| c["id"].is_string() && c["params"].is_string()));
}

#[test]
fn closed_stdout_is_not_a_crash() {
    let mut child = Command::new(BIN).args(["commands", "--json"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
    drop(child.stdout.take());
    let status = child.wait().unwrap();
    assert_ne!(status.code(), Some(101), "a panic exits with 101");
}
