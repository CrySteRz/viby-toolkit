//! viby-toolkit safety-net differ — the executable half of /viby-toolkit:adopt.
//!
//! Answers one question after a refactor, especially an agent-driven one: **did the tests get
//! weaker?** Not "do they pass" — a suite passes beautifully once you delete the tests that
//! failed. This compares the test suite between two git refs and reports any shrinkage: files
//! gone, test cases gone, assertions gone, skips added, or an early exit inserted.
//!
//! Usage:
//!   viby check-test-drift [--base <ref>] [paths...] [--json] [--quiet]
//! Exit: 0 = the net held or grew, 1 = it shrank, 2 = no tests found to compare.
//!
//! WHY THIS IS THE GATE. Repository-level refactoring is genuinely hard for models — on a
//! benchmark of 1,099 developer-written behaviour-preserving refactorings the best model
//! succeeded 41.58% of the time, and an agent scored 39.4% on compound cases. Under that much
//! failure pressure, the cheapest path to a green run is to edit the check rather than the code:
//! documented shortcuts include modifying or deleting the test file, and inserting sys.exit(0) to
//! leave the harness with a success code. The measured payoff of watching for it is the reason
//! this script exists rather than a paragraph of advice: trajectory-level behaviour monitoring
//! "reduces average hacked-resolved rate from 28.57% to 0.56%, while improving clean resolved
//! rate from 40.22% to 60.53%" (arXiv 2606.26300, fetched 2026-07-29). Watching for the shortcut
//! did not just stop the cheating — it made the honest work substantially more likely to land.
//!
//! WHAT IT IS NOT: it counts declarations and assertions. It cannot see an assertion that was
//! weakened in place (`toEqual(3)` → `toBeTruthy()`) while the count held, and it does not run
//! anything. A clean result means the net did not shrink, never that behaviour is preserved —
//! that is what the characterization suite and the held-out acceptance suite are for.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use fancy_regex::Regex as Fancy;
use regex::Regex;
use serde_json::{json, Value};

use crate::core::args::{flag, parse_or_exit, value};
use crate::core::git::git;
use crate::core::js::{js_fancy, js_re, read_text};
use crate::core::nodepath::{extname, relative, walk_files};
use crate::core::strip::strip_noncode;

