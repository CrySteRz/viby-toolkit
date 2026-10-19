//! Contract tests for the subagent return-size-contract check.
//!
//! Pins both halves: a checker that never flags anything is as useless as one that flags
//! everything. These tests build minimal fixture agents to prove each rule fires on a
//! deliberately broken input AND stays silent on a compliant one, then run the check against
//! the real shipped agents to prove it isn't merely passing on toy fixtures.
//!
//! Port of `tests/check-agents.test.ts`.

use std::fs;
use std::path::PathBuf;

use regex::Regex;
use viby::checks::check_agents::{
    check_agent, check_agents, grants_writing, has_citation_first_shape,
    has_clean_report_instruction, has_overflow_escape, load_agents, parallel_claim_lacks_isolation,
    stated_ceiling, Finding,
};

fn real_agents_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/viby-toolkit/agents")
}

fn real_agents_str() -> String {
    real_agents_dir().to_string_lossy().into_owned()
}

fn library(agents: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in agents {
        fs::write(dir.path().join(format!("{name}.md")), content).unwrap();
    }
    dir
}

fn dir_str(dir: &tempfile::TempDir) -> String {
    dir.path().to_string_lossy().into_owned()
}

fn findings(agents: &[(&str, String)]) -> Vec<Finding> {
    let dir = library(agents);
    check_agents(&dir_str(&dir)).findings
}

fn debug(f: &[Finding]) -> String {
    format!("{f:?}")
}

const GOOD_BODY: &str = "
You do the thing.

## Return-size contract

Hard ceiling: **50 lines**. Justified because this is a narrow job.
- Report what you checked and found clean, not only what you found.
- If it doesn't fit, write detail to a scratch file and return the headline plus the path
  (two-tier return).

## Output format
- **field**: value
";

#[derive(Default)]
struct Opts<'a> {
    name: Option<&'a str>,
    description: Option<&'a str>,
    tools: Option<&'a str>,
    body: Option<&'a str>,
}

fn fixture(opts: Opts) -> String {
    let name = opts.name.unwrap_or("widget");
    let description = opts
        .description
        .unwrap_or("Use this to do the widget thing.");
    let tools = opts.tools.unwrap_or("Read, Grep, Glob, Bash");
    let body = opts.body.unwrap_or(GOOD_BODY);
    format!("---\nname: {name}\ndescription: {description}\ntools: {tools}\n---\n{body}")
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|x| x.to_string()).collect()
}

#[test]
fn real_agents_all_eight_shipped_agent_files_pass_clean() {
    let report = check_agents(&real_agents_str());
    assert_eq!(
        report.agents.len(),
        8,
        "expected 8 real agent files, got {}",
        report.agents.len()
    );
    assert!(
        report.findings.is_empty(),
        "real agents must pass clean, got {}",
        debug(&report.findings)
    );
}

/// One scalar frontmatter field of a shipped agent, or None when the key is absent.
fn real_field(agent: &str, key: &str) -> Option<String> {
    let text = fs::read_to_string(real_agents_dir().join(format!("{agent}.md"))).unwrap();
    let end = text[3..].find("\n---").map(|i| i + 3).unwrap();
    let fm = &text[3..end];
    let re = Regex::new(&format!(r"(?m)^{key}:[ \t]*(.*)$")).unwrap();
    re.captures(fm)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}

// The six tools default workflow agents never call (study5: 1 Artifact call in 171,522 tool calls,
// Skill twice in 4,088 agents, ListAgents 16 times, the other three never). Exactly these, so a
// widened list cannot quietly take ToolSearch, Bash or an MCP tool away from every workflow stage.
const WORKER_DENIED: [&str; 6] = [
    "Artifact",
    "Skill",
    "SendFeedback",
    "ShareOnboardingGuide",
    "ReportFindings",
    "ListAgents",
];

#[test]
fn worker_denies_exactly_the_six_never_called_tools_and_keeps_every_other_tool() {
    let report = check_agents(&real_agents_str());
    let worker = report
        .agents
        .iter()
        .find(|a| a.name == "worker")
        .expect("agents/worker.md must exist and be named worker");
    let mut denied = worker.disallowed_tools.clone();
    denied.sort();
    let mut want = s(&WORKER_DENIED);
    want.sort();
    assert_eq!(denied, want);
    assert!(
        worker.tools.is_empty(),
        "an allowlist would drop MCP and deferred tools (ToolSearch) from every stage"
    );
}

