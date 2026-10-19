//! viby-toolkit diff hygiene — the executable half of /viby-toolkit:orchestrate.
//!
//! The build phase was the last uninstrumented part of this toolkit: deciding and proving had
//! checkers, writing code had prose. This closes it by auditing the artifact the build phase
//! actually produces — the diff — for the things that make a change hard to review or unsafe to
//! land, none of which a compiler or a test run objects to.
//!
//! Usage:
//!   viby check-diff-hygiene [--base <ref>] [--staged] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no diff to check.
//!
//! WHY SIZE IS A FINDING, not a style opinion. The largest study of code review — SmartBear at
//! Cisco, 2,500 reviews over 3.2 million lines — found reviewers spot defects most effectively at
//! **200–400 changed lines**, with detection dropping off past 400. Reported detection falls from
//! ~87% on changes under 100 lines to ~28% on changes over 1,000. A 2,000-line diff is not a
//! bigger review, it is an unreviewed one, and splitting it is the highest-leverage thing available
//! before anyone reads it.
//!
//! DESIGN: diff-scoped, so it judges what you ADDED rather than what the file already contained —
//! `check-release` scans the tree and will fairly complain about pre-existing debris; this only
//! complains about debris you are introducing. Every finding names the fix.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use super::check_test_drift::to_pretty;
use crate::core::args::{flag, parse_or_exit, value};
use crate::core::git::git;
use crate::core::js::{is_js_space, js_re, js_trim, locale_compare};
use crate::core::nodepath::{basename, extname, join};

#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: i64,
    pub check: String,
    /// "P1" | "P2" | "P3"
    pub severity: &'static str,
    pub problem: String,
    pub fix: String,
}

