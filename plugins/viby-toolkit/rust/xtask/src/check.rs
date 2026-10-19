//! `cargo xtask check`: everything that must pass before a commit, with one verdict.
//!
//! Exit 0 when every check passed (or was skipped with its reason shown), 1 when at least one
//! failed, with its output. It replaces `tests/run-all.ts`, `tests/check-references.ts` and
//! `tests/check-version.ts`.

use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Minimum passing tests per test binary, as `cargo test` names it after "Running". Exit 0 alone
/// is weak proof: a suite that lost its cases still exits 0. Raise a number when you add cases;
/// never lower one to make a run green. A test binary missing from this table fails the check, so
/// a new suite gets a minimum the day it lands.
const MIN_PASSING: &[(&str, usize)] = &[
    ("unittests src/lib.rs", 8),
    ("tests/agentgrep.rs", 8),
    ("tests/analytics_sql.rs", 25),
    ("tests/api_surface.rs", 20),
    ("tests/browser.rs", 49),
    ("tests/check_agents.rs", 26),
    ("tests/check_docs.rs", 14),
    ("tests/check_hooks.rs", 32),
    ("tests/check_memory.rs", 19),
    ("tests/check_orchestration.rs", 26),
    ("tests/check_plan.rs", 18),
    ("tests/check_skills.rs", 19),
    ("tests/check_study.rs", 37),
    ("tests/detect_stack.rs", 14),
    ("tests/diff_hygiene.rs", 31),
    ("tests/format_hook.rs", 9),
    ("tests/launcher.rs", 6),
    ("tests/logging.rs", 20),
    ("tests/mcp.rs", 2),
    ("tests/migration.rs", 19),
    ("tests/read_cost.rs", 20),
    ("tests/release.rs", 20),
    ("tests/scanner.rs", 67),
    ("tests/skill_safety.rs", 22),
    ("tests/statusline.rs", 19),
    ("tests/strip.rs", 3),
    ("tests/test_drift.rs", 23),
];

/// Test binaries with no tests of their own (the two `main.rs` files); they need no minimum.
const NO_TESTS: &[&str] = &["unittests src/main.rs"];

#[derive(PartialEq)]
enum Status {
    Pass,
    Fail,
    Skip,
}

struct Outcome {
    name: String,
    status: Status,
    note: String,
    output: String,
}

impl Outcome {
    fn pass(name: &str, note: impl Into<String>) -> Self {
        Self::new(name, Status::Pass, note.into(), String::new())
    }
    fn fail(name: &str, output: impl Into<String>) -> Self {
        Self::new(name, Status::Fail, String::new(), output.into())
    }
    fn new(name: &str, status: Status, note: String, output: String) -> Self {
        Outcome {
            name: name.to_string(),
            status,
            note,
            output,
        }
    }
}

/// Runs a command to completion and returns (succeeded with one of `ok`, its output). stdout and
/// stderr share one pipe so their lines stay in order: cargo prints "Running" on stderr and each
/// "test result" on stdout.
fn run(cmd: &mut Command, ok: &[i32]) -> (bool, String) {
    let spawned = std::io::pipe().and_then(|(mut reader, writer)| {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(writer.try_clone()?)
            .stderr(writer)
            .spawn()?;
        // The Command still holds the write ends; drop them, or the read never sees the end.
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut text)?;
        Ok((child.wait()?, text))
    });
    match spawned {
        Ok((status, text)) => (
            status.code().is_some_and(|c| ok.contains(&c)),
            String::from_utf8_lossy(&text).into_owned(),
        ),
        Err(e) => (false, format!("cannot run {:?}: {e}", cmd.get_program())),
    }
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|dir| dir.join(name).is_file()))
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn manifests(root: &Path) -> Outcome {
    let name = "plugin manifests";
    if !on_path("claude") {
        return Outcome::new(
            name,
            Status::Skip,
            "`claude` CLI not on PATH".into(),
            String::new(),
        );
    }
    match run(
        Command::new("claude")
            .args(["plugin", "validate", "."])
            .current_dir(root),
        &[0],
    ) {
        (true, _) => Outcome::pass(name, ""),
        (false, out) => Outcome::fail(name, out),
    }
}