#[test]
fn worker_no_max_turns_and_no_model_so_it_neither_dies_at_a_cap_nor_overrides_the_stages_model() {
    assert_eq!(
        real_field("worker", "maxTurns"),
        None,
        "81 of 90 failed typed agents stopped exactly at maxTurns"
    );
    assert_eq!(
        real_field("worker", "model"),
        None,
        "the script's per-stage model must decide"
    );
}

#[test]
fn worker_pins_a_moderate_default_effort_so_a_call_that_omits_effort_no_longer_inherits_the_sessions(
) {
    let effort = real_field("worker", "effort")
        .expect("without it, 74% of workflow agents ran at the session's effort, often max");
    assert!(
        ["low", "medium", "high"].contains(&effort.as_str()),
        "a default for every stage must not be xhigh or max, got {effort}"
    );
}

// Floors from study6/M1 (1,455 typed-agent runs): ceil5(p99 of successful runs x 1.5). Every run cut
// at its cap stopped at exactly the cap, and most returned an empty result. Lowering one re-opens that.
const MAX_TURNS_FLOOR: [(&str, f64); 6] = [
    ("implementer", 60.0),
    ("researcher", 45.0),
    ("reviewer", 40.0),
    ("scout", 30.0),
    ("skeptic", 30.0),
    ("debugger", 25.0),
];

#[test]
fn caps_no_capped_agents_max_turns_is_below_its_measured_floor() {
    for (agent, floor) in MAX_TURNS_FLOOR {
        let raw =
            real_field(agent, "maxTurns").unwrap_or_else(|| panic!("{agent} must keep a maxTurns"));
        let turns: f64 = raw.parse().unwrap_or(f64::NAN);
        assert!(
            turns >= floor,
            "{agent} maxTurns {raw} is below the measured floor {floor}"
        );
    }
}

