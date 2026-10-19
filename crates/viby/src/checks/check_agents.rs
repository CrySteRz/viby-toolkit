//! viby-toolkit subagent contract check — guards the return-size contract, the mechanism
//! that makes fan-out worth doing at all.
//!
//! Subagents are the toolkit's context firewall: a scout can read a whole subsystem and the
//! caller only pays for a few lines back. Nothing enforced that split — a subagent whose body
//! never states a ceiling can (and, unwatched, will) return prose by the screenful and quietly
//! undo the mechanism the whole design depends on. This is the checkable half of the "four-part
//! subagent contract" in skills/principles/references/model-routing.md.
//!
//! It also catches a narrower but sharper bug: an agent with Write/Edit in its tools that
//! describes itself as safe to dispatch in parallel WITHOUT naming isolation (worktrees, or
//! "don't touch the same files"). Two writers on the same file in parallel is a known conflict
//! generator; a description that invites parallel writes without that caveat is a footgun
//! baked into the routing text itself.
//!
//! Usage:
//!   viby check-agents [agents-dir] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no agents found.
//!
//! Port of `check-agents.ts`.

use std::path::Path;
use std::sync::LazyLock;

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde_json::{json, Map, Value};

use super::check_skills::{
    fence_index, frontmatter, js_trim, node_join, node_resolve, read_dir_sorted, read_text,
    yaml_field, WS_CLASS,
};
use crate::core::args::{flag, parse_or_exit};

#[derive(Clone, Debug)]
pub struct Finding {
    pub check: String,
    pub severity: &'static str,
    pub agent: String,
    pub message: String,
    pub detail: Option<String>,
}

impl Finding {
    fn new(
        check: &str,
        severity: &'static str,
        agent: &str,
        message: &str,
        detail: Option<&str>,
    ) -> Finding {
        Finding {
            check: check.to_string(),
            severity,
            agent: agent.to_string(),
            message: message.to_string(),
            detail: detail.map(str::to_string),
        }
    }

    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        map.insert("check".into(), json!(self.check));
        map.insert("severity".into(), json!(self.severity));
        map.insert("agent".into(), json!(self.agent));
        map.insert("message".into(), json!(self.message));
        if let Some(d) = &self.detail {
            map.insert("detail".into(), json!(d));
        }
        Value::Object(map)
    }
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub file: String,
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    /// `disallowedTools`: with no `tools` allowlist, the agent inherits every tool except these.
    pub disallowed_tools: Vec<String>,
    pub has_frontmatter: bool,
    pub body: String,
}

pub struct AgentsReport {
    pub agents: Vec<Agent>,
    pub findings: Vec<Finding>,
}

fn yaml_list(fm: &str, key: &str) -> Vec<String> {
    let raw = yaml_field(fm, key);
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(',')
        .map(|s| js_trim(s).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn load_agents(dir: &str) -> Vec<Agent> {
    let mut out = Vec::new();
    let Some(entries) = read_dir_sorted(Path::new(dir)) else {
        return out;
    };
    for entry in entries {
        let Some(stem) = entry.strip_suffix(".md") else {
            continue;
        };
        let file = node_join(&[dir, &entry]);
        // The TypeScript let an unreadable file throw; here it is skipped.
        let Some(text) = read_text(Path::new(&file)) else {
            continue;
        };
        let fm = frontmatter(&text);
        let body = match fence_index(&text) {
            Some(i) => text[i + 4..].to_string(),
            None => text.clone(),
        };
        out.push(Agent {
            file,
            name: match yaml_field(fm, "name") {
                n if n.is_empty() => stem.to_string(),
                n => n,
            },
            description: yaml_field(fm, "description"),
            tools: yaml_list(fm, "tools"),
            disallowed_tools: yaml_list(fm, "disallowedTools"),
            has_frontmatter: !fm.is_empty(),
            body,
        });
    }
    out
}

/// A stated ceiling: a number tied to "lines" or "words" within the same short span as the
/// word "ceiling" or "cap". Loose enough to allow "Hard ceiling: **80 lines**" and "cap each
/// finding to ... 15 findings", tight enough that an unrelated number elsewhere in the body
/// (a maxTurns value, a severity list) can't satisfy it by accident.
///
/// `(?i-u:..)` is JavaScript's ASCII-only `/i`; `(?-u:\b)` and `[0-9]` are its ASCII `\b` and `\d`.
static CEILING_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?-u:\b)(?i-u:ceiling|cap)(?-u:\b)[^\n]{0,60}?(?-u:\b)([0-9]+)(?-u:\b)[^\n]{0,20}?(?-u:\b)(?i-u:lines?|words?|findings?)(?-u:\b)",
    )
    .ok()
});

