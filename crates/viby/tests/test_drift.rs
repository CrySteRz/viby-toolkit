//! Contract tests for the safety-net differ (`viby check-test-drift`), ported from
//! `tests/check-test-drift.test.ts`.
//!
//! Both halves pinned. The must-NOT half is the one that decides whether anyone leaves this
//! checker switched on: adding tests, splitting a test in two, reformatting, and moving a file
//! must all be silent, because those are what a good refactor looks like.

use std::fs;
use std::path::Path;
use std::process::Command;

use regex::Regex;
use tempfile::TempDir;
use viby::checks::check_test_drift::{check_test_drift, is_test_file, stats_for};

/// Fixtures must not inherit the developer's global git config. A global `tag.gpgSign = true`
/// turns `git tag <name>` into a signed annotated tag, which fails with "no tag message?" in a
/// non-interactive run, and a global `commit.gpgSign` would break the fixture commit. Identity is
/// set locally in each fixture repo.
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

fn write(dir: &Path, rel: &str, body: &str) {
    let full = dir.join(rel);
    fs::create_dir_all(full.parent().expect("has parent")).expect("mkdir");
    fs::write(full, body).expect("write");
}

/// A repo with `before` committed, then `after` applied (`None` deletes the file).
fn repo(before: &[(&str, &str)], after: &[(&str, Option<&str>)]) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("drift-")
        .tempdir()
        .expect("tempdir");
    let p = dir.path();
    git(p, &["init", "-q"]);
    git(p, &["config", "user.email", "t@example.com"]);
    git(p, &["config", "user.name", "T"]);
    for (rel, body) in before {
        write(p, rel, body);
    }
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "base"]);
    for (rel, body) in after {
        match body {
            None => {
                let _ = fs::remove_file(p.join(rel));
            }
            Some(b) => write(p, rel, b),
        }
    }
    dir
}

fn path(dir: &TempDir) -> String {
    dir.path().to_string_lossy().into_owned()
}

fn drift(before: &[(&str, &str)], after: &[(&str, Option<&str>)]) -> Vec<String> {
    let dir = repo(before, after);
    let d = path(&dir);
    check_test_drift(&d, "HEAD", std::slice::from_ref(&d))
        .findings
        .into_iter()
        .map(|f| f.check)
        .collect()
}

fn has(found: &[String], check: &str) -> bool {
    found.iter().any(|c| c == check)
}

const TWO_TESTS: &str = concat!(
    "test(\"adds\", () => {\n",
    "  expect(add(1, 2)).toBe(3);\n",
    "});\n",
    "test(\"subtracts\", () => {\n",
    "  expect(sub(3, 1)).toBe(2);\n",
    "});\n",
);

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let p = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-test-drift")
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
fn deleting_a_test_file_that_took_coverage_with_it_is_p1() {
    let f = drift(&[("a.test.ts", TWO_TESTS)], &[("a.test.ts", None)]);
    assert!(has(&f, "test-file-deleted"), "{f:?}");
}

#[test]
fn moving_a_test_file_is_not_reported_as_deleted_coverage() {
    // The suite-wide count is unchanged, so this is a rename. Reporting it as lost coverage on
    // every reorganisation is how a checker gets switched off.
    let f = drift(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", None), ("tests/a.test.ts", Some(TWO_TESTS))],
    );
    assert!(
        !has(&f, "test-file-deleted"),
        "a move must not be a deletion: {f:?}"
    );
    assert!(!has(&f, "suite-assertions-down"), "{f:?}");
}

#[test]
fn removing_one_test_case_from_a_file_is_p1() {
    let f = drift(
        &[("a.test.ts", TWO_TESTS)],
        &[(
            "a.test.ts",
            Some("test(\"adds\", () => {\n  expect(add(1, 2)).toBe(3);\n});\n"),
        )],
    );
    assert!(has(&f, "tests-removed"), "{f:?}");
}

#[test]
fn removing_assertions_while_keeping_the_test_is_p1_the_worst_case_because_it_looks_like_cover() {
    let weakened = [
        "test(\"adds\", () => {",
        "  add(1, 2);",
        "});",
        "test(\"subtracts\", () => {",
        "  sub(3, 1);",
        "});",
        "",
    ]
    .join("\n");
    let f = drift(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", Some(&weakened))],
    );
    assert!(has(&f, "assertions-removed"), "{f:?}");
}

#[test]
fn adding_a_skip_is_p1() {
    let skipped = TWO_TESTS.replace("test(\"subtracts\"", "test.skip(\"subtracts\"");
    let f = drift(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", Some(&skipped))],
    );
    assert!(has(&f, "skips-added"), "{f:?}");
}

#[test]
fn adding_only_is_p2_it_silences_the_rest_of_the_file() {
    let focused = TWO_TESTS.replace("test(\"adds\"", "test.only(\"adds\"");
    let dir = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", Some(&focused))],
    );
    let d = path(&dir);
    let hit = check_test_drift(&d, "HEAD", std::slice::from_ref(&d))
        .findings
        .into_iter()
        .find(|f| f.check == "focus-added")
        .expect("expected focus-added");
    assert_eq!(hit.severity, "P2");
}

