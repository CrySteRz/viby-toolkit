//! Verify the delegation defaults are actually written into the skills, not merely believed.
//!
//! Run:  viby check-orchestration [skills-dir]
//! Exit: 0 = clean, 1 = at least one P1.
//!
//! §3 says fanning out is a DEFAULT and §3b says staged fan-out is declared as a script. Prose
//! decays: a skill gets edited, a directive softens into a suggestion, and nothing notices because
//! no test reads intent. This does — the same reason check-plan exists for the partition rule.
//!
//! Three rules, each deliberately narrow so it reports an omission rather than a style preference:
//!   P1 hedged-fanout            a fan-out instruction phrased as a permission. "You may fan out"
//!                               is the phrasing that produced zero fan-outs; that is the whole
//!                               reason these defaults were rewritten.
//!   P1 unstaged-pipeline        a skill whose work is genuinely staged does not instruct a Workflow,
//!                               so its stages get re-improvised on each run.
//!   P1 unisolated-parallel-write  a WRITING agent (`implementer`, `worker`) is parallelised without
//!                               worktree isolation.
//!
//! Port of `check-orchestration.ts`. The TypeScript always checked the skills directory next to
//! the script; the binary has no such place, so the directory is the first argument, else
//! `$CLAUDE_PLUGIN_ROOT/skills`, else `plugins/viby-toolkit/skills` under the cwd.

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::core::js::{read_text, static_regex, JS_SPACE};
use crate::core::nodepath::{join, read_dir_sorted, resolve};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub skill: String,
    pub check: String,
    pub message: String,
}

/// The skills whose work is staged — fan out, then act on each result, then synthesise. These are
/// the ones a script buys something for; everything else is a single sweep and dispatches directly.
/// An explicit list rather than a heuristic: "how many stages does this prose describe" is not
/// mechanically decidable, and a checker that guessed at it would cry wolf.
///
/// Every name here is asserted to exist below, because a rename would otherwise silently retire the
/// rule — `review-cluster` was renamed to `review` while this file was being written.
const STAGED: [&str; 3] = ["orchestrate", "review", "study"];

// The patterns below are Rust syntax with JavaScript's meaning: `/i` in JavaScript is ASCII-only
// here, which `(?i-u:..)` is; `\b` is ASCII, which `(?-u:\b)` is; `\S` is spelled out with the
// JavaScript white-space set.

/// Phrases that mean "run several agents at once". Matched per line.
static FANOUT: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(&format!(
        r"(?-u:\b)(?:(?i-u:in parallel|fan out|fan-out|dispatch (?:one|several))|(?i-u:dispatch )[0-9]+|(?i-u:one )[^{JS_SPACE}]+(?i-u: per (?:dimension|angle|area|question|subsystem|node)))(?-u:\b)"
    ))
});
/// A toolkit agent, backticked — the strict form, used where a false positive would be costly.
static AGENT: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(r"`(scout|reviewer|skeptic|debugger|researcher|implementer|worker)`")
});
/// The same agents unbackticked. Used ONLY for the hedge rule: formatting must not decide whether a
/// softened directive is caught, and "You may optionally fan out scout agents" is exactly the
/// sentence that has to fire. Still requires a NAMED agent, so a retrospective aside like "a
/// web-search fan-out when every agent was filesystem-only" stays clean.
static AGENT_LOOSE: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?-u:\b)(?i-u:scouts?|reviewers?|skeptics?|debuggers?|researchers?|implementers?)(?-u:\b)",
    )
});
/// Softeners that turn a directive back into the permission nobody acts on.
static HEDGE: LazyLock<Regex> = LazyLock::new(|| {
    static_regex(
        r"(?-u:\b)(?i-u:you may|optionally|if you (?:want|like|prefer)|feel free|consider (?:dispatching|fanning|spawning)|could (?:dispatch|fan|spawn))(?-u:\b)",
    )
});
/// The agents that write. Reading agents are safe to parallelise; these are not. `worker` keeps
/// Edit/Write (it is the general Workflow agent) and counts only backticked, because "worker" is
/// also an ordinary English word in skill prose.
static WRITER: LazyLock<Regex> =
    LazyLock::new(|| static_regex(r"(?i-u:`?implementers?`?|`worker`)"));
