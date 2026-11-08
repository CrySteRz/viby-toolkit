//! viby-toolkit Stop gate (OPT-IN: not registered by default, and unmeasured).
//!
//! When Claude is about to finish a turn in which it edited code and ran no check after the last
//! edit, block the stop once and ask for the check. Conservative by design:
//!   - Blocks at most once per stop: `stop_hook_active` true means it already blocked.
//!   - Only this turn counts: edits before the last prompt from the person were judged when
//!     that turn ended.
//!   - Documentation edits (.md, .mdx, .txt, .rst, anything under docs/) never count.
//!   - Fails open: unreadable input, a missing transcript or a malformed line prints nothing.
//!
//! To enable, add a `Stop` hook running `viby stop-gate` to your settings.json (the README has
//! the snippet). It also works as a `SubagentStop` hook, reading `agent_transcript_path`.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path};

use serde_json::{json, Value};

const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];
const DOC_EXTS: &[&str] = &["md", "mdx", "txt", "rst"];

/// Tools that are a check whatever their arguments.
const CHECK_TOOLS: &[&str] = &[
    "pytest",
    "py.test",
    "jest",
    "vitest",
    "tsc",
    "ruff",
    "mypy",
    "eslint",
    "make",
    "gmake",
    "gradle",
    "gradlew",
    "mvn",
    "mvnw",
    "ctest",
    "phpunit",
    "rspec",
    "xcodebuild",
];

/// Words that only start another command: `time cargo test`, `npx jest`.
const PREFIXES: &[&str] = &[
    "time", "sudo", "nice", "nohup", "env", "command", "exec", "npx", "bunx", "xcrun",
];

/// Two-word prefixes: `uv run pytest`, `bundle exec rspec`.
const PREFIX_PAIRS: &[(&str, &str)] = &[
    ("uv", "run"),
    ("poetry", "run"),
    ("pipenv", "run"),
    ("pdm", "run"),
    ("hatch", "run"),
    ("rye", "run"),
    ("bundle", "exec"),
    ("pnpm", "exec"),
    ("pnpm", "dlx"),
    ("yarn", "dlx"),
    ("bun", "x"),
];

fn basename(word: &str) -> &str {
    word.rsplit(['/', '\\']).next().unwrap_or(word)
}

/// Whether one simple command (no `&&`, `|` or `;`) runs tests, a build, a type-check or a
/// linter.
fn is_check_command(segment: &str) -> bool {
    let words: Vec<&str> = segment
        .split_whitespace()
        .map(|w| w.trim_matches(|c| matches!(c, '(' | ')' | '{' | '}' | '"' | '\'')))
        .filter(|w| !w.is_empty())
        .collect();
    let mut w = words.as_slice();
    loop {
        match w {
            [first, rest @ ..]
                if PREFIXES.contains(first)
                    || (first.contains('=') && !first.starts_with('-') && !first.contains('/')) =>
            {
                w = rest;
            }
            ["timeout", _, rest @ ..] => w = rest,
            [a, b, rest @ ..] if PREFIX_PAIRS.contains(&(a, b)) => w = rest,
            _ => break,
        }
    }
    let Some((&first, rest)) = w.split_first() else {
        return false;
    };
    let tool = basename(first);
    let arg = |i: usize| rest.get(i).copied().unwrap_or("");
    match tool {
        "cargo" => {
            let sub = if arg(0).starts_with('+') {
                arg(1)
            } else {
                arg(0)
            };
            matches!(
                sub,
                "test" | "check" | "clippy" | "build" | "xtask" | "nextest"
            )
        }
        "go" => matches!(arg(0), "test" | "vet" | "build"),
        "npm" | "pnpm" | "yarn" | "bun" => {
            let sub = if arg(0) == "run" { arg(1) } else { arg(0) };
            // `test:unit` and `lint:ci` are scripts of the same kind.
            let script = sub.split(':').next().unwrap_or("");
            matches!(
                script,
                "test" | "check" | "lint" | "build" | "typecheck" | "type-check"
            ) || CHECK_TOOLS.contains(&sub)
        }
        "dotnet" | "swift" => matches!(arg(0), "test" | "build"),
        "mix" => arg(0) == "test",
        _ if tool.starts_with("python") => {
            arg(0) == "-m" && matches!(arg(1), "pytest" | "unittest" | "mypy" | "ruff")
        }
        _ => CHECK_TOOLS.contains(&tool),
    }
}

