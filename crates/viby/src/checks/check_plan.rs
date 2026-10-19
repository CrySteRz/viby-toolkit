//! viby-toolkit plan validator — the executable half of /viby-toolkit:plan.
//!
//! `principles` §3 permits parallel writes only when you "can name the partition and the hubs". That
//! has always been a rule with no way to check it: a plan either had a real partition or it didn't,
//! and you found out when two agents edited the same file. This validates the partition mechanically
//! BEFORE anything is dispatched.
//!
//! Usage:
//!   viby check-plan <plan.md>... [--json] [--quiet]
//! Exit: 0 = the plan is dispatchable, 1 = findings, 2 = no task list found.
//!
//! Expected task-line shape (order of fields does not matter, `·` or `|` both separate):
//!
//!   - [ ] T1 — add the CSV parser · files: src/parse.ts · verify: npm test -- parse · deps: —
//!   - [x] T2 — wire it into the route · files: src/route.ts · verify: npm test · deps: T1
//!
//! WHAT IT CANNOT DO: it cannot tell you the plan is a good plan, that the steps are in a sensible
//! order, or that the tasks add up to the goal. It checks that the plan is *dispatchable* — that
//! every task says what it touches and how it is proved, and that no two tasks which could run at
//! the same time own the same file. That last check is the one worth the whole script.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::core::args::{flag, parse_or_exit};

// JavaScript's `\s` (Unicode white space without U+0085, plus the byte-order mark) and the
// characters its `.` refuses to match.
const JS_S: &str = r"\t\n\x0B\x0C\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JavaScript's `String.prototype.trim`.
fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub files: Vec<String>,
    pub verify: String,
    pub deps: Vec<String>,
    pub done: bool,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub line: usize,
    pub task: String,
    pub check: String,
    pub severity: String,
    pub problem: String,
    pub fix: String,
}

impl Finding {
    fn new(line: usize, task: &str, check: &str, problem: String, fix: String) -> Finding {
        Finding {
            line,
            task: task.to_string(),
            check: check.to_string(),
            severity: "P1".to_string(),
            problem,
            fix,
        }
    }
}

static TASK_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^[{JS_S}]*[-*][{JS_S}]*\[( |x|X)\][{JS_S}]*([A-Za-z0-9_.-]+)[{JS_S}]*[—–:-]([^\n\r\u{{2028}}\u{{2029}}]+)$"
    ))
    .expect("task-line regex")
});
static FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?:^|[·|])[{JS_S}]*((?i-u:files|verify|deps|depends on|wave))[{JS_S}]*:[{JS_S}]*([^·|]*)"
    ))
    .expect("field regex")
});
static LIST_SPLIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"[,{JS_S}]+")).expect("list-split regex"));
static SLASHES: LazyLock<Regex> = LazyLock::new(|| Regex::new("/+").expect("slash regex"));

fn is_sep(c: char) -> bool {
    c == '·' || c == '|'
}

pub fn parse_tasks(text: &str) -> Vec<Task> {
    let mut tasks = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        let Some(m) = TASK_LINE.captures(line) else {
            continue;
        };
        let (box_, id, rest) = (&m[1], &m[2], &m[3]);
        let title = rest.split(is_sep).next().map(trim).unwrap_or("");
        let mut task = Task {
            id: id.to_string(),
            title: title.to_string(),
            files: Vec::new(),
            verify: String::new(),
            deps: Vec::new(),
            done: box_ != " ",
            line: i + 1,
        };
        // Fields live AFTER the first separator. Anchoring on `^` let a task titled
        // "Verify: end-to-end auth flow works" populate `task.verify` from its own title and suppress
        // the no-verify P1 — the check reported a verification that did not exist.
        let tail = rest.find(is_sep).map_or("", |at| &rest[at..]);
        for f in FIELD.captures_iter(tail) {
            let key = f[1].to_ascii_lowercase();
            let val = trim(&f[2]);
            let words = || {
                LIST_SPLIT
                    .split(val)
                    .filter(|v| !v.is_empty() && *v != "—" && *v != "-")
            };
            if key == "files" {
                task.files = words().map(str::to_string).collect();
            } else if key == "verify" {
                task.verify = val.to_string();
            } else if key == "deps" || key == "depends on" {
                task.deps = words()
                    .filter(|v| v.to_lowercase() != "none")
                    .map(str::to_string)
                    .collect();
            }
        }
        tasks.push(task);
    }
    tasks
}