impl Finding {
    fn to_json(&self) -> Value {
        json!({
            "file": self.file,
            "line": self.line,
            "check": self.check,
            "severity": self.severity,
            "problem": self.problem,
            "fix": self.fix,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AddedLine {
    pub line: i64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub file: String,
    pub added: Vec<AddedLine>,
    pub removed_count: usize,
    /// Added lines that differ from a removed line only in whitespace.
    pub whitespace_only: usize,
}

pub struct Hygiene {
    pub findings: Vec<Finding>,
    pub added_lines: usize,
    pub files: usize,
}

const SOURCE_EXT: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rs", ".java", ".rb",
    ".php", ".cs", ".kt", ".swift", ".scala", ".c", ".cc", ".cpp", ".h", ".hpp",
];
const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "poetry.lock",
    "cargo.lock",
    "uv.lock",
    "composer.lock",
    "gemfile.lock",
    "go.sum",
];
const MANIFESTS: &[&str] = &[
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "cargo.toml",
    "go.mod",
    "composer.json",
    "gemfile",
    "build.gradle",
    "pom.xml",
];

/// Review-size thresholds, from the Cisco/SmartBear measurement quoted in the header.
const REVIEWABLE_MAX: usize = 400;
const UNREVIEWABLE: usize = 1000;

struct Res {
    test_path: Regex,
    /// A script's stdout IS its interface. `console.log` in a CLI, a migration script, a seed file
    /// or a task runner is progress output for the human running it, not leftover debugging — and
    /// measured on four real repositories, every single `debug-added` finding (20 of them) was
    /// exactly that. Path is the reliable signal; the message text is not.
    script_path: Regex,
    script_file: Regex,
    diff_file: Regex,
    hunk: Regex,
    conflict: Regex,
    /// Credentials published as examples: AWS's `...EXAMPLE` convention, `YOUR_API_KEY`
    /// placeholders, and masked values. Never real, and flagging them is indistinguishable from
    /// crying wolf.
    example_credential: Regex,
    /// Explicit inline allowlist, the mechanism every secret scanner ends up needing. A checker
    /// that tests credential detection must contain a credential-shaped string, so without this
    /// the rule can never run clean against its own repo — and a permanently-red gate is one this
    /// library rejects.
    allow_secret: Regex,
    secrets: Vec<Regex>,
    comment_start: Regex,
    debugger: Regex,
    console: Regex,
    py_breakpoint: Regex,
    py_print_open: Regex,
    py_print_marker: Regex,
    py_print_ident: Regex,
    go_print: Regex,
    rs_debug: Regex,
    php_debug: Regex,
    todo: Regex,
    commented_head: Regex,
    commented_tail: Regex,
}

static RES: LazyLock<Res> = LazyLock::new(|| Res {
    test_path: js_re(
        r"(?i)(^|/)(tests?|specs?|__tests__)/|[._-](test|tests|spec|specs)\.[a-z]+$|(^|/)test_[^/]*\.py$|_test\.[a-z]+$|Test(s)?\.(java|kt|cs)$",
        "",
    ),
    script_path: js_re(
        r"(?i)(^|/)(scripts?|bin|tools?|cli|tasks?|jobs?|migrations?|seeds?)/",
        "",
    ),
    script_file: js_re(
        r"(?i)(^|/)(seed|migrate|backfill|bootstrap|setup|deploy|codemod)[\w.-]*\.[a-z]+$",
        "",
    ),
    // `.` in JavaScript stops at \r,   and   as well as \n.
    diff_file: Regex::new(r" b/([^\n\r\x{2028}\x{2029}]+)$").expect("static pattern"),
    hunk: js_re(r"@@ -\d+(?:,\d+)? \+(\d+)", ""),
    conflict: js_re(r"^(<{7}|={7}|>{7})(\s|$)", ""),
    example_credential: js_re(
        r"(?i)EXAMPLE(KEY)?\b|\bYOUR[_-][A-Z_-]*(KEY|TOKEN|SECRET)\b|X{8,}",
        "",
    ),
    allow_secret: js_re(
        r"\b(hygiene:allow-secret|gitleaks:allow|pragma: allowlist secret)\b",
        "",
    ),
    secrets: [
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        r"\bAKIA[0-9A-Z]{16}\b",
        r"\bgh[pousr]_[A-Za-z0-9]{36,}\b",
        r"\bsk-[A-Za-z0-9]{32,}\b",
        r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b",
    ]
    .iter()
    .map(|p| js_re(p, ""))
    .collect(),
    comment_start: js_re(r"^\s*(//|#|\*)", ""),
    debugger: js_re(r"\bdebugger\s*;?\s*$", ""),
    console: js_re(r"\bconsole\.(log|debug|dir)\s*\(", ""),
    py_breakpoint: js_re(r"\b(breakpoint\s*\(\)|(?:i?pdb)\.set_trace\s*\()", ""),
    py_print_open: js_re(r"^\s*print\s*\(", ""),
    py_print_marker: js_re(
        r#"(?i)^\s*print\s*\(\s*["'](here|hi|test|debug|xxx+|!+|\?+)[\s!?.]*["']\s*\)\s*$"#,
        "",
    ),
    py_print_ident: js_re(r"^\s*print\s*\(\s*[A-Za-z_]\w*\s*\)\s*$", ""),
    go_print: js_re(r"\bfmt\.Print(ln|f)?\s*\(", ""),
    rs_debug: js_re(r"\b(dbg!|println!)\s*\(", ""),
    php_debug: js_re(r"\b(var_dump|dd)\s*\(", ""),
    todo: js_re(r"\b(TODO|FIXME|XXX|HACK)\b", ""),
    commented_head: js_re(r"^\s*(//|#)\s*[\w$.\[\]]+\s*[({=]", ""),
    commented_tail: js_re(r"[;){]\s*$", ""),
});

fn is_source_file(file: &str) -> bool {
    SOURCE_EXT.contains(&extname(file).to_lowercase().as_str())
}

fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !is_js_space(*c)).collect()
}

