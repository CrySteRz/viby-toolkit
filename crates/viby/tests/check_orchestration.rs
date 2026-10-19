//! Contract tests for the delegation-defaults checker.
//!
//! Both halves pinned, as always: what must be flagged, and — the half that keeps a checker
//! trustworthy — what must NOT be. Several negative cases are false positives this checker actually
//! produced against the real library before being narrowed, including one whole rule that had to be
//! replaced when the library moved to the hybrid.
//!
//! Port of `tests/check-orchestration.test.ts`.

use std::fs;
use std::path::PathBuf;

use viby::checks::check_orchestration::{check_orchestration, check_staged_skills_exist};

fn checks(skills: &[(&str, &str)]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in skills {
        fs::create_dir_all(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join("SKILL.md"), body).unwrap();
    }
    check_orchestration(&dir.path().to_string_lossy())
        .iter()
        .map(|f| format!("{}:{}", f.skill, f.check))
        .collect()
}

fn none() -> Vec<String> {
    Vec::new()
}

#[test]
fn fan_out_phrased_as_a_permission_is_flagged() {
    let found = checks(&[(
        "explore",
        "You may dispatch several `scout` agents in parallel if it helps.\n",
    )]);
    assert_eq!(found, vec!["explore:hedged-fanout"]);
}

#[test]
fn optionally_on_a_fan_out_line_is_flagged() {
    let found = checks(&[("explore", "Optionally, run one `reviewer` per dimension.\n")]);
    assert_eq!(found, vec!["explore:hedged-fanout"]);
}

