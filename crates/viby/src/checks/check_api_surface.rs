//! viby-toolkit public-surface differ — the executable half of /viby-toolkit:api, and the
//! missing half of /viby-toolkit:release.
//!
//! `release` says the version number is a promise about the public surface rather than about
//! the size of the diff, and then asks for a judgement it gave no tool for. This computes the
//! input to that judgement: which exported symbols were added, removed, or had their signature
//! changed between two refs.
//!
//! Usage:
//!   viby check-api-surface [--base <ref>] [paths...] [--json] [--quiet]
//! Exit: 0 = no breaking change detected, 1 = breaking change(s) found, 2 = nothing analysable.
//!
//! WHAT IT DOES NOT DO, said plainly. It reads the SYNTACTIC surface only. The literature this
//! repo already cites is unambiguous that this is the easy half: detection handles syntactic
//! breaks well and behavioural ones poorly, and 67% of Maven artifacts violate SemVer anyway.
//! A function whose signature is unchanged and whose meaning inverted is a major break this
//! tool will call a patch. It also cannot follow `export * from` re-export barrels — the same
//! blind spot that a tree-sitter-based code graph has, and for the same reason: resolving it
//! needs module resolution, not pattern matching. So it REPORTS every barrel it could not
//! follow instead of quietly excluding it, because a surface report that silently omits part of
//! the surface is worse than no report.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use super::check_test_drift::to_pretty;
use crate::core::args::{flag, parse_or_exit, value};
use crate::core::git::git;
use crate::core::js::{js_re, js_trim, utf16_cmp};
use crate::core::nodepath::{extname, relative, resolve, walk_files};
use crate::core::strip::strip_noncode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// `name` for a value/type, or `name(params)` normalised for a callable.
    pub name: String,
    pub kind: String,
    /// Normalised parameter list, empty for non-callables.
    pub params: String,
    pub file: String,
}

#[derive(Debug, Clone)]
pub struct Change {
    /// "P1" | "P2" | "P3"
    pub severity: &'static str,
    /// removed | added | signature | param-name | unresolved-reexport | not-analysed
    pub kind: &'static str,
    pub symbol: String,
    pub file: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SurfaceDiff {
    pub base: String,
    pub changes: Vec<Change>,
    /// major = something was removed or re-signatured; minor = additions only; patch = neither.
    pub verdict: &'static str,
    pub analysed: usize,
    pub skipped: Vec<String>,
}

impl SurfaceDiff {
    fn to_json(&self) -> Value {
        json!({
            "base": self.base,
            "changes": self.changes.iter().map(|c| json!({
                "severity": c.severity,
                "kind": c.kind,
                "symbol": c.symbol,
                "file": c.file,
                "detail": c.detail,
            })).collect::<Vec<_>>(),
            "verdict": self.verdict,
            "analysed": self.analysed,
            "skipped": self.skipped,
        })
    }
}

const ANALYSED_EXT: &[&str] = &[
    ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".py", ".go", ".rs",
];
const JS_EXT: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "venv",
    ".venv",
    "__pycache__",
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    ".next",
    ".turbo",
];

struct Res {
    ws: Regex,
    param_prefix: Regex,
    param_name: Regex,
    ts_function: Regex,
    ts_class: Regex,
    ts_interface: Regex,
    ts_type: Regex,
    ts_const: Regex,
    ts_default: Regex,
    ts_export_list: Regex,
    ts_alias: Regex,
    ts_barrel: Regex,
    quoted: Regex,
    py_all: Regex,
    py_all_item: Regex,
    py_def: Regex,
    py_class: Regex,
    go_func: Regex,
    go_type: Regex,
    go_var: Regex,
    rs_fn: Regex,
    rs_type: Regex,
}

