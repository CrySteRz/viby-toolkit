//! Contract tests for the hook-wiring auditor.
//!
//! Both halves, per house style: what must be flagged, and — just as load-bearing — what must
//! NOT be. The two real opt-in scripts (post-tool-use-format.ts, statusline.ts) are the
//! canonical must-NOT case: an earlier analysis wrongly called them a defect, and this file
//! pins that they are clean by design.
//!
//! Port of `tests/check-hooks.test.ts`.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use viby::checks::check_hooks::{
    check_hooks, check_orphan_scripts, check_path_usage, check_shape, documented_as_opt_in,
    dry_run_hooks, extract_commands, parse_hooks_json, path_tokens, CommandRef, Finding,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../..")
}

fn real_plugin_root() -> String {
    repo_root()
        .join("plugins/viby-toolkit")
        .to_string_lossy()
        .into_owned()
}

fn real_readme() -> String {
    repo_root().join("README.md").to_string_lossy().into_owned()
}

/// A minimal plugin skeleton: hooks/ dir with the given hooks.json + extra files.
fn fixture(hooks_json: &Value, extra_files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let hooks_dir = root.path().join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let text = match hooks_json {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap(),
    };
    fs::write(hooks_dir.join("hooks.json"), text).unwrap();
    for (rel, body) in extra_files {
        let full = hooks_dir.join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, body).unwrap();
        #[cfg(unix)]
        if rel.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&full, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    root
}

fn root_str(root: &tempfile::TempDir) -> String {
    root.path().to_string_lossy().into_owned()
}

fn r(event: &str, command: &str) -> CommandRef {
    CommandRef {
        event: event.to_string(),
        command: command.to_string(),
    }
}

fn read_config(root: &tempfile::TempDir) -> Value {
    serde_json::from_str(&fs::read_to_string(root.path().join("hooks").join("hooks.json")).unwrap())
        .unwrap()
}

fn debug(f: &[Finding]) -> String {
    format!("{f:?}")
}

// ---------------------------------------------------------------------------------
// The real shipped artifact must pass — dogfooding.
// ---------------------------------------------------------------------------------

#[test]
fn the_real_shipped_hooks_json_has_zero_findings() {
    let findings = check_hooks(&real_plugin_root(), &real_readme());
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn the_real_opt_in_scripts_do_not_trip_the_orphan_check() {
    let hooks_json: Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("plugins/viby-toolkit/hooks/hooks.json")).unwrap(),
    )
    .unwrap();
    let readme = fs::read_to_string(real_readme()).unwrap();
    let hooks_dir = repo_root().join("plugins/viby-toolkit/hooks");
    let findings = check_orphan_scripts(&hooks_dir.to_string_lossy(), &hooks_json, &readme);
    let hit: Vec<_> = findings
        .iter()
        .filter(|f| f.location == "post-tool-use-format.ts" || f.location == "statusline.ts")
        .collect();
    assert!(hit.is_empty(), "{hit:?}");
}

// ---------------------------------------------------------------------------------
// hooks.json parsing / shape
// ---------------------------------------------------------------------------------

#[test]
fn invalid_json_is_a_p1() {
    let (config, findings) = parse_hooks_json("{ not json");
    assert!(config.is_none());
    assert_eq!(findings[0].severity, "P1");
    assert_eq!(findings[0].check, "invalid-json");
}

#[test]
fn valid_json_that_is_an_array_not_an_object_is_a_p1() {
    let (config, findings) = parse_hooks_json("[]");
    assert!(config.is_none());
    assert_eq!(findings[0].check, "invalid-shape");
}

#[test]
fn missing_top_level_hooks_key_is_a_p1() {
    let findings = check_shape(&json!({}));
    assert!(findings
        .iter()
        .any(|f| f.check == "missing-hooks-key" && f.severity == "P1"));
}

#[test]
fn an_unrecognised_event_name_is_a_p1_the_typo_case() {
    let findings = check_shape(&json!({ "hooks": { "SessionStrat": [] } }));
    assert!(findings
        .iter()
        .any(|f| f.check == "unknown-event" && f.severity == "P1"));
}

