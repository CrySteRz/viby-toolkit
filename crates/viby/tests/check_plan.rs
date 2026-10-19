//! Contract tests for the plan validator.
//!
//! Both halves. The rule that earns this script's existence is `unpartitioned-file`: it makes the
//! fan-out law's "you must be able to name the partition" checkable instead of aspirational.

use std::process::Command;

use viby::checks::check_plan::{check_plan, parse_tasks};

fn plan(lines: &[&str]) -> String {
    let mut all = vec!["# Plan", "", "## Tasks", ""];
    all.extend_from_slice(lines);
    all.push("");
    all.join("\n")
}

fn checks(text: &str) -> Vec<String> {
    check_plan(text).1.into_iter().map(|f| f.check).collect()
}

fn good() -> String {
    plan(&[
        "- [ ] T1 — add the parser · files: src/parse.ts · verify: npm test -- parse · deps: —",
        "- [ ] T2 — add the writer · files: src/write.ts · verify: npm test -- write · deps: —",
        "- [ ] T3 — wire them up · files: src/index.ts · verify: npm test · deps: T1, T2",
    ])
}

fn run(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-plan")
        .args(args)
        .output()
        .expect("spawn viby");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_well_formed_plan_parses_into_tasks_with_files_verify_and_deps() {
    let t = parse_tasks(&good());
    assert_eq!(t.len(), 3);
    assert_eq!(t[0].files, vec!["src/parse.ts"]);
    assert_eq!(t[2].verify, "npm test");
    assert_eq!(t[2].deps, vec!["T1", "T2"]);
    assert!(!t[0].done);
}

#[test]
fn a_well_formed_plan_produces_no_findings() {
    let got = checks(&good());
    assert!(
        got.is_empty(),
        "expected dispatchable, got {}",
        got.join(",")
    );
}

#[test]
fn a_completed_task_is_parsed_as_done() {
    let t = parse_tasks(&plan(&[
        "- [x] T1 — done thing · files: a.ts · verify: npm test · deps: —",
    ]));
    assert!(t[0].done);
}

#[test]
fn a_task_with_no_files_is_p1_it_cannot_be_partitioned() {
    let f = check_plan(&plan(&[
        "- [ ] T1 — do the thing · verify: npm test · deps: —",
    ]))
    .1;
    let hit = f.iter().find(|x| x.check == "no-files").expect("no-files");
    assert_eq!(hit.severity, "P1");
}

#[test]
fn a_task_with_no_verification_is_p1_done_would_be_a_feeling() {
    assert!(checks(&plan(&["- [ ] T1 — do it · files: a.ts · deps: —"]))
        .iter()
        .any(|c| c == "no-verify"));
}

#[test]
fn the_check_two_independent_tasks_owning_the_same_file_is_p1() {
    // This is the partition principles §3 demands before any parallel write, made mechanical.
    let f = check_plan(&plan(&[
        "- [ ] T1 — add a field · files: src/model.ts · verify: npm test · deps: —",
        "- [ ] T2 — rename a field · files: src/model.ts · verify: npm test · deps: —",
    ]))
    .1;
    let hit = f
        .iter()
        .find(|x| x.check == "unpartitioned-file")
        .unwrap_or_else(|| panic!("no unpartitioned-file in {f:?}"));
    assert_eq!(hit.severity, "P1");
    assert!(hit.problem.contains("src/model.ts"));
}

#[test]
fn the_same_two_tasks_ordered_by_a_dependency_are_fine() {
    // Shared ownership is only a problem when they could run at the same time.
    let f = checks(&plan(&[
        "- [ ] T1 — add a field · files: src/model.ts · verify: npm test · deps: —",
        "- [ ] T2 — rename it · files: src/model.ts · verify: npm test · deps: T1",
    ]));
    assert!(
        !f.iter().any(|c| c == "unpartitioned-file"),
        "an ordered pair is safe: {}",
        f.join(",")
    );
}

#[test]
fn a_transitive_dependency_also_counts_as_ordered() {
    let f = checks(&plan(&[
        "- [ ] T1 — first · files: src/a.ts · verify: t · deps: —",
        "- [ ] T2 — middle · files: src/b.ts · verify: t · deps: T1",
        "- [ ] T3 — last, touches a.ts again · files: src/a.ts · verify: t · deps: T2",
    ]));
    assert!(
        !f.iter().any(|c| c == "unpartitioned-file"),
        "T1 → T2 → T3 is an order: {}",
        f.join(",")
    );
}

#[test]
fn a_file_owned_by_three_or_more_tasks_is_flagged_as_a_structural_hub() {
    let f = check_plan(&plan(&[
        "- [ ] T1 — a · files: src/hub.ts · verify: t · deps: —",
        "- [ ] T2 — b · files: src/hub.ts · verify: t · deps: T1",
        "- [ ] T3 — c · files: src/hub.ts · verify: t · deps: T2",
    ]))
    .1;
    let hit = f.iter().find(|x| x.check == "hub-file").unwrap_or_else(|| {
        panic!(
            "no hub-file in {:?}",
            f.iter().map(|x| &x.check).collect::<Vec<_>>()
        )
    });
    assert!(hit.fix.contains("take the hub yourself"));
}

#[test]
fn a_dependency_cycle_is_p1_no_execution_order_exists() {
    let f = checks(&plan(&[
        "- [ ] T1 — a · files: a.ts · verify: t · deps: T2",
        "- [ ] T2 — b · files: b.ts · verify: t · deps: T1",
    ]));
    assert!(f.iter().any(|c| c == "dep-cycle"), "{}", f.join(","));
}

#[test]
fn a_dependency_on_a_task_that_does_not_exist_is_p1() {
    assert!(checks(&plan(&[
        "- [ ] T1 — a · files: a.ts · verify: t · deps: T9"
    ]))
    .iter()
    .any(|c| c == "unknown-dep"));
}

#[test]
fn a_plan_with_no_task_list_is_exit_2_territory_not_clean() {
    let (tasks, findings) = check_plan("# Plan\n\nWe will refactor the parser and then ship it.\n");
    assert_eq!(tasks.len(), 0);
    assert!(findings.is_empty());
}

#[test]
fn prose_that_merely_looks_like_a_checkbox_is_not_parsed_as_a_task() {
    // Must-not: ordinary markdown checklists in a document are not dispatchable tasks.
    let t = parse_tasks("- [ ] remember to tell the client\n- [x] coffee\n");
    assert_eq!(t.len(), 0, "needs an id and a title separator, got {t:?}");
}

#[test]
fn both_middle_dot_and_pipe_separate_fields_and_deps_none_is_empty() {
    let t = parse_tasks(&plan(&[
        "- [ ] T1 — a | files: a.ts | verify: npm test | deps: none",
    ]));
    assert!(t[0].deps.is_empty());
    assert_eq!(t[0].files, vec!["a.ts"]);
}

#[test]
fn cli_a_dispatchable_plan_exits_0_and_reports_how_many_are_ready_now() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("plan.md");
    std::fs::write(&p, good()).expect("write plan");
    let (status, stdout) = run(&[p.to_str().expect("utf8 path")]);
    assert_eq!(status, Some(0), "{stdout}");
    assert!(stdout.contains("2 ready now"), "{stdout}");
    assert!(stdout.contains("not that it is a good plan"), "{stdout}");
}