static RES: LazyLock<Res> = LazyLock::new(|| Res {
    ws: js_re(r"\s+", ""),
    param_prefix: js_re(r"^\s*(?:pub\s+)?", ""),
    param_name: js_re(r"^\s*(?:\*{1,2})?([A-Za-z_$][\w$]*)", ""),
    ts_function: js_re(
        r"(?m)^\s*export\s+(?:async\s+)?function\s*\*?\s*([\w$]+)\s*\(([^)]*)\)",
        "",
    ),
    ts_class: js_re(r"(?m)^\s*export\s+(?:abstract\s+)?class\s+([\w$]+)", ""),
    ts_interface: js_re(r"(?m)^\s*export\s+interface\s+([\w$]+)", ""),
    ts_type: js_re(
        r"(?m)^\s*export\s+type\s+([\w$]+)\s*(?:<[^=]*>)?\s*=\s*([^\n;]*)",
        "",
    ),
    ts_const: js_re(
        r"(?m)^\s*export\s+(?:declare\s+)?(?:const|let|var)\s+([\w$]+)",
        "",
    ),
    ts_default: js_re(r"(?m)^\s*export\s+default\b", ""),
    ts_export_list: js_re(r"(?m)^\s*export\s*\{([^}]*)\}", ""),
    ts_alias: js_re(r"(?:\bas\s+)?([\w$]+)\s*$", ""),
    ts_barrel: js_re(
        r#"(?m)^\s*export\s+\*(?:\s+as\s+[\w$]+)?\s+from\s+['"][^'"]*['"]"#,
        "",
    ),
    quoted: js_re(r#"['"]([^'"]+)['"]"#, ""),
    py_all: js_re(r"(?m)^__all__\s*=\s*[\[(][^\])]*[\])]", ""),
    py_all_item: js_re(r#"['"](\w+)['"]"#, ""),
    py_def: js_re(r"(?m)^(?:async\s+)?def\s+(\w+)\s*\(([^)]*)\)", ""),
    py_class: js_re(r"(?m)^class\s+(\w+)", ""),
    go_func: js_re(r"(?m)^func\s+(?:\([^)]*\)\s*)?([A-Z]\w*)\s*\(([^)]*)\)", ""),
    go_type: js_re(r"(?m)^type\s+([A-Z]\w*)", ""),
    go_var: js_re(r"(?m)^(?:var|const)\s+([A-Z]\w*)", ""),
    rs_fn: js_re(
        r"(?m)^\s*pub\s+(?:async\s+)?fn\s+(\w+)\s*(?:<[^>]*>)?\s*\(([^)]*)\)",
        "",
    ),
    rs_type: js_re(r"(?m)^\s*pub\s+(?:struct|enum|trait|type)\s+(\w+)", ""),
});

fn normalise_params(raw: &str) -> String {
    js_trim(&RES.ws.replace_all(raw, " ")).to_string()
}

/// Parameter identifiers only — used to tell "renamed a param" from "changed the signature".
fn param_names(params: &str) -> Vec<String> {
    if params.is_empty() {
        return Vec::new();
    }
    params
        .split(',')
        .map(|p| {
            let stripped = RES.param_prefix.replace(p, "");
            RES.param_name
                .captures(&stripped)
                .and_then(|m| m.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .filter(|n| !n.is_empty() && n != "self" && n != "cls")
        .collect()
}

fn group<'a>(m: &regex::Captures<'a>, i: usize) -> &'a str {
    m.get(i).map_or("", |g| g.as_str())
}

/// Extract the exported surface. Runs on CODE, never raw text — a `export function` inside a
/// string fixture or a comment is not part of anyone's API. That rule is in this repo's memory
/// because four separate defects came from ignoring it.
pub fn extract_surface(text: &str, file: &str) -> (Vec<Symbol>, Vec<String>) {
    let ext = extname(file).to_lowercase();
    let code = strip_noncode(text, &ext);
    // Two constructs here have their VALUE inside a string literal — a re-export's module path
    // and Python's `__all__` — and stripping blanks exactly that. So: decide WHERE from the
    // stripped code (a barrel in a comment is still not a barrel), then read WHAT from the raw
    // text at the same offset. This relies on the blanking pass being offset-preserving, which
    // it is: it overwrites in place rather than deleting.
    let raw_at = |start: usize, end: usize| text.get(start..end).unwrap_or("");
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut barrels: Vec<String> = Vec::new();
    let mut add = |name: &str, kind: &str, params: &str| {
        symbols.push(Symbol {
            name: name.to_string(),
            kind: kind.to_string(),
            params: normalise_params(params),
            file: file.to_string(),
        });
    };
    let r = &*RES;

    if JS_EXT.contains(&ext.as_str()) {
        for m in r.ts_function.captures_iter(&code) {
            add(group(&m, 1), "function", group(&m, 2));
        }
        for m in r.ts_class.captures_iter(&code) {
            add(group(&m, 1), "class", "");
        }
        for m in r.ts_interface.captures_iter(&code) {
            add(group(&m, 1), "interface", "");
        }
        for m in r.ts_type.captures_iter(&code) {
            add(group(&m, 1), "type", group(&m, 2));
        }
        for m in r.ts_const.captures_iter(&code) {
            add(group(&m, 1), "const", "");
        }
        for _ in r.ts_default.find_iter(&code) {
            add("default", "default", "");
        }
        // `export { a, b as c }` — the alias is the exported name, which is what a caller imports.
        for m in r.ts_export_list.captures_iter(&code) {
            for part in group(&m, 1).split(',') {
                if let Some(alias) = r.ts_alias.captures(js_trim(part)).and_then(|a| a.get(1)) {
                    add(alias.as_str(), "re-export", "");
                }
            }
        }
        // `export * from './x'` — cannot be resolved without module resolution. Reported, not
        // dropped.
        for m in r.ts_barrel.find_iter(&code) {
            let raw = raw_at(m.start(), m.end());
            barrels.push(
                r.quoted
                    .captures(raw)
                    .and_then(|q| q.get(1))
                    .map_or("?", |q| q.as_str())
                    .to_string(),
            );
        }
    } else if ext == ".py" {
        // An explicit __all__ IS the surface; anything else is convention.
        if let Some(all) = r.py_all.find(&code) {
            for m in r.py_all_item.captures_iter(raw_at(all.start(), all.end())) {
                add(group(&m, 1), "__all__", "");
            }
        } else {
            for m in r.py_def.captures_iter(&code) {
                if !group(&m, 1).starts_with('_') {
                    add(group(&m, 1), "function", group(&m, 2));
                }
            }
            for m in r.py_class.captures_iter(&code) {
                if !group(&m, 1).starts_with('_') {
                    add(group(&m, 1), "class", "");
                }
            }
        }
    } else if ext == ".go" {
        // Exported means capitalised in Go — the language's own visibility rule.
        for m in r.go_func.captures_iter(&code) {
            add(group(&m, 1), "func", group(&m, 2));
        }
        for m in r.go_type.captures_iter(&code) {
            add(group(&m, 1), "type", "");
        }
        for m in r.go_var.captures_iter(&code) {
            add(group(&m, 1), "var", "");
        }
    } else if ext == ".rs" {
        for m in r.rs_fn.captures_iter(&code) {
            add(group(&m, 1), "fn", group(&m, 2));
        }
        for m in r.rs_type.captures_iter(&code) {
            add(group(&m, 1), "type", "");
        }
    }
    (symbols, barrels)
}

/// A `Map` that keeps first-insertion order when a key is set again, as JavaScript's does.
#[derive(Default)]
struct OrderedMap {
    order: Vec<(String, Symbol)>,
    index: HashMap<String, usize>,
}

impl OrderedMap {
    fn set(&mut self, k: String, s: Symbol) {
        match self.index.get(&k) {
            Some(&i) => self.order[i].1 = s,
            None => {
                self.index.insert(k.clone(), self.order.len());
                self.order.push((k, s));
            }
        }
    }
    fn get(&self, k: &str) -> Option<&Symbol> {
        self.index.get(k).map(|&i| &self.order[i].1)
    }
    fn len(&self) -> usize {
        self.order.len()
    }
}

/// The map key: the name alone, so a file's `A` the type and `A` the function are one symbol.
fn key(s: &Symbol) -> &str {
    &s.name
}

pub fn diff_surface(cwd: &str, base: &str, targets: &[String]) -> SurfaceDiff {
    let mut changes: Vec<Change> = Vec::new();
    let skipped: Vec<String> = Vec::new();
    let cwd_path = Path::new(cwd);

    if !git(cwd_path, &["rev-parse", "--git-dir"]).ok {
        return SurfaceDiff {
            base: base.to_string(),
            changes: Vec::new(),
            verdict: "unknown",
            analysed: 0,
            skipped: vec!["not a git repository — nothing to compare against".to_string()],
        };
    }

    let mut files: Vec<String> = Vec::new();
    for t in targets {
        match fs::metadata(t) {
            Ok(m) if m.is_dir() => files.extend(walk_files(
                t,
                |name| SKIP_DIRS.contains(&name),
                |name, _, is_file| {
                    is_file && ANALYSED_EXT.contains(&extname(name).to_lowercase().as_str())
                },
            )),
            Ok(_) => files.push(t.clone()),
            Err(_) => {}
        }
    }
    files.sort_by(|a, b| utf16_cmp(a, b));
    files.dedup();

    let mut now = OrderedMap::default();
    let mut before = OrderedMap::default();
    let mut analysed = 0;

    for file in &files {
        let Ok(bytes) = fs::read(file) else { continue };
        let head = String::from_utf8_lossy(&bytes);
        analysed += 1;
        let rel = match relative(cwd, &resolve(&[file])) {
            r if r.is_empty() => file.clone(),
            r => r,
        };
        let (symbols, barrels) = extract_surface(&head, file);
        for s in symbols {
            now.set(format!("{rel}#{}", key(&s)), s);
        }
        for b in barrels {
            changes.push(Change {
                severity: "P2",
                kind: "unresolved-reexport",
                symbol: format!("export * from \"{b}\""),
                file: rel.clone(),
                detail: "re-export barrel — the symbols behind it are NOT in this report; resolving it needs module resolution, so check it by hand".to_string(),
            });
        }

        let old = git(cwd_path, &["show", &format!("{base}:{rel}")]);
        if !old.ok {
            continue; // new file: its symbols show up as additions
        }
        for s in extract_surface(&old.stdout, file).0 {
            before.set(format!("{rel}#{}", key(&s)), s);
        }
    }

    for (k, s) in &before.order {
        let Some(still) = now.get(k) else {
            changes.push(Change {
                severity: "P1",
                kind: "removed",
                symbol: format!("{} {}", s.kind, s.name),
                file: s.file.clone(),
                detail: "removed from the public surface — any caller importing it breaks on upgrade: MAJOR".to_string(),
            });
            continue;
        };
        if still.params != s.params {
            let was_names = param_names(&s.params);
            let now_names = param_names(&still.params);
            let same_shape = was_names.len() == now_names.len()
                && !s.params.is_empty()
                && !still.params.is_empty();
            if same_shape && was_names.join(",") != now_names.join(",") {
                changes.push(Change {
                    severity: "P2",
                    kind: "param-name",
                    symbol: format!("{} {}", s.kind, s.name),
                    file: s.file.clone(),
                    detail: format!(
                        "parameter renamed ({} → {}) — breaking only where callers pass it by keyword",
                        was_names.join(", "),
                        now_names.join(", ")
                    ),
                });
            } else {
                let show = |p: &str| {
                    if p.is_empty() {
                        "∅".to_string()
                    } else {
                        p.to_string()
                    }
                };
                changes.push(Change {
                    severity: "P1",
                    kind: "signature",
                    symbol: format!("{} {}", s.kind, s.name),
                    file: s.file.clone(),
                    detail: format!(
                        "signature changed ({} → {}) — existing calls may no longer compile: MAJOR",
                        show(&s.params),
                        show(&still.params)
                    ),
                });
            }
        }
    }
    for (k, s) in &now.order {
        if before.get(k).is_none() && before.len() > 0 {
            changes.push(Change {
                severity: "P3",
                kind: "added",
                symbol: format!("{} {}", s.kind, s.name),
                file: s.file.clone(),
                detail: "new public symbol — additive: MINOR".to_string(),
            });
        }
    }

    let breaking = changes.iter().any(|c| c.severity == "P1");
    let additive = changes.iter().any(|c| c.kind == "added");
    let verdict = if breaking {
        "major"
    } else if additive {
        "minor"
    } else {
        "patch"
    };
    SurfaceDiff {
        base: base.to_string(),
        changes,
        verdict,
        analysed,
        skipped,
    }
}

pub fn main(args: &[String]) -> i32 {
    let opts = [value("base", Some("HEAD")), flag("json"), flag("quiet")];
    let parsed = match parse_or_exit(args, &opts) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let cwd = std::env::current_dir()
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());
    let targets = if parsed.positionals.is_empty() {
        vec![".".to_string()]
    } else {
        parsed.positionals.clone()
    };
    let mut d = diff_surface(&cwd, parsed.value("base").unwrap_or("HEAD"), &targets);

    if d.verdict == "unknown" || d.analysed == 0 {
        if json_out {
            println!("{}", to_pretty(&d.to_json()));
        } else if !quiet {
            println!(
                "{}",
                d.skipped.first().map_or(
                    "no files in a language this understands (ts/js, py, go, rs) — surface unknown, decide by hand",
                    String::as_str
                )
            );
        }
        return 2;
    }

    if json_out {
        println!("{}", to_pretty(&d.to_json()));
        return i32::from(d.verdict == "major");
    }

    let rank = |s: &str| match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    };
    d.changes.sort_by_key(|c| rank(c.severity));
    for c in &d.changes {
        println!("[{} {}] {}  ({})", c.severity, c.kind, c.symbol, c.file);
        println!("    {}", c.detail);
    }

    if !quiet {
        println!();
        println!(
            "{} file(s) compared against {}: surface change suggests **{}**",
            d.analysed,
            d.base,
            d.verdict.to_uppercase()
        );
        println!(
            "Syntactic surface only. A signature that stayed the same while its MEANING changed is a\n\
             major break this cannot see — behavioural compatibility is still your call. Re-export\n\
             barrels are listed above rather than followed. See /viby-toolkit:release."
        );
    }
    i32::from(d.verdict == "major")
}