#[test]
fn a_real_event_name_is_not_flagged_as_unknown() {
    let findings = check_shape(
        &json!({ "hooks": { "PreToolUse": [], "SessionEnd": [], "SubagentStop": [] } }),
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.check == "unknown-event")
            .count(),
        0
    );
}

#[test]
fn a_matcher_block_missing_the_hooks_array_is_a_p1() {
    let findings = check_shape(&json!({ "hooks": { "Stop": [{ "matcher": "" }] } }));
    assert!(findings.iter().any(|f| f.check == "missing-hooks-array"));
}

#[test]
fn a_hook_entry_with_the_wrong_type_is_a_p1() {
    let findings = check_shape(
        &json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "prompt", "command": "echo hi" }] }] } }),
    );
    assert!(findings.iter().any(|f| f.check == "bad-hook-type"));
}

#[test]
fn a_hook_entry_with_no_command_string_is_a_p1() {
    let findings =
        check_shape(&json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command" }] }] } }));
    assert!(findings.iter().any(|f| f.check == "missing-command"));
}

#[test]
fn a_well_formed_config_produces_no_shape_findings() {
    let findings = check_shape(&json!({
        "hooks": { "SessionStart": [{ "matcher": "startup", "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/x.sh\"" }] }] },
    }));
    assert!(findings.is_empty(), "{}", debug(&findings));
}

// ---------------------------------------------------------------------------------
// path extraction / plugin-root usage
// ---------------------------------------------------------------------------------

#[test]
fn path_tokens_finds_a_claude_plugin_root_relative_path_inside_quotes() {
    let tokens = path_tokens("sh \"${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh\"");
    assert_eq!(tokens, vec!["${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh"]);
}

#[test]
fn path_tokens_does_not_treat_the_bare_command_name_as_a_path() {
    let tokens = path_tokens("sh \"${CLAUDE_PLUGIN_ROOT}/hooks/x.sh\"");
    assert!(!tokens.contains(&"sh".to_string()));
}

#[test]
fn a_relative_path_is_a_p1_not_a_silent_pass() {
    let root = fixture(&json!({}), &[]);
    fs::write(
        root.path().join("hooks").join("x.sh"),
        "#!/bin/sh\nexit 0\n",
    )
    .unwrap();
    let findings = check_path_usage(&[r("Stop", "sh \"hooks/x.sh\"")], &root_str(&root));
    assert!(findings
        .iter()
        .any(|f| f.check == "not-plugin-root-relative" && f.severity == "P1"));
}

#[test]
fn an_absolute_path_is_a_p1() {
    let findings = check_path_usage(&[r("Stop", "sh \"/usr/local/bin/x.sh\"")], "/nonexistent");
    assert!(findings
        .iter()
        .any(|f| f.check == "not-plugin-root-relative"));
}

#[test]
fn a_claude_plugin_root_path_that_does_not_exist_on_disk_is_a_p1() {
    let root = fixture(&json!({}), &[]);
    let findings = check_path_usage(
        &[r("Stop", "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/missing.sh\"")],
        &root_str(&root),
    );
    assert!(findings
        .iter()
        .any(|f| f.check == "missing-script" && f.severity == "P1"));
}