/// Backticked deliberately: a bare occurrence of the word satisfies nothing. A URL containing
/// "Workflow", or a fenced sample mentioning `.github/workflows`, is not an instruction to author one.
static WORKFLOW: LazyLock<Regex> = LazyLock::new(|| static_regex(r"`Workflow`"));
static ISOLATION: LazyLock<Regex> = LazyLock::new(|| static_regex(r"(?i-u:worktree)"));
static FENCE: LazyLock<Regex> = LazyLock::new(|| static_regex(r"(?s)```.*?```"));
/// How far from a parallel-write instruction the isolation guidance may sit and still count.
const ISOLATION_WINDOW: usize = 3;

/// Strip fenced code so a sample script isn't mistaken for an instruction.
fn prose(text: &str) -> String {
    FENCE.replace_all(text, "").into_owned()
}

/// Assert every STAGED name still resolves to a real skill. Kept SEPARATE from `check_orchestration`
/// because that function must stay usable against an arbitrary fixture directory — folding this in
/// made every fixture report three spurious missing-skill findings, which is how the test suite
/// caught it. This one is for the real library, and `main` runs it.
pub fn check_staged_skills_exist(skills_dir: &str) -> Vec<Finding> {
    let present: HashSet<String> = read_dir_sorted(Path::new(skills_dir))
        .unwrap_or_default()
        .into_iter()
        .collect();
    STAGED
        .iter()
        .filter(|name| !present.contains(**name))
        .map(|name| Finding {
            skill: name.to_string(),
            check: "staged-skill-missing".to_string(),
            message: format!("STAGED names \"{name}\" but no such skill directory exists — a rename retired this rule silently"),
        })
        .collect()
}

pub fn check_orchestration(skills_dir: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    for name in read_dir_sorted(Path::new(skills_dir)).unwrap_or_default() {
        let file = Path::new(skills_dir).join(&name).join("SKILL.md");
        if !file.exists() {
            continue;
        }

        // principles itself defines the laws; it is the reference, not a consumer of them.
        if name == "principles" {
            continue;
        }

        // An unreadable SKILL.md (the TypeScript threw) is skipped.
        let Some(text) = read_text(&file) else {
            continue;
        };
        let stripped = prose(&text);
        let lines: Vec<&str> = stripped.split('\n').collect();

        if lines
            .iter()
            .any(|l| FANOUT.is_match(l) && AGENT_LOOSE.is_match(l) && HEDGE.is_match(l))
        {
            findings.push(Finding {
                skill: name.clone(),
                check: "hedged-fanout".to_string(),
                message: "fan-out is phrased as a permission rather than a default — that phrasing is why it never happened".to_string(),
            });
        }

        if STAGED.contains(&name.as_str()) && !WORKFLOW.is_match(&stripped) {
            findings.push(Finding {
                skill: name.clone(),
                check: "unstaged-pipeline".to_string(),
                message: "staged work that never instructs a `Workflow` — its stages will be re-improvised each run (§3b)".to_string(),
            });
        }

        let unisolated = lines.iter().enumerate().any(|(i, l)| {
            if !(FANOUT.is_match(l) && AGENT.is_match(l) && WRITER.is_match(l)) {
                return false;
            }
            let from = i.saturating_sub(ISOLATION_WINDOW);
            let to = (i + ISOLATION_WINDOW + 1).min(lines.len());
            !ISOLATION.is_match(&lines[from..to].join("\n"))
        });
        if unisolated {
            findings.push(Finding {
                skill: name.clone(),
                check: "unisolated-parallel-write".to_string(),
                message: "parallelises a writing agent (`implementer` or `worker`) with no worktree isolation alongside the instruction".to_string(),
            });
        }
    }
    findings
}

pub fn main(args: &[String]) -> i32 {
    let skills = match args.first() {
        Some(dir) => resolve(&[dir]),
        None => match std::env::var("CLAUDE_PLUGIN_ROOT") {
            Ok(root) if !root.is_empty() => join(&[&root, "skills"]),
            _ => resolve(&["plugins/viby-toolkit/skills"]),
        },
    };
    // The TypeScript threw on an unreadable directory (exit 1).
    if read_dir_sorted(Path::new(&skills)).is_none() {
        eprintln!("error: cannot read the skills directory {skills}");
        return 1;
    }
    let mut findings = check_staged_skills_exist(&skills);
    findings.extend(check_orchestration(&skills));
    for f in &findings {
        println!("[P1 {}] {}", f.check, f.skill);
        println!("    {}", f.message);
    }
    if findings.is_empty() {
        println!(
            "✓ fan-out is stated as a default everywhere, and staged skills declare their stages"
        );
        return 0;
    }
    println!("\n{} finding(s).", findings.len());
    1
}
