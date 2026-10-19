//! viby-toolkit memory auditor — the executable half of /viby-toolkit:brain.
//!
//! A memory store degrades silently. Nothing errors when a memory goes stale, contradicts another,
//! or becomes unfindable — it just quietly starts misleading every future session, with the
//! authority of something the agent "knows". This audits the store itself.
//!
//! Usage:
//!   viby check-memory <memory-dir> [--root <repo-root>] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no memory found.
//!
//! WHY THESE CHECKS. Three findings from the agent-memory literature shape the rule set:
//!
//!  1. **Retrieval failure is where the errors are.** Across LongMemEval, LoCoMo, STALE and
//!     PersonaMem, answer errors concentrate in cases where retrieval failed; retrieval succeeding
//!     but the answer still being wrong accounts for only 5.8–13.7%. So the highest-value property
//!     of a memory store is that entries are *findable* — hence the index checks.
//!  2. **Staleness is the hard, under-measured failure.** A dedicated benchmark exists for exactly
//!     this question ("can agents know when their memories are no longer valid?") and the noted gap
//!     is that benchmarks "rarely isolate whether a model can determine that a previously valid
//!     memory has been rendered obsolete". In a coding project the obsolete memory is usually
//!     mechanically detectable: it cites a path that no longer exists.
//!  3. **Poisoning is defended with provenance.** The proposed defense is reliability-conditional
//!     updating with a provenance cap — an entry's influence bounded by how well-sourced it is. That
//!     requires every entry to carry provenance at all, which is checkable.
//!
//! WHAT IT CANNOT DO: it cannot tell you a memory is *true*, cannot detect a contradiction phrased
//! differently from its counterpart, and cannot know which of two conflicting entries is current. A
//! clean run means the store is well-formed and its references resolve — not that it is correct.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

use crate::core::args::{flag, parse_or_exit, value};
use crate::core::js::{is_js_space, locale_compare, static_regex, JS_SPACE};
use crate::core::nodepath::{basename, join};

// The `regex` crate's `\s`, `\b`, `\w` and `\d` differ from JavaScript's, so every pattern below
// spells them out (`JS_SPACE`) or opts out of Unicode with `(?-u:..)` / `(?i-u:..)`.

#[derive(Debug, Clone)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub check: &'static str,
    pub severity: &'static str,
    pub problem: String,
    pub fix: &'static str,
}

impl Finding {
    fn new(
        file: &str,
        line: usize,
        check: &'static str,
        severity: &'static str,
        problem: String,
        fix: &'static str,
    ) -> Finding {
        Finding {
            file: file.to_string(),
            line,
            check,
            severity,
            problem,
            fix,
        }
    }
}

static DATE: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(&format!(
        r"(?-u:\b)(20[0-9]{{2}}-[0-9]{{2}}-[0-9]{{2}}|(?-u:\b)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*[{JS_SPACE}]+20[0-9]{{2}})(?-u:\b)"
    ))
});
/// How the entry knows what it claims.
static PROVENANCE: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?i-u:\b(measured|verified|confirmed|tested|reproduced|observed|ran\b|source:|per the|according to|from the (docs?|changelog|error|logs?)|file:line|because|found (that|out)|turns out|it was|the error|the fix|documented in|see \[|https?://))",
    )
});
/// A claim that rests only on assertion. Sycophantic memory is storing what was said as if true.
static HEARSAY: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?i-u:\b(the user (said|says|thinks|believes|mentioned)|apparently|I think|probably|seems? like|assume[ds]?)\b)",
    )
});

/// Filename tokens that carry no topic: naming-convention prefixes, dates, and the project label that
/// every entry in a store shares. Including them made same-project notes look like duplicates.
const NOISE_TOKEN: &[&str] = &[
    "project", "feedback", "note", "notes", "memory", "user", "client", "jan", "feb", "mar", "apr",
    "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// A path reference worth resolving: backticked, has a slash, has an extension.
static PATH_REF: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"`([~A-Za-z0-9_./-]*/[A-Za-z0-9_.-]+\.[A-Za-z]{1,5})(?::[0-9]+)?`")
});
/// Placeholders that are illustrations, not references.
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?i-u:(^|/)(foo|bar|baz|example|sample|your|my|path|dir|some|thing|todo|xxx)([./]|$)|[<>*]|\.\.\.)",
    )
});
static FRONT_MATTER: LazyLock<Regex> = LazyLock::new(|| static_regex(r"(?s)^---.*?\n---\n"));
static INDEX_LINK: LazyLock<Regex> = LazyLock::new(|| static_regex(r"\(([A-Za-z0-9_.-]+\.md)\)"));

