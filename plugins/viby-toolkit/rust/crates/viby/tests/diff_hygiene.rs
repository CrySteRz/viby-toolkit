//! Contract tests for the diff-hygiene checker (`viby check-diff-hygiene`), ported from
//! `tests/check-diff-hygiene.test.ts`.
//!
//! Both halves. The must-NOT half is the whole ballgame here: this runs on every change, so a rule
//! that fires on an ordinary small commit would be switched off within a day.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use viby::checks::check_diff_hygiene::{check_diff_hygiene, parse_diff};

/// Same fail-loud contract as the sibling fixture files: a silently-failed `git add` or
/// `git commit` leaves the fixture in a state that surfaces later as a confusing, unrelated
/// assertion failure. Fixtures must not inherit the developer's global git config either.
fn git(dir: &Path, args: &[&str]) {
    let r = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git runs");
    assert!(
        r.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&r.stderr).trim()
    );
}

/// Build a unified diff the way `git diff --unified=0` emits it.
fn diff_with_removed<A: AsRef<str>, R: AsRef<str>>(
    file: &str,
    added: &[A],
    removed: &[R],
) -> String {
    let mut lines = vec![
        format!("diff --git a/{file} b/{file}"),
        format!("--- a/{file}"),
        format!("+++ b/{file}"),
        format!("@@ -1,{} +1,{} @@", removed.len(), added.len()),
    ];
    lines.extend(removed.iter().map(|l| format!("-{}", l.as_ref())));
    lines.extend(added.iter().map(|l| format!("+{}", l.as_ref())));
    lines.push(String::new());
    lines.join("\n")
}

fn diff_of<A: AsRef<str>>(file: &str, added: &[A]) -> String {
    diff_with_removed::<A, &str>(file, added, &[])
}

fn checks(diff: &str) -> Vec<String> {
    check_diff_hygiene(diff)
        .findings
        .into_iter()
        .map(|f| f.check)
        .collect()
}

fn has(found: &[String], check: &str) -> bool {
    found.iter().any(|c| c == check)
}

/// A repo with one committed `a.ts`; the caller then dirties it.
fn committed_repo() -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("hyg-")
        .tempdir()
        .expect("tempdir");
    let p = dir.path();
    git(p, &["init", "-q"]);
    git(p, &["config", "user.email", "t@e.com"]);
    git(p, &["config", "user.name", "T"]);
    fs::write(p.join("a.ts"), "const a = 1;\n").expect("write");
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "init"]);
    dir
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let p = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-diff-hygiene")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("viby runs");
    (
        p.status.code(),
        String::from_utf8_lossy(&p.stdout).into_owned(),
    )
}