#[test]
fn not_vacuous_a_body_with_no_stated_ceiling_is_flagged_p1() {
    let f = findings(&[(
        "widget",
        fixture(Opts {
            body: Some("You do the thing.\n\n## Output\n- field: value\n"),
            ..Default::default()
        }),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "no-return-ceiling")
        .unwrap_or_else(|| panic!("expected no-return-ceiling, got {}", debug(&f)));
    assert_eq!(hit.severity, "P1");
}

#[test]
fn a_compliant_body_with_a_stated_ceiling_is_not_flagged_for_it() {
    let f = findings(&[("widget", fixture(Opts::default()))]);
    assert!(
        !f.iter().any(|x| x.check == "no-return-ceiling"),
        "must not flag a body with a stated ceiling, got {}",
        debug(&f)
    );
}

#[test]
fn stated_ceiling_parses_the_number_near_ceiling_or_cap_not_an_unrelated_number() {
    assert_eq!(
        stated_ceiling("Hard ceiling: **80 lines**. Some other detail: 25 turns."),
        Some(80.0)
    );
    assert_eq!(
        stated_ceiling("maxTurns: 25\n\nNo ceiling stated here, just 25 turns."),
        None
    );
}

#[test]
fn a_body_missing_the_clean_report_instruction_is_flagged_p2() {
    let f = findings(&[(
        "widget",
        fixture(Opts {
            body: Some(
                "Hard ceiling: **50 lines**.\n- write detail to a file, headline plus the path (two-tier return)\n",
            ),
            ..Default::default()
        }),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "no-clean-report-instruction")
        .unwrap_or_else(|| panic!("expected no-clean-report-instruction, got {}", debug(&f)));
    assert_eq!(hit.severity, "P2");
}

#[test]
fn has_clean_report_instruction_present_vs_absent() {
    assert!(has_clean_report_instruction(
        "report what you checked and found clean"
    ));
    assert!(!has_clean_report_instruction("just report your findings"));
}

#[test]
fn a_body_missing_the_overflow_escape_is_flagged_p2() {
    let f = findings(&[(
        "widget",
        fixture(Opts {
            body: Some("Hard ceiling: **50 lines**.\n- report what you checked and found clean.\n"),
            ..Default::default()
        }),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "no-overflow-escape")
        .unwrap_or_else(|| panic!("expected no-overflow-escape, got {}", debug(&f)));
    assert_eq!(hit.severity, "P2");
}

#[test]
fn has_overflow_escape_two_tier_and_headline_plus_path_both_count_plain_text_does_not() {
    assert!(has_overflow_escape("use the two-tier return"));
    assert!(has_overflow_escape("return the headline plus the path"));
    assert!(!has_overflow_escape("just truncate it"));
}

#[test]
fn scout_researcher_with_no_citation_first_shape_are_flagged_p1_a_generic_agent_is_not() {
    let body = "Hard ceiling: **50 lines**. found clean. two-tier return.\n";
    let f = findings(&[
        (
            "scout",
            fixture(Opts {
                name: Some("scout"),
                body: Some(body),
                ..Default::default()
            }),
        ),
        (
            "widget",
            fixture(Opts {
                name: Some("widget"),
                body: Some(body),
                ..Default::default()
            }),
        ),
    ]);
    let scout_hit = f
        .iter()
        .find(|x| x.check == "not-citation-first" && x.agent == "scout")
        .unwrap_or_else(|| panic!("expected not-citation-first for scout, got {}", debug(&f)));
    assert_eq!(scout_hit.severity, "P1");
    assert!(
        !f.iter()
            .any(|x| x.check == "not-citation-first" && x.agent == "widget"),
        "a non scout/researcher agent must not be held to the citation-first rule"
    );
}

#[test]
fn has_citation_first_shape_requires_both_a_location_cite_and_a_no_pasting_rule() {
    assert!(has_citation_first_shape(
        "cite file:line, never paste file contents back"
    ));
    assert!(!has_citation_first_shape("cite file:line only"));
    assert!(!has_citation_first_shape("never paste content back"));
}

#[test]
fn write_edit_agent_claiming_parallel_safety_without_isolation_is_flagged_p1() {
    let f = findings(&[(
        "writer",
        fixture(Opts {
            name: Some("writer"),
            tools: Some("Read, Edit, Write"),
            description: Some("Dispatch several in parallel to build the pieces."),
            body: Some(GOOD_BODY),
        }),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "parallel-write-without-isolation")
        .unwrap_or_else(|| {
            panic!(
                "expected parallel-write-without-isolation, got {}",
                debug(&f)
            )
        });
    assert_eq!(hit.severity, "P1");
}

#[test]
fn write_edit_agent_that_names_worktree_isolation_for_parallel_dispatch_is_not_flagged() {
    let f = findings(&[(
        "writer",
        fixture(Opts {
            name: Some("writer"),
            tools: Some("Read, Edit, Write"),
            description: Some(
                "Dispatch several in parallel; use worktree isolation if they'd touch the same files.",
            ),
            body: Some(GOOD_BODY),
        }),
    )]);
    assert!(
        !f.iter()
            .any(|x| x.check == "parallel-write-without-isolation"),
        "must not flag isolation-qualified parallel dispatch, got {}",
        debug(&f)
    );
}

#[test]
fn a_read_only_agent_no_write_edit_claiming_parallel_safety_is_not_flagged() {
    let f = findings(&[(
        "reader",
        fixture(Opts {
            name: Some("reader"),
            tools: Some("Read, Grep, Glob, Bash"),
            description: Some("Dispatch several in parallel on different areas."),
            body: Some(GOOD_BODY),
        }),
    )]);
    assert!(
        !f.iter()
            .any(|x| x.check == "parallel-write-without-isolation"),
        "must not apply the isolation rule to a read-only agent, got {}",
        debug(&f)
    );
}

#[test]
fn parallel_claim_lacks_isolation_direct_unit_behaviour() {
    assert!(parallel_claim_lacks_isolation(
        "Dispatch several in parallel."
    ));
    assert!(!parallel_claim_lacks_isolation(
        "Dispatch several in parallel with worktree isolation."
    ));
    assert!(!parallel_claim_lacks_isolation(
        "No mention of concurrency at all."
    ));
}

#[test]
fn a_denylist_only_agent_disallowed_tools_no_tools_is_not_flagged_as_having_no_tools() {
    let dir = library(&[(
        "lean",
        format!(
            "---\nname: lean\ndescription: Use this for a stage.\ndisallowedTools: Artifact, Skill\n---\n{GOOD_BODY}"
        ),
    )]);
    let agents = load_agents(&dir_str(&dir));
    let agent = agents.first().expect("fixture agent should load");
    assert_eq!(agent.disallowed_tools, s(&["Artifact", "Skill"]));
    assert!(
        !check_agent(agent).iter().any(|x| x.check == "no-tools"),
        "a denylist states what the agent may touch"
    );
}

#[test]
fn a_denylist_only_agent_inherits_write_edit_so_an_unisolated_parallel_claim_is_flagged_p1() {
    let f = findings(&[(
        "lean",
        format!(
            "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Artifact\n---\n{GOOD_BODY}"
        ),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "parallel-write-without-isolation")
        .unwrap_or_else(|| {
            panic!(
                "expected parallel-write-without-isolation, got {}",
                debug(&f)
            )
        });
    assert_eq!(hit.severity, "P1");
}

#[test]
fn a_denylist_only_agent_that_denies_write_edit_and_notebook_edit_is_not_held_to_the_isolation_rule(
) {
    let f = findings(&[(
        "lean",
        format!(
            "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Write, Edit, NotebookEdit\n---\n{GOOD_BODY}"
        ),
    )]);
    assert!(
        !f.iter()
            .any(|x| x.check == "parallel-write-without-isolation"),
        "got {}",
        debug(&f)
    );
}

#[test]
fn a_denylist_only_agent_that_denies_only_write_and_edit_still_writes_via_notebook_edit_so_it_is_flagged_p1(
) {
    let f = findings(&[(
        "lean",
        format!(
            "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Write, Edit\n---\n{GOOD_BODY}"
        ),
    )]);
    let hit = f
        .iter()
        .find(|x| x.check == "parallel-write-without-isolation")
        .unwrap_or_else(|| {
            panic!(
                "expected parallel-write-without-isolation, got {}",
                debug(&f)
            )
        });
    assert_eq!(hit.severity, "P1");
}

#[test]
fn grants_writing_allowlist_denylist_both_and_neither() {
    assert!(grants_writing(&s(&["Read", "Edit"]), &[]));
    assert!(!grants_writing(&s(&["Read", "Edit"]), &s(&["Edit"])));
    assert!(!grants_writing(
        &s(&["Read", "Bash"]),
        &s(&["Write", "Edit"])
    ));
    assert!(grants_writing(&[], &s(&["Skill"])));
    assert!(grants_writing(&[], &s(&["Write", "Edit"])));
    assert!(!grants_writing(&[], &s(&["Write", "Edit", "NotebookEdit"])));
    assert!(grants_writing(&s(&["Read", "NotebookEdit"]), &[]));
    assert!(!grants_writing(&[], &[]));
}

#[test]
fn missing_frontmatter_fields_name_description_tools_are_each_flagged_p1() {
    let f = findings(&[(
        "broken",
        "---\nname: broken\n---\nHard ceiling: **50 lines**. found clean. two-tier return.\n"
            .to_string(),
    )]);
    assert!(f
        .iter()
        .any(|x| x.check == "no-description" && x.severity == "P1"));
    assert!(f
        .iter()
        .any(|x| x.check == "no-tools" && x.severity == "P1"));
    assert!(!f.iter().any(|x| x.check == "no-name"));
}

#[test]
fn check_agent_on_a_single_fully_broken_agent_surfaces_every_relevant_finding_at_once() {
    let dir = library(&[(
        "bad",
        "---\nname: bad\n---\nno contract at all here.\n".to_string(),
    )]);
    let agents = load_agents(&dir_str(&dir));
    let agent = agents.first().expect("fixture agent should load");
    let mut checks: Vec<String> = check_agent(agent).into_iter().map(|x| x.check).collect();
    checks.sort();
    assert_eq!(
        checks,
        vec![
            "no-clean-report-instruction",
            "no-description",
            "no-overflow-escape",
            "no-return-ceiling",
            "no-tools"
        ]
    );
}

#[test]
fn no_agents_found_in_an_empty_missing_directory_does_not_throw() {
    let dir = tempfile::tempdir().unwrap();
    let report = check_agents(&dir_str(&dir));
    assert_eq!(report.agents.len(), 0);
    assert!(report.findings.is_empty());
}