#[test]
fn cli_a_conflicting_plan_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("plan.md");
    std::fs::write(
        &p,
        plan(&[
            "- [ ] T1 — a · files: x.ts · verify: t · deps: —",
            "- [ ] T2 — b · files: x.ts · verify: t · deps: —",
        ]),
    )
    .expect("write plan");
    let (status, _) = run(&[p.to_str().expect("utf8 path"), "--quiet"]);
    assert_eq!(status, Some(1));
}

#[test]
fn p1_regression_a_title_beginning_verify_does_not_satisfy_the_verify_field() {
    // The FIELD regex was anchored on ^, so a task whose TITLE started with "Verify:" populated
    // task.verify from its own title and suppressed the no-verify finding — the check reported a
    // verification that did not exist.
    let p = plan(&["- [ ] T1 — Verify: end-to-end auth flow works · files: src/auth.ts · deps: —"]);
    assert_eq!(
        parse_tasks(&p)[0].verify,
        "",
        "a title is not a verification step"
    );
    assert!(
        checks(&p).iter().any(|c| c == "no-verify"),
        "no-verify must still fire"
    );
}

#[test]
fn p1_regression_case_variant_paths_are_the_same_file_on_a_case_insensitive_filesystem() {
    // Raw string keys made src/Auth.ts and src/auth.ts two entries, so two tasks writing the SAME
    // file were reported as disjoint — the one conclusion this check exists to refuse.
    let p = plan(&[
        "- [ ] T1 — add the guard · files: src/Auth.ts · verify: npm test · deps: —",
        "- [ ] T2 — add the session · files: ./src/auth.ts · verify: npm test · deps: —",
    ]);
    assert!(
        checks(&p).iter().any(|c| c == "unpartitioned-file"),
        "case/./ variants must collide"
    );
}
