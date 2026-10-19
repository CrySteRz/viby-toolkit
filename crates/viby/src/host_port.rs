//! `viby host-port`: wires opencode to this plugin's files, so both hosts stay in lockstep.
//!
//! One source of truth: `<repo>/plugins/viby-toolkit/`. Claude Code loads it as a plugin; this
//! points opencode at the same files:
//!
//!   ~/.config/opencode/opencode.json
//!       skills.paths   -> <plugin>/skills (opencode scans SKILL.md recursively)
//!       instructions   -> ~/.config/opencode/instructions/viby-contract.md
//!       mcp.viby-browser -> the plugin's launcher, `browser-server`
//!   ~/.config/opencode/agents/<name>.md   generated from the plugin's agents/
//!   ~/.config/opencode/command/<name>.md  symlinks into the plugin's commands/
//!
//! Usage: viby host-port [--plugin <dir>] [--dry-run] [--uninstall]
//! Idempotent; re-running after an update keeps everything in place.
//!
//! Port of tools/host-port.ts, with two of its bugs fixed: `--dry-run` now prints what it would
//! do (the TS built the lines and never printed them), and `--uninstall` no longer rewrites the
//! agent files it has just removed.

use crate::core::args::{flag, parse_or_exit, value};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// `worker` exists only for Claude Code's Workflow tool, so it is not ported.
const CLAUDE_CODE_ONLY_AGENTS: &[&str] = &["worker"];

const CONTRACT_BODY: &str = "# viby-toolkit standing contract (opencode)

The viby-toolkit skills and agents are loaded from the plugin repo; invoke them by bare name:
`review`, `verify`, `plan`, `orchestrate`, `brainstorm`, `principles`, `learn`, `handoff`,
`debug`, `incident`, `refactor`, `migrate`, `schema`, `observe`, `api`, `perf`, `test`, `deps`,
`docs`, `ui`, `explore`, `kpi`, `analytics`, `study`, `evaluate`, `secure`, `adopt`, `brain`,
`worktrees`, `extend`, `release`, and the `ship` command.

Agents (bare names): `scout`, `researcher`, `implementer`, `reviewer`, `skeptic`, `debugger`.

Standing rules for this session, regardless of skill in play:
- EVIDENCE GATE: never claim done without running the check fresh and showing its command, output, and exit code.
- FAN OUT BY DEFAULT, DON'T ASK: work needing breadth gets 3-4 read-only subagents (scout, researcher,
  reviewer, skeptic) dispatched IN ONE MESSAGE, each read-only. Writes stay single-threaded on the
  main thread (parallel writers make conflicting decisions).
- CHEAP MODELS FIND, MAIN THREAD DECIDES; escalate on low confidence.
- ROUTING: if WHAT to build is unsettled, run `brainstorm` first; else `plan` / `orchestrate`.
- NEAR A CONTEXT LIMIT: expect a refusal, not a hallucination -- clear and re-dispatch beats re-asking.
- NEVER SHIP SPECULATIVE FIXES to production under pressure; use `incident` before `debug`.

Full contract: plugins/viby-toolkit/skills/principles/SKILL.md -- read it before non-trivial work.";

struct Port {
    dry: bool,
    uninstall: bool,
}

impl Port {
    fn log(&self, m: &str) {
        println!("{}{m}", if self.dry { "[dry-run] " } else { "" });
    }

    fn ensure(&self, dir: &Path) -> std::io::Result<()> {
        if self.dry {
            self.log(&format!("mkdir {}", dir.display()));
            Ok(())
        } else {
            fs::create_dir_all(dir)
        }
    }

    fn remove(&self, path: &Path) {
        self.log(&format!("rm {}", path.display()));
        if !self.dry {
            let _ = fs::remove_file(path);
        }
    }

    /// Writes `body` to `path` unless it already holds exactly that.
    fn write(&self, path: &Path, body: &str, label: &str) -> std::io::Result<()> {
        if fs::read_to_string(path).is_ok_and(|cur| cur == body) {
            self.log(&format!("{label} (unchanged)"));
            return Ok(());
        }
        self.log(label);
        if self.dry {
            Ok(())
        } else {
            fs::write(path, body)
        }
    }
}

