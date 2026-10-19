//! Command-line parsing with the semantics of `node:util` `parseArgs` in strict mode, which every
//! checker used: `--flag`, `--name value` or `--name=value`, `--` ends options, and an unknown
//! option or a missing value is an error (exit 1, as the uncaught throw was).

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `--name`; `--name=x` is an error.
    Flag,
    /// `--name value`; the last one wins.
    Value,
    /// `--name value`, repeatable; every value is kept.
    Values,
}

#[derive(Clone, Copy, Debug)]
pub struct Opt {
    pub name: &'static str,
    pub kind: Kind,
    pub default: Option<&'static str>,
}

pub const fn flag(name: &'static str) -> Opt {
    Opt {
        name,
        kind: Kind::Flag,
        default: None,
    }
}

pub const fn value(name: &'static str, default: Option<&'static str>) -> Opt {
    Opt {
        name,
        kind: Kind::Value,
        default,
    }
}

pub const fn values(name: &'static str) -> Opt {
    Opt {
        name,
        kind: Kind::Values,
        default: None,
    }
}

#[derive(Debug, Default)]
pub struct Parsed {
    flags: HashMap<&'static str, bool>,
    values: HashMap<&'static str, Vec<String>>,
    pub positionals: Vec<String>,
}

impl Parsed {
    pub fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }

    /// The last value given, else the default.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    pub fn values(&self, name: &str) -> &[String] {
        self.values.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
}

pub fn parse(args: &[String], opts: &[Opt]) -> Result<Parsed, String> {
    let mut out = Parsed::default();
    for opt in opts {
        if let Some(d) = opt.default {
            out.values.insert(opt.name, vec![d.to_string()]);
        }
    }
    let mut given: HashMap<&'static str, bool> = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if arg == "--" {
            out.positionals.extend(args[i..].iter().cloned());
            break;
        }
        let Some(body) = arg.strip_prefix("--") else {
            if arg.len() > 1 && arg.starts_with('-') {
                return Err(format!("Unknown option '{arg}'"));
            }
            out.positionals.push(arg.clone());
            continue;
        };
        let (name, inline) = match body.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (body, None),
        };
        let opt = opts
            .iter()
            .find(|o| o.name == name)
            .ok_or_else(|| format!("Unknown option '--{name}'"))?;
        match opt.kind {
            Kind::Flag => {
                if inline.is_some() {
                    return Err(format!("Option '--{name}' does not take an argument"));
                }
                out.flags.insert(opt.name, true);
            }
            Kind::Value | Kind::Values => {
                let v = match inline {
                    Some(v) => v,
                    None => {
                        let next = args
                            .get(i)
                            .filter(|n| !n.starts_with('-'))
                            .ok_or_else(|| format!("Option '--{name} <value>' argument missing"))?;
                        i += 1;
                        next.clone()
                    }
                };
                // A given value replaces the default; repeats accumulate only for Values.
                let entry = out.values.entry(opt.name).or_default();
                if !given.contains_key(opt.name) || opt.kind == Kind::Value {
                    entry.clear();
                }
                given.insert(opt.name, true);
                entry.push(v);
            }
        }
    }
    Ok(out)
}

/// `parse`, or the error printed the way a checker reports it, with exit code 1.
pub fn parse_or_exit(args: &[String], opts: &[Opt]) -> Result<Parsed, i32> {
    parse(args, opts).map_err(|e| {
        eprintln!("error: {e}");
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    const OPTS: &[Opt] = &[flag("json"), value("base", Some("HEAD")), values("against")];

    #[test]
    fn flags_values_defaults_and_positionals() {
        let p = parse(&s(&["a.md", "--json", "--base=main", "b.md"]), OPTS).unwrap();
        assert!(p.flag("json"));
        assert_eq!(p.value("base"), Some("main"));
        assert_eq!(p.positionals, s(&["a.md", "b.md"]));
        assert_eq!(parse(&[], OPTS).unwrap().value("base"), Some("HEAD"));
    }

    #[test]
    fn repeated_values_accumulate_and_dashdash_ends_options() {
        let p = parse(
            &s(&["--against", "x", "--against", "y", "--", "--json"]),
            OPTS,
        )
        .unwrap();
        assert_eq!(p.values("against"), s(&["x", "y"]));
        assert!(!p.flag("json"));
        assert_eq!(p.positionals, s(&["--json"]));
    }

    #[test]
    fn strict_errors() {
        assert!(parse(&s(&["--nope"]), OPTS)
            .unwrap_err()
            .contains("Unknown option"));
        assert!(parse(&s(&["-x"]), OPTS).is_err());
        assert!(parse(&s(&["--json=1"]), OPTS).is_err());
        assert!(parse(&s(&["--base"]), OPTS)
            .unwrap_err()
            .contains("argument missing"));
        assert_eq!(parse(&s(&["-"]), OPTS).unwrap().positionals, s(&["-"]));
    }
}