/// One outcome per test binary, judged against `MIN_PASSING`.
fn tests(workspace: &Path) -> Vec<Outcome> {
    let (ok, out) = run(
        cargo()
            .args(["test", "--workspace", "--no-fail-fast", "--color", "never"])
            .current_dir(workspace),
        &[0],
    );
    let running = Regex::new(r"^\s+Running (.+?)(?: \(|$)").unwrap();
    let result = Regex::new(r"^test result: \w+\. (\d+) passed; (\d+) failed").unwrap();
    let mut seen: HashMap<String, (usize, usize)> = HashMap::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if let Some(m) = running.captures(line) {
            current = Some(m[1].to_string());
        } else if let Some(m) = result.captures(line) {
            if let Some(name) = current.take() {
                seen.insert(name, (m[1].parse().unwrap(), m[2].parse().unwrap()));
            }
        }
    }

    let mut outcomes = Vec::new();
    for (suite, min) in MIN_PASSING {
        let name = format!("cargo test: {suite}");
        outcomes.push(match seen.remove(*suite) {
            None => Outcome::fail(
                &name,
                format!("no result for {suite}: it did not build or was renamed"),
            ),
            Some((_, failed)) if failed > 0 => {
                Outcome::fail(&name, format!("{failed} test(s) failed"))
            }
            Some((passed, _)) if passed < *min => Outcome::fail(
                &name,
                format!(
                    "expected at least {min} passing tests, saw {passed}: the suite lost cases"
                ),
            ),
            Some((passed, _)) => Outcome::pass(&name, format!("{passed} passed")),
        });
    }
    for suite in NO_TESTS {
        seen.remove(*suite);
    }
    let mut unlisted: Vec<_> = seen.into_keys().collect();
    unlisted.sort();
    for suite in unlisted {
        outcomes.push(Outcome::fail(
            &format!("cargo test: {suite}"),
            "no minimum in MIN_PASSING (xtask/src/check.rs): add one",
        ));
    }
    // The log, once, after the per-suite verdicts: its tail is where a build error or a failed
    // assertion lands.
    if !ok || outcomes.iter().any(|o| o.status == Status::Fail) {
        outcomes.push(Outcome::fail("cargo test", out));
    }
    outcomes
}

fn clippy(workspace: &Path) -> Outcome {
    let name = "cargo clippy -D warnings";
    match run(
        cargo()
            .args([
                "clippy",
                "--workspace",
                "--all-targets",
                "--color",
                "never",
                "--",
                "-D",
                "warnings",
            ])
            .current_dir(workspace),
        &[0],
    ) {
        (true, _) => Outcome::pass(name, ""),
        (false, out) => Outcome::fail(name, out),
    }
}

/// The toolkit's own tests must survive its own auditor: 0 is clean, 2 is nothing to scan, and 1
/// (findings) fails, because the suite is meant to be clean.
fn self_audit(root: &Path, workspace: &Path) -> Outcome {
    let name = "self-audit (scanner on own tests)";
    match run(
        cargo()
            .args(["run", "--quiet", "--package", "viby", "--manifest-path"])
            .arg(workspace.join("Cargo.toml"))
            .args(["--", "scan-test-quality", "--all", "--quiet"])
            .current_dir(root),
        &[0, 2],
    ) {
        (true, _) => Outcome::pass(name, ""),
        (false, out) => Outcome::fail(name, out),
    }
}

fn parse_version(v: &str) -> [u64; 3] {
    let mut out = [0; 3];
    for (slot, part) in out.iter_mut().zip(v.trim_start_matches('v').split('.')) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        *slot = digits.parse().unwrap_or(0);
    }
    out
}