#[test]
fn a_claude_plugin_root_path_that_exists_is_not_flagged() {
    let root = fixture(&json!({}), &[("x.sh", "#!/bin/sh\nexit 0\n")]);
    let findings = check_path_usage(
        &[r("Stop", "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/x.sh\"")],
        &root_str(&root),
    );
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn a_ts_hook_invoked_with_bare_node_bypassing_run_sh_is_a_p1() {
    let root = fixture(&json!({}), &[("foo.ts", "console.log('x');\n")]);
    let findings = check_path_usage(
        &[r("Stop", "node \"${CLAUDE_PLUGIN_ROOT}/hooks/foo.ts\"")],
        &root_str(&root),
    );
    assert!(findings
        .iter()
        .any(|f| f.check == "ts-bypasses-run-sh" && f.severity == "P1"));
}

#[test]
fn a_ts_hook_invoked_through_run_sh_is_not_flagged_for_bypassing_it() {
    let root = fixture(
        &json!({}),
        &[
            ("foo.ts", "console.log('x');\n"),
            ("run.sh", "#!/bin/sh\nexec node \"$1\"\n"),
        ],
    );
    let findings = check_path_usage(
        &[r(
            "Stop",
            "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/run.sh\" \"${CLAUDE_PLUGIN_ROOT}/hooks/foo.ts\"",
        )],
        &root_str(&root),
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.check == "ts-bypasses-run-sh")
            .count(),
        0
    );
}

// ---------------------------------------------------------------------------------
// dry-run execution
// ---------------------------------------------------------------------------------

#[test]
fn a_hook_that_exits_non_zero_on_its_representative_event_is_a_p1() {
    let root = fixture(&json!({}), &[("bad.sh", "#!/bin/sh\nexit 3\n")]);
    let findings = dry_run_hooks(
        &[r("Stop", "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/bad.sh\"")],
        &root_str(&root),
    );
    assert!(findings
        .iter()
        .any(|f| f.check == "dry-run-nonzero-exit" && f.severity == "P1"));
}

#[test]
fn a_hook_that_exits_0_is_not_flagged_by_the_dry_run() {
    let root = fixture(
        &json!({}),
        &[("ok.sh", "#!/bin/sh\ncat >/dev/null\nexit 0\n")],
    );
    let findings = dry_run_hooks(
        &[r("Stop", "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/ok.sh\"")],
        &root_str(&root),
    );
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn a_sessionstart_hook_that_emits_malformed_json_is_a_p1_it_wedges_the_session() {
    let root = fixture(
        &json!({}),
        &[(
            "bad-start.sh",
            "#!/bin/sh\ncat >/dev/null\necho 'not json'\nexit 0\n",
        )],
    );
    let findings = dry_run_hooks(
        &[r(
            "SessionStart",
            "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/bad-start.sh\"",
        )],
        &root_str(&root),
    );
    assert!(findings
        .iter()
        .any(|f| f.check == "sessionstart-malformed-json" && f.severity == "P1"));
}

#[test]
fn a_sessionstart_hook_that_emits_valid_json_is_not_flagged() {
    let root = fixture(
        &json!({}),
        &[(
            "good-start.sh",
            "#!/bin/sh\ncat >/dev/null\necho '{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\"}}'\n",
        )],
    );
    let findings = dry_run_hooks(
        &[r(
            "SessionStart",
            "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/good-start.sh\"",
        )],
        &root_str(&root),
    );
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn a_sessionstart_hook_that_emits_no_stdout_is_a_p1() {
    let root = fixture(
        &json!({}),
        &[("silent-start.sh", "#!/bin/sh\ncat >/dev/null\nexit 0\n")],
    );
    let findings = dry_run_hooks(
        &[r(
            "SessionStart",
            "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/silent-start.sh\"",
        )],
        &root_str(&root),
    );
    assert!(findings
        .iter()
        .any(|f| f.check == "sessionstart-empty-stdout"));
}

// ---------------------------------------------------------------------------------
// orphan / opt-in scripts
// ---------------------------------------------------------------------------------

#[test]
fn documented_as_opt_in_requires_opt_in_and_the_basename_on_the_same_line() {
    assert!(documented_as_opt_in(
        "- **Opt-in** (shipped, not enabled): `hooks/post-tool-use-format.ts` does X.",
        "post-tool-use-format.ts"
    ));
    assert!(!documented_as_opt_in(
        "Some unrelated line about opt-in features.\nhooks/post-tool-use-format.ts is nice.",
        "post-tool-use-format.ts"
    ));
}

#[test]
fn a_hook_script_registered_nowhere_and_undocumented_is_a_p1_orphan() {
    let root = fixture(
        &json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/wired.sh\"" }] }] } }),
        &[
            ("wired.sh", "#!/bin/sh\nexit 0\n"),
            ("stray.ts", "console.log('x');\n"),
        ],
    );
    let config = read_config(&root);
    let hooks_dir = root.path().join("hooks").to_string_lossy().into_owned();
    let findings = check_orphan_scripts(&hooks_dir, &config, "no mention of it here\n");
    assert!(findings.iter().any(|f| f.check == "orphaned-hook-script"
        && f.location == "stray.ts"
        && f.severity == "P1"));
    assert!(!findings.iter().any(|f| f.location == "wired.sh"));
}

