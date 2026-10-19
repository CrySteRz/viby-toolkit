//! viby-toolkit release pre-flight — the mechanical half of /viby-toolkit:release.
//!
//! Checks the things that are exactly decidable before a release, so human attention goes to
//! the thing that is not: whether the change is actually backward-compatible.
//!
//!   version-drift     the same artifact declares different versions in different manifests
//!   dirty-tree        uncommitted changes would not be in the release
//!   unpushed          local commits the tag would point at but nobody else has
//!   tag-exists        the tag for this version is already taken
//!   changelog-stale   a CHANGELOG exists but does not mention this version
//!   debug-artifact    a focused/skipped test or a debugger statement is still in the tree
//!   no-ci             nothing automated gates this release
//!
//! Usage:
//!   viby check-release [dir] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = not a git repo / nothing to check.
//!
//! DESIGN: every check here is mechanical and unambiguous — that is the whole selection
//! criterion. Judgement calls (is this breaking? is the version bump right?) are the skill's
//! job, not this script's, because a checker that guesses at judgement trains you to ignore
//! it. Monorepos are detected and per-package versions are then reported as expected rather
//! than flagged as drift.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Map, Value};

use super::check_test_drift::{js_basename, js_extname, js_regex, js_trim, resolve, to_pretty};
use crate::core::args::{flag, parse_or_exit};
use crate::core::git::git_within;
use crate::core::strip::strip_noncode;

#[derive(Debug, Clone)]
pub struct Finding {
    pub check: String,
    /// "P1" | "P2" | "P3"
    pub severity: &'static str,
    pub message: String,
    pub detail: Option<String>,
}

pub struct Release {
    pub findings: Vec<Finding>,
    /// Version to the manifests that declare it, in first-seen order.
    pub versions: Vec<(String, Vec<String>)>,
    pub is_repo: bool,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "venv",
    ".venv",
    "dist",
    "build",
    "target",
    "out",
    "__pycache__",
    ".next",
    "vendor",
    "coverage",
    ".tox",
    "fixture",
    "fixtures",
    "testdata",
    "test-data",
];
const CODE_EXT: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".rb", ".rs", ".go", ".java",
];