pub fn parse_diff(diff: &str) -> Vec<DiffFile> {
    let r = &*RES;
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current: Option<DiffFile> = None;
    let mut line_no: i64 = 0;
    // Whitespace-collapsed text of removed lines not yet matched by an added one.
    let mut pending_removals: Vec<String> = Vec::new();

    for raw in diff.split('\n') {
        if raw.starts_with("diff --git ") {
            if let Some(c) = current.take() {
                files.push(c);
            }
            let name = r
                .diff_file
                .captures(raw)
                .and_then(|m| m.get(1))
                .map_or("?", |m| m.as_str());
            current = Some(DiffFile {
                file: name.to_string(),
                added: Vec::new(),
                removed_count: 0,
                whitespace_only: 0,
            });
            pending_removals.clear();
            continue;
        }
        let Some(cur) = current.as_mut() else {
            continue;
        };
        if raw.starts_with("@@") {
            line_no = r
                .hunk
                .captures(raw)
                .and_then(|m| m.get(1))
                .and_then(|n| n.as_str().parse::<i64>().ok())
                .map_or(0, |n| n - 1);
            pending_removals.clear();
            continue;
        }
        if raw.starts_with("---") || raw.starts_with("+++") {
            continue;
        }
        if let Some(text) = raw.strip_prefix('-') {
            cur.removed_count += 1;
            pending_removals.push(strip_ws(text));
            continue;
        }
        if let Some(text) = raw.strip_prefix('+') {
            line_no += 1;
            cur.added.push(AddedLine {
                line: line_no,
                text: text.to_string(),
            });
            // Whitespace-only churn: an added line matching a removed one once spaces are
            // collapsed.
            let norm = strip_ws(text);
            // Consume the match. Without this one removed line satisfied unlimited added lines, so
            // a file that genuinely added a duplicate line was classified as pure formatting
            // churn.
            let at = pending_removals.iter().position(|r| *r == norm);
            if let (false, Some(at)) = (norm.is_empty(), at) {
                cur.whitespace_only += 1;
                pending_removals.remove(at);
            }
            continue;
        }
        if raw.starts_with(' ') {
            line_no += 1;
            pending_removals.clear();
        }
    }
    if let Some(c) = current {
        files.push(c);
    }
    files
}

struct LineRule {
    check: &'static str,
    severity: &'static str,
    /// Which files the rule applies to. Comment-shaped rules must not run on prose: `#` is a
    /// heading in Markdown, so `commented-out-code` fired on six SKILL.md files in this repo's own
    /// dogfood. A credential or a conflict marker, by contrast, matters in every file type there
    /// is.
    source_only: bool,
    test: fn(&str, &str) -> bool,
    problem: &'static str,
    fix: &'static str,
}

fn conflict_marker(t: &str, _: &str) -> bool {
    RES.conflict.is_match(t)
}

fn secret_shaped(t: &str, _: &str) -> bool {
    // Deliberately only the shapes that are almost never a false positive. /viby-toolkit:secure is
    // the real pass; this catches the obvious one at the moment it would enter history.
    //
    // Published placeholders are excluded first. AWS documents keys ending in EXAMPLE so they can
    // be written down safely, and they appear in docs, tests and this checker's own fixtures —
    // flagging them is how a P1 rule teaches the reader to ignore it. Found by this checker firing
    // on its own test file once it learned to read untracked files.
    !RES.example_credential.is_match(t)
        && !RES.allow_secret.is_match(t)
        && RES.secrets.iter().any(|re| re.is_match(t))
}