#[test]
fn the_parser_reports_the_added_lines_with_their_new_line_numbers() {
    let p = parse_diff(&diff_of("src/a.ts", &["const a = 1;", "const b = 2;"]));
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].file, "src/a.ts");
    assert_eq!(
        p[0].added.iter().map(|a| a.line).collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn a_merge_conflict_marker_is_p1() {
    let f = check_diff_hygiene(&diff_of(
        "src/a.ts",
        &["<<<<<<< HEAD", "const a = 1;", ">>>>>>> other"],
    ))
    .findings;
    let hit = f
        .iter()
        .find(|x| x.check == "conflict-marker")
        .expect("conflict-marker");
    assert_eq!(hit.severity, "P1");
}

#[test]
fn a_credential_shaped_line_is_p1() {
    for line in [
        "const key = 'AKIA3XQ7BZP2LMWV6KTD';", // hygiene:allow-secret
        "token = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';",
        "-----BEGIN RSA PRIVATE KEY-----",
    ] {
        assert!(
            has(&checks(&diff_of("src/a.ts", &[line])), "secret-shaped"),
            "should flag: {line}"
        );
    }
}

#[test]
fn an_ordinary_variable_named_key_or_token_is_not_flagged_as_a_secret() {
    let f = checks(&diff_of(
        "src/a.ts",
        &[
            "const apiKey = process.env.API_KEY;",
            "const token = await getToken();",
        ],
    ));
    assert!(
        !has(&f, "secret-shaped"),
        "must not fire on env reads: {f:?}"
    );
}

#[test]
fn added_debug_output_is_flagged_per_language() {
    assert!(has(
        &checks(&diff_of("src/a.ts", &["  console.log('here', x);"])),
        "debug-added"
    ));
    assert!(has(
        &checks(&diff_of("src/a.ts", &["  debugger;"])),
        "debug-added"
    ));
    assert!(has(
        &checks(&diff_of("app/m.py", &["    print(x)"])),
        "debug-added"
    ));
    assert!(has(
        &checks(&diff_of("m.go", &["\tfmt.Println(x)"])),
        "debug-added"
    ));
    assert!(has(
        &checks(&diff_of("m.rs", &["    dbg!(x);"])),
        "debug-added"
    ));
}

#[test]
fn real_logging_and_a_commented_out_print_are_not_debug_added() {
    let f = checks(&diff_of(
        "src/a.ts",
        &[
            "  logger.info({ orderId }, 'order settled');",
            "  // console.log('old')",
        ],
    ));
    assert!(
        !has(&f, "debug-added"),
        "structured logging is not debug output: {f:?}"
    );
}

#[test]
fn a_new_todo_is_p3_not_a_blocker() {
    let hit = check_diff_hygiene(&diff_of("src/a.ts", &["// TODO: handle the retry case"]))
        .findings
        .into_iter()
        .find(|x| x.check == "todo-added")
        .expect("todo-added");
    assert_eq!(hit.severity, "P3");
}

#[test]
fn commented_out_code_is_flagged_a_prose_comment_is_not() {
    assert!(has(
        &checks(&diff_of("src/a.ts", &["// doThing(arg);"])),
        "commented-out-code"
    ));
    assert!(
        !has(
            &checks(&diff_of(
                "src/a.ts",
                &["// this exists because the upstream API returns 200 on failure"]
            )),
            "commented-out-code"
        ),
        "explanatory comments are the good kind"
    );
}

#[test]
fn a_diff_over_1000_changed_lines_is_p1_unreviewable() {
    let many: Vec<String> = (0..1100).map(|i| format!("const x{i} = {i};")).collect();
    let hit = check_diff_hygiene(&diff_of("src/big.ts", &many))
        .findings
        .into_iter()
        .find(|x| x.check == "unreviewable-size")
        .expect("expected unreviewable-size");
    assert_eq!(hit.severity, "P1");
    assert!(
        hit.problem.contains("28%"),
        "the finding should carry the measured detection rate"
    );
}

#[test]
fn a_diff_between_400_and_1000_lines_is_p2_oversized_not_p1() {
    let many: Vec<String> = (0..500).map(|i| format!("const x{i} = {i};")).collect();
    let f = checks(&diff_of("src/big.ts", &many));
    assert!(has(&f, "oversized-diff"));
    assert!(!has(&f, "unreviewable-size"));
}

#[test]
fn an_ordinary_small_diff_with_a_test_is_completely_silent() {
    // The single most important test in this file: the normal case must produce nothing.
    let diff = diff_of(
        "src/pricing.ts",
        &[
            "export function total(items: Item[]) {",
            "  return items.reduce((s, i) => s + i.price, 0);",
            "}",
        ],
    ) + &diff_of(
        "src/pricing.test.ts",
        &[
            "test(\"total sums prices\", () => {",
            "  expect(total([{ price: 2 }])).toBe(2);",
            "});",
        ],
    );
    let f = checks(&diff);
    assert!(
        f.is_empty(),
        "a good small change must be silent, got {f:?}"
    );
}

#[test]
fn source_changed_with_no_test_touched_is_p2_above_the_tiny_change_threshold_only() {
    let big: Vec<String> = (0..30).map(|i| format!("  const step{i} = {i};")).collect();
    assert!(has(
        &checks(&diff_of("src/a.ts", &big)),
        "code-without-test"
    ));
    // A three-line change does not need its own test to be defensible.
    assert!(!has(
        &checks(&diff_of("src/a.ts", &["const a = 1;"])),
        "code-without-test"
    ));
}

#[test]
fn a_docs_only_or_config_only_change_does_not_demand_a_test() {
    let lines: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
    let f = checks(&diff_of("README.md", &lines));
    assert!(
        !has(&f, "code-without-test"),
        "prose needs no unit test: {f:?}"
    );
}

#[test]
fn formatting_churn_mixed_with_real_changes_is_flagged() {
    // The churn file's added lines match its removed lines except for whitespace.
    let body: Vec<String> = (0..10).map(|i| format!("const v{i} = {i};")).collect();
    let indented: Vec<String> = body.iter().map(|l| format!("    {l}")).collect();
    let churn = diff_with_removed("src/formatted.ts", &indented, &body);
    let real = diff_of(
        "src/logic.ts",
        &["export const rate = compute(base) * 1.2;"],
    );
    assert!(
        has(&checks(&(churn + &real)), "mixed-concerns"),
        "reformatting hides the real change"
    );
}

#[test]
fn a_formatting_only_diff_is_not_mixed_there_is_nothing_it_hides() {
    let body: Vec<String> = (0..10).map(|i| format!("const v{i} = {i};")).collect();
    let indented: Vec<String> = body.iter().map(|l| format!("    {l}")).collect();
    let f = checks(&diff_with_removed("src/formatted.ts", &indented, &body));
    assert!(
        !has(&f, "mixed-concerns"),
        "a pure reformat is a legitimate commit: {f:?}"
    );
}

#[test]
fn a_lockfile_moving_alone_is_flagged_with_its_manifest_it_is_not() {
    assert!(has(
        &checks(&diff_of(
            "package-lock.json",
            &["    \"resolved\": \"https://registry.npmjs.org/x/-/x-1.2.4.tgz\","]
        )),
        "lockfile-without-manifest"
    ));
    let with_manifest = diff_of("package-lock.json", &["    \"version\": \"1.2.4\","])
        + &diff_of("package.json", &["    \"x\": \"^1.2.4\","]);
    assert!(!has(&checks(&with_manifest), "lockfile-without-manifest"));
}

#[test]
fn cli_an_unresolvable_base_ref_is_exit_2_not_a_pass() {
    let dir = committed_repo();
    let (status, stdout) = run(dir.path(), &["--base", "v99-nope"]);
    assert_eq!(status, Some(2));
    assert!(stdout.contains("NOTHING was compared"), "{stdout}");
}

#[test]
fn cli_a_real_dirty_working_tree_is_audited_and_the_report_says_what_it_does_not_judge() {
    let dir = committed_repo();
    fs::write(
        dir.path().join("a.ts"),
        "const a = 1;\nconsole.log('debugging');\n",
    )
    .expect("write");
    let (status, stdout) = run(dir.path(), &[]);
    assert_eq!(status, Some(1), "{stdout}");
    assert!(stdout.contains("debug-added"), "{stdout}");
    assert!(stdout.contains("not whether it is correct"), "{stdout}");
}

#[test]
fn cli_an_untracked_new_files_credential_and_conflict_marker_are_reported() {
    let dir = committed_repo();
    fs::write(
        dir.path().join("leaked.ts"),
        "const key = \"AKIA3XQ7BZP2LMWV6KTD\";\nconsole.log(\"debugging\", key);\n<<<<<<< HEAD\n", // hygiene:allow-secret
    )
    .expect("write");
    let (status, stdout) = run(dir.path(), &[]);
    assert_eq!(status, Some(1), "{stdout}");
    assert!(
        stdout.contains("secret-shaped"),
        "an untracked file's credential must be reported: {stdout}"
    );
    assert!(
        stdout.contains("conflict-marker"),
        "an untracked file's conflict marker must be reported: {stdout}"
    );
}

#[test]
fn cli_a_gitignored_untracked_file_is_not_reported_even_with_the_same_content() {
    let dir = tempfile::Builder::new()
        .prefix("hyg-")
        .tempdir()
        .expect("tempdir");
    let p = dir.path();
    git(p, &["init", "-q"]);
    git(p, &["config", "user.email", "t@e.com"]);
    git(p, &["config", "user.name", "T"]);
    fs::write(p.join("a.ts"), "const a = 1;\n").expect("write");
    fs::write(p.join(".gitignore"), "ignored.ts\n").expect("write");
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "init"]);
    fs::write(
        p.join("ignored.ts"),
        "const key = \"AKIA3XQ7BZP2LMWV6KTD\";\n<<<<<<< HEAD\n", // hygiene:allow-secret
    )
    .expect("write");
    let (status, stdout) = run(p, &[]);
    assert_eq!(status, Some(2), "{stdout}");
    assert!(
        stdout.contains("no changes"),
        "an ignored file must never be scanned: {stdout}"
    );
}