/// plugin.json is the one version: the Rust servers report the Cargo version and the browser bakes
/// plugin.json in, so the two must agree. The manifest must also not lag the newest tag: the
/// marketplace serves the manifest, so a tag ahead of it installs as the older number and
/// `claude plugin update` says "already at the latest version" while shipping older code.
fn version(root: &Path) -> Outcome {
    let name = "versions agree and are not behind the newest tag";
    let manifest_path = root.join("plugins/viby-toolkit/.claude-plugin/plugin.json");
    let manifest = match std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["version"].as_str().map(str::to_string))
    {
        Some(v) => v,
        None => {
            return Outcome::fail(
                name,
                format!("cannot read the version in {}", manifest_path.display()),
            )
        }
    };
    let cargo_version = env!("CARGO_PKG_VERSION");
    if manifest != cargo_version {
        return Outcome::fail(
            name,
            format!(
                "plugin.json is {manifest} but the Cargo workspace is {cargo_version}.\n\
                 Set [workspace.package] version in Cargo.toml to {manifest}."
            ),
        );
    }
    let tag = Regex::new(r"^v\d+\.\d+\.\d+$").unwrap();
    let newest = match Command::new("git")
        .args(["tag", "--sort=-v:refname"])
        .current_dir(root)
        .output()
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .find(|t| tag.is_match(t))
            .map(str::to_string),
        Err(_) => return Outcome::pass(name, format!("{manifest}; git unavailable")),
    };
    let Some(newest) = newest else {
        return Outcome::pass(name, format!("{manifest}; no version tags yet"));
    };
    if parse_version(&manifest) < parse_version(&newest) {
        let bare = newest.trim_start_matches('v');
        return Outcome::fail(
            name,
            format!(
                "plugin.json is {manifest}, but the newest tag is {newest}.\n\
                 The marketplace serves the MANIFEST, so {newest} would install as {manifest}, and\n\
                 `claude plugin update` would say \"already at the latest version\" while shipping older code.\n\
                 Bump plugin.json (and Cargo.toml) to {bare} or higher BEFORE tagging."
            ),
        );
    }
    Outcome::pass(name, format!("{manifest} >= {newest}"))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn frontmatter(text: &str) -> &str {
    if !text.starts_with("---") {
        return "";
    }
    match text[3..].find("\n---") {
        Some(end) => &text[3..3 + end],
        None => "",
    }
}