/// Candidate absolute paths for a reference. A memory about a project naturally cites paths relative
/// to the repo root, to a package subdirectory, or to home — so a single assumed root produces false
/// "stale" findings on perfectly good references. Try the root and two ancestors before concluding a
/// path is gone.
fn candidates(reference: &str, root: &str) -> Vec<PathBuf> {
    if PLACEHOLDER.is_match(reference) {
        return Vec::new();
    }
    if let Some(rest) = reference.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        return vec![PathBuf::from(join(&[&home, rest]))];
    }
    if reference.starts_with('/') {
        return vec![PathBuf::from(reference)];
    }
    vec![
        PathBuf::from(join(&[root, reference])),
        PathBuf::from(join(&[root, "..", reference])),
        PathBuf::from(join(&[root, "..", "..", reference])),
    ]
}

fn exists_anywhere(reference: &str, root: &str) -> Option<bool> {
    let c = candidates(reference, root);
    if c.is_empty() {
        return None;
    }
    Some(c.iter().any(|p| Path::new(p).exists()))
}

struct Ref {
    reference: String,
    line: usize,
    exists: bool,
}

pub fn audit_entry(file: &str, text: &str, root: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let lines: Vec<&str> = text.split('\n').collect();
    let body = FRONT_MATTER.replace(text, "");

    // 1. Stale references — the mechanically detectable half of the staleness problem.
    //
    // SELF-CALIBRATING, because the first run of this check was confidently wrong. A memory store's
    // relative paths belong to ITS project, and auditing five stores against one root reported 23
    // stale references that were simply resolved against the wrong tree. So: if NOTHING in a file
    // resolves, the root is wrong, not the memory — say that instead of inventing findings.
    let mut refs: Vec<Ref> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for m in PATH_REF.captures_iter(line) {
            let reference = m[1].to_string();
            if let Some(exists) = exists_anywhere(&reference, root) {
                refs.push(Ref {
                    reference,
                    line: i + 1,
                    exists,
                });
            }
        }
    }
    // A memory entry legitimately cites TWO namespaces: sibling memory files (`[[slug]]`,
    // `other-entry.md`) which live next to it, and PROJECT paths which live in a repo this checker was
    // not given. The single-root guard above was defeated by that mix — one resolving sibling made the
    // root "look right", and then every project path was reported stale. So judge the project root
    // only on refs that look like project paths, and stay silent when none of those resolve.
    let is_project_ref = |r: &str| r.contains('/');
    let project_root_known = !refs.iter().any(|r| is_project_ref(&r.reference))
        || refs
            .iter()
            .any(|r| is_project_ref(&r.reference) && r.exists);
    let root_looks_right = refs.is_empty() || refs.iter().any(|r| r.exists);
    if !root_looks_right {
        findings.push(Finding::new(
            file,
            refs.first().map_or(1, |r| r.line),
            "root-unknown",
            "P3",
            format!("none of its {} path reference(s) resolve against the given root, so staleness could NOT be checked — the root is probably wrong for this store", refs.len()),
            "re-run with --root pointing at the project this memory belongs to",
        ));
    }
    if root_looks_right {
        for r in refs
            .iter()
            .filter(|r| !is_project_ref(&r.reference) || project_root_known)
        {
            if !r.exists {
                findings.push(Finding::new(
                    file,
                    r.line,
                    "stale-reference",
                    "P1",
                    format!("cites `{}`, which does not exist — a memory pointing at a moved or deleted path misleads with the authority of something the agent \"knows\"", r.reference),
                    "update the reference, or delete the entry if the thing it describes is gone; a memory that cannot be checked cannot be trusted",
                ));
            }
        }
    }

    // 2. Provenance and dating — the poisoning defense requires both.
    if !DATE.is_match(text) {
        findings.push(Finding::new(
            file,
            1,
            "undated",
            "P2",
            "carries no date, so nothing can decide whether it has been superseded".into(),
            "add the date it was established; a memory without a timestamp can never be retired on evidence",
        ));
    }
    if !PROVENANCE.is_match(&body) {
        findings.push(Finding::new(
            file,
            1,
            "no-provenance",
            "P2",
            "states facts without saying how they were established, so a guess and an executed check carry equal weight in future sessions".into(),
            "say how you know — measured, verified by running X, observed in the logs, stated by the user",
        ));
    }
    if let Some(at) = lines
        .iter()
        .position(|l| HEARSAY.is_match(l) && !PROVENANCE.is_match(l))
    {
        findings.push(Finding::new(
            file,
            at + 1,
            "unverified-claim",
            "P2",
            "records something asserted rather than established — storing what was said as if it were true is how a memory store becomes sycophantic".into(),
            "verify it and record the check, or label it explicitly as a claim rather than a fact",
        ));
    }

    // 3. Size — a memory that grew into a document defeats retrieval, because it all comes back.
    let words = body.split(is_js_space).filter(|w| !w.is_empty()).count();
    if words > 900 {
        findings.push(Finding::new(
            file,
            1,
            "oversized-entry",
            "P2",
            format!("{words} words in one entry — it has become a document, and retrieving any part of it drags the whole thing into context"),
            "split it by topic into separate entries that can be recalled independently, and link them",
        ));
    }

    findings
}