#[test]
fn a_script_registered_in_hooks_json_is_not_an_orphan() {
    let root = fixture(
        &json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/wired.sh\"" }] }] } }),
        &[("wired.sh", "#!/bin/sh\nexit 0\n")],
    );
    let config = read_config(&root);
    let hooks_dir = root.path().join("hooks").to_string_lossy().into_owned();
    let findings = check_orphan_scripts(&hooks_dir, &config, "");
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn a_script_documented_as_opt_in_but_unregistered_is_not_an_orphan() {
    let root = fixture(
        &json!({ "hooks": {} }),
        &[("optional.ts", "console.log('x');\n")],
    );
    let config = read_config(&root);
    let readme = "- **Opt-in** (shipped, not enabled): `hooks/optional.ts` does a thing.\n";
    let hooks_dir = root.path().join("hooks").to_string_lossy().into_owned();
    let findings = check_orphan_scripts(&hooks_dir, &config, readme);
    assert!(findings.is_empty(), "{}", debug(&findings));
}

#[test]
fn run_sh_itself_is_never_flagged_as_an_orphan() {
    let root = fixture(
        &json!({ "hooks": {} }),
        &[("run.sh", "#!/bin/sh\nexit 0\n")],
    );
    let config = read_config(&root);
    let hooks_dir = root.path().join("hooks").to_string_lossy().into_owned();
    let findings = check_orphan_scripts(&hooks_dir, &config, "");
    assert!(findings.is_empty(), "{}", debug(&findings));
}

// ---------------------------------------------------------------------------------
// extractCommands
// ---------------------------------------------------------------------------------

#[test]
fn extract_commands_pulls_every_command_across_every_event_and_matcher_block() {
    let refs = extract_commands(&json!({
        "hooks": {
            "SessionStart": [{ "matcher": "startup", "hooks": [{ "type": "command", "command": "a" }] }],
            "Stop": [{ "hooks": [{ "type": "command", "command": "b" }, { "type": "command", "command": "c" }] }],
        },
    }));
    let got: Vec<String> = refs
        .iter()
        .map(|r| format!("{}:{}", r.event, r.command))
        .collect();
    assert_eq!(got, vec!["SessionStart:a", "Stop:b", "Stop:c"]);
}

// ---------------------------------------------------------------------------------
// end-to-end checkHooks
// ---------------------------------------------------------------------------------

#[test]
fn a_missing_hooks_json_is_a_p1_not_a_silent_pass() {
    let root = tempfile::tempdir().unwrap();
    let readme = root.path().join("README.md").to_string_lossy().into_owned();
    let findings = check_hooks(&root_str(&root), &readme);
    assert!(findings
        .iter()
        .any(|f| f.check == "missing-hooks-json" && f.severity == "P1"));
}

#[test]
fn a_fully_wired_existing_well_behaved_hook_produces_zero_findings_end_to_end() {
    let root = fixture(
        &json!({ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/ok.sh\"" }] }] } }),
        &[("ok.sh", "#!/bin/sh\ncat >/dev/null\nexit 0\n")],
    );
    let readme = root.path().join("README.md").to_string_lossy().into_owned();
    let findings = check_hooks(&root_str(&root), &readme);
    assert!(findings.is_empty(), "{}", debug(&findings));
}