#[test]
fn inserting_a_zero_status_exit_into_the_suite_is_p1() {
    // The documented harness escape: leave the runner successful without running anything.
    let f = drift(
        &[("test_a.py", "def test_adds():\n    assert add(1, 2) == 3\n")],
        &[(
            "test_a.py",
            Some("import sys\nsys.exit(0)\n\ndef test_adds():\n    assert add(1, 2) == 3\n"),
        )],
    );
    assert!(has(&f, "early-exit-added"), "{f:?}");
}

#[test]
fn adding_tests_and_assertions_is_silent() {
    let grown =
        format!("{TWO_TESTS}test(\"multiplies\", () => {{\n  expect(mul(2, 3)).toBe(6);\n}});\n");
    let f = drift(&[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(&grown))]);
    assert!(f.is_empty(), "a growing suite must be silent, got {f:?}");
}

#[test]
fn splitting_one_test_into_two_keeping_every_assertion_is_silent() {
    let f = drift(
        &[(
            "a.test.ts",
            "test(\"maths\", () => {\n  expect(add(1, 2)).toBe(3);\n  expect(sub(3, 1)).toBe(2);\n});\n",
        )],
        &[("a.test.ts", Some(TWO_TESTS))],
    );
    assert!(f.is_empty(), "a split is a refactor, not a loss: {f:?}");
}

#[test]
fn reformatting_without_changing_counts_is_silent() {
    let spaced = TWO_TESTS.replace('\n', "\n\n");
    let f = drift(&[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(&spaced))]);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn changes_to_non_test_files_are_ignored_entirely() {
    let f = drift(
        &[
            ("a.test.ts", TWO_TESTS),
            (
                "src/a.ts",
                "export const add = (a: number, b: number) => a + b;\n",
            ),
        ],
        &[(
            "src/a.ts",
            Some("export const add = (a: number, b: number) => b + a;\n"),
        )],
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn an_assertion_inside_a_string_fixture_or_comment_is_not_counted() {
    // Decide on code, never raw text — the rule this repo has re-learned four times.
    let with_fixture = [
        "const FIXTURE = `",
        "  test(\"ghost\", () => { expect(1).toBe(1); });",
        "`;",
        "// expect(this).toBe(ignored)",
        "test(\"real\", () => {",
        "  expect(add(1, 2)).toBe(3);",
        "});",
        "",
    ]
    .join("\n");
    let s = stats_for("a.test.ts", &with_fixture);
    assert_eq!(s.tests, 1, "only the real test counts");
    assert_eq!(s.assertions, 1, "only the real assertion counts");
}

#[test]
fn python_unittest_assertions_are_counted() {
    let s = stats_for(
        "test_x.py",
        "class T(TestCase):\n    def test_a(self):\n        self.assertEqual(1, 1)\n        self.assertTrue(True)\n",
    );
    assert_eq!(s.tests, 1);
    assert_eq!(s.assertions, 2);
}

#[test]
fn each_assert_shape_is_counted_exactly_once() {
    // Regression: overlapping patterns counted `self.assertEqual(...)` twice. A miscount that
    // differs between two refs invents drift that isn't there, or hides drift that is.
    assert_eq!(
        stats_for("test_a.py", "def test_a():\n    assert add(1, 2) == 3\n").assertions,
        1,
        "bare python assert"
    );
    assert_eq!(
        stats_for("test_a.py", "def test_a():\n    self.assertEqual(1, 1)\n").assertions,
        1,
        "unittest member assert"
    );
    assert_eq!(
        stats_for(
            "a_test.go",
            "func TestA(t *testing.T) {\n\tassert.Equal(t, 1, 1)\n}\n"
        )
        .assertions,
        1,
        "testify"
    );
    assert_eq!(
        stats_for(
            "lib_test.rs",
            "#[test]\nfn a() {\n    assert!(x);\n    assert_eq!(1, 1);\n}\n"
        )
        .assertions,
        2,
        "rust macros"
    );
    assert_eq!(
        stats_for(
            "a.test.ts",
            "test(\"a\", () => {\n  expect(1).toBe(1);\n});\n"
        )
        .assertions,
        1,
        "jest expect"
    );
}

#[test]
fn go_table_tests_and_t_fatal_are_counted() {
    let s = stats_for(
        "x_test.go",
        "func TestAdd(t *testing.T) {\n\tif add(1,2) != 3 {\n\t\tt.Fatalf(\"bad\")\n\t}\n}\n",
    );
    assert_eq!(s.tests, 1);
    assert_eq!(s.assertions, 1);
}

#[test]
fn rust_test_and_assert_eq_are_counted_and_ignore_reads_as_a_skip() {
    let s = stats_for(
        "lib_test.rs",
        "#[test]\n#[ignore]\nfn adds() {\n    assert_eq!(add(1,2), 3);\n}\n",
    );
    assert_eq!(s.tests, 1);
    assert_eq!(s.assertions, 1);
    assert_eq!(s.skips, 1);
}

#[test]
fn a_variable_named_test_is_not_a_test_declaration() {
    let s = stats_for("a.test.ts", "const test = 1;\nconst other = test + 1;\n");
    assert_eq!(s.tests, 0, "only call heads count");
}

#[test]
fn test_file_detection_covers_the_common_layouts_and_excludes_source_files() {
    for p in [
        "a.test.ts",
        "src/b.spec.js",
        "tests/c.ts",
        "test_d.py",
        "e_test.go",
        "FooTest.java",
        "__tests__/g.tsx",
    ] {
        assert!(is_test_file(p), "{p} should be a test file");
    }
    for p in ["src/index.ts", "README.md", "latest.ts", "contest.py"] {
        assert!(!is_test_file(p), "{p} must NOT be treated as a test file");
    }
}

#[test]
fn a_non_git_directory_returns_no_findings_rather_than_inventing_them() {
    let dir = tempfile::Builder::new()
        .prefix("nogit-drift-")
        .tempdir()
        .expect("tempdir");
    write(dir.path(), "a.test.ts", TWO_TESTS);
    let d = path(&dir);
    let r = check_test_drift(&d, "HEAD", std::slice::from_ref(&d));
    assert!(r.findings.is_empty());
    assert_eq!(r.compared, 0);
}

#[test]
fn an_unresolvable_base_ref_is_exit_2_not_a_clean_pass() {
    // Regression, caught by running the tool for real: a tag that did not exist yielded an empty
    // baseline, so the report read "0 → 198 tests, the safety net grew" — a perfect score for a
    // typo. A comparison that never happened must never read as a pass.
    let dir = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", Some(TWO_TESTS))],
    );
    let d = path(&dir);
    let r = check_test_drift(&d, "v99.0.0-does-not-exist", std::slice::from_ref(&d));
    assert!(!r.base_resolved);
    assert_eq!(r.before.tests, 0, "nothing was read at base");

    let (status, stdout) = run(dir.path(), &[".", "--base", "v99.0.0-does-not-exist"]);
    assert_eq!(
        status,
        Some(2),
        "must be nothing-to-check, not success: {stdout}"
    );
    assert!(stdout.contains("NOTHING was compared"), "{stdout}");
}

#[test]
fn cli_a_weakened_suite_exits_1_an_intact_one_exits_0() {
    let weakened = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[(
            "a.test.ts",
            Some("test(\"adds\", () => {\n  expect(add(1,2)).toBe(3);\n});\n"),
        )],
    );
    assert_eq!(
        run(weakened.path(), &[".", "--quiet"]).0,
        Some(1),
        "a shrunken suite must fail"
    );

    let grown = format!("{TWO_TESTS}test(\"more\", () => {{\n  expect(1).toBe(1);\n}});\n");
    let intact = repo(&[("a.test.ts", TWO_TESTS)], &[("a.test.ts", Some(&grown))]);
    assert_eq!(
        run(intact.path(), &[".", "--quiet"]).0,
        Some(0),
        "a grown suite must pass"
    );
}