pub struct Store {
    pub findings: Vec<Finding>,
    pub entries: usize,
}

/// Compare only the DISTINGUISHING tokens. Under a `project_<name>_<topic>_<date>` naming
/// convention the project and date tokens dominate, so two genuinely different notes about the
/// same project on the same day looked like duplicates — measured on a real store.
fn distinguishing(name: &str) -> HashSet<String> {
    let stem = name.strip_suffix(".md").unwrap_or(name);
    stem.split(['-', '_'])
        .filter(|w| {
            w.encode_utf16().count() > 2
                && !w.bytes().all(|b| b.is_ascii_digit())
                && !NOISE_TOKEN.contains(&w.to_lowercase().as_str())
        })
        .map(str::to_string)
        .collect()
}

/// `Err` when the index exists but cannot be read; the TypeScript threw there, uncaught.
pub fn audit_store(dir: &str, root: &str) -> Result<Store, String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Ok(Store {
            findings: Vec::new(),
            entries: 0,
        });
    };
    // Sorted: `readdirSync` hands names back in OS order; byte order is the deterministic choice.
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    names.sort();
    let mut findings = Vec::new();
    let index_at = names.iter().position(|n| n.to_uppercase() == "MEMORY.MD");
    let index_name = index_at.map(|i| names[i].clone());
    let entries: Vec<&String> = names
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != index_at)
        .map(|(_, n)| n)
        .collect();

    for n in &entries {
        let full = Path::new(dir).join(n);
        // Unreadable: skipped, as the TypeScript did.
        if let Ok(bytes) = std::fs::read(&full) {
            findings.extend(audit_entry(n, &String::from_utf8_lossy(&bytes), root));
        }
    }

    // 4. Findability. Retrieval failure is where memory errors concentrate, so an entry missing from
    //    the index is the highest-severity structural defect in a store.
    match &index_name {
        None => {
            if !entries.is_empty() {
                findings.push(Finding::new(
                    "(store)",
                    1,
                    "no-index",
                    "P1",
                    format!("{} entr(ies) and no MEMORY.md index — nothing tells a new session what is in here", entries.len()),
                    "add MEMORY.md with one line per entry: title, link, and a hook describing when it is relevant",
                ));
            }
        }
        Some(index_name) => {
            let path = Path::new(dir).join(index_name);
            let bytes =
                std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let index = String::from_utf8_lossy(&bytes);
            for n in &entries {
                if !index.contains(n.as_str()) {
                    findings.push(Finding::new(
                        n,
                        1,
                        "not-indexed",
                        "P1",
                        "exists but is not listed in MEMORY.md, so it will not be found when it matters — and retrieval failure, not reasoning, is where memory errors concentrate".into(),
                        "add a one-line pointer to MEMORY.md",
                    ));
                }
            }
            for m in INDEX_LINK.captures_iter(&index) {
                let target = &m[1];
                if !entries.iter().any(|e| e.as_str() == target)
                    && target.to_uppercase() != "MEMORY.MD"
                {
                    findings.push(Finding::new(
                        index_name,
                        1,
                        "index-dangling",
                        "P1",
                        format!("the index links `{target}`, which does not exist — a session told to read it finds nothing"),
                        "fix the link or remove the line",
                    ));
                }
            }
        }
    }

    // 5. Duplicate topics — real stores need to merge duplicates, and near-identical names are the
    //    detectable case.
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (a, b) = (entries[i], entries[j]);
            let (ta, tb) = (distinguishing(a), distinguishing(b));
            // A name with one distinguishing token cannot be judged a duplicate: `auth.md` vs
            // `auth-token-refresh-race.md` shared exactly one token and scored 1.0 under min().
            if ta.len() < 2 || tb.len() < 2 {
                continue;
            }
            let shared = ta.iter().filter(|w| tb.contains(*w)).count();
            // shared / max >= 0.75, without a float.
            if shared * 4 >= ta.len().max(tb.len()) * 3 {
                findings.push(Finding::new(
                    &format!("{a} + {b}"),
                    1,
                    "duplicate-topic",
                    "P2",
                    "two entries cover nearly the same topic — when they drift apart, a future session gets whichever it happens to retrieve".into(),
                    "merge them into one entry, or make the distinction explicit in both names",
                ));
            }
        }
    }

    Ok(Store {
        findings,
        entries: entries.len(),
    })
}