/// An agent file's name, one-line description, prompt body, and whether its tools can edit.
pub fn agent_parts(text: &str) -> (String, String, String, bool) {
    let Some(end) = text
        .strip_prefix("---")
        .and_then(|t| t.find("\n---"))
        .map(|e| e + 3)
    else {
        return (String::new(), String::new(), text.to_string(), false);
    };
    let fm = &text[3..end];
    let rest = text[end + 4..]
        .strip_prefix('\n')
        .unwrap_or(&text[end + 4..]);
    let field = |key: &str| {
        fm.lines()
            .find_map(|l| l.strip_prefix(key).map(|v| v.trim().to_string()))
    };
    let name = field("name:").unwrap_or_default();
    // description: inline, or a folded block (`description: >`) of more-indented lines.
    let lines: Vec<&str> = fm.split('\n').collect();
    let mut description = String::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(v) = line.strip_prefix("description:") else {
            continue;
        };
        let v = v.trim();
        if v.is_empty() || v == ">" || v == "|" {
            for next in &lines[i + 1..] {
                if !next.starts_with(char::is_whitespace) || next.trim().is_empty() {
                    break;
                }
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(next.trim());
            }
        } else {
            description = v.to_string();
        }
        if !description.is_empty() {
            break;
        }
    }
    let tools = field("tools:").unwrap_or_default();
    let can_edit = tools.split(',').any(|t| {
        matches!(t.trim(), "Edit" | "Write")
            || t.trim().starts_with("Edit(")
            || t.trim().starts_with("Write(")
    });
    (name, description, rest.trim().to_string(), can_edit)
}

fn md_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".md")
                .map(String::from)
        })
        .collect();
    names.sort();
    names
}

/// The opencode config after adding (or removing) this plugin.
pub fn merged_config(
    mut out: Map<String, Value>,
    skills: &str,
    contract: &str,
    browser: Value,
    uninstall: bool,
) -> Map<String, Value> {
    let strings = |v: Option<&Value>| -> Vec<Value> {
        v.and_then(Value::as_array).cloned().unwrap_or_default()
    };
    if uninstall {
        if let Some(s) = out.get_mut("skills").and_then(Value::as_object_mut) {
            let paths: Vec<Value> = strings(s.get("paths"))
                .into_iter()
                .filter(|p| p != skills)
                .collect();
            // The TS left an empty `paths: []` behind; an empty list is removed like the rest.
            if paths.is_empty() {
                s.remove("paths");
            } else {
                s.insert("paths".into(), Value::Array(paths));
            }
        }
        if out
            .get("skills")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            out.remove("skills");
        }
        let instructions: Vec<Value> = strings(out.get("instructions"))
            .into_iter()
            .filter(|p| p != contract)
            .collect();
        if instructions.is_empty() {
            out.remove("instructions");
        } else {
            out.insert("instructions".into(), Value::Array(instructions));
        }
        if let Some(m) = out.get_mut("mcp").and_then(Value::as_object_mut) {
            m.remove("viby-browser");
        }
        if out
            .get("mcp")
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
        {
            out.remove("mcp");
        }
    } else {
        let mut s = out
            .get("skills")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut paths = vec![json!(skills)];
        paths.extend(strings(s.get("paths")).into_iter().filter(|p| p != skills));
        s.insert("paths".into(), Value::Array(paths));
        out.insert("skills".into(), Value::Object(s));
        let mut instructions = vec![json!(contract)];
        instructions.extend(
            strings(out.get("instructions"))
                .into_iter()
                .filter(|p| p != contract),
        );
        out.insert("instructions".into(), Value::Array(instructions));
        let mut mcp = out
            .get("mcp")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        mcp.insert("viby-browser".into(), browser);
        out.insert("mcp".into(), Value::Object(mcp));
    }
    out
}