#[test]
fn cli_the_report_states_that_counts_are_not_proof_of_preserved_behaviour() {
    let dir = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", Some(TWO_TESTS))],
    );
    let (_, stdout) = run(dir.path(), &["."]);
    assert!(
        Regex::new(r"never that behaviour was\s+preserved")
            .expect("pattern")
            .is_match(&stdout),
        "{stdout}"
    );
    assert!(stdout.contains("weakened in place"), "{stdout}");
}

#[test]
fn a_moved_test_file_is_p3_when_the_suite_grew_on_both_counts_p2_otherwise() {
    // Measured on a real 30-commit range: 4 renames produced 4 P2 findings, none of them a
    // problem. "The net is bigger than it was" is strong evidence nothing was lost.
    let extra = format!("{TWO_TESTS}test(\"extra\", () => {{\n  expect(1).toBe(1);\n}});\n");
    let grown = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", None), ("tests/a.test.ts", Some(&extra))],
    );
    let g = path(&grown);
    let hit = check_test_drift(&g, "HEAD", std::slice::from_ref(&g))
        .findings
        .into_iter()
        .find(|f| f.check == "test-file-moved")
        .expect("the move is still reported");
    assert_eq!(hit.severity, "P3", "but at P3, because the suite grew");

    let flat = repo(
        &[("a.test.ts", TWO_TESTS)],
        &[("a.test.ts", None), ("tests/a.test.ts", Some(TWO_TESTS))],
    );
    let f = path(&flat);
    let hit = check_test_drift(&f, "HEAD", std::slice::from_ref(&f))
        .findings
        .into_iter()
        .find(|f| f.check == "test-file-moved");
    assert_eq!(
        hit.map(|h| h.severity),
        Some("P2"),
        "a move with no growth stays P2 — confirm it really moved"
    );
}
