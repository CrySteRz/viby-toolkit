//! viby-toolkit hook-wiring auditor — the one category of shipped artifact with no checker.
//!
//! Every other executable surface (skills, statusline, test scanner, migration linter...) has
//! a contract test. hooks/hooks.json did not: it is hand-edited JSON with three ways to fail
//! silently — a typo'd event name that Claude Code never recognises, a relative path that
//! resolves against the user's cwd instead of the plugin, and a .ts entry invoked with bare
//! `node` instead of through hooks/run.sh (which is the thing that degrades to a no-op on a
//! machine with no TypeScript runtime; bare `node` just crashes there). None of these throw at
//! parse time — they throw, or silently do nothing, the first time a real session hits them.
//!
//! This also closes the opposite failure: `post-tool-use-format.ts` and `statusline.ts` are
//! shipped deliberately UNREGISTERED (opt-in, wired by the user in their own settings.json).
//! A checker that flags every unregistered script as "orphaned" would be wrong about the
//! artifacts that are working exactly as designed. The only defensible rule here is the
//! inverse one: a hook script that is neither registered in hooks.json NOR documented as
//! opt-in in README.md is unreachable by any path — that is the actual defect to catch.
//!
//! Usage:
//!   viby check-hooks [plugin-root] [readme-path] [--json] [--quiet]
//! Exit: 1 if any P1 finding, 0 otherwise (P2/P3 are advisory, not blocking).
//!
//! Port of `check-hooks.ts`.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{json, Map, Value};

use crate::core::args::{flag, parse_or_exit};
use crate::core::js::{js_trim, read_text, slice_utf16, JS_SPACE};
use crate::core::nodepath::{basename, join, read_dir_sorted, resolve};

#[derive(Clone, Debug)]
pub struct Finding {
    pub check: String,
    pub severity: &'static str,
    pub location: String,
    pub message: String,
    pub detail: Option<String>,
}

impl Finding {
    fn new(check: &str, location: &str, message: String) -> Finding {
        Finding {
            check: check.to_string(),
            severity: "P1",
            location: location.to_string(),
            message,
            detail: None,
        }
    }

    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        map.insert("check".into(), json!(self.check));
        map.insert("severity".into(), json!(self.severity));
        map.insert("location".into(), json!(self.location));
        map.insert("message".into(), json!(self.message));
        if let Some(d) = &self.detail {
            map.insert("detail".into(), json!(d));
        }
        Value::Object(map)
    }
}

/// The parsed `hooks.json`: `{ hooks?: { [event]: [ { matcher?, hooks: [ { type, command } ] } ] } }`.
pub type HooksConfig = Value;

/// The event names Claude Code's hook system recognises. Hardcoded rather than discovered,
/// because there is nowhere in this repo that enumerates them — if Claude Code adds an event,
/// this list needs a manual update, and a name here that Claude Code no longer accepts would
/// start producing false P1s. That tradeoff is deliberate: a typo silently never firing is a
/// worse failure mode than an occasional false alarm on a genuinely new event name.
const KNOWN_EVENTS: [&str; 9] = [
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "UserPromptSubmit",
    "Stop",
    "SubagentStop",
    "PreCompact",
    "SessionStart",
    "SessionEnd",
];

/// A representative stdin event per hook type, for the dry-run pass.
fn event_stdin(event: &str) -> Value {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = |extra: Vec<(&str, Value)>| {
        let mut m = Map::new();
        m.insert("hook_event_name".into(), json!(event));
        m.insert("session_id".into(), json!("check-hooks"));
        m.insert("cwd".into(), json!(cwd));
        for (k, v) in extra {
            m.insert(k.into(), v);
        }
        Value::Object(m)
    };
    match event {
        "SessionStart" => base(vec![("matcher", json!("startup"))]),
        "PostToolUse" => base(vec![
            ("tool_name", json!("Write")),
            (
                "tool_input",
                json!({ "file_path": "/tmp/check-hooks-sentinel.ts" }),
            ),
            ("tool_response", json!({})),
        ]),
        "PreToolUse" => base(vec![
            ("tool_name", json!("Bash")),
            ("tool_input", json!({ "command": "echo hi" })),
        ]),
        "UserPromptSubmit" => base(vec![("prompt", json!("hello"))]),
        "Stop" | "SubagentStop" => base(vec![("stop_hook_active", json!(false))]),
        "PreCompact" => base(vec![("trigger", json!("manual"))]),
        "SessionEnd" => base(vec![("reason", json!("exit"))]),
        "Notification" => base(vec![("message", json!("test"))]),
        _ => base(vec![]),
    }
}