pub fn stated_ceiling(body: &str) -> Option<f64> {
    let caps = CEILING_RE.as_ref()?.captures(body)?;
    caps.get(1)?.as_str().parse::<f64>().ok()
}

fn has(pattern: &str, text: &str) -> bool {
    Regex::new(pattern).is_ok_and(|re| re.is_match(text))
}

/// "report negative results" instruction — the phrase this repo standardised on.
pub fn has_clean_report_instruction(body: &str) -> bool {
    has(r"(?i-u:found clean)", body)
}

/// The two-tier escape hatch: write detail to a file, return headline + path.
pub fn has_overflow_escape(body: &str) -> bool {
    has(r"(?i-u:two-tier)", body)
        || (has(r"(?i-u:headline)", body) && has(r"(?-u:\b)(?i-u:path)(?-u:\b)", body))
}

/// Citation-first shape: cite a location/URL, don't paste content back.
pub fn has_citation_first_shape(body: &str) -> bool {
    let cites_location = has(r"(?i-u:file:line|file:start-end|`URL`|URL` plus)", body);
    // JavaScript's `.` stops at the four line terminators.
    let forbids_pasting = has(
        r"(?i-u:never paste|do not paste)|(?i-u:not)[^\n\r\u2028\u2029]*(?i-u:paste)[^\n\r\u2028\u2029]*(?i-u:back|content)",
        body,
    );
    cites_location && forbids_pasting
}

/// `text.split(/(?<=[.!?])\s+/)`.
fn sentences(text: &str) -> Vec<&str> {
    static SPLIT: LazyLock<Option<FancyRegex>> =
        LazyLock::new(|| FancyRegex::new(&format!(r"(?<=[.!?])[{WS_CLASS}]+")).ok());
    let Some(re) = SPLIT.as_ref() else {
        return vec![text];
    };
    let mut out = Vec::new();
    let mut last = 0;
    for m in re.find_iter(text).flatten() {
        out.push(&text[last..m.start()]);
        last = m.end();
    }
    out.push(&text[last..]);
    out
}

/// Claims safety running in parallel near the word "parallel" without naming isolation.
pub fn parallel_claim_lacks_isolation(text: &str) -> bool {
    for s in sentences(text) {
        if has(r"(?i-u:parallel)", s)
            && !has(
                r"(?i-u:isolat|worktree|different files|touch the same files)",
                s,
            )
        {
            // Only a problem if no OTHER sentence in the file supplies the isolation caveat at all.
            if !has(r"(?i-u:isolat|worktree)", text) {
                return true;
            }
        }
    }
    false
}

/// Whether the agent can write. An allowlist grants Write, Edit or NotebookEdit only by naming them; a
/// denylist-only agent inherits all three unless it denies every one, so `disallowedTools: Write, Edit`
/// alone still writes through NotebookEdit and is held to the same parallel-isolation rule. Either way
/// a denied tool is removed, as Claude Code removes it. Bash and MCP tools are not counted, as on main.
pub fn grants_writing(tools: &[String], disallowed_tools: &[String]) -> bool {
    let kept = |t: &str| !disallowed_tools.iter().any(|d| d == t);
    let writers = ["Write", "Edit", "NotebookEdit"];
    if !tools.is_empty() {
        return tools
            .iter()
            .any(|t| writers.contains(&t.as_str()) && kept(t));
    }
    if disallowed_tools.is_empty() {
        return false;
    }
    writers.iter().any(|t| kept(t))
}

const CITATION_FIRST_AGENTS: [&str; 2] = ["scout", "researcher"];