fn order(severity: &str) -> u8 {
    match severity {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

fn to_json(f: &Finding) -> Value {
    json!({"file": f.file, "line": f.line, "check": f.check, "severity": f.severity,
        "problem": f.problem, "fix": f.fix})
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[value("root", None), flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    if parsed.positionals.is_empty() {
        if !quiet {
            println!("usage: check-memory.ts <memory-dir> [--root <repo-root>]");
        }
        return 2;
    }
    let root = match parsed.value("root") {
        Some(r) => r.to_string(),
        None => std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let mut all: Vec<Finding> = Vec::new();
    let mut entries = 0;
    for d in &parsed.positionals {
        let store = match audit_store(d, &root) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: {e}");
                return 1;
            }
        };
        entries += store.entries;
        let base = basename(d);
        all.extend(store.findings.into_iter().map(|mut f| {
            f.file = format!("{base}/{}", f.file);
            f
        }));
    }

    if entries == 0 {
        if json_out {
            println!("{}", json!({"entries": 0, "findings": []}));
        } else if !quiet {
            println!("no memory entries found");
        }
        return 2;
    }

    let code = i32::from(!all.is_empty());
    if json_out {
        let findings: Vec<Value> = all.iter().map(to_json).collect();
        let out = json!({"entries": entries, "findings": findings});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return code;
    }

    all.sort_by(|a, b| {
        order(a.severity)
            .cmp(&order(b.severity))
            .then_with(|| locale_compare(&a.file, &b.file))
    });
    for f in &all {
        let at = if f.line > 1 {
            format!(":{}", f.line)
        } else {
            String::new()
        };
        println!("{}{at}  [{} {}]", f.file, f.severity, f.check);
        println!("    {}", f.problem);
        println!("    fix: {}", f.fix);
    }
    if !quiet {
        println!();
        if all.is_empty() {
            println!("clean: {entries} entr(ies), well-formed and resolvable");
        } else {
            println!("{} finding(s) across {entries} entr(ies)", all.len());
        }
        println!(
            "Form and resolvability only. It cannot tell you a memory is TRUE, cannot see a contradiction\n\
             phrased differently from its counterpart, and cannot know which of two conflicting entries is\n\
             current. See /viby-toolkit:brain."
        );
    }
    code
}