struct Res {
    monorepo_dirs: Regex,
    monorepo_marker: Regex,
    toml_header: Regex,
    toml_version: Regex,
    setup_py: Regex,
    pubspec: Regex,
    gradle: Regex,
    init_py: Regex,
    mix: Regex,
    changelog_name: Regex,
    ci: Regex,
    /// Narrow, unambiguous patterns only.
    debug: Vec<(Regex, &'static str)>,
}

static RES: LazyLock<Res> = LazyLock::new(|| Res {
    monorepo_dirs: js_regex(
        r"(^|/)(packages|apps|crates|libs|modules|services|examples|playground)/",
    ),
    monorepo_marker: js_regex(r"(^|/)(pnpm-workspace\.yaml|nx\.json|turbo\.json|lerna\.json)$"),
    toml_header: js_regex(r"^\[([^\]]+)\]"),
    toml_version: js_regex(r#"^version\s*=\s*["']([^"']+)["']"#),
    setup_py: js_regex(r#"version\s*=\s*["']([^"']+)["']"#),
    pubspec: js_regex(r#"(?m)^version:\s*["']?([^"'\s]+)"#),
    gradle: js_regex(r#"(?m)^\s*version\s*=?\s*["']([^"']+)["']"#),
    init_py: js_regex(r#"__version__\s*=\s*["']([^"']+)["']"#),
    mix: js_regex(r#"version:\s*["']([^"']+)["']"#),
    changelog_name: js_regex(r"(?i)^(CHANGELOG|HISTORY|RELEASES)(\.md|\.rst|\.txt)?$"),
    // `.` in JavaScript stops at \r,   and   as well as \n.
    ci: js_regex(
        r"(^|/)(\.github/workflows/[^\n\r\x{2028}\x{2029}]+\.ya?ml|\.gitlab-ci\.yml|\.circleci/config\.yml|Jenkinsfile|azure-pipelines\.yml)$",
    ),
    debug: vec![
        (
            js_regex(r"\b(?:describe|it|test|context)\s*\.\s*only\b"),
            "focused test (.only) — the suite silently shrinks",
        ),
        (
            js_regex(r"\bf(?:describe|it)\s*\("),
            "focused test (fdescribe/fit)",
        ),
        (js_regex(r"(?m)^\s*debugger;?\s*$"), "debugger statement"),
        (js_regex(r"\bbreakpoint\s*\(\s*\)"), "breakpoint() call"),
        (js_regex(r"\bbinding\.pry\b"), "binding.pry"),
        (js_regex(r"\bdbg!\s*\("), "dbg!() macro"),
        (js_regex(r"\.only\s*\("), "focused block (.only)"),
    ],
});

fn first_group(text: &str, re: &Regex) -> Option<String> {
    re.captures(text)
        .and_then(|m| m.get(1))
        .map(|m| m.as_str().to_string())
}

fn json_field(text: &str, field: &str) -> Option<String> {
    let d: Value = serde_json::from_str(text).ok()?;
    d.get(field)?.as_str().map(str::to_string)
}

/// A plugin marketplace manifest carries the version inside its plugins array.
fn nested_plugin_version(text: &str) -> Option<String> {
    let d: Value = serde_json::from_str(text).ok()?;
    d.get("plugins")?
        .as_array()?
        .first()?
        .get("version")?
        .as_str()
        .map(str::to_string)
}

/// Version under the first of `sections` that has one, so `[project]` beats a dependency's.
///
/// Scans lines rather than regex-slicing the section body: with the `m` flag, a `$` in the
/// terminator lookahead matches the first end-of-LINE, so the captured body was always empty
/// and every TOML version silently read as null.
fn toml_version(text: &str, sections: &[&str]) -> Option<String> {
    let mut found: Vec<(String, String)> = Vec::new();
    let mut current = String::new();
    for raw in text.split('\n') {
        let line = js_trim(raw);
        if let Some(h) = first_group(line, &RES.toml_header) {
            current = js_trim(&h).to_string();
            continue;
        }
        if let Some(v) = first_group(line, &RES.toml_version) {
            if !found.iter().any(|(s, _)| *s == current) {
                found.push((current.clone(), v));
            }
        }
    }
    sections
        .iter()
        .find_map(|s| found.iter().find(|(f, _)| f == s).map(|(_, v)| v.clone()))
}

fn short_trimmed(t: &str) -> Option<String> {
    let t = js_trim(t);
    let len = t.encode_utf16().count();
    (len > 0 && len < 40).then(|| t.to_string())
}

/// Manifest files that declare a version; `extract_version` says how to read each.
const VERSION_FILES: &[&str] = &[
    "package.json",
    "plugin.json",
    "marketplace.json",
    "composer.json",
    "Cargo.toml",
    "pyproject.toml",
    "setup.py",
    "pubspec.yaml",
    "build.gradle",
    "VERSION",
    "version.txt",
    "__init__.py",
    "mix.exs",
];

fn extract_version(name: &str, t: &str) -> Option<String> {
    let r = &*RES;
    match name {
        "package.json" | "plugin.json" | "composer.json" => json_field(t, "version"),
        "marketplace.json" => nested_plugin_version(t),
        "Cargo.toml" => toml_version(t, &["package"]),
        "pyproject.toml" => toml_version(t, &["project", "tool.poetry"]),
        "setup.py" => first_group(t, &r.setup_py),
        "pubspec.yaml" => first_group(t, &r.pubspec),
        "build.gradle" => first_group(t, &r.gradle),
        "VERSION" | "version.txt" => short_trimmed(t),
        "__init__.py" => first_group(t, &r.init_py),
        "mix.exs" => first_group(t, &r.mix),
        _ => None,
    }
}

/// A `git` run with the 15 s limit this checker always used; `out` is trimmed.
fn git(root: &Path, args: &[&str]) -> (bool, String) {
    let o = git_within(root, args, Duration::from_secs(15));
    (o.ok, js_trim(&o.stdout).to_string())
}

/// Files under `root` as (path, path relative to the root), to a depth of 4. Entries are visited
/// in name order, so the result is deterministic.
fn walk(root: &Path, rel: &str, depth: usize, out: &mut Vec<(String, String)>) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(root) else { return };
    let mut entries: Vec<(String, bool)> = rd
        .flatten()
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.file_type().is_ok_and(|t| t.is_dir()),
            )
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, is_dir) in entries {
        let full = root.join(&name);
        let rel_path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if is_dir {
            if !SKIP_DIRS.contains(&name.as_str()) {
                walk(&full, &rel_path, depth + 1, out);
            }
        } else {
            out.push((full.to_string_lossy().into_owned(), rel_path));
        }
    }
}

/// Strip a UTF-8 BOM: Node's "utf8" keeps it, and JSON.parse then throws on an otherwise-valid
/// manifest. That silently dropped the file from version detection, so a real version drift went
/// unreported because of an encoding artifact.
fn read(p: &str) -> String {
    match fs::read(p) {
        Ok(b) => {
            let s = String::from_utf8_lossy(&b);
            s.strip_prefix('\u{feff}').unwrap_or(&s).to_string()
        }
        Err(_) => String::new(),
    }
}

pub fn check_release(root: &str) -> Release {
    let r = &*RES;
    let root_path = Path::new(root);
    let mut findings: Vec<Finding> = Vec::new();
    let mut versions: Vec<(String, Vec<String>)> = Vec::new();
    let is_repo = git(root_path, &["rev-parse", "--git-dir"]).0;

    let mut files: Vec<(String, String)> = Vec::new();
    walk(root_path, "", 0, &mut files);

    // ---- versions
    let monorepo = files.iter().any(|(_, rel)| r.monorepo_marker.is_match(rel));
    for (full, rel) in &files {
        let base = js_basename(full);
        // In a monorepo, per-package versions are expected. Only compare the top of the tree.
        if r.monorepo_dirs.is_match(rel) && monorepo {
            continue;
        }
        if !VERSION_FILES.contains(&base) {
            continue;
        }
        let Some(v) = extract_version(base, &read(full)) else {
            continue;
        };
        match versions.iter_mut().find(|(k, _)| *k == v) {
            Some((_, list)) => list.push(rel.clone()),
            None => versions.push((v, vec![rel.clone()])),
        }
    }

    if versions.len() > 1 {
        let summary = versions
            .iter()
            .map(|(v, fs_)| format!("{v} ({})", fs_.join(", ")))
            .collect::<Vec<_>>()
            .join("  vs  ");
        findings.push(Finding {
            check: "version-drift".to_string(),
            severity: "P1",
            message: "manifests disagree about the version — releasing now ships an inconsistent artifact".to_string(),
            detail: Some(summary),
        });
    }

    let current: Option<&str> = (versions.len() == 1).then(|| versions[0].0.as_str());

    // ---- git state
    if is_repo {
        let status = git(root_path, &["status", "--porcelain"]).1;
        if !status.is_empty() {
            let count = status.split('\n').count();
            findings.push(Finding {
                check: "dirty-tree".to_string(),
                severity: "P1",
                message: format!(
                    "{count} uncommitted change(s) — these would NOT be in the release"
                ),
                detail: Some(status.split('\n').take(8).collect::<Vec<_>>().join("\n")),
            });
        }

        let branch = git(root_path, &["branch", "--show-current"]).1;
        let branch = if branch.is_empty() { "HEAD" } else { &branch };
        let (up_ok, upstream) = git(
            root_path,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        );
        if up_ok {
            let range = format!("{upstream}..HEAD");
            let ahead = git(root_path, &["rev-list", "--count", &range]).1;
            if !ahead.is_empty() && ahead != "0" {
                findings.push(Finding {
                    check: "unpushed".to_string(),
                    severity: "P1",
                    message: format!("{ahead} commit(s) on {branch} are not pushed — a tag would point at code nobody else has"),
                    detail: None,
                });
            }
        } else {
            findings.push(Finding {
                check: "unpushed".to_string(),
                severity: "P2",
                message: format!(
                    "branch {branch} has no upstream, so push state cannot be verified"
                ),
                detail: None,
            });
        }

        if let Some(cur) = current {
            let tags = git(root_path, &["tag", "--list"]).1;
            let v_cur = format!("v{cur}");
            let taken = tags
                .split('\n')
                .filter(|t| !t.is_empty())
                .find(|t| *t == cur || *t == v_cur);
            if let Some(taken) = taken {
                findings.push(Finding {
                    check: "tag-exists".to_string(),
                    severity: "P1",
                    message: format!("tag {taken} already exists — either the version was not bumped, or this release already happened"),
                    detail: None,
                });
            }
        }
    }

    // ---- changelog
    let changelog = files
        .iter()
        .find(|(full, _)| r.changelog_name.is_match(js_basename(full)));
    if let (Some((full, rel)), Some(cur)) = (changelog, current) {
        // Token match, not substring: `"12.0.1".includes("2.0")` is true, so an unrelated older
        // release silently satisfied the check for version 2.0.
        let pattern = format!("(^|[^0-9.]){}([^0-9.]|$)", regex::escape(cur));
        let stale = Regex::new(&pattern).map_or(true, |re| !re.is_match(&read(full)));
        if stale {
            findings.push(Finding {
                check: "changelog-stale".to_string(),
                severity: "P2",
                message: format!(
                    "{rel} does not mention {cur} — the release would be undocumented"
                ),
                detail: None,
            });
        }
    }

    // ---- debug artifacts.
    let mut debug_hits: Vec<String> = Vec::new();
    for (full, rel) in &files {
        let ext = js_extname(full);
        if !CODE_EXT.contains(&ext) {
            continue;
        }
        let raw = read(full);
        if raw.encode_utf16().count() > 400_000 {
            continue;
        }
        // Match against CODE only. A test fixture, a doc comment, or a regex that merely mentions
        // `describe.only` is not a focused test — and this checker flagged exactly that on its
        // own repo before the shared blanking pass was wired in.
        let text = strip_noncode(&raw, &ext.to_lowercase());
        for (re, what) in &r.debug {
            // Every matching line, not just the first: reporting one hit per pattern per file
            // let a second focused test in the same file ship silently even after the first was
            // "fixed". Each line is reported once per pattern.
            for (idx, l) in text.split('\n').enumerate() {
                if re.is_match(l) {
                    debug_hits.push(format!("{rel}:{} — {what}", idx + 1));
                }
            }
        }
    }
    if !debug_hits.is_empty() {
        findings.push(Finding {
            check: "debug-artifact".to_string(),
            severity: "P2",
            message: format!("{} debug artifact(s) still in the tree", debug_hits.len()),
            detail: Some(
                debug_hits
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        });
    }

    // ---- CI
    if !files.iter().any(|(_, rel)| r.ci.is_match(rel)) {
        findings.push(Finding {
            check: "no-ci".to_string(),
            severity: "P3",
            message: "no CI config found — nothing automated gates this release, so the checks are all manual".to_string(),
            detail: None,
        });
    }

    Release {
        findings,
        versions,
        is_repo,
    }
}

/// True for a string `Object.keys` orders as an array index: a canonical non-negative integer
/// below 2^32 - 1.
fn array_index(k: &str) -> Option<u32> {
    if k.is_empty() || (k.len() > 1 && k.starts_with('0')) || !k.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    k.parse::<u32>().ok().filter(|n| *n != u32::MAX)
}

/// `Object.fromEntries(versions)`: integer-like keys come first, in numeric order, as JavaScript
/// objects order them.
fn versions_json(versions: &[(String, Vec<String>)]) -> Value {
    let mut idx: Vec<(u32, &(String, Vec<String>))> = versions
        .iter()
        .filter_map(|e| array_index(&e.0).map(|n| (n, e)))
        .collect();
    idx.sort_by_key(|(n, _)| *n);
    let rest = versions.iter().filter(|e| array_index(&e.0).is_none());
    let mut m = Map::new();
    for (_, (k, v)) in idx {
        m.insert(k.clone(), json!(v));
    }
    for (k, v) in rest {
        m.insert(k.clone(), json!(v));
    }
    Value::Object(m)
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let root = resolve(parsed.positionals.first().map_or(".", String::as_str));
    let Release {
        mut findings,
        versions,
        is_repo,
    } = check_release(&root);

    if parsed.flag("json") {
        let v = json!({
            "root": root,
            "isRepo": is_repo,
            "versions": versions_json(&versions),
            "findings": findings.iter().map(|f| {
                let mut o = Map::new();
                o.insert("check".into(), json!(f.check));
                o.insert("severity".into(), json!(f.severity));
                o.insert("message".into(), json!(f.message));
                if let Some(d) = &f.detail {
                    o.insert("detail".into(), json!(d));
                }
                Value::Object(o)
            }).collect::<Vec<_>>(),
        });
        println!("{}", to_pretty(&v));
        return i32::from(!findings.is_empty());
    }

    let rank = |s: &str| match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    };
    findings.sort_by_key(|f| rank(f.severity));
    for f in &findings {
        println!("[{} {}] {}", f.severity, f.check, f.message);
        if let Some(d) = &f.detail {
            for l in d.split('\n') {
                println!("    {l}");
            }
        }
    }

    if !parsed.flag("quiet") {
        if !versions.is_empty() {
            println!();
            for (v, files) in &versions {
                println!("version {v}: {}", files.join(", "));
            }
        }
        println!();
        if findings.is_empty() {
            println!("✓ mechanical pre-flight clean.");
        } else {
            println!("{} finding(s).", findings.len());
        }
        println!(
            "This checks only what is exactly decidable. It does NOT judge whether the change is\n\
             backward-compatible — semantic versioning is violated routinely, so decide that from\n\
             the actual public-surface diff (see /viby-toolkit:release)."
        );
    }
    if !is_repo {
        return 2;
    }
    i32::from(!findings.is_empty())
}