#[test]
fn fan_out_phrased_as_a_directive_is_not_flagged() {
    let found = checks(&[(
        "explore",
        "Dispatch several `scout` agents in parallel, in one message.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_single_stage_skill_need_not_name_workflow_this_is_the_hybrid() {
    // Regression against the rule this checker used to have. Requiring Workflow everywhere flagged
    // explore/debug/plan/migrate/adopt, none of which has stages to declare.
    let found = checks(&[(
        "debug",
        "Dispatch several `debugger` agents in parallel, in one message.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_staged_skill_that_never_names_workflow_is_flagged() {
    let found = checks(&[(
        "review",
        "Run one `reviewer` per dimension, then one `skeptic` per finding.\n",
    )]);
    assert_eq!(found, vec!["review:unstaged-pipeline"]);
}

#[test]
fn a_staged_skill_that_names_workflow_is_not_flagged() {
    let found = checks(&[(
        "review",
        "Author a `Workflow`: one `reviewer` per dimension, then one `skeptic` per finding.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn all_three_staged_skills_are_covered_by_the_rule() {
    let bare = "Run one `reviewer` per dimension.\n";
    let found = checks(&[("orchestrate", bare), ("study", bare), ("review", bare)]);
    let mut unstaged: Vec<String> = found
        .into_iter()
        .filter(|f| f.ends_with("unstaged-pipeline"))
        .collect();
    unstaged.sort();
    assert_eq!(
        unstaged,
        vec![
            "orchestrate:unstaged-pipeline",
            "review:unstaged-pipeline",
            "study:unstaged-pipeline",
        ]
    );
}

#[test]
fn a_retrospective_mention_of_fan_out_with_no_named_agent_is_not_flagged() {
    let found = checks(&[(
        "extend",
        "It described a web-search fan-out when every agent was filesystem-only.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn dispatching_a_single_agent_is_not_a_fan_out() {
    let found = checks(&[(
        "evaluate",
        "Dispatch a `researcher` rather than reading repos inline.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_fenced_example_script_does_not_count_as_an_instruction() {
    let found = checks(&[(
        "demo",
        "Read inline.\n\n```js\nparallel(D.map(d => agent(d, {agentType: 'viby-toolkit:reviewer'})))\n```\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn parallelising_an_implementer_without_worktree_isolation_is_flagged() {
    let found = checks(&[(
        "build",
        "Dispatch several `implementer` agents in parallel, one per node.\n",
    )]);
    assert_eq!(found, vec!["build:unisolated-parallel-write"]);
}

#[test]
fn parallelising_an_implementer_with_worktree_isolation_is_not_flagged() {
    let found = checks(&[(
        "build",
        "Dispatch several `implementer` agents in parallel, one per node, each in its own worktree.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn parallelising_the_worker_it_keeps_edit_write_without_worktree_isolation_is_flagged() {
    let found = checks(&[(
        "build",
        "Dispatch several `worker` agents in parallel, one per module.\n",
    )]);
    assert_eq!(found, vec!["build:unisolated-parallel-write"]);
}

#[test]
fn parallelising_the_worker_with_worktree_isolation_is_not_flagged() {
    let found = checks(&[(
        "build",
        "Dispatch several `worker` agents in parallel, one per module, each in its own worktree.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn the_plain_english_word_workers_beside_a_read_only_fan_out_is_not_a_writing_agent() {
    let found = checks(&[(
        "explore",
        "Dispatch several `scout` agents in parallel; the workers only read.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn principles_itself_is_exempt_it_defines_the_laws_rather_than_consuming_them() {
    let found = checks(&[(
        "principles",
        "You may dispatch several `scout` agents in parallel.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_directory_with_no_skill_md_is_skipped_rather_than_crashing() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("empty")).unwrap();
    assert!(check_orchestration(&dir.path().to_string_lossy()).is_empty());
}

#[test]
fn a_bare_workflow_inside_a_url_does_not_satisfy_a_staged_skill() {
    // Regression: the staged check tested the RAW file while every other rule used stripped prose, so
    // any occurrence of the word — a link, a path — hollowly satisfied it.
    let found = checks(&[(
        "study",
        "Run one `researcher` per angle.\n\nSee https://example.com/Workflow-diagram for background.\n",
    )]);
    assert_eq!(found, vec!["study:unstaged-pipeline"]);
}

#[test]
fn a_workflow_mentioned_only_inside_a_fenced_code_block_does_not_satisfy_a_staged_skill() {
    let found = checks(&[(
        "study",
        "Run one `researcher` per angle.\n\n```js\n// no Workflow needed here\n```\n",
    )]);
    assert_eq!(found, vec!["study:unstaged-pipeline"]);
}

#[test]
fn a_backticked_workflow_in_prose_does_satisfy_a_staged_skill() {
    let found = checks(&[(
        "study",
        "Author a `Workflow` that runs one `researcher` per angle.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_hedged_fan_out_is_caught_even_when_the_agent_name_is_not_backticked() {
    // Regression: the hedge rule only scanned lines with a BACKTICKED agent, so formatting alone
    // exempted a softened directive from detection.
    let found = checks(&[(
        "explore",
        "You may optionally fan out scout agents in parallel if you want more coverage.\n",
    )]);
    assert_eq!(found, vec!["explore:hedged-fanout"]);
}

#[test]
fn loosening_the_agent_match_did_not_start_flagging_retrospective_asides() {
    let found = checks(&[(
        "extend",
        "It described a web-search fan-out when every agent was filesystem-only.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn an_unrelated_worktree_elsewhere_in_the_file_does_not_suppress_an_unisolated_parallel_write() {
    // Regression: isolation was matched against the whole document, so any stray mention counted.
    let found = checks(&[(
        "build",
        "Dispatch several `implementer` agents in parallel, one per node.\n\
         \n\n\n\n\n\nUnrelated aside: worktree isolation is discussed elsewhere.\n",
    )]);
    assert_eq!(found, vec!["build:unisolated-parallel-write"]);
}

#[test]
fn worktree_isolation_alongside_the_instruction_does_suppress_it() {
    let found = checks(&[(
        "build",
        "Dispatch several `implementer` agents in parallel, one per node.\nEach runs in its own worktree.\n",
    )]);
    assert_eq!(found, none());
}

#[test]
fn a_staged_name_with_no_matching_skill_directory_fails_loudly() {
    // `review-cluster` was renamed to `review` mid-development; a stale entry must not silently
    // retire the rule.
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("orchestrate")).unwrap();
    fs::write(
        dir.path().join("orchestrate").join("SKILL.md"),
        "Author a `Workflow`.\n",
    )
    .unwrap();
    let found: Vec<String> = check_staged_skills_exist(&dir.path().to_string_lossy())
        .iter()
        .map(|f| format!("{}:{}", f.skill, f.check))
        .collect();
    assert!(
        found.contains(&"review:staged-skill-missing".to_string()),
        "expected a missing-skill finding, got {}",
        found.join(", ")
    );
    assert!(found.contains(&"study:staged-skill-missing".to_string()));
}

#[test]
fn the_real_skill_library_passes_its_own_rules() {
    let real: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/viby-toolkit/skills");
    let found = check_orchestration(&real.to_string_lossy());
    assert!(
        found.is_empty(),
        "library regressed: {}",
        found
            .iter()
            .map(|f| format!("{}:{}", f.skill, f.check))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