fn run(plugin: &Path, home: &Path, port: &Port) -> std::io::Result<()> {
    let oc = home.join(".config/opencode");
    let (agents_dir, commands_dir) = (oc.join("agents"), oc.join("command"));
    let contract = oc.join("instructions/viby-contract.md");
    let config = oc.join("opencode.json");
    let skills = plugin.join("skills");

    // Agents: the repo's frontmatter is Claude Code's (model, effort, tools), which opencode does
    // not accept. Generate opencode-native agents with the same body; edit permission follows
    // whether the agent can Edit/Write.
    let agent_names: Vec<String> = md_names(&plugin.join("agents"))
        .into_iter()
        .filter(|n| !CLAUDE_CODE_ONLY_AGENTS.contains(&n.as_str()))
        .collect();
    port.ensure(&agents_dir)?;
    for n in md_names(&agents_dir) {
        if agent_names.contains(&n) || n.starts_with("viby-") {
            port.remove(&agents_dir.join(format!("{n}.md")));
        }
    }
    if !port.uninstall {
        for n in &agent_names {
            let (name, description, body, can_edit) =
                agent_parts(&fs::read_to_string(plugin.join(format!("agents/{n}.md")))?);
            let perm = if can_edit { "allow" } else { "deny" };
            let name = if name.is_empty() { n.clone() } else { name };
            let description = if description.is_empty() {
                "viby-toolkit helper agent".to_string()
            } else {
                description
            };
            let out = format!("---\nname: {name}\nmode: subagent\ndescription: {description}\npermission:\n  edit: {perm}\n---\n\n{body}\n");
            port.write(
                &agents_dir.join(format!("{n}.md")),
                &out,
                &format!("agents/{n}.md  (edit:{perm})"),
            )?;
        }
    }

    // Commands: symlinks, so an edit in the repo shows up in opencode at once.
    let command_names = md_names(&plugin.join("commands"));
    port.ensure(&commands_dir)?;
    for n in md_names(&commands_dir) {
        if command_names.contains(&n) || n.starts_with("viby-") {
            port.remove(&commands_dir.join(format!("{n}.md")));
        }
    }
    if !port.uninstall {
        for n in &command_names {
            port.log(&format!("command/{n}.md"));
            #[cfg(unix)]
            if !port.dry {
                std::os::unix::fs::symlink(
                    plugin.join(format!("commands/{n}.md")),
                    commands_dir.join(format!("{n}.md")),
                )?;
            }
            #[cfg(windows)]
            if !port.dry {
                fs::copy(
                    plugin.join(format!("commands/{n}.md")),
                    commands_dir.join(format!("{n}.md")),
                )?;
            }
        }
    }

    // The standing contract.
    port.ensure(contract.parent().expect("has a parent"))?;
    if port.uninstall {
        port.remove(&contract);
    } else {
        port.write(
            &contract,
            CONTRACT_BODY,
            &format!("wrote {}", contract.display()),
        )?;
    }

    // The config.
    let current: Map<String, Value> = fs::read_to_string(&config)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let launcher = plugin.join(if cfg!(windows) {
        "launcher/viby.cmd"
    } else {
        "launcher/viby"
    });
    let browser = json!({ "type": "local", "command": [launcher.to_string_lossy(), "browser-server"], "enabled": true });
    let next = merged_config(
        current,
        &skills.to_string_lossy(),
        &contract.to_string_lossy(),
        browser,
        port.uninstall,
    );
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&Value::Object(next)).unwrap_or_default()
    );
    port.write(&config, &text, &format!("wrote {}", config.display()))?;
    Ok(())
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(
        args,
        &[
            flag("dry-run"),
            flag("uninstall"),
            value("plugin", Some("plugins/viby-toolkit")),
        ],
    ) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let plugin = PathBuf::from(parsed.value("plugin").unwrap_or("plugins/viby-toolkit"));
    let Ok(plugin) = plugin.canonicalize() else {
        eprintln!(
            "host-port: no plugin at {}; run it from the viby-toolkit repo or pass --plugin <dir>",
            plugin.display()
        );
        return 2;
    };
    if !plugin.join("skills").is_dir() {
        eprintln!(
            "host-port: {} has no skills/; pass --plugin <dir>",
            plugin.display()
        );
        return 2;
    }
    let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
    else {
        eprintln!("host-port: HOME is not set");
        return 1;
    };
    let port = Port {
        dry: parsed.flag("dry-run"),
        uninstall: parsed.flag("uninstall"),
    };
    match run(&plugin, &home, &port) {
        Ok(()) => {
            println!(
                "{}",
                if port.dry {
                    "dry-run complete"
                } else {
                    "port complete — restart opencode to load"
                }
            );
            0
        }
        Err(e) => {
            eprintln!("host-port: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_inline_and_folded_descriptions_and_edit_rights() {
        let (n, d, b, e) = agent_parts("---\nname: scout\ndescription: >\n  Finds things.\n  Fast.\ntools: Read, Grep\n---\n\nBody.\n");
        assert_eq!(
            (n.as_str(), d.as_str(), b.as_str(), e),
            ("scout", "Finds things. Fast.", "Body.", false)
        );
        let (_, d, _, e) =
            agent_parts("---\nname: impl\ndescription: Builds.\ntools: Read, Edit, Write\n---\nX");
        assert_eq!((d.as_str(), e), ("Builds.", true));
    }

    #[test]
    fn config_merge_is_idempotent_and_uninstall_removes_only_ours() {
        let browser = json!({ "type": "local" });
        let mut start = Map::new();
        start.insert("theme".into(), json!("x"));
        start.insert("instructions".into(), json!(["other.md"]));
        let once = merged_config(start.clone(), "/p/skills", "/c.md", browser.clone(), false);
        let twice = merged_config(once.clone(), "/p/skills", "/c.md", browser.clone(), false);
        assert_eq!(once, twice);
        assert_eq!(once["instructions"], json!(["/c.md", "other.md"]));
        let back = merged_config(twice, "/p/skills", "/c.md", browser, true);
        assert_eq!(back.get("theme"), Some(&json!("x")));
        assert_eq!(back["instructions"], json!(["other.md"]));
        assert!(back.get("mcp").is_none() && back.get("skills").is_none());
    }
}
