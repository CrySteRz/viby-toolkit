//! Contract tests for the viby-toolkit test-quality scanner (`viby scan-test-quality`).
//!
//! Port of `tests/scanner.test.ts`. Each case is a synthetic test file written to a temp dir and
//! scanned. Both halves of the contract are pinned: snippets that MUST produce a given check, and
//! good-practice snippets that must produce NOTHING. A scanner that flags healthy tests gets
//! ignored, which costs more than it ever saves.
//!
//! assertion-roulette and magic-number checks are deliberately not implemented (see
//! scan_test_quality.rs) — cases that would have exercised them are pinned to expect nothing
//! instead.
//!
//! The snippet sources live in `fixtures/scanner/cases.json`, not in this file: this file sits
//! under `tests/`, so `--all` scans it, and strip-noncode does not blank Rust raw strings. The
//! expected checks stay here, next to each test.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use viby::checks::scan_test_quality::is_test_file;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../..")
}

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/scanner/cases.json")).expect("cases.json")
}

/// `source.replace(/^\n/, "")`.
fn body(source: &str) -> &str {
    source.strip_prefix('\n').unwrap_or(source)
}

struct Run {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_cli(args: &[&str], cwd: &Path) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("scan-test-quality")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn viby");
    Run {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// The set of checks reported, or `None` when the output was not valid JSON.
fn run_scanner(paths: &[&str]) -> (Option<BTreeSet<String>>, Run) {
    let mut args = vec!["--json"];
    args.extend(paths);
    let r = run_cli(&args, &root());
    if r.status == Some(2) {
        return (Some(BTreeSet::new()), r);
    }
    let checks = serde_json::from_str::<Value>(&r.stdout)
        .ok()
        .and_then(|data| {
            data["findings"].as_array().map(|f| {
                f.iter()
                    .filter_map(|x| x["check"].as_str().map(str::to_string))
                    .collect()
            })
        });
    (checks, r)
}

fn assert_case(name: &str, expected: &[&str]) {
    let data = fixtures();
    let case = data["cases"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["name"] == name))
        .unwrap_or_else(|| panic!("no fixture named {name:?}"));
    let dir = tempfile::Builder::new()
        .prefix("scanner-test-")
        .tempdir()
        .expect("tempdir");
    let file = dir
        .path()
        .join(case["filename"].as_str().expect("filename"));
    std::fs::write(&file, body(case["source"].as_str().expect("source"))).expect("write");
    let (checks, r) = run_scanner(&[file.to_str().expect("utf-8 path")]);
    assert!(
        checks.is_some(),
        "scanner did not emit valid JSON: {} {}",
        r.stdout.chars().take(200).collect::<String>(),
        r.stderr.chars().take(200).collect::<String>()
    );
    let got: Vec<String> = checks.unwrap_or_default().into_iter().collect();
    let want: BTreeSet<&str> = expected.iter().copied().collect();
    let want: Vec<String> = want.into_iter().map(str::to_string).collect();
    assert_eq!(got, want, "{name}: got {got:?}, want {want:?}");
}

/// Write the cross-file fixture `name` into a temp dir and scan its target; the checks, in order.
fn scan_in(name: &str) -> Vec<String> {
    let data = fixtures();
    let case = data["cross_file"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["name"] == name))
        .unwrap_or_else(|| panic!("no cross-file fixture named {name:?}"));
    let dir = tempfile::Builder::new()
        .prefix("xfile-")
        .tempdir()
        .expect("tempdir");
    for (rel, text) in case["files"].as_object().expect("files") {
        let full = dir.path().join(rel);
        std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        std::fs::write(&full, body(text.as_str().expect("body"))).expect("write");
    }
    let target = dir.path().join(case["target"].as_str().expect("target"));
    let r = run_cli(
        &["--json", target.to_str().expect("utf-8 path")],
        &std::env::current_dir().expect("cwd"),
    );
    let data: Value = serde_json::from_str(&r.stdout).expect("JSON output");
    data["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|f| f["check"].as_str().map(str::to_string))
        .collect()
}

// (path, is_test) — which files the scanner considers tests. Load-bearing: the
// checks are meaningless on production or support code.
const DETECTION_CASES: &[(&str, bool)] = &[
    ("test_auth.py", true),
    ("auth_test.py", true),
    ("src/auth.test.ts", true),
    ("src/auth.spec.tsx", true),
    ("pkg/auth_test.go", true),
    ("src/AuthTest.java", true),
    ("spec/auth_spec.rb", true),
    ("tests/user_flows.py", true), // unconventional name, but in tests/
    ("tests/utils/parse_test.go", true), // filename wins over the utils/ dir
    ("src/auth.py", false),
    ("src/authenticate.ts", false),
    ("tests/scripts/run_all.py", false), // support code under tests/
    ("tests/fixtures/sample.py", false),
    ("tests/__mocks__/api.ts", false),
    ("skills/test/scripts/scan.py", false), // the bug this pins: a tool, not a test
    ("testdata/golden.go", false),
];

#[test]
fn file_detection() {
    for (p, want) in DETECTION_CASES {
        assert_eq!(is_test_file(p), *want, "is_test_file({p})");
    }
}

#[test]
fn self_scan_of_tests_is_clean_not_merely_non_crashing() {
    // This previously accepted exit 1 — which means "findings were found" — while being named
    // "runs cleanly". It would have stayed green if a real defect appeared in this suite.
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let r = run_cli(&["--json", tests.to_str().expect("utf-8 path")], &root());
    assert_eq!(
        r.status,
        Some(0),
        "expected a clean scan (exit 0); exit {:?} means defects were found: {}",
        r.status,
        r.stdout.chars().take(400).collect::<String>()
    );
}

#[test]
fn all_on_the_repo_is_clean_0_or_nothing_to_scan_2_never_1() {
    // Same defect: exit 1 is a finding, not a pass. This repo dogfoods a clean suite.
    let r = run_cli(&["--all", "--quiet"], &root());
    assert!(
        r.status == Some(0) || r.status == Some(2),
        "expected 0 or 2, got {:?} — exit 1 means real findings: {}",
        r.status,
        r.stdout.chars().take(400).collect::<String>()
    );
}

// ---------------------------------------------------------------------------
// Cross-file delegation. A test whose assertions live in a base class or mixin in ANOTHER
// module was the largest single cause of false `no-assertion` — one pair of CPython files
// accounted for ~10% of all findings. These cases need two files on disk, so they can't use
// the single-file cases above.
// ---------------------------------------------------------------------------

#[test]
fn py_assertion_inherited_from_a_mixin_in_a_sibling_module_is_recognised() {
    let checks = scan_in("py: assertion inherited from a mixin in a sibling module is recognised");
    assert_eq!(
        checks,
        Vec::<String>::new(),
        "expected no findings, got {checks:?}"
    );
}

#[test]
fn py_mixin_reached_through_a_dotted_package_path_is_recognised() {
    let checks = scan_in("py: mixin reached through a dotted package path is recognised");
    assert_eq!(
        checks,
        Vec::<String>::new(),
        "expected no findings, got {checks:?}"
    );
}

#[test]
fn ts_assertion_inherited_from_a_relative_import_is_recognised() {
    let checks = scan_in("ts: assertion inherited from a relative import is recognised");
    assert_eq!(
        checks,
        Vec::<String>::new(),
        "expected no findings, got {checks:?}"
    );
}

#[test]
fn cross_file_delegation_does_not_excuse_a_helper_that_never_asserts() {
    let checks = scan_in("cross-file delegation does NOT excuse a helper that never asserts");
    assert_eq!(
        checks,
        vec!["no-assertion"],
        "a non-asserting inherited helper must not suppress the finding, got {checks:?}"
    );
}

#[test]
fn cross_file_names_only_count_via_a_receiver_not_a_bare_call() {
    let checks = scan_in("cross-file names only count via a receiver, not a bare call");
    // `render(...)` is a bare call to an unrelated name; the imported `check` must not
    // excuse it. Guards against the over-suppression that name-only matching caused.
    assert_eq!(checks, vec!["no-assertion"], "got {checks:?}");
}

#[test]
fn py_calling_a_locally_declared_abstract_method_suppresses_no_assertion() {
    let checks = scan_in("py: calling a locally-declared abstract method suppresses no-assertion");
    assert_eq!(
        checks,
        Vec::<String>::new(),
        "abstract-method delegation, got {checks:?}"
    );
}

// ---------------------------------------------------------------------------
// The single-file cases: (name, expected_checks) — expected is the exact set of check names
// the scanner must report. Empty = must be silent.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------- no-assertion
#[test]
fn py_test_with_no_assertion() {
    assert_case("py: test with no assertion", &["no-assertion"]);
}

#[test]
fn py_test_with_assertion_is_clean() {
    assert_case("py: test with assertion is clean", &[]);
}

#[test]
fn ts_it_with_no_expect() {
    assert_case("ts: it() with no expect", &["no-assertion"]);
}

// node:test + node:assert style. The scanner must recognise `assert.<method>(...)`,
// not just bare `assert x` / `assertEqual(...)` — this is the style the toolkit's own
// test suite uses, and missing it made the scanner flag its own tests.
#[test]
fn ts_node_assert_style_counts_as_an_assertion() {
    assert_case("ts: node:assert style counts as an assertion", &[]);
}

#[test]
fn ts_node_assert_deepstrictequal_counts_as_an_assertion() {
    assert_case(
        "ts: node:assert deepStrictEqual counts as an assertion",
        &[],
    );
}

#[test]
fn ts_it_with_expect_is_clean() {
    assert_case("ts: it() with expect is clean", &[]);
}

// `#` opens a comment in Python, shell and Ruby, but `#[ignore]` is a Rust attribute: treating
// it as a comment meant the `#[ignore]` pattern could never fire.
#[test]
fn rs_ignore_attribute_is_a_skipped_test() {
    assert_case(
        "rs: #[ignore] is a skipped test, not a comment",
        &["focused-or-skipped"],
    );
}

#[test]
fn py_comment_naming_ignore_is_still_a_comment() {
    assert_case("py: a # comment naming #[ignore] is still a comment", &[]);
}

#[test]
fn go_test_with_t_fatal_is_clean() {
    assert_case("go: test with t.Fatal is clean", &[]);
}

#[test]
fn ts_single_line_smoke_test_is_clean() {
    assert_case("ts: single-line smoke test is clean", &[]);
}

// ---------------------------------------------------------- tautology
#[test]
fn py_asserttrue_true() {
    assert_case("py: assertTrue(True)", &["tautology"]);
}

#[test]
fn ts_expect_true_tobe_true() {
    assert_case("ts: expect(true).toBe(true)", &["tautology"]);
}

#[test]
fn ts_expect_x_tobe_x_self_comparison() {
    assert_case("ts: expect(x).toBe(x) self-comparison", &["tautology"]);
}

#[test]
fn py_real_equality_assertion_is_clean() {
    assert_case("py: real equality assertion is clean", &[]);
}

// ---------------------------------------------------------- focused / skipped
#[test]
fn ts_describe_only_left_in() {
    assert_case("ts: describe.only left in", &["focused-or-skipped"]);
}

#[test]
fn ts_it_skip_left_in() {
    assert_case("ts: it.skip left in", &["focused-or-skipped"]);
}

#[test]
fn py_pytest_skip_mark() {
    assert_case("py: pytest skip mark", &["focused-or-skipped"]);
}

#[test]
fn go_t_skip() {
    assert_case("go: t.Skip", &["focused-or-skipped"]);
}

#[test]
fn ts_ordinary_describe_it_is_clean() {
    assert_case("ts: ordinary describe/it is clean", &[]);
}

// ---------------------------------------------------------- sleep-wait
#[test]
fn py_time_sleep_in_test() {
    assert_case("py: time.sleep in test", &["sleep-wait"]);
}

#[test]
fn ts_settimeout_promise_wait() {
    assert_case("ts: setTimeout promise wait", &["sleep-wait"]);
}

#[test]
fn ts_settimeout_with_fake_timers_is_clean() {
    assert_case("ts: setTimeout with fake timers is clean", &[]);
}

#[test]
fn py_polling_for_a_condition_is_clean() {
    assert_case("py: polling for a condition is clean", &[]);
}

// P0 regression: a stray triple quote in a NON-Python file used to open a fake
// triple-quoted region with no closing guard, blanking the rest of the file. A skipped
// test and a tautology both vanished and the scanner reported clean at exit 0.
#[test]
fn ts_a_stray_triple_quote_must_not_blank_the_rest_of_the_file() {
    assert_case(
        "ts: a stray triple quote must not blank the rest of the file",
        &["focused-or-skipped", "no-assertion", "tautology"],
    );
}

// The other half: in Python, triple quotes ARE delimiters and must still blank.
#[test]
fn py_a_real_docstring_is_still_blanked() {
    assert_case("py: a real docstring is still blanked", &[]);
}

// The multi-line handler form had NO coverage, despite its own comment calling it
// "far more common than the single-line ones".
#[test]
fn ts_multi_line_empty_catch_is_a_swallowed_error() {
    assert_case(
        "ts: multi-line empty catch is a swallowed error",
        &["swallowed-error"],
    );
}

// over-mocking boundary: exactly MOCK_DENSITY_MAX must NOT fire (4 > 4 is false).
#[test]
fn py_exactly_four_mocks_is_at_the_boundary_and_must_not_fire() {
    assert_case(
        "py: exactly four mocks is at the boundary and must not fire",
        &[],
    );
}

// ...and above the max it must still not fire when assertions outnumber the mocks.
#[test]
fn py_mock_heavy_but_assertion_richer_must_not_fire() {
    assert_case("py: mock-heavy but assertion-richer must not fire", &[]);
}

// Regression: a quote-free regex literal was scanned as code, so the pattern TEXT
// `/it.skip/` was reported as a focused test.
#[test]
fn ts_regex_literal_containing_it_skip_is_not_a_focused_test() {
    assert_case(
        "ts: regex literal containing it.skip is not a focused test",
        &[],
    );
}

#[test]
fn ts_division_is_not_mistaken_for_a_regex_literal() {
    assert_case("ts: division is not mistaken for a regex literal", &[]);
}

// ava's assertion family matched nothing at all before.
#[test]
fn ts_ava_t_is_counts_as_an_assertion() {
    assert_case("ts: ava t.is() counts as an assertion", &[]);
}

// A chain split across lines: no line holds `expect(`, so recognition depended on the
// matcher whitelist, which missed toStrictEqual. 9 of vite's 14 findings were this.
#[test]
fn ts_multi_line_expect_poll_chain_counts_as_an_assertion() {
    assert_case(
        "ts: multi-line expect.poll chain counts as an assertion",
        &[],
    );
}

// `try { x(); expect.unreachable() } catch {}` is a manual raise-assertion.
#[test]
fn ts_expect_unreachable_empty_catch_is_not_a_swallowed_error() {
    assert_case(
        "ts: expect.unreachable + empty catch is not a swallowed error",
        &[],
    );
}

// python equivalent: the `else: self.fail(...)` branch IS the assertion.
#[test]
fn py_except_pass_with_else_fail_is_a_raise_assertion_not_a_swallow() {
    assert_case(
        "py: except/pass with else/fail is a raise-assertion, not a swallow",
        &[],
    );
}

#[test]
fn py_draining_a_generator_to_stopiteration_is_not_a_swallowed_error() {
    assert_case(
        "py: draining a generator to StopIteration is not a swallowed error",
        &[],
    );
}

// A zero-delay timer is a deterministic tick flush, not an arbitrary wait.
#[test]
fn ts_zero_delay_settimeout_tick_flush_is_not_a_sleep_wait() {
    assert_case(
        "ts: zero-delay setTimeout tick-flush is not a sleep-wait",
        &[],
    );
}

#[test]
fn ts_a_real_delayed_wait_is_still_flagged() {
    assert_case("ts: a real delayed wait is still flagged", &["sleep-wait"]);
}

// `.mock.calls` INSPECTS a spy inside an assertion; counting it as mocking made a
// carefully-asserted single-spy test look over-mocked.
#[test]
fn ts_spy_introspection_is_not_mock_density() {
    assert_case("ts: spy introspection is not mock density", &[]);
}

// `check = self.check_match` — an alias whose own name says nothing about asserting.
#[test]
fn py_custom_named_alias_to_an_asserting_helper_is_recognised() {
    assert_case(
        "py: custom-named alias to an asserting helper is recognised",
        &[],
    );
}

// `eq = self.assertEqual` then `eq(a, b)` — the assertion-alias idiom. Without this,
// every test in suites that use it (CPython does, throughout) looks assertion-free.
#[test]
fn py_assertion_bound_to_a_local_alias_counts_as_an_assertion() {
    assert_case(
        "py: assertion bound to a local alias counts as an assertion",
        &[],
    );
}

// `self.fail(...)` is how you assert "we should not have reached here".
#[test]
fn py_self_fail_in_an_except_branch_counts_as_an_assertion() {
    assert_case(
        "py: self.fail in an except branch counts as an assertion",
        &[],
    );
}

// Transitive delegation: the helper the test calls does not assert directly, but the
// helper IT calls does. Resolved to a fixpoint.
#[test]
fn py_transitive_helper_chain_counts_as_delegation() {
    assert_case("py: transitive helper chain counts as delegation", &[]);
}

// Regression: a quote inside a regex literal used to open "string mode", blanking the
// rest of the line and erasing the real assertion after it.
#[test]
fn ts_regex_literal_containing_a_quote_does_not_erase_the_line() {
    assert_case(
        "ts: regex literal containing a quote does not erase the line",
        &[],
    );
}

#[test]
fn ts_regex_literal_with_a_quote_assertion_on_the_same_line() {
    assert_case(
        "ts: regex literal with a quote, assertion on the same line",
        &[],
    );
}

// Regression: delegation was keyed on the helper's NAME only, so a test calling an
// unrelated `cache.get(...)` was excused because some fixture class defined `get`.
#[test]
fn py_unrelated_local_name_does_not_excuse_a_missing_assertion() {
    assert_case(
        "py: unrelated local name does not excuse a missing assertion",
        &["no-assertion"],
    );
}

#[test]
fn py_delegating_to_a_helper_that_really_asserts_is_clean() {
    assert_case(
        "py: delegating to a helper that really asserts is clean",
        &[],
    );
}

// Regression: fake timers anywhere in the file used to suppress sleep-wait everywhere,
// hiding a genuine flaky wait in an unrelated suite.
#[test]
fn ts_fake_timers_in_one_suite_do_not_excuse_a_real_wait_in_another() {
    assert_case(
        "ts: fake timers in one suite do not excuse a real wait in another",
        &["sleep-wait"],
    );
}

#[test]
fn py_freeze_time_does_not_excuse_a_blocking_time_sleep() {
    assert_case(
        "py: freeze_time does not excuse a blocking time.sleep",
        &["sleep-wait"],
    );
}

#[test]
fn ts_template_literal_mentioning_time_sleep_is_not_flagged() {
    assert_case(
        "ts: template literal mentioning time.sleep is not flagged",
        &[],
    );
}

#[test]
fn ts_multi_line_template_literal_mentioning_banned_patterns_is_not_flagged() {
    assert_case(
        "ts: multi-line template literal mentioning banned patterns is not flagged",
        &[],
    );
}

// ---------------------------------------------------------- swallowed-error
#[test]
fn py_except_pass_in_test() {
    assert_case("py: except pass in test", &["swallowed-error", "tautology"]);
}

#[test]
fn ts_empty_catch_block() {
    assert_case("ts: empty catch block", &["swallowed-error"]);
}

#[test]
fn py_pytest_raises_is_clean() {
    assert_case("py: pytest.raises is clean", &[]);
}

// ---------------------------------------------------------- over-mocking
#[test]
fn py_mock_heavy_test() {
    assert_case("py: mock-heavy test", &["over-mocking"]);
}

#[test]
fn py_two_mocks_and_real_assertions_is_clean() {
    assert_case("py: two mocks and real assertions is clean", &[]);
}

// ------------------------------------------------- assertion-roulette (not implemented)
#[test]
fn py_many_bare_assertions_is_not_flagged_assertion_roulette_deliberately_removed() {
    assert_case(
        "py: many bare assertions is not flagged (assertion-roulette deliberately removed)",
        &[],
    );
}

#[test]
fn py_assertions_with_messages_are_clean() {
    assert_case("py: assertions with messages are clean", &[]);
}

#[test]
fn py_three_bare_assertions_is_under_threshold() {
    assert_case("py: three bare assertions is under threshold", &[]);
}

// ---------------------------------------------------------- non-test files
#[test]
fn non_test_file_is_not_scanned() {
    assert_case("non-test file is not scanned", &[]);
}

// ---------------------------------------------------------- comments
#[test]
fn commented_out_smell_is_ignored() {
    assert_case("commented-out smell is ignored", &[]);
}