fn is_check(command: &str) -> bool {
    command.split(['\n', ';', '&', '|']).any(is_check_command)
}

fn is_doc(path: &str) -> bool {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    DOC_EXTS.contains(&ext.as_str())
        || p.components()
            .any(|c| matches!(c, Component::Normal(n) if n == "docs"))
}

/// The files edited after the last check in this turn, in order and without repeats. `None`
/// when the transcript cannot be read or holds a malformed line.
fn edits_after_check(transcript: &Path, skip_sidechain: bool) -> Option<Vec<String>> {
    let reader = BufReader::new(std::fs::File::open(transcript).ok()?);
    let mut pending: Vec<String> = Vec::new();
    for line in reader.lines() {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        // A line that does not parse (most often the one still being written) is skipped: judge
        // the session on what can be read, rather than letting one bad line switch the gate off.
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if skip_sidechain && entry.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        let content = entry.pointer("/message/content");
        match (kind, content) {
            // A prompt from the person (or any text that is not a tool result) starts a turn.
            ("user", Some(Value::String(_))) => pending.clear(),
            ("user", Some(Value::Array(blocks)))
                if blocks.iter().any(|b| b.get("type") == Some(&json!("text"))) =>
            {
                pending.clear()
            }
            ("assistant", Some(Value::Array(blocks))) => {
                for block in blocks {
                    if block.get("type") != Some(&json!("tool_use")) {
                        continue;
                    }
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("");
                    let input = block.get("input").unwrap_or(&Value::Null);
                    if name == "Bash" {
                        let command = input.get("command").and_then(Value::as_str).unwrap_or("");
                        if is_check(command) {
                            pending.clear();
                        }
                    } else if EDIT_TOOLS.contains(&name) {
                        let path = ["file_path", "notebook_path", "path"]
                            .iter()
                            .find_map(|k| input.get(*k).and_then(Value::as_str))
                            .unwrap_or("");
                        if !path.is_empty() && !is_doc(path) && !pending.iter().any(|p| p == path) {
                            pending.push(path.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Some(pending)
}

/// The block decision for this hook input, or `None` to let Claude stop.
fn decide(input: &str) -> Option<Value> {
    let d: Value = serde_json::from_str(input).ok()?;
    if d.get("stop_hook_active").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let subagent = d.get("hook_event_name").and_then(Value::as_str) == Some("SubagentStop");
    // A subagent is judged on its own transcript; without one there is nothing to judge.
    let key = if subagent {
        "agent_transcript_path"
    } else {
        "transcript_path"
    };
    let transcript = d
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    let edited = edits_after_check(Path::new(transcript), !subagent)?;
    if edited.is_empty() {
        return None;
    }
    let cwd = d.get("cwd").and_then(Value::as_str).unwrap_or("");
    let shown: Vec<String> = edited
        .iter()
        .take(3)
        .map(|p| {
            let rel = Path::new(p)
                .strip_prefix(cwd)
                .ok()
                .filter(|_| !cwd.is_empty());
            rel.map_or_else(|| p.clone(), |r| r.display().to_string())
        })
        .collect();
    let mut files = shown.join(", ");
    if edited.len() > 3 {
        files.push_str(&format!(" and {} more", edited.len() - 3));
    }
    Some(json!({
        "decision": "block",
        "reason": format!(
            "You changed {files} after the last check ran. Run the project's checks (or the \
             narrowest test that covers this) and report the result, or say plainly why no \
             check applies."
        ),
    }))
}

pub fn main(_args: &[String]) -> i32 {
    let mut input = Vec::new();
    if std::io::stdin().lock().read_to_end(&mut input).is_err() {
        return 0;
    }
    if let Some(block) = decide(&String::from_utf8_lossy(&input)) {
        // A closed pipe must not change the exit code: the gate never errors.
        let _ = writeln!(std::io::stdout().lock(), "{block}");
    }
    0
}
