//! `cargo xtask parity`: every checker, TypeScript and Rust, on the same real inputs.
//!
//! The TypeScript is the oracle until it is deleted (plan T18). For each invocation both are run
//! in the same folder; the exit codes must match, plain output byte for byte, and `--json` output
//! as parsed JSON. Inputs are this repo and its sibling repos, read-only.
//!
//! `cargo xtask parity [filter]` runs the invocations whose command contains `filter`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// (subcommand, TS script under plugins/viby-toolkit, folder relative to the ecosystem dir, args)
type Case = (
    &'static str,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const CASES: &[Case] = &[
    (
        "scan-test-quality",
        "skills/test/scripts/scan-test-quality.ts",
        "viby-toolkit",
        &["--all"],
    ),
    (
        "scan-test-quality",
        "skills/test/scripts/scan-test-quality.ts",
        "viby-toolkit",
        &["--all", "--json"],
    ),
    (
        "scan-test-quality",
        "skills/test/scripts/scan-test-quality.ts",
        "viby",
        &["--all", "--json"],
    ),
    (
        "scan-test-quality",
        "skills/test/scripts/scan-test-quality.ts",
        "viby-flow",
        &["--all"],
    ),
    (
        "detect-stack",
        "skills/verify/scripts/detect-stack.ts",
        "viby-toolkit",
        &["."],
    ),
    (
        "detect-stack",
        "skills/verify/scripts/detect-stack.ts",
        "viby",
        &[".", "--json"],
    ),
    (
        "detect-stack",
        "skills/verify/scripts/detect-stack.ts",
        "viby-flow",
        &[".", "--json"],
    ),
    (
        "detect-stack",
        "skills/verify/scripts/detect-stack.ts",
        "viby-shot",
        &["."],
    ),
    (
        "measure-read-cost",
        "skills/evaluate/scripts/measure-read-cost.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit/skills/test", "--top", "5"],
    ),
    (
        "measure-read-cost",
        "skills/evaluate/scripts/measure-read-cost.ts",
        "viby",
        &["bot/src", "--json"],
    ),
    (
        "check-docs",
        "skills/docs/scripts/check-docs.ts",
        "viby-toolkit",
        &["README.md"],
    ),
    (
        "check-docs",
        "skills/docs/scripts/check-docs.ts",
        "viby-toolkit",
        &["README.md", "--json"],
    ),
    (
        "check-docs",
        "skills/docs/scripts/check-docs.ts",
        "viby-shot",
        &["README.md", "--json"],
    ),
    (
        "check-skills",
        "skills/principles/scripts/check-skills.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit/skills"],
    ),
    (
        "check-skills",
        "skills/principles/scripts/check-skills.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit/skills", "--json"],
    ),
    (
        "check-agents",
        "skills/principles/scripts/check-agents.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit/agents"],
    ),
    (
        "check-agents",
        "skills/principles/scripts/check-agents.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit/agents", "--json"],
    ),
    (
        "check-orchestration",
        "skills/principles/scripts/check-orchestration.ts",
        "viby-toolkit",
        &[],
    ),
    (
        "check-hooks",
        "skills/extend/scripts/check-hooks.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit", "README.md"],
    ),
    (
        "check-hooks",
        "skills/extend/scripts/check-hooks.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit", "README.md", "--json"],
    ),
    (
        "check-test-drift",
        "skills/adopt/scripts/check-test-drift.ts",
        "viby-toolkit",
        &["--base", "HEAD~8", "."],
    ),
    (
        "check-test-drift",
        "skills/adopt/scripts/check-test-drift.ts",
        "viby-flow",
        &["--base", "HEAD~20", ".", "--json"],
    ),
    (
        "check-api-surface",
        "skills/api/scripts/check-api-surface.ts",
        "viby-toolkit",
        &["--base", "HEAD~8"],
    ),
    (
        "check-api-surface",
        "skills/api/scripts/check-api-surface.ts",
        "viby-flow",
        &["--base", "HEAD~20", "--json"],
    ),
    (
        "check-diff-hygiene",
        "skills/orchestrate/scripts/check-diff-hygiene.ts",
        "viby-toolkit",
        &["--base", "HEAD~3"],
    ),
    (
        "check-diff-hygiene",
        "skills/orchestrate/scripts/check-diff-hygiene.ts",
        "viby-flow",
        &["--base", "HEAD~10", "--json"],
    ),
    (
        "check-release",
        "skills/release/scripts/check-release.ts",
        "viby-toolkit",
        &["."],
    ),
    (
        "check-release",
        "skills/release/scripts/check-release.ts",
        "viby-flow",
        &[".", "--json"],
    ),
    (
        "check-analytics-sql",
        "skills/analytics/scripts/check-analytics-sql.ts",
        "viby",
        &["web/migrations"],
    ),
    (
        "check-analytics-sql",
        "skills/analytics/scripts/check-analytics-sql.ts",
        "viby",
        &["web/migrations", "--json"],
    ),
    (
        "check-migration",
        "skills/schema/scripts/check-migration.ts",
        "viby",
        &["web/migrations"],
    ),
    (
        "check-migration",
        "skills/schema/scripts/check-migration.ts",
        "viby",
        &["web/migrations", "--json"],
    ),
    (
        "check-migration",
        "skills/schema/scripts/check-migration.ts",
        "viby-shot",
        &["server", "--json"],
    ),
    (
        "check-logging",
        "skills/observe/scripts/check-logging.ts",
        "viby",
        &["bot/src"],
    ),
    (
        "check-logging",
        "skills/observe/scripts/check-logging.ts",
        "viby-flow",
        &[".", "--json"],
    ),
    (
        "check-skill-safety",
        "skills/secure/scripts/check-skill-safety.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit"],
    ),
    (
        "check-skill-safety",
        "skills/secure/scripts/check-skill-safety.ts",
        "viby-toolkit",
        &["plugins/viby-toolkit", "--json"],
    ),
    (
        "check-plan",
        "skills/plan/scripts/check-plan.ts",
        "viby-toolkit",
        &["docs/plans/2026-10-19-rust-port.md"],
    ),
    (
        "check-plan",
        "skills/plan/scripts/check-plan.ts",
        "viby-toolkit",
        &[
            "docs/plans/2026-10-19-rust-port.md",
            "docs/plans/2026-10-19-browser-parity-and-agentgrep.md",
            "--json",
        ],
    ),
    (
        "check-study",
        "skills/study/scripts/check-study.ts",
        "viby-toolkit",
        &["docs/research/2026-10-19-agentgrep-measurement.md"],
    ),
    (
        "check-study",
        "skills/study/scripts/check-study.ts",
        "viby-toolkit",
        &[
            "docs/studies/2026-08-27-workflow-orchestration-default.md",
            "--json",
        ],
    ),
];