// ---- the checker

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStats {
    pub file: String,
    pub tests: usize,
    pub assertions: usize,
    pub skips: usize,
    pub focus: usize,
    pub early_exit: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Totals {
    pub files: usize,
    pub tests: usize,
    pub assertions: usize,
    pub skips: usize,
    pub focus: usize,
}

impl Totals {
    fn add(&mut self, s: &FileStats) {
        self.files += 1;
        self.tests += s.tests;
        self.assertions += s.assertions;
        self.skips += s.skips;
        self.focus += s.focus;
    }

    fn to_json(&self) -> Value {
        json!({
            "files": self.files,
            "tests": self.tests,
            "assertions": self.assertions,
            "skips": self.skips,
            "focus": self.focus,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub check: String,
    pub severity: &'static str,
    pub message: String,
    pub fix: String,
}

#[derive(Debug, Clone)]
pub struct Drift {
    pub base: String,
    pub findings: Vec<Finding>,
    pub before: Totals,
    pub after: Totals,
    pub compared: usize,
    /// False when the base ref does not exist — the comparison never happened.
    pub base_resolved: bool,
}

impl Drift {
    fn to_json(&self) -> Value {
        json!({
            "base": self.base,
            "findings": self.findings.iter().map(|f| json!({
                "file": f.file,
                "check": f.check,
                "severity": f.severity,
                "message": f.message,
                "fix": f.fix,
            })).collect::<Vec<_>>(),
            "before": self.before.to_json(),
            "after": self.after.to_json(),
            "compared": self.compared,
            "baseResolved": self.base_resolved,
        })
    }
}

const TEST_EXT: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rs", ".java", ".rb",
    ".php", ".cs", ".kt", ".swift",
];
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
];

struct TestPathRes(Vec<Regex>);

static TEST_PATH: LazyLock<TestPathRes> = LazyLock::new(|| {
    TestPathRes(vec![
        js_re(r"(^|/)(tests?|specs?|__tests__)/", ""),
        js_re(r"[._-](test|tests|spec|specs)\.[a-z]+$", ""),
        js_re(r"(^|/)test_[^/]*\.py$", ""),
        js_re(r"[^/]*_test\.[a-z]+$", ""),
        js_re(r"(?i)[^/]*Test(s)?\.(java|kt|cs)$", ""),
    ])
});

pub fn is_test_file(p: &str) -> bool {
    let norm = p.replace(std::path::MAIN_SEPARATOR, "/").to_lowercase();
    if !TEST_EXT.contains(&extname(&norm)) {
        return false;
    }
    TEST_PATH.0.iter().any(|re| re.is_match(&norm))
}

struct Counters {
    tests: Vec<Regex>,
    assertions: Vec<Regex>,
    assertions_lookbehind: Vec<Fancy>,
    skips: Vec<Regex>,
    focus: Vec<Regex>,
    early_exit: Vec<Regex>,
}

fn all(pats: &[&str]) -> Vec<Regex> {
    pats.iter().map(|p| js_re(p, "")).collect()
}

static COUNTERS: LazyLock<Counters> = LazyLock::new(|| Counters {
    // Test declarations across the common runners. `it`/`test`/`describe` are matched only as
    // call heads so a variable named `test` does not count.
    tests: all(&[
        r"\b(?:it|test)\s*(?:\.\s*(?:each|concurrent|failing)\s*(?:\([^)]*\))?)?\s*\(",
        r"(?m)^\s*(?:async\s+)?def\s+test\w*\s*\(",
        r"(?m)^\s*func\s+Test\w*\s*\(",
        r"#\[\s*(?:tokio::)?test\s*\]",
        r"@Test\b",
        r#"(?m)^\s*(?:it|specify)\s+['"]"#,
    ]),
    // The assert shapes are deliberately mutually exclusive. Overlapping patterns double-counted
    // `self.assertEqual(...)` as two assertions, which matters more than it sounds: this checker
    // compares counts across two refs, so a miscount that differs between refs invents drift that
    // isn't there, or hides drift that is.
    assertions: all(&[
        r"\bexpect\s*\(",
        r"\.\s*assert\w*\s*\(", // self.assertEqual(, this.assertX(
        r"\bshould\s*\.\s*\w+",
        r"\bt\s*\.\s*(?:Error|Errorf|Fatal|Fatalf)\s*\(",
        r"\brequire\s*\.\s*\w+\s*\(",
        r"\bverify\s*\(",
    ]),
    assertions_lookbehind: [
        r"(?<![.\w])assert\w*\s*\(",  // assert(, assertEqual(, assertThrows(
        r"(?<![.\w])assert\w*\s*!",   // Rust: assert!, assert_eq!
        r"(?<![.\w])assert\s+[^\s=]", // Python/Rust bare: assert x == 1
        r"(?<![.\w])assert\s*\.\s*\w+\s*\(", // testify: assert.Equal(
    ]
    .iter()
    .map(|p| js_fancy(p, ""))
    .collect(),
    skips: all(&[
        r"\b(?:it|test|describe|context)\s*\.\s*(?:skip|todo)\b",
        r"\bx(?:it|test|describe)\s*\(",
        r"@(?:pytest\.mark\.)?skip\w*",
        r"\bt\s*\.\s*Skip\s*\(",
        r"#\[\s*ignore\s*\]",
        r"@(?:Disabled|Ignore)\b",
        r"\bunittest\s*\.\s*skip",
    ]),
    focus: all(&[
        r"\b(?:it|test|describe|context)\s*\.\s*only\b",
        r"\bf(?:it|test|describe)\s*\(",
    ]),
    // The documented harness escape: leave the runner with a success code before anything runs.
    early_exit: all(&[
        r"\bsys\s*\.\s*exit\s*\(\s*0\s*\)",
        r"\bprocess\s*\.\s*exit\s*\(\s*0\s*\)",
        r"\bos\s*\.\s*_?exit\s*\(\s*0\s*\)",
        r"\bpytest\s*\.\s*exit\s*\(",
    ]),
});

/// Count occurrences of the patterns, on code with strings and comments blanked.
fn count(code: &str, res: &[Regex]) -> usize {
    res.iter().map(|re| re.find_iter(code).count()).sum()
}

fn count_lookbehind(code: &str, res: &[Fancy]) -> usize {
    res.iter()
        .map(|re| re.find_iter(code).take_while(Result::is_ok).count())
        .sum()
}

pub fn stats_for(file: &str, raw: &str) -> FileStats {
    let code = strip_noncode(raw, &extname(file).to_lowercase());
    let c = &*COUNTERS;
    FileStats {
        file: file.to_string(),
        tests: count(&code, &c.tests),
        assertions: count(&code, &c.assertions) + count_lookbehind(&code, &c.assertions_lookbehind),
        skips: count(&code, &c.skips),
        focus: count(&code, &c.focus),
        early_exit: count(&code, &c.early_exit),
    }
}

fn git_out(cwd: &Path, args: &[&str]) -> (bool, String) {
    let o = git(cwd, args);
    (o.ok, o.stdout)
}

pub fn check_test_drift(cwd: &str, base: &str, targets: &[String]) -> Drift {
    let mut findings: Vec<Finding> = Vec::new();
    let mut before = Totals::default();
    let mut after = Totals::default();
    let unresolved = |findings: Vec<Finding>, before: Totals, after: Totals| Drift {
        base: base.to_string(),
        findings,
        before,
        after,
        compared: 0,
        base_resolved: false,
    };
    let cwd_path = Path::new(cwd);

    if !git_out(cwd_path, &["rev-parse", "--git-dir"]).0 {
        return unresolved(findings, before, after);
    }
    // A base ref that does not exist used to yield an empty baseline, which reported "0 → 198
    // tests, the safety net grew" — a perfect score for a typo. A comparison that did not happen
    // must never read as a pass.
    let verify = format!("{base}^{{commit}}");
    if !git_out(cwd_path, &["rev-parse", "--verify", "--quiet", &verify]).0 {
        return unresolved(findings, before, after);
    }

    // Test files as they exist NOW.
    let mut now: HashMap<String, FileStats> = HashMap::new();
    for t in targets {
        let files: Vec<String> = match fs::metadata(t) {
            Ok(m) if m.is_dir() => walk_files(
                t,
                |name| SKIP_DIRS.contains(&name),
                |_, full, is_file| is_file && is_test_file(full),
            ),
            Ok(_) if is_test_file(t) => vec![t.clone()],
            _ => Vec::new(),
        };
        for f in files {
            let rel = match relative(cwd, &f) {
                r if r.is_empty() => f.clone(),
                r => r,
            };
            // Unreadable: falls through as a deletion if it existed at base.
            if let Some(raw) = read_text(&f) {
                let s = stats_for(&rel, &raw);
                after.add(&s);
                now.insert(rel, s);
            }
        }
    }

    // Test files as they existed at BASE — listed from git, so a file deleted entirely is seen.
    let listed = git_out(cwd_path, &["ls-tree", "-r", "--name-only", base]).1;
    let mut was_there: Vec<(String, FileStats)> = Vec::new();
    for rel in listed
        .split('\n')
        .filter(|f| !f.is_empty() && is_test_file(f))
    {
        let (ok, out) = git_out(cwd_path, &["show", &format!("{base}:{rel}")]);
        if !ok {
            continue;
        }
        let s = stats_for(rel, &out);
        before.add(&s);
        was_there.push((rel.to_string(), s));
    }

    // Suite-wide movement first: a file that moved is not a loss, and reporting it as one on every
    // rename would make this checker the boy who cried wolf.
    let lost_tests = before.tests as i64 - after.tests as i64;
    let lost_assertions = before.assertions as i64 - after.assertions as i64;

    let mut push = |file: &str, check: &str, severity: &'static str, message: String, fix: &str| {
        findings.push(Finding {
            file: file.to_string(),
            check: check.to_string(),
            severity,
            message,
            fix: fix.to_string(),
        });
    };

    for (rel, was) in &was_there {
        let Some(is_now) = now.get(rel) else {
            if was.tests == 0 {
                continue;
            }
            push(
                rel,
                if lost_tests > 0 {
                    "test-file-deleted"
                } else {
                    "test-file-moved"
                },
                // A move is P3 rather than P2 when the suite grew on BOTH counts: "the net is
                // bigger than it was" is strong evidence nothing was lost, and over a long history
                // window this rule otherwise fires once per rename. Measured on a real 30-commit
                // range: 4 renames, 4 findings, zero of them a problem.
                if lost_tests > 0 {
                    "P1"
                } else if lost_assertions <= 0 && after.tests > before.tests {
                    "P3"
                } else {
                    "P2"
                },
                if lost_tests > 0 {
                    format!("test file gone, and the suite lost {lost_tests} test(s) overall — coverage was removed, not relocated")
                } else {
                    "test file gone but the suite-wide test count held — looks relocated; confirm it really moved".to_string()
                },
                "restore it, or point at where the same cases now live",
            );
            continue;
        };
        if is_now.tests < was.tests {
            push(
                rel,
                "tests-removed",
                "P1",
                format!(
                    "{} test(s) → {}: {} removed",
                    was.tests,
                    is_now.tests,
                    was.tests - is_now.tests
                ),
                "a refactor does not delete test cases; restore them, or state which behaviour was deliberately dropped and why",
            );
        }
        if is_now.assertions < was.assertions {
            push(
                rel,
                "assertions-removed",
                "P1",
                format!(
                    "{} assertion(s) → {}: the tests may still run, but they check less",
                    was.assertions, is_now.assertions
                ),
                "restore the assertions — a passing test that asserts nothing is worse than a deleted one, because it looks like cover",
            );
        }
        if is_now.skips > was.skips {
            push(
                rel,
                "skips-added",
                "P1",
                format!(
                    "{} test(s) newly skipped — a skipped test reports green while checking nothing",
                    is_now.skips - was.skips
                ),
                "fix the code so it passes, or delete the test deliberately with a stated reason; never park it as skipped to get a green run",
            );
        }
        if is_now.focus > was.focus {
            push(
                rel,
                "focus-added",
                "P2",
                format!(
                    "{} focused test(s) added — .only silently stops the rest of the file from running",
                    is_now.focus - was.focus
                ),
                "remove the focus before this lands",
            );
        }
        if is_now.early_exit > was.early_exit {
            push(
                rel,
                "early-exit-added",
                "P1",
                "a zero-status exit was added inside the test suite — this leaves the harness successful without running the tests".to_string(),
                "remove it; this is the documented shortcut for faking a green run, and it must never survive review",
            );
        }
    }

    if lost_assertions > 0 && !findings.iter().any(|f| f.check == "assertions-removed") {
        findings.push(Finding {
            file: "(suite)".to_string(),
            check: "suite-assertions-down".to_string(),
            severity: "P1",
            message: format!("the suite lost {lost_assertions} assertion(s) overall even though no single file dropped — check moved or rewritten files"),
            fix: "account for every assertion that disappeared".to_string(),
        });
    }

    Drift {
        base: base.to_string(),
        findings,
        before,
        after,
        compared: was_there.len(),
        base_resolved: true,
    }
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

pub fn main(args: &[String]) -> i32 {
    let opts = [value("base", Some("HEAD")), flag("json"), flag("quiet")];
    let parsed = match parse_or_exit(args, &opts) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let base = parsed.value("base").unwrap_or("HEAD");
    let cwd = std::env::current_dir()
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string());
    let targets = if parsed.positionals.is_empty() {
        vec![".".to_string()]
    } else {
        parsed.positionals.clone()
    };
    let mut d = check_test_drift(&cwd, base, &targets);

    if !d.base_resolved {
        if json_out {
            println!("{}", to_pretty(&d.to_json()));
        } else {
            println!(
                "cannot resolve base ref \"{}\" (or this is not a git repository) — NOTHING was compared.\n\
                 This is deliberately not a pass: an unresolvable ref used to report an empty baseline,\n\
                 which looked like a suite that had grown from zero.",
                d.base
            );
        }
        return 2;
    }
    if d.compared == 0 && d.after.files == 0 {
        if json_out {
            println!("{}", to_pretty(&d.to_json()));
        } else if !quiet {
            println!("no test files found at either ref — nothing to compare");
        }
        return 2;
    }

    if json_out {
        println!("{}", to_pretty(&d.to_json()));
        return i32::from(!d.findings.is_empty());
    }

    d.findings.sort_by_key(|f| severity_rank(f.severity));
    for f in &d.findings {
        println!("{}  [{} {}]", f.file, f.severity, f.check);
        println!("    {}", f.message);
        println!("    fix: {}", f.fix);
    }

    if !quiet {
        println!();
        println!(
            "vs {}: {} → {} tests, {} → {} assertions, {} → {} skipped, {} → {} focused",
            d.base,
            d.before.tests,
            d.after.tests,
            d.before.assertions,
            d.after.assertions,
            d.before.skips,
            d.after.skips,
            d.before.focus,
            d.after.focus,
        );
        println!(
            "{}",
            if d.findings.is_empty() {
                "the safety net held or grew".to_string()
            } else {
                format!("{} sign(s) that the safety net shrank", d.findings.len())
            }
        );
        println!(
            "Counts only. An assertion weakened in place keeps the count and passes this check, and\n\
             nothing here was executed — so this proves the net was not cut, never that behaviour was\n\
             preserved. That is what the characterization and held-out suites are for. See\n\
             /viby-toolkit:adopt."
        );
    }
    i32::from(!d.findings.is_empty())
}

/// `JSON.stringify(v, null, 2)`.
pub fn to_pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}