fn debug_added(t: &str, f: &str) -> bool {
    let r = &*RES;
    let ext = extname(f).to_lowercase();
    if r.comment_start.is_match(t) {
        return false; // a commented-out one is a different rule
    }
    if r.script_path.is_match(f) || r.script_file.is_match(f) {
        return false; // a script's output is not debug
    }
    if r.debugger.is_match(t) {
        return true;
    }
    if r.console.is_match(t) && !f.ends_with(".test.ts") {
        return true;
    }
    // Python is not JavaScript here: `print()` IS the language's output mechanism, so flagging
    // every one floods any CLI, scorer or report generator with false positives — 23 of them on
    // tests/routing/score.py alone, which is a reporting tool whose output is the product. Only
    // the unambiguous debug forms count: an interactive breakpoint, or a print that is visibly a
    // debugging marker (the `{x=}` f-string form, or a bare scratch literal).
    if ext == ".py" {
        if r.py_breakpoint.is_match(t) {
            return true;
        }
        if r.py_print_open.is_match(t) && t.contains("=}") {
            return true;
        }
        // The literal must be ONLY the marker. `print("!! no runs found, skipping")` is a warning
        // a user reads; `print("!!!")` is something you typed to see if a branch was reached.
        if r.py_print_marker.is_match(t) {
            return true;
        }
        // `print(x)` — a bare identifier with no formatting — is the classic scratch print. A
        // formatted row (`print(f"{a:<8}{b:>4}")`) is output, and that is the distinction.
        return r.py_print_ident.is_match(t);
    }
    (ext == ".go" && r.go_print.is_match(t))
        || (ext == ".rs" && r.rs_debug.is_match(t))
        || (ext == ".php" && r.php_debug.is_match(t))
}

fn todo_added(t: &str, _: &str) -> bool {
    RES.todo.is_match(t)
}

fn commented_out_code(t: &str, _: &str) -> bool {
    RES.commented_head.is_match(t) && RES.commented_tail.is_match(t)
}

const LINE_RULES: &[LineRule] = &[
    LineRule {
        check: "conflict-marker",
        severity: "P1",
        source_only: false,
        test: conflict_marker,
        problem: "a merge conflict marker is being committed — the file is in a half-merged state",
        fix: "resolve the conflict and remove the markers",
    },
    LineRule {
        check: "secret-shaped",
        severity: "P1",
        source_only: false,
        test: secret_shaped,
        problem: "this line looks like a real credential, and a secret in a commit is compromised even if you remove it later",
        fix: "remove it, rotate the credential, and load it from the environment — see /viby-toolkit:secure",
    },
    LineRule {
        check: "debug-added",
        severity: "P2",
        source_only: true,
        test: debug_added,
        problem: "a debug print or breakpoint is being added — these are for the ten minutes you were debugging, not for the branch",
        fix: "remove it, or replace it with real instrumentation via /viby-toolkit:observe if the signal is worth keeping",
    },
    LineRule {
        check: "todo-added",
        severity: "P3",
        source_only: true,
        test: todo_added,
        problem: "a new TODO/FIXME — it will outlive the branch, and nobody is assigned to it",
        fix: "do it, delete it, or file it with an owner; a marker with no owner is a wish",
    },
    LineRule {
        check: "commented-out-code",
        severity: "P3",
        source_only: true,
        test: commented_out_code,
        problem: "commented-out code is being added — the next reader cannot tell if it is a spare part or a mistake",
        fix: "delete it; git remembers, and a commented block does not",
    },
];