pub struct Outcome {
    pub code: Option<i32>,
    pub stdout: String,
}

/// Runs `cmd` with stdout going to a file: the TypeScript calls process.exit before a pipe has
/// drained, which cut piped output at 64 KiB (seen on check-api-surface in viby-flow).
fn run(cmd: &mut Command) -> Outcome {
    let path = std::env::temp_dir().join(format!("viby-parity-{}.out", std::process::id()));
    let file = std::fs::File::create(&path).expect("temp file");
    let status = cmd
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(Stdio::null())
        .status()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default()).into_owned();
    let _ = std::fs::remove_file(&path);
    Outcome {
        code: status.code(),
        stdout,
    }
}

/// Where the outputs first differ, or None when they agree.
fn difference(json: bool, ts: &Outcome, rs: &Outcome) -> Option<String> {
    if ts.code != rs.code {
        return Some(format!("exit code: ts {:?}, rust {:?}", ts.code, rs.code));
    }
    if json {
        let parse = |s: &str| serde_json::from_str::<Value>(s).ok();
        match (parse(&ts.stdout), parse(&rs.stdout)) {
            (Some(a), Some(b)) if a == b => return None,
            (Some(_), Some(_)) => {}
            (None, _) => return Some("ts output is not JSON".into()),
            (_, None) => return Some("rust output is not JSON".into()),
        }
    } else if ts.stdout == rs.stdout {
        return None;
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (ts.stdout.lines().collect(), rs.stdout.lines().collect());
    let at = a
        .iter()
        .zip(&b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    Some(format!(
        "first difference at line {} ({} vs {} lines)\n      ts:   {:?}\n      rust: {:?}",
        at + 1,
        a.len(),
        b.len(),
        a.get(at).unwrap_or(&"<end>"),
        b.get(at).unwrap_or(&"<end>")
    ))
}

pub fn main(root: &Path, filter: Option<&str>) -> i32 {
    let plugin = root.join("plugins/viby-toolkit");
    let ecosystem = root.parent().expect("the repo has a parent").to_path_buf();
    let viby = root.join("target/debug/viby");
    if !Command::new("cargo")
        .args(["build", "-q", "-p", "viby"])
        .current_dir(root)
        .status()
        .is_ok_and(|s| s.success())
    {
        eprintln!("parity: cargo build failed");
        return 1;
    }
    let run_sh = plugin.join("hooks/run.sh");
    let (mut same, mut differ, mut skipped) = (0, 0, 0);
    for (cmd, script, folder, args) in CASES {
        if filter.is_some_and(|f| !cmd.contains(f)) {
            continue;
        }
        let dir: PathBuf = if *folder == "viby-toolkit" {
            root.to_path_buf()
        } else {
            ecosystem.join(folder)
        };
        let label = format!("{cmd} {} (in {folder})", args.join(" "));
        if !dir.is_dir() {
            println!("SKIP {label}: no {}", dir.display());
            skipped += 1;
            continue;
        }
        let ts = run(Command::new("sh")
            .arg(&run_sh)
            .arg(plugin.join(script))
            .args(*args)
            .current_dir(&dir));
        let rs = run(Command::new(&viby).arg(cmd).args(*args).current_dir(&dir));
        match difference(args.contains(&"--json"), &ts, &rs) {
            None => {
                println!("SAME {label}");
                same += 1;
            }
            Some(why) => {
                println!("DIFF {label}\n      {why}");
                differ += 1;
            }
        }
    }
    println!("\nparity: {same} same, {differ} different, {skipped} skipped");
    i32::from(differ > 0)
}
