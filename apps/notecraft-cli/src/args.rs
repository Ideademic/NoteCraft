//! A small argument parser: positionals, `--flag`, `--opt VALUE` / `--opt=VALUE`, repeatable
//! options. Unknown options are usage errors, and an option's value is never read as a path.

use crate::Fail;

/// One option a subcommand accepts.
pub struct Opt {
    pub name: &'static str,
    /// Takes a value (`--save OUT`); otherwise a switch (`--json`).
    pub value: bool,
}

pub const fn flag(name: &'static str) -> Opt {
    Opt { name, value: false }
}

pub const fn valued(name: &'static str) -> Opt {
    Opt { name, value: true }
}

/// Parsed arguments of one subcommand.
#[derive(Debug, Default)]
pub struct Parsed {
    pub positionals: Vec<String>,
    /// Options in the order given, `(name, value)`; a switch has `None`.
    pub opts: Vec<(&'static str, Option<String>)>,
}

impl Parsed {
    /// Whether switch `name` was given.
    pub fn has(&self, name: &str) -> bool {
        self.opts.iter().any(|(n, _)| *n == name)
    }

    /// The last value of option `name`.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.opts.iter().rev().find(|(n, _)| *n == name).and_then(|(_, v)| v.as_deref())
    }

    /// Every value of a repeatable option, in order.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.opts.iter().filter(|(n, _)| *n == name).filter_map(|(_, v)| v.as_deref()).collect()
    }
}

/// Parses `args` (after the subcommand) against `spec`. `--` ends the options.
pub fn parse(cmd: &str, args: &[String], spec: &[Opt]) -> Result<Parsed, Fail> {
    let mut out = Parsed::default();
    let mut i = 0;
    let mut only_positionals = false;
    while let Some(arg) = args.get(i) {
        i += 1;
        if only_positionals || !arg.starts_with("--") || arg == "-" {
            out.positionals.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_positionals = true;
            continue;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v.to_owned())),
            None => (arg.as_str(), None),
        };
        let Some(opt) = spec.iter().find(|o| o.name == name) else {
            return Err(Fail::usage(format!("{cmd}: unknown option `{}`", name.chars().take(60).collect::<String>())));
        };
        if !opt.value {
            if inline.is_some() {
                return Err(Fail::usage(format!("{cmd}: `{name}` takes no value")));
            }
            out.opts.push((opt.name, None));
            continue;
        }
        let value = match inline {
            Some(v) => v,
            None => {
                let v = args.get(i).ok_or_else(|| Fail::usage(format!("{cmd}: `{name}` needs a value")))?;
                i += 1;
                v.clone()
            }
        };
        out.opts.push((opt.name, Some(value)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn options_and_positionals() {
        let spec = [flag("--json"), valued("--save"), valued("--cmd")];
        let p = parse("run", &s(&["in.one", "--cmd", "a={}", "--cmd=b", "--json", "--save", "--x.json", "--", "--literal"]), &spec).unwrap();
        assert_eq!(p.positionals, ["in.one", "--literal"]);
        assert_eq!(p.values("--cmd"), ["a={}", "b"]);
        assert_eq!(p.value("--save"), Some("--x.json"), "an option's value is never an option");
        assert!(p.has("--json"));
        assert!(parse("run", &s(&["--nope"]), &spec).is_err());
        assert!(parse("run", &s(&["--save"]), &spec).is_err());
        assert!(parse("run", &s(&["--json=1"]), &spec).is_err());
    }
}