fn names(files: &[&DiffFile]) -> String {
    files
        .iter()
        .take(3)
        .map(|f| f.file.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn check_diff_hygiene(diff: &str) -> Hygiene {
    let parsed = parse_diff(diff);
    let mut findings: Vec<Finding> = Vec::new();

    for f in &parsed {
        for a in &f.added {
            for r in LINE_RULES {
                if r.source_only && !is_source_file(&f.file) {
                    continue;
                }
                if (r.test)(&a.text, &f.file) {
                    findings.push(Finding {
                        file: f.file.clone(),
                        line: a.line,
                        check: r.check.to_string(),
                        severity: r.severity,
                        problem: r.problem.to_string(),
                        fix: r.fix.to_string(),
                    });
                }
            }
        }
    }

    let added_lines: usize = parsed.iter().map(|f| f.added.len()).sum();
    let removed: usize = parsed.iter().map(|f| f.removed_count).sum();
    let file_count = parsed.len();
    let mut diff_level =
        |check: &str, severity: &'static str, file: String, problem: String, fix: &str| {
            findings.push(Finding {
                file,
                line: 1,
                check: check.to_string(),
                severity,
                problem,
                fix: fix.to_string(),
            });
        };

    // ---- diff-level checks
    if added_lines + removed > UNREVIEWABLE {
        diff_level(
            "unreviewable-size",
            "P1",
            "(diff)".to_string(),
            format!("{added_lines} added lines across {file_count} file(s): measured defect detection falls to ~28% past 1,000 changed lines, so this will not be reviewed so much as skimmed"),
            "split it — mechanical/formatting changes in their own commit, then structural moves, then behaviour",
        );
    } else if added_lines > REVIEWABLE_MAX {
        diff_level(
            "oversized-diff",
            "P2",
            "(diff)".to_string(),
            format!("{added_lines} added lines: reviewers spot defects best at 200–400 changed lines and detection drops past that"),
            "split it if the parts are separable; if it genuinely is not, say so in the description and point the reviewer at the risky part",
        );
    }

    // Formatting churn mixed with real work makes a diff unreadable, and hides the real change.
    let ratio = |f: &DiffFile| f.whitespace_only as f64 / f.added.len() as f64;
    let churn: Vec<&DiffFile> = parsed
        .iter()
        .filter(|f| !f.added.is_empty() && ratio(f) > 0.6 && f.added.len() > 5)
        .collect();
    let real: Vec<&DiffFile> = parsed
        .iter()
        .filter(|f| !f.added.is_empty() && ratio(f) <= 0.6)
        .collect();
    if !churn.is_empty() && !real.is_empty() {
        diff_level(
            "mixed-concerns",
            "P2",
            names(&churn),
            format!("{} file(s) are almost entirely whitespace/formatting churn while {} carry real changes — the reformatting hides the change that matters", churn.len(), real.len()),
            "commit the formatting separately, so the behavioural diff is readable on its own",
        );
    }

    let is_test = |f: &str| RES.test_path.is_match(f);
    let touched_source: Vec<&DiffFile> = parsed
        .iter()
        .filter(|f| is_source_file(&f.file) && !is_test(&f.file))
        .collect();
    let touched_tests = parsed.iter().filter(|f| is_test(&f.file)).count();
    if !touched_source.is_empty() && touched_tests == 0 && added_lines > 20 {
        diff_level(
            "code-without-test",
            "P2",
            names(&touched_source),
            format!("{} source file(s) changed and no test file touched", touched_source.len()),
            "add or update a test, or state explicitly why this change is not testable — 'no test' is a decision, not an oversight",
        );
    }

    let base_lower = |f: &DiffFile| basename(&f.file).to_lowercase();
    let lock: Vec<&DiffFile> = parsed
        .iter()
        .filter(|f| LOCKFILES.contains(&base_lower(f).as_str()))
        .collect();
    let has_manifest = parsed
        .iter()
        .any(|f| MANIFESTS.contains(&base_lower(f).as_str()));
    if let (Some(first), false) = (lock.first(), has_manifest) {
        diff_level(
            "lockfile-without-manifest",
            "P2",
            first.file.clone(),
            "a lockfile changed with no manifest change — usually an accidental resolution drift from someone's local install rather than an intended upgrade".to_string(),
            "revert it, or make the version change explicit in the manifest — see /viby-toolkit:deps",
        );
    }

    Hygiene {
        findings,
        added_lines,
        files: file_count,
    }
}

fn rank(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

/// A brand-new file has no git history to diff against, so `git diff` never sees it and every
/// content rule below is blind to it — the exact gap this closes. `--exclude-standard` is git's own
/// .gitignore/.git/info/exclude/core.excludesFile logic, so an ignored build artifact is never
/// synthesized into a diff here.
fn untracked_diff(cwd: &Path) -> String {
    let listed = git(cwd, &["ls-files", "--others", "--exclude-standard", "-z"]).stdout;
    let mut diff = String::new();
    for file in listed.split('\0').filter(|f| !f.is_empty()) {
        // Gone, unreadable, or not a regular file.
        let Ok(bytes) = fs::read(join(&[&cwd.to_string_lossy(), file])) else {
            continue;
        };
        let content = String::from_utf8_lossy(&bytes);
        if content.contains('\0') || content.is_empty() {
            continue; // binary, or empty
        }
        let mut lines: Vec<&str> = content.split('\n').collect();
        if content.ends_with('\n') {
            lines.pop();
        }
        let mut parts = vec![
            format!("diff --git a/{file} b/{file}"),
            "new file mode 100644".to_string(),
            "--- /dev/null".to_string(),
            format!("+++ b/{file}"),
            format!("@@ -0,0 +1,{} @@", lines.len()),
        ];
        parts.extend(lines.iter().map(|l| format!("+{l}")));
        parts.push(String::new());
        diff.push_str(&parts.join("\n"));
        diff.push('\n');
    }
    diff
}

pub fn main(args: &[String]) -> i32 {
    let opts = [
        value("base", None),
        flag("staged"),
        flag("json"),
        flag("quiet"),
    ];
    let parsed = match parse_or_exit(args, &opts) {
        Ok(p) => p,
        Err(code) => return code,
    };
    // No `allowPositionals`: a stray argument is an error, as in `parseArgs` strict mode.
    if let Some(extra) = parsed.positionals.first() {
        eprintln!(
            "error: Unexpected argument '{extra}'. This command does not take positional arguments"
        );
        return 1;
    }
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    if !git(&cwd, &["rev-parse", "--git-dir"]).ok {
        if !quiet {
            println!("not a git repository — nothing to diff");
        }
        return 2;
    }

    let (diff, what) = if let Some(base) = parsed.value("base") {
        let verify = format!("{base}^{{commit}}");
        if !git(&cwd, &["rev-parse", "--verify", "--quiet", &verify]).ok {
            println!("cannot resolve base ref \"{base}\" — NOTHING was compared, which is deliberately not a pass");
            return 2;
        }
        let range = format!("{base}...HEAD");
        (git(&cwd, &["diff", "--unified=0", &range]).stdout, range)
    } else if parsed.flag("staged") {
        (
            git(&cwd, &["diff", "--unified=0", "--staged"]).stdout,
            "staged changes".to_string(),
        )
    } else {
        (
            git(&cwd, &["diff", "--unified=0"]).stdout
                + &git(&cwd, &["diff", "--unified=0", "--staged"]).stdout
                + &untracked_diff(&cwd),
            "working tree + staged".to_string(),
        )
    };

    if js_trim(&diff).is_empty() {
        if json_out {
            println!("{}", json!({ "findings": [], "addedLines": 0, "files": 0 }));
        } else if !quiet {
            println!("no changes in {what}");
        }
        return 2;
    }

    let mut r = check_diff_hygiene(&diff);

    if json_out {
        let v = json!({
            "findings": r.findings.iter().map(Finding::to_json).collect::<Vec<_>>(),
            "addedLines": r.added_lines,
            "files": r.files,
            "target": what,
        });
        println!("{}", to_pretty(&v));
        return i32::from(!r.findings.is_empty());
    }

    r.findings.sort_by(|a, b| {
        rank(a.severity)
            .cmp(&rank(b.severity))
            .then_with(|| locale_compare(&a.file, &b.file))
    });
    for f in &r.findings {
        let line = if f.line > 1 {
            format!(":{}", f.line)
        } else {
            String::new()
        };
        println!("{}{line}  [{} {}]", f.file, f.severity, f.check);
        println!("    {}", f.problem);
        println!("    fix: {}", f.fix);
    }
    if !quiet {
        println!();
        println!(
            "{}",
            if r.findings.is_empty() {
                format!(
                    "clean: {} added line(s) across {} file(s) in {what}",
                    r.added_lines, r.files
                )
            } else {
                format!(
                    "{} finding(s) — {} added line(s) across {} file(s) in {what}",
                    r.findings.len(),
                    r.added_lines,
                    r.files
                )
            }
        );
        println!(
            "This judges what the diff LOOKS like, not whether it is correct — a perfectly hygienic diff\n\
             can be entirely wrong. Correctness is /viby-toolkit:review and /viby-toolkit:verify."
        );
    }
    i32::from(!r.findings.is_empty())
}