/// Every task that must finish before `id` can start, transitively.
fn ancestors(id: &str, by_id: &HashMap<&str, &Task>, seen: &mut HashSet<String>) {
    let Some(t) = by_id.get(id) else {
        return;
    };
    for d in &t.deps {
        if !seen.insert(d.clone()) {
            continue;
        }
        ancestors(d, by_id, seen);
    }
}

fn ancestor_set(id: &str, by_id: &HashMap<&str, &Task>) -> HashSet<String> {
    let mut seen = HashSet::new();
    ancestors(id, by_id, &mut seen);
    seen
}

fn by_id(tasks: &[Task]) -> HashMap<&str, &Task> {
    // A later task with the same id replaces the earlier one, as `new Map(entries)` does.
    tasks.iter().map(|t| (t.id.as_str(), t)).collect()
}

pub fn check_plan(text: &str) -> (Vec<Task>, Vec<Finding>) {
    let tasks = parse_tasks(text);
    let mut findings = Vec::new();
    if tasks.is_empty() {
        return (tasks, findings);
    }

    let by_id = by_id(&tasks);

    for t in &tasks {
        if t.files.is_empty() {
            findings.push(Finding::new(
                t.line,
                &t.id,
                "no-files",
                "does not name the files it owns, so it cannot be partitioned — this is the task that collides with another agent".into(),
                "add `files: <paths>`; if you genuinely do not know yet, the task is research, not implementation".into(),
            ));
        }
        if t.verify.is_empty() {
            findings.push(Finding::new(
                t.line,
                &t.id,
                "no-verify",
                "has no verification, so 'done' is a feeling — the evidence gate cannot be applied to it".into(),
                "add `verify: <command or observable outcome>` — the thing you will run to prove it works".into(),
            ));
        }
        for d in &t.deps {
            if !by_id.contains_key(d.as_str()) {
                findings.push(Finding::new(
                    t.line,
                    &t.id,
                    "unknown-dep",
                    format!("depends on \"{d}\", which is not a task in this plan"),
                    "fix the reference, or add the missing task".into(),
                ));
            }
        }
        // A cycle means no valid order exists at all.
        if ancestor_set(&t.id, &by_id).contains(&t.id) {
            findings.push(Finding::new(
                t.line,
                &t.id,
                "dep-cycle",
                "is part of a dependency cycle, so there is no order in which this plan can be executed".into(),
                "break the cycle — usually one of the two tasks needs splitting into the part that can go first".into(),
            ));
        }
    }

    // THE check: two tasks that could run at the same time must not own the same file. This is the
    // partition `principles` §3 demands before any parallel write, made mechanical.
    // Key on a normalised path. Raw strings made `src/Auth.ts` and `src/auth.ts` two entries, so on
    // a case-insensitive filesystem — including this machine's — two tasks writing the SAME file were
    // reported as disjoint, which is the one conclusion this check exists to refuse.
    let norm = |f: &str| {
        let f = f.strip_prefix("./").unwrap_or(f);
        SLASHES.replace_all(f, "/").to_lowercase()
    };
    // Insertion order matters: findings come out in the order the files were first seen.
    let mut owners: Vec<(String, Vec<&Task>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for t in &tasks {
        for f in &t.files {
            let key = norm(f);
            let at = *index.entry(key).or_insert_with(|| {
                owners.push((f.clone(), Vec::new()));
                owners.len() - 1
            });
            owners[at].1.push(t);
        }
    }
    for (file, list) in &owners {
        if list.len() < 2 {
            continue;
        }
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let (a, b) = (list[i], list[j]);
                let ordered = ancestor_set(&a.id, &by_id).contains(&b.id)
                    || ancestor_set(&b.id, &by_id).contains(&a.id);
                if !ordered {
                    findings.push(Finding::new(
                        a.line,
                        &format!("{}+{}", a.id, b.id),
                        "unpartitioned-file",
                        format!("both own `{file}` with no dependency between them, so they can be dispatched in parallel and will conflict"),
                        format!("either add a dependency so they are ordered, merge them, or split `{file}` — parallel writes need disjoint ownership"),
                    ));
                }
            }
        }
        if list.len() > 2 {
            let ids: Vec<&str> = list.iter().map(|t| t.id.as_str()).collect();
            let mut f = Finding::new(
                list[0].line,
                &ids.join("+"),
                "hub-file",
                format!("`{file}` is touched by {} tasks — that is a structural hub, and everything else depends on it", list.len()),
                "take the hub yourself, sequentially, before dispatching the tasks around it (principles §3)".into(),
            );
            f.severity = "P2".to_string();
            findings.push(f);
        }
    }

    (tasks, findings)
}