fn list_field(fm: &str, key: &str) -> Vec<String> {
    let re = Regex::new(&format!(r"(?m)^{key}:[ \t]*(.+)$")).unwrap();
    re.captures(fm)
        .map(|c| {
            c[1].split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn line_of(text: &str, index: usize) -> usize {
    text[..index].matches('\n').count() + 1
}

/// Names of the entries in `dir` with their `.md` suffix removed.
fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    n.strip_suffix(".md").map(str::to_string).unwrap_or(n)
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Every cross-reference in the plugin must resolve. Two shipped bugs were this class and were
/// invisible to every other check: `principles` set `disable-model-invocation: true` while nine
/// skills told Claude to follow it, and `test/SKILL.md` built a path from `CLAUDE_PLUGIN_ROOT`,
/// which is set for hooks only and empty in a skill body.
fn references(root: &Path) -> Outcome {
    let name = "cross-references resolve";
    let plugin = root.join("plugins/viby-toolkit");
    let skills_dir = plugin.join("skills");
    let skills: Vec<String> = names(&skills_dir)
        .into_iter()
        .filter(|s| skills_dir.join(s).join("SKILL.md").is_file())
        .collect();
    let commands = names(&plugin.join("commands"));
    let agents = names(&plugin.join("agents"));
    let read = |p: &Path| std::fs::read_to_string(p).unwrap_or_default();

    // What each agent's frontmatter grants. An allowlist (`tools`) grants what it names; a
    // denylist alone (`disallowedTools`, as on `worker`) inherits every tool except those.
    let grants: HashMap<&str, (Vec<String>, Vec<String>)> = agents
        .iter()
        .map(|a| {
            let text = read(&plugin.join("agents").join(format!("{a}.md")));
            let fm = frontmatter(&text);
            (
                a.as_str(),
                (list_field(fm, "tools"), list_field(fm, "disallowedTools")),
            )
        })
        .collect();
    let grants_tool = |agent: &str, tool: &str| {
        grants.get(agent).is_some_and(|(allowed, denied)| {
            let is_denied = denied.iter().any(|d| d == tool);
            if allowed.is_empty() && !denied.is_empty() {
                !is_denied
            } else {
                allowed.iter().any(|t| t == tool) && !is_denied
            }
        })
    };

    // Capabilities a skill body can instruct, and the tools an agent needs to do them. Short and
    // specific: a vague pattern would fire on prose that merely mentions the web.
    let capabilities = [
        (
            "web research fanned out to agents",
            Regex::new(r"(?i)\b(?:dispatch|fan out|spawn)[^.\n]{0,80}\b(?:researcher|search|web)\b|one\s+`?researcher`?\s+per").unwrap(),
            ["WebSearch", "WebFetch"],
        ),
        (
            "agents that write code",
            Regex::new(r"(?i)\bone\s+`implementer`\s+per\b").unwrap(),
            ["Edit", "Write"],
        ),
    ];

    // `disable-model-invocation: true` makes a skill user-only, so an instruction to load it is
    // unfollowable.
    let blocked_re = Regex::new(r"(?m)^disable-model-invocation:\s*true").unwrap();
    let blocked: Vec<&String> = skills
        .iter()
        .filter(|s| blocked_re.is_match(frontmatter(&read(&skills_dir.join(s).join("SKILL.md")))))
        .collect();

    let mut files = Vec::new();
    walk(&plugin, &mut files);
    files.push(root.join("README.md"));
    files.retain(|f| f.extension().is_some_and(|e| e == "md"));

    let skill_ref = Regex::new(r"/viby-toolkit:([a-z-]+)").unwrap();
    let root_var = Regex::new(r"\$\{?CLAUDE_PLUGIN_ROOT\}?").unwrap();
    let agent_ref =
        Regex::new(r"`(scout|reviewer|skeptic|implementer|debugger|researcher|worker)`").unwrap();
    let script_ref = Regex::new(r"[\w/.-]*/[\w-]+\.ts\b").unwrap();
    let viby_ref = Regex::new(r#"`viby ([a-z][a-z-]*)|"\$VIBY" ([a-z][a-z-]*)"#).unwrap();

    let mut problems: Vec<String> = Vec::new();
    for file in &files {
        let text = read(file);
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        let mut problem = |index: usize, message: String| {
            problems.push(format!("✗ {rel}:{}  {message}", line_of(&text, index)));
        };

        // 1. /viby-toolkit:<name> must exist and be model-invocable.
        for c in skill_ref.captures_iter(&text) {
            let skill = &c[1];
            let at = c.get(0).unwrap().start();
            if !skills.iter().any(|s| s == skill) && !commands.iter().any(|s| s == skill) {
                problem(at, format!("/viby-toolkit:{skill} does not exist"));
            } else if blocked.iter().any(|s| *s == skill) {
                problem(
                    at,
                    format!("/viby-toolkit:{skill} is model-blocked (disable-model-invocation) — Claude cannot follow this reference"),
                );
            }
        }

        // 2. CLAUDE_PLUGIN_ROOT is hook-only; in a skill body it expands to nothing. Only an
        //    actual expansion counts: prose saying the variable is unavailable is correct.
        if rel.contains("/skills/") {
            for m in root_var.find_iter(&text) {
                problem(
                    m.start(),
                    "CLAUDE_PLUGIN_ROOT is set for hooks only — it is empty in a skill body, so this path breaks".into(),
                );
            }
        }

        // 3. Referenced agents must exist.
        for c in agent_ref.captures_iter(&text) {
            if !agents.iter().any(|a| a == &c[1]) {
                problem(
                    c.get(0).unwrap().start(),
                    format!("agent `{}` does not exist", &c[1]),
                );
            }
        }

        // 3b. A skill that instructs a capability must name at least one agent that has it:
        //     `study` once told the reader to fan out web searches while every agent was
        //     filesystem-only, so the fan-out had nowhere to go.
        if rel.contains("/skills/") && rel.ends_with("SKILL.md") {
            let named: Vec<&String> = agents
                .iter()
                .filter(|a| text.contains(&format!("`{a}`")))
                .collect();
            for (cap, implied_by, tools) in &capabilities {
                let Some(m) = implied_by.find(&text) else {
                    continue;
                };
                if !named
                    .iter()
                    .any(|a| tools.iter().any(|t| grants_tool(a, t)))
                {
                    let named_list = if named.is_empty() {
                        "none".to_string()
                    } else {
                        named
                            .iter()
                            .map(|a| a.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    problem(
                        m.start(),
                        format!(
                            "instructs {cap} but names no agent that can do it (named: {named_list} — none grants {})",
                            tools.join("/")
                        ),
                    );
                }
            }
        }

        // 4. Referenced .ts scripts in skills/, hooks/ or tests/ must exist. A shell expansion
        //    (`"$DIR/skills/.../script.ts"`) is a runtime path, preceded by `$`.
        for m in script_ref.find_iter(&text) {
            let whole = m.as_str();
            if !["skills/", "hooks/", "tests/"]
                .iter()
                .any(|d| whole.contains(d))
            {
                continue;
            }
            if m.start() > 0 && text.as_bytes()[m.start() - 1] == b'$' {
                continue;
            }
            // Node's path.join keeps the base when the part starts with `/`; Path::join does not.
            let part = whole.trim_start_matches('/');
            let candidate = if part.starts_with("plugins/") {
                root.join(part)
            } else {
                plugin.join(part)
            };
            if !candidate.exists() && !root.join(part).exists() {
                problem(
                    m.start(),
                    format!("referenced script {whole} does not exist"),
                );
            }
        }
    }

    // 4b. `viby <command>` in prose and `sh "$VIBY" <command>` in a skill's block must name a
    //     subcommand the binary has.
    for file in &files {
        let text = read(file);
        let rel = file.strip_prefix(root).unwrap_or(file).to_string_lossy();
        for c in viby_ref.captures_iter(&text) {
            let command = c.get(1).or(c.get(2)).unwrap();
            if !viby::COMMANDS.iter().any(|(n, _)| *n == command.as_str()) {
                problems.push(format!(
                    "✗ {rel}:{}  viby has no subcommand {}",
                    line_of(&text, command.start()),
                    command.as_str()
                ));
            }
        }
    }

    // 5. Hook commands in hooks.json must point at files that exist.
    let hooks_json = read(&plugin.join("hooks/hooks.json"));
    let hook_target = Regex::new(r"\$\{CLAUDE_PLUGIN_ROOT\}/([\w/.-]+)").unwrap();
    for c in hook_target.captures_iter(&hooks_json) {
        if !plugin.join(&c[1]).exists() {
            problems.push(format!(
                "✗ plugins/viby-toolkit/hooks/hooks.json:1  hook target {} does not exist",
                &c[1]
            ));
        }
    }

    let summary = format!("{} markdown files", files.len());
    if problems.is_empty() {
        Outcome::pass(name, summary)
    } else {
        Outcome::fail(
            name,
            format!(
                "{}\n\n{} broken reference(s) in {summary}",
                problems.join("\n"),
                problems.len()
            ),
        )
    }
}

/// The `.ts` entry points of 2.x, which settings written then still pass to `hooks/run.sh`, and
/// the subcommand each must reach.
const OLD_ENTRY_POINTS: &[(&str, &str)] = &[
    ("agentgrep/server.ts", "agentgrep-server"),
    ("browser/host.ts", "browser-host"),
    ("browser/server.ts", "browser-server"),
    ("hooks/post-tool-use-format.ts", "format-hook"),
    ("hooks/statusline.ts", "statusline"),
    (
        "skills/adopt/scripts/check-test-drift.ts",
        "check-test-drift",
    ),
    (
        "skills/analytics/scripts/check-analytics-sql.ts",
        "check-analytics-sql",
    ),
    (
        "skills/api/scripts/check-api-surface.ts",
        "check-api-surface",
    ),
    ("skills/brain/scripts/check-memory.ts", "check-memory"),
    ("skills/docs/scripts/check-docs.ts", "check-docs"),
    (
        "skills/evaluate/scripts/measure-read-cost.ts",
        "measure-read-cost",
    ),
    ("skills/extend/scripts/check-hooks.ts", "check-hooks"),
    ("skills/observe/scripts/check-logging.ts", "check-logging"),
    (
        "skills/orchestrate/scripts/check-diff-hygiene.ts",
        "check-diff-hygiene",
    ),
    ("skills/plan/scripts/check-plan.ts", "check-plan"),
    ("skills/principles/scripts/check-agents.ts", "check-agents"),
    (
        "skills/principles/scripts/check-orchestration.ts",
        "check-orchestration",
    ),
    ("skills/principles/scripts/check-skills.ts", "check-skills"),
    ("skills/release/scripts/check-release.ts", "check-release"),
    (
        "skills/schema/scripts/check-migration.ts",
        "check-migration",
    ),
    (
        "skills/secure/scripts/check-skill-safety.ts",
        "check-skill-safety",
    ),
    ("skills/study/scripts/check-study.ts", "check-study"),
    (
        "skills/test/scripts/scan-test-quality.ts",
        "scan-test-quality",
    ),
    ("skills/verify/scripts/detect-stack.ts", "detect-stack"),
];

/// `hooks/run.sh <old .ts path> args` must reach `viby <subcommand> args` through the launcher.
/// A fake binary in a fake cache prints what it was given, so nothing is downloaded.
fn run_sh(root: &Path) -> Outcome {
    let name = "run.sh maps 2.x .ts paths to subcommands";
    let plugin = root.join("plugins/viby-toolkit");
    let version = serde_json::from_str::<serde_json::Value>(
        &std::fs::read_to_string(plugin.join(".claude-plugin/plugin.json")).unwrap_or_default(),
    )
    .ok()
    .and_then(|v| v["version"].as_str().map(String::from))
    .unwrap_or_default();
    let data = std::env::temp_dir().join(format!("viby-xtask-run-sh-{}", std::process::id()));
    let bin = data.join("bin").join(&version);
    let fake = bin.join(if cfg!(windows) { "viby.exe" } else { "viby" });
    let made = std::fs::create_dir_all(&bin)
        .and_then(|()| std::fs::write(&fake, "#!/bin/sh\necho \"$*\"\n"))
        .and_then(|()| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755))?;
            }
            Ok(())
        });
    if let Err(e) = made {
        return Outcome::fail(
            name,
            format!("cannot make a fake cache in {}: {e}", data.display()),
        );
    }
    let mut problems = Vec::new();
    for (path, command) in OLD_ENTRY_POINTS {
        if !viby::COMMANDS.iter().any(|(n, _)| n == command) {
            problems.push(format!(
                "{path} maps to {command}, which viby does not have"
            ));
            continue;
        }
        let (_, out) = run(
            Command::new("sh")
                .arg(plugin.join("hooks/run.sh"))
                .arg(plugin.join(path))
                .arg("--json")
                .env("CLAUDE_PLUGIN_DATA", &data),
            &[0],
        );
        let want = format!("{command} --json");
        if out.trim() != want {
            problems.push(format!(
                "run.sh {path} --json ran `viby {}`, not `viby {want}`",
                out.trim()
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&data);
    if problems.is_empty() {
        Outcome::pass(name, format!("{} paths", OLD_ENTRY_POINTS.len()))
    } else {
        Outcome::fail(name, problems.join("\n"))
    }
}

fn session_start(root: &Path) -> Outcome {
    let name = "SessionStart emits valid JSON";
    let (ok, out) = run(
        Command::new("sh")
            .arg("plugins/viby-toolkit/hooks/session-start.sh")
            .current_dir(root),
        &[0],
    );
    if !ok {
        return Outcome::fail(name, out);
    }
    match serde_json::from_str::<serde_json::Value>(&out) {
        Ok(_) => Outcome::pass(name, ""),
        Err(e) => Outcome::fail(name, format!("{e}\n\n{out}")),
    }
}

pub fn main(root: &Path) -> i32 {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask sits inside the workspace");

    let mut results = vec![manifests(root)];
    results.extend(tests(workspace));
    results.push(clippy(workspace));
    results.push(self_audit(root, workspace));
    results.push(version(root));
    results.push(references(root));
    results.push(run_sh(root));
    results.push(session_start(root));

    for r in &results {
        let status = match r.status {
            Status::Pass => "pass",
            Status::Fail => "FAIL",
            Status::Skip => "skip",
        };
        let note = if r.note.is_empty() {
            String::new()
        } else {
            format!("  ({})", r.note)
        };
        println!("{status:>4}  {}{note}", r.name);
    }

    println!("\n{}", "─".repeat(74));
    let failed: Vec<&Outcome> = results
        .iter()
        .filter(|r| r.status == Status::Fail)
        .collect();
    if !failed.is_empty() {
        println!("✗ {} check(s) failed\n", failed.len());
        for r in failed {
            println!(
                "── {} {}",
                r.name,
                "─".repeat(50usize.saturating_sub(r.name.len()))
            );
            let out = r.output.trim_end();
            let start = out.len().saturating_sub(2000);
            let start = (start..=out.len())
                .find(|&i| out.is_char_boundary(i))
                .unwrap_or(0);
            println!(
                "{}\n",
                if out.is_empty() {
                    "(no output)"
                } else {
                    &out[start..]
                }
            );
        }
        return 1;
    }
    let passed = results.iter().filter(|r| r.status == Status::Pass).count();
    let skipped = results.len() - passed;
    let tail = if skipped > 0 {
        format!(" ({skipped} skipped)")
    } else {
        String::new()
    };
    println!("✓ all {passed} check(s) passed{tail} — safe to commit and push");
    0
}