#[test]
fn console_log_in_a_script_or_seed_file_is_output_not_debug() {
    // Measured on four real repositories: every one of the 20 `debug-added` findings was progress
    // output in a CLI, a migration script or a seed file. A script's stdout is its interface.
    for f in [
        "scripts/migrate-funds.mjs",
        "src/db/seed.ts",
        "bin/deploy.ts",
        "tools/codemod.js",
        "tasks/backfill.ts",
    ] {
        assert!(
            !has(
                &checks(&diff_of(
                    f,
                    &["  console.log(\"Renaming existing funds...\");"]
                )),
                "debug-added"
            ),
            "{f} is a script — its output is not debug"
        );
    }
}

#[test]
fn console_log_in_application_code_is_still_debug() {
    assert!(
        has(
            &checks(&diff_of(
                "src/app/checkout/page.tsx",
                &["  console.log('here', order);"]
            )),
            "debug-added"
        ),
        "the must-flag half of the same rule"
    );
}

#[test]
fn comment_shaped_rules_do_not_run_on_prose_files() {
    // Found dogfooding this on its own repo: `#` is a heading in Markdown, so `commented-out-code`
    // fired on six SKILL.md files at once. A TODO in a design doc is also normal writing.
    let md = diff_of(
        "docs/design.md",
        &[
            "## 3. Capture the behaviour",
            "# TODO: decide the grain",
            "// doThing(arg);",
        ],
    );
    let f = checks(&md);
    assert!(!has(&f, "commented-out-code"), "markdown is prose: {f:?}");
    assert!(!has(&f, "todo-added"), "a TODO in a doc is normal: {f:?}");
}