pub fn parse_hooks_json(text: &str) -> (Option<HooksConfig>, Vec<Finding>) {
    let parsed: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return (
                None,
                vec![Finding::new(
                    "invalid-json",
                    "hooks.json",
                    format!("does not parse as JSON: {e}"),
                )],
            )
        }
    };
    if !parsed.is_object() {
        return (
            None,
            vec![Finding::new(
                "invalid-shape",
                "hooks.json",
                "top-level value is not an object".to_string(),
            )],
        );
    }
    (Some(parsed), Vec::new())
}

/// JavaScript `typeof x === "object" && x !== null`: arrays count.
fn is_object_like(v: &Value) -> bool {
    v.is_object() || v.is_array()
}

/// `JSON.stringify(v)` for the value of a missing-or-wrong `type`; a missing key prints `undefined`.
fn js_stringify(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Validates the shape Claude Code accepts: hooks.<event>[].hooks[].{type, command}.
pub fn check_shape(config: &HooksConfig) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(hooks) = config.get("hooks").and_then(Value::as_object) else {
        findings.push(Finding::new(
            "missing-hooks-key",
            "hooks.json",
            "no top-level \"hooks\" object".to_string(),
        ));
        return findings;
    };
    for (event, blocks) in hooks {
        if !KNOWN_EVENTS.contains(&event.as_str()) {
            findings.push(Finding::new(
                "unknown-event",
                event,
                format!("\"{event}\" is not a Claude Code hook event — most likely a typo, and it will silently never fire"),
            ));
        }
        let Some(blocks) = blocks.as_array() else {
            findings.push(Finding::new(
                "bad-matcher-array",
                event,
                format!("value for \"{event}\" must be an array of matcher blocks"),
            ));
            continue;
        };
        for (i, block) in blocks.iter().enumerate() {
            let loc = format!("{event}[{i}]");
            if !is_object_like(block) {
                findings.push(Finding::new(
                    "bad-matcher-block",
                    &loc,
                    "matcher entry is not an object".to_string(),
                ));
                continue;
            }
            let Some(hooks_list) = block.get("hooks").and_then(Value::as_array) else {
                findings.push(Finding::new(
                    "missing-hooks-array",
                    &loc,
                    "matcher entry has no \"hooks\" array".to_string(),
                ));
                continue;
            };
            for (j, h) in hooks_list.iter().enumerate() {
                let hloc = format!("{loc}.hooks[{j}]");
                if !is_object_like(h) {
                    findings.push(Finding::new(
                        "bad-hook-entry",
                        &hloc,
                        "hook entry is not an object".to_string(),
                    ));
                    continue;
                }
                let ty = h.get("type");
                if ty.and_then(Value::as_str) != Some("command") {
                    findings.push(Finding::new(
                        "bad-hook-type",
                        &hloc,
                        format!("type is {}, expected \"command\"", js_stringify(ty)),
                    ));
                }
                let command_ok = h
                    .get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| !js_trim(c).is_empty());
                if !command_ok {
                    findings.push(Finding::new(
                        "missing-command",
                        &hloc,
                        "no non-empty \"command\" string".to_string(),
                    ));
                }
            }
        }
    }
    findings
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRef {
    pub event: String,
    pub command: String,
}