pub fn check_agent(agent: &Agent) -> Vec<Finding> {
    let mut findings = Vec::new();
    let label = agent.name.as_str();

    if !agent.has_frontmatter {
        findings.push(Finding::new(
            "no-frontmatter",
            "P1",
            label,
            "no frontmatter block parsed — the agent is likely unusable",
            None,
        ));
        return findings;
    }
    if js_trim(&agent.name).is_empty() {
        findings.push(Finding::new(
            "no-name",
            "P1",
            &agent.file,
            "frontmatter has no `name`",
            None,
        ));
    }
    if js_trim(&agent.description).is_empty() {
        findings.push(Finding::new(
            "no-description",
            "P1",
            label,
            "frontmatter has no `description` — dispatch has nothing to route on",
            None,
        ));
    }
    // A denylist is as explicit as an allowlist: `disallowedTools` alone means "every inherited tool
    // except these", which is how the general `worker` keeps MCP and deferred tools reachable.
    if agent.tools.is_empty() && agent.disallowed_tools.is_empty() {
        findings.push(Finding::new(
            "no-tools",
            "P1",
            label,
            "frontmatter has neither a `tools` allowlist nor a `disallowedTools` denylist — unclear what the agent is allowed to touch",
            None,
        ));
    }

    if stated_ceiling(&agent.body).is_none() {
        findings.push(Finding::new(
            "no-return-ceiling",
            "P1",
            label,
            "body states no hard ceiling on its returned report — nothing stops it returning a full \
             file dump instead of a summary, which defeats the reason a subagent exists",
            Some("state one, e.g. \"Hard ceiling: N lines\" sized to this agent's job"),
        ));
    }

    if !has_clean_report_instruction(&agent.body) {
        findings.push(Finding::new(
            "no-clean-report-instruction",
            "P2",
            label,
            "body never instructs reporting what was checked and found \"clean\" — without it the caller \
             cannot tell \"nothing there\" from \"never looked\"",
            None,
        ));
    }

    if !has_overflow_escape(&agent.body) {
        findings.push(Finding::new(
            "no-overflow-escape",
            "P2",
            label,
            "body never says what to do when the real answer doesn't fit the ceiling — the failure mode \
             without this is silent truncation or a dump straight into the ceiling",
            Some("state the two-tier return: write detail to a file, return the headline + path"),
        ));
    }

    if CITATION_FIRST_AGENTS.contains(&agent.name.as_str())
        && !has_citation_first_shape(&agent.body)
    {
        findings.push(Finding::new(
            "not-citation-first",
            "P1",
            label,
            "return shape is not citation-first — must cite file:line/URL plus a short clause and \
             explicitly forbid pasting file contents back, or this agent silently becomes a file dump",
            None,
        ));
    }

    let grants_write_or_edit = grants_writing(&agent.tools, &agent.disallowed_tools);
    if grants_write_or_edit
        && parallel_claim_lacks_isolation(&format!("{}\n{}", agent.description, agent.body))
    {
        findings.push(Finding::new(
            "parallel-write-without-isolation",
            "P1",
            label,
            "grants Write/Edit and discusses running in parallel without naming isolation (worktrees, or \
             not touching the same files) — two writers on one file in parallel is a known conflict source",
            None,
        ));
    }

    findings
}

pub fn check_agents(dir: &str) -> AgentsReport {
    let agents = load_agents(dir);
    let mut findings = Vec::new();
    for a in &agents {
        findings.extend(check_agent(a));
    }
    AgentsReport { agents, findings }
}

pub fn main(args: &[String]) -> i32 {
    let p = match parse_or_exit(args, &[flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let as_json = p.flag("json");
    let quiet = p.flag("quiet");
    let dir = node_resolve(
        p.positionals
            .first()
            .map_or("plugins/viby-toolkit/agents", String::as_str),
    );
    let AgentsReport {
        agents,
        mut findings,
    } = check_agents(&dir);

    if agents.is_empty() {
        if as_json {
            println!("{{\"agents\":0,\"findings\":[]}}");
        } else if !quiet {
            println!("no agents found under {dir}");
        }
        return 2;
    }

    let exit = if findings.iter().any(|f| f.severity == "P1") {
        1
    } else {
        0
    };
    if as_json {
        let doc = json!({
            "agents": agents.len(),
            "findings": findings.iter().map(Finding::to_json).collect::<Vec<_>>(),
        });
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
        println!("[{} {}] {}", f.severity, f.check, f.agent);
        println!("    {}", f.message);
        if let Some(d) = &f.detail {
            println!("    fix: {d}");
        }
    }

    if !quiet {
        println!();
        println!("{} agents checked under {dir}", agents.len());
        if findings.is_empty() {
            println!("✓ every agent states a return-size contract");
        }
    }
    exit
}