#[test]
fn but_a_credential_or_a_conflict_marker_matters_in_any_file_type() {
    assert!(has(
        &checks(&diff_of(
            "README.md",
            &["export AWS_KEY=AKIA3XQ7BZP2LMWV6KTD"]
        )), // hygiene:allow-secret
        "secret-shaped"
    ));
    assert!(has(
        &checks(&diff_of("config.yaml", &["<<<<<<< HEAD"])),
        "conflict-marker"
    ));
}

#[test]
fn p2_regression_one_removed_line_cannot_excuse_two_added_copies_of_it() {
    // pendingRemovals was not consumed, so a single removal satisfied unlimited added lines and a
    // file that genuinely duplicated a line was classified as pure formatting churn.
    let p = parse_diff(&diff_with_removed(
        "src/a.ts",
        &["const a = 1;", "const a = 1;"],
        &["const a  =  1;"],
    ));
    assert_eq!(
        p[0].whitespace_only, 1,
        "only the first added copy pairs with the one removal"
    );
}

#[test]
fn a_published_example_credential_is_not_flagged() {
    // AWS documents keys ending in EXAMPLE so they can be written down. This checker fired on its
    // own test fixtures once it learned to read untracked files; a P1 that cries wolf gets ignored
    // wholesale.
    let f = checks(&diff_of(
        "src/a.ts",
        &["const key = \"AKIAIOSFODNN7EXAMPLE\";"],
    ));
    assert!(
        !has(&f, "secret-shaped"),
        "example credential flagged: {f:?}"
    );
}

#[test]
fn but_a_realistic_key_of_the_same_shape_is_still_flagged() {
    let f = checks(&diff_of(
        "src/a.ts",
        &["const key = \"AKIA3XQ7BZP2LMWV6KTD\";"],
    )); // hygiene:allow-secret
    assert!(
        has(&f, "secret-shaped"),
        "the exclusion must not blind the rule"
    );
}

#[test]
fn an_explicit_inline_allowlist_comment_suppresses_the_secret_rule() {
    let f = checks(&diff_of(
        "src/a.ts",
        &["const k = \"AKIA3XQ7BZP2LMWV6KTD\"; // hygiene:allow-secret"],
    ));
    assert!(
        !has(&f, "secret-shaped"),
        "an explicit allow marker must suppress"
    );
    let g = checks(&diff_of(
        "src/a.ts",
        &["const k = \"AKIA3XQ7BZP2LMWV6KTD\";"],
    )); // hygiene:allow-secret
    assert!(
        has(&g, "secret-shaped"),
        "and the rule must still fire without it"
    );
}

#[test]
fn python_an_interactive_breakpoint_is_a_debug_artifact() {
    for line in [
        "    breakpoint()",
        "    import pdb; pdb.set_trace()",
        "    ipdb.set_trace()",
    ] {
        assert!(
            has(&checks(&diff_of("app/x.py", &[line])), "debug-added"),
            "should flag: {line}"
        );
    }
}

#[test]
fn python_a_visibly_debugging_print_is_flagged() {
    for line in [
        "    print(f\"{total=}\")",
        "    print(\"here\")",
        "    print(\"XXXX\")",
    ] {
        assert!(
            has(&checks(&diff_of("app/x.py", &[line])), "debug-added"),
            "should flag: {line}"
        );
    }
}

#[test]
fn python_a_report_print_is_not_a_debug_artifact() {
    // Regression: every print( in a .py file was flagged, which put 23 findings on a scorer whose
    // entire job is printing a table. A P2 that fires 23 times on correct code gets ignored
    // wholesale.
    for line in [
        "    print(f\"{pid:<18}{n:>4}   {tokens:>10}   {cost:>10}\")",
        "    print(\"probe                reps  hit  none\")",
        "    print(",
        "    print(f\"  {label}: {lo:.0%}-{hi:.0%}\")",
        "    print(f\"!! no runs found under {name!r}, skipping comparison\")",
        "    print(\"!! 1 run(s) had not finished and were EXCLUDED\")",
    ] {
        assert!(
            !has(
                &checks(&diff_of("tests/routing/score.py", &[line])),
                "debug-added"
            ),
            "should NOT flag: {line}"
        );
    }
}