struct Reported {
    file: String,
    finding: Finding,
    /// The unreadable finding lists `file` first; every other one lists it last.
    file_first: bool,
}

impl Reported {
    fn to_json(&self) -> Value {
        let f = &self.finding;
        if self.file_first {
            json!({"file": self.file, "line": f.line, "task": f.task, "check": f.check,
                "severity": f.severity, "problem": f.problem, "fix": f.fix})
        } else {
            json!({"line": f.line, "task": f.task, "check": f.check, "severity": f.severity,
                "problem": f.problem, "fix": f.fix, "file": self.file})
        }
    }
}

static PLAN_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i-u:\.(md|markdown|txt))$").expect("plan-file regex"));

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let files: Vec<&String> = parsed
        .positionals
        .iter()
        .filter(|p| PLAN_FILE.is_match(p))
        .collect();
    if files.is_empty() {
        if !quiet {
            println!("usage: check-plan.ts <plan.md>");
        }
        return 2;
    }

    let mut all: Vec<Reported> = Vec::new();
    let mut task_count = 0;
    let mut parallelisable = 0;
    for f in files {
        let text = match std::fs::read(f) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(_) => {
                all.push(Reported {
                    file: f.clone(),
                    finding: Finding {
                        line: 1,
                        task: "-".into(),
                        check: "unreadable".into(),
                        severity: "P2".into(),
                        problem: "could not read this plan, so it was NOT checked".into(),
                        fix: "fix the path and re-run".into(),
                    },
                    file_first: true,
                });
                continue;
            }
        };
        let (tasks, findings) = check_plan(&text);
        task_count += tasks.len();
        // How many tasks have no unfinished dependency — the first wave you could dispatch.
        let by_id = by_id(&tasks);
        parallelisable += tasks
            .iter()
            .filter(|t| {
                t.deps
                    .iter()
                    .all(|d| by_id.get(d.as_str()).is_some_and(|x| x.done))
            })
            .count();
        for finding in findings {
            all.push(Reported {
                file: f.clone(),
                finding,
                file_first: false,
            });
        }
    }

    if task_count == 0 {
        if json_out {
            let findings: Vec<Value> = all.iter().map(Reported::to_json).collect();
            println!("{}", json!({"tasks": 0, "findings": findings}));
        } else if !quiet {
            println!("no task list found — expected lines like `- [ ] T1 — title · files: … · verify: … · deps: …`");
        }
        return 2;
    }

    let code = i32::from(!all.is_empty());
    if json_out {
        let findings: Vec<Value> = all.iter().map(Reported::to_json).collect();
        let out =
            json!({"tasks": task_count, "dispatchableNow": parallelisable, "findings": findings});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return code;
    }

    let order = |s: &str| match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    };
    all.sort_by_key(|r| (order(&r.finding.severity), r.finding.line));
    for r in &all {
        let f = &r.finding;
        println!(
            "{}:{}  [{} {}]  {}",
            r.file, f.line, f.severity, f.check, f.task
        );
        println!("    {}", f.problem);
        println!("    fix: {}", f.fix);
    }
    if !quiet {
        println!();
        if all.is_empty() {
            println!("dispatchable: {task_count} task(s), {parallelisable} ready now, ownership is disjoint");
        } else {
            println!("{} finding(s) across {task_count} task(s)", all.len());
        }
        println!(
            "This checks the plan is DISPATCHABLE, not that it is a good plan — it cannot tell you the\n\
             steps are sensible or that they add up to the goal. See /viby-toolkit:plan."
        );
    }
    code
}