pub fn extract_commands(config: &HooksConfig) -> Vec<CommandRef> {
    let mut out = Vec::new();
    let Some(hooks) = config.get("hooks").and_then(Value::as_object) else {
        return out;
    };
    for (event, blocks) in hooks {
        let Some(blocks) = blocks.as_array() else {
            continue;
        };
        for block in blocks {
            if !is_object_like(block) {
                continue;
            }
            let Some(list) = block.get("hooks").and_then(Value::as_array) else {
                continue;
            };
            for h in list {
                if let Some(command) = h.get("command").and_then(Value::as_str) {
                    out.push(CommandRef {
                        event: event.clone(),
                        command: command.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// `/"([^"]*)"|'([^']*)'|(\S+)/g`, with JavaScript's `\S`.
static TOKEN_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(&format!(r#""([^"]*)"|'([^']*)'|([^{JS_SPACE}]+)"#)).ok());
static HAS_SLASH_OR_TILDE: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"[/~]").ok());
static HAS_EXTENSION: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"\.[A-Za-z0-9_]+$").ok());

/// Shell-aware tokenizer (respects quotes) filtered to path-shaped tokens (has a slash, ends in an extension).
pub fn path_tokens(command: &str) -> Vec<String> {
    let (Some(tok), Some(slash), Some(ext)) = (
        TOKEN_RE.as_ref(),
        HAS_SLASH_OR_TILDE.as_ref(),
        HAS_EXTENSION.as_ref(),
    ) else {
        return Vec::new();
    };
    tok.captures_iter(command)
        .filter_map(|m| {
            let t = m
                .get(1)
                .or_else(|| m.get(2))
                .or_else(|| m.get(3))
                .map_or("", |g| g.as_str());
            (!t.is_empty()).then(|| t.to_string())
        })
        .filter(|t| slash.is_match(t) && ext.is_match(t))
        .collect()
}

const PLUGIN_ROOT_VAR: &str = "${CLAUDE_PLUGIN_ROOT}";

/// Every path a command references must be ${CLAUDE_PLUGIN_ROOT}-relative and must resolve to
/// a real file, and any .ts entry must be executed through hooks/run.sh — never bare `node`,
/// because run.sh is what degrades to a silent no-op on a machine with no TS runtime.
pub fn check_path_usage(refs: &[CommandRef], plugin_root: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for r in refs {
        for tok in path_tokens(&r.command) {
            let loc = format!("{}: {}", r.event, tok);
            let Some(after) = tok.strip_prefix(PLUGIN_ROOT_VAR) else {
                findings.push(Finding::new(
                    "not-plugin-root-relative",
                    &loc,
                    "a relative or absolute path resolves against the user's cwd, not the plugin, and silently breaks — use ${CLAUDE_PLUGIN_ROOT}".to_string(),
                ));
                continue;
            };
            let rel = after.trim_start_matches('/');
            let resolved = join(&[plugin_root, rel]);
            if !Path::new(&resolved).exists() {
                findings.push(Finding::new(
                    "missing-script",
                    &loc,
                    format!("{resolved} does not exist on disk"),
                ));
                continue;
            }
            if resolved.ends_with(".ts") && !r.command.contains("run.sh") {
                findings.push(Finding::new(
                    "ts-bypasses-run-sh",
                    &loc,
                    "a .ts hook must be invoked through hooks/run.sh, never bare node — run.sh is what degrades to a silent no-op when no TS runtime is present".to_string(),
                ));
            }
        }
    }
    findings
}

/// The outcome of `spawnSync("sh", ["-c", command], { input, timeout })`.
struct Ran {
    /// `None` when a signal ended it, as `status` was `null`.
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

const DRY_RUN_TIMEOUT: Duration = Duration::from_millis(15_000);

/// Run `sh -c command` with `input` on stdin and `CLAUDE_PLUGIN_ROOT` set; the error text is the
/// one Node's `result.error.message` gave.
fn run_sh(command: &str, input: &str, plugin_root: &str) -> Result<Ran, String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("CLAUDE_PLUGIN_ROOT", plugin_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "spawnSync sh ENOENT".to_string(),
            std::io::ErrorKind::PermissionDenied => "spawnSync sh EACCES".to_string(),
            _ => format!("spawnSync sh {e}"),
        })?;
    // A hook that never reads its stdin and exits early breaks this pipe; that is not an error.
    if let Some(mut stdin) = child.stdin.take() {
        let input = input.to_string();
        std::thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let (Some(mut out), Some(mut err)) = (child.stdout.take(), child.stderr.take()) else {
        return Err("spawnSync sh EPIPE".to_string());
    };
    let out_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > DRY_RUN_TIMEOUT => {
                // Not joining the readers: a grandchild may still hold the pipes open.
                let _ = child.kill();
                let _ = child.wait();
                return Err("spawnSync sh ETIMEDOUT".to_string());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(format!("spawnSync sh {e}")),
        }
    };
    let text = |h: std::thread::JoinHandle<Vec<u8>>| {
        String::from_utf8_lossy(&h.join().unwrap_or_default()).into_owned()
    };
    Ok(Ran {
        status: status.code(),
        stdout: text(out_reader),
        stderr: text(err_reader),
    })
}

/// Dry-runs every registered command with a representative stdin event and asserts exit 0. A
/// SessionStart hook additionally must emit parseable JSON on stdout — malformed JSON there
/// wedges the session rather than just failing quietly.
pub fn dry_run_hooks(refs: &[CommandRef], plugin_root: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for r in refs {
        let key = format!("{}|{}", r.event, r.command);
        if !seen.insert(key) {
            continue;
        }
        let payload = serde_json::to_string(&event_stdin(&r.event)).unwrap_or_default();
        let result = match run_sh(&r.command, &payload, plugin_root) {
            Ok(r) => r,
            Err(message) => {
                findings.push(Finding::new("dry-run-crash", &r.event, message));
                continue;
            }
        };
        if result.status != Some(0) {
            let status = result.status.map_or("null".to_string(), |s| s.to_string());
            findings.push(Finding::new(
                "dry-run-nonzero-exit",
                &r.event,
                format!(
                    "exited {status} on a representative {} event: {}",
                    r.event,
                    slice_utf16(js_trim(&result.stderr), 300)
                ),
            ));
            continue;
        }
        if r.event == "SessionStart" {
            let out = js_trim(&result.stdout);
            if out.is_empty() {
                findings.push(Finding::new(
                    "sessionstart-empty-stdout",
                    &r.event,
                    "SessionStart hook produced no stdout".to_string(),
                ));
                continue;
            }
            if serde_json::from_str::<serde::de::IgnoredAny>(out).is_err() {
                let mut f = Finding::new(
                    "sessionstart-malformed-json",
                    &r.event,
                    "SessionStart hook stdout is not valid JSON — a malformed SessionStart payload wedges the session".to_string(),
                );
                f.detail = Some(slice_utf16(out, 300));
                findings.push(f);
            }
        }
    }
    findings
}

/// Does `readme` document `basename` as opt-in? Both must appear on the same line.
pub fn documented_as_opt_in(readme: &str, basename: &str) -> bool {
    static OPT_IN: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?i-u:opt-in)").ok());
    let Some(opt_in) = OPT_IN.as_ref() else {
        return false;
    };
    readme
        .split('\n')
        .any(|line| opt_in.is_match(line) && line.contains(basename))
}

/// A hook script reachable by no path at all: not wired in hooks.json, not documented as
/// opt-in in README.md. Neither post-tool-use-format.ts nor statusline.ts should ever trip
/// this — they are shipped deliberately unregistered and documented as such.
///
/// The TypeScript walked `readdirSync` order; this walks sorted order.
pub fn check_orphan_scripts(hooks_dir: &str, config: &HooksConfig, readme: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(entries) = read_dir_sorted(Path::new(hooks_dir)) else {
        return findings;
    };
    let registered: HashSet<String> = extract_commands(config)
        .iter()
        .flat_map(|r| path_tokens(&r.command))
        .map(|t| basename(&t).to_string())
        .collect();
    for file in entries {
        if !(file.ends_with(".ts") || file.ends_with(".sh")) {
            continue;
        }
        if file == "run.sh" {
            continue; // the TS runtime shim, not a hook script itself
        }
        if registered.contains(&file) {
            continue;
        }
        if documented_as_opt_in(readme, &file) {
            continue;
        }
        findings.push(Finding::new(
            "orphaned-hook-script",
            &file,
            format!("{file} is neither registered in hooks.json nor documented as opt-in in README.md — it is unreachable"),
        ));
    }
    findings
}

pub fn check_hooks(plugin_root: &str, readme_path: &str) -> Vec<Finding> {
    let hooks_json_path = join(&[plugin_root, "hooks", "hooks.json"]);
    let Some(text) = read_text(Path::new(&hooks_json_path)) else {
        return vec![Finding::new(
            "missing-hooks-json",
            &hooks_json_path,
            "hooks.json not found".to_string(),
        )];
    };
    let (config, findings) = parse_hooks_json(&text);
    let Some(config) = config else {
        return findings;
    };

    let shape_findings = check_shape(&config);
    let refs = extract_commands(&config);
    let path_findings = check_path_usage(&refs, plugin_root);
    let dry_run_findings = dry_run_hooks(&refs, plugin_root);

    // The orphan check degrades gracefully without a README to consult.
    let readme = read_text(Path::new(readme_path)).unwrap_or_default();
    let orphan_findings = check_orphan_scripts(&join(&[plugin_root, "hooks"]), &config, &readme);

    let mut all = findings;
    all.extend(shape_findings);
    all.extend(path_findings);
    all.extend(dry_run_findings);
    all.extend(orphan_findings);
    all
}

pub fn main(args: &[String]) -> i32 {
    let p = match parse_or_exit(args, &[flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let plugin_root = resolve(&[p
        .positionals
        .first()
        .map_or("plugins/viby-toolkit", String::as_str)]);
    let readme_path = resolve(&[&match p.positionals.get(1) {
        Some(r) => r.clone(),
        None => join(&[&plugin_root, "..", "..", "README.md"]),
    }]);

    let mut findings = check_hooks(&plugin_root, &readme_path);
    let exit = if findings.iter().any(|f| f.severity == "P1") {
        1
    } else {
        0
    };

    if p.flag("json") {
        let doc = json!({ "findings": findings.iter().map(Finding::to_json).collect::<Vec<_>>() });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return exit;
    }

    let rank = |s: &str| match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    };
    findings.sort_by_key(|f| rank(f.severity));
    for f in &findings {
        println!("[{} {}] {}", f.severity, f.check, f.location);
        println!("    {}", f.message);
        if let Some(d) = &f.detail {
            println!("    detail: {d}");
        }
    }
    if !p.flag("quiet") {
        println!();
        if findings.is_empty() {
            println!("✓ hook wiring is clean");
        } else {
            println!("{} finding(s)", findings.len());
        }
    }
    exit
}
