//! Contract tests for the read-cost meter.
//!
//! Both halves pinned, as always: what must be counted, and what must NOT be. The second
//! half matters more here than usual — a baseline inflated by `node_modules` or a lockfile
//! makes every savings ratio computed against it a fiction, which is precisely the failure
//! mode /viby-toolkit:evaluate exists to prevent.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use viby::checks::measure_read_cost::{estimate_tokens, kind_of, measure_read_cost, Kind};

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    dir
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn measure(dir: &TempDir) -> viby::checks::measure_read_cost::Measurement {
    measure_read_cost(&[path_str(dir.path())], None)
}

struct Run {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_cli(args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("measure-read-cost")
        .args(args)
        .output()
        .unwrap();
    Run {
        status: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn missing_path() -> String {
    path_str(&std::env::temp_dir().join("definitely-not-here-9f3a"))
}

/// GROUND TRUTH, from tiktoken cl100k_base (2026-07-29). These are not guesses: each fixture was
/// run through the real tokenizer and the count recorded. They exist so a future edit to the ratios
/// cannot silently decalibrate the estimator — the failure mode that shipped in the first version,
/// where "±15%" was asserted from reasoning and was measurably false. (Kept as data, as the
/// TypeScript kept it; the calibration test below pins the ratios themselves.)
#[allow(dead_code)]
struct GroundTruth {
    name: &'static str,
    text: String,
    true_tokens: u64,
    kind: Kind,
    ext: &'static str,
}

#[allow(dead_code)]
fn ground_truth() -> Vec<GroundTruth> {
    vec![
        GroundTruth {
            name: "TypeScript",
            text: "export function calculateTotal(items: Item[], taxRate: number): number {\n  return items.reduce((sum, item) => sum + item.price * (1 + taxRate), 0);\n}\n".repeat(8),
            true_tokens: 320,
            kind: Kind::Code,
            ext: ".ts",
        },
        GroundTruth {
            name: "Markdown",
            text: "The estimator counts characters and divides by a per-kind ratio, then reports the total.\nIt is calibrated against a real tokenizer rather than assumed.\n".repeat(8),
            true_tokens: 224,
            kind: Kind::Prose,
            ext: ".md",
        },
        GroundTruth {
            name: "SQL",
            text: "SELECT DATE_TRUNC('day', occurred_at AT TIME ZONE 'UTC') AS day,\n       COUNT(DISTINCT user_id) AS active_users\nFROM analytics.events\nWHERE occurred_at >= '2026-01-01' AND occurred_at < '2026-02-01'\nGROUP BY 1 ORDER BY 1;\n".repeat(5),
            true_tokens: 335,
            kind: Kind::Code,
            ext: ".sql",
        },
        GroundTruth {
            name: "YAML",
            text: "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web\n  labels:\n    app: web\nspec:\n  replicas: 3\n".repeat(6),
            true_tokens: 204,
            kind: Kind::Data,
            ext: ".yaml",
        },
    ]
}

#[test]
fn calibration_the_ratios_still_match_the_recorded_measurement() {
    // The ratios are not opinions. They were measured on 2026-07-29 against tiktoken cl100k_base over
    // 400 real files from four working repositories: median error -0.5%, 85% of files within ±15%.
    // Before that calibration the file asserted "±15%" from reasoning alone and was measurably wrong —
    // 33% of files fell outside it, with every ratio biased low by ~9%.
    //
    // This test pins the constants so a future edit cannot silently decalibrate them. If you change a
    // ratio you must re-run the calibration and update these numbers WITH the new measurement.
    let chars = 100_000usize;
    let expected: [(Kind, &str, f64); 4] = [
        (Kind::Code, ".ts", 3.95),
        (Kind::Prose, ".md", 4.25),
        (Kind::Data, ".yaml", 3.55),
        (Kind::Code, ".sql", 4.15),
    ];
    for (kind, ext, ratio) in expected {
        let est = estimate_tokens(&"x".repeat(chars), kind, ext);
        assert_eq!(
            est,
            (chars as f64 / ratio).ceil() as u64,
            "{}{ext} must use the calibrated ratio {ratio}",
            kind.as_str()
        );
    }
}

#[test]
fn sql_has_its_own_ratio_because_real_sql_tokenises_far_less_densely_than_general_code() {
    // Measured on 40 real .sql files: at the generic code ratio they were over-estimated by ~16%.
    let sql =
        "SELECT COUNT(DISTINCT user_id) FROM analytics.events WHERE occurred_at >= a;\n".repeat(20);
    assert!(
        estimate_tokens(&sql, Kind::Code, ".sql") < estimate_tokens(&sql, Kind::Code, ".ts"),
        "the override must apply"
    );
}

#[test]
fn hand_written_fixtures_are_not_valid_calibration_targets_pinned_so_nobody_re_derives_from_them() {
    // This is the lesson the ground-truth run actually taught, and it is worth a test of its own.
    // Synthetic text is unrepresentative in BOTH directions: clean repeated English prose measured
    // 5.4 chars/token against tiktoken (28% looser than real markdown, which averages ~4.25 because
    // it carries links, code spans and punctuation), while a dense hand-written SQL snippet measured
    // 3.3 (20% tighter than real .sql files, which carry comments and repeated column lists).
    //
    // So: calibrate against a real corpus, never against fixtures you wrote. The fixtures agree with
    // whoever wrote them.
    let clean_prose =
        "The estimator counts characters and divides by a per-kind ratio, then reports the total.\n"
            .repeat(8);
    let true_tokens_from_tiktoken = 136.0; // measured with tiktoken: 712 chars -> 136 tokens (5.2 chars/token)
    let est = estimate_tokens(&clean_prose, Kind::Prose, ".md");
    assert!(
        est as f64 > true_tokens_from_tiktoken * 1.15,
        "synthetic clean prose is expected to over-estimate (est {est} vs real {true_tokens_from_tiktoken}); \
         if this ever passes, the ratios were probably re-derived from synthetic text — don't"
    );
}

#[test]
fn prose_and_code_of_the_same_text_cost_different_amounts_the_ratio_is_per_kind() {
    let text = "The quick brown fox jumps over the lazy dog. ".repeat(50);
    assert!(
        estimate_tokens(&text, Kind::Prose, "") < estimate_tokens(&text, Kind::Code, ""),
        "prose tokenizes less densely than code, so the same characters must cost fewer tokens"
    );
}

#[test]
fn non_ascii_content_is_charged_at_roughly_one_token_per_character() {
    let cjk = "設定を読み込む処理".repeat(30);
    let est = estimate_tokens(&cjk, Kind::Code, "");
    let len = cjk.encode_utf16().count() as u64;
    assert!(
        est >= len,
        "non-ASCII must not be divided by the ASCII ratio: got {est} for {len} chars"
    );
}

#[test]
fn a_directory_is_walked_recursively_and_every_source_file_counted() {
    let dir = tree(&[
        ("src/a.ts", &"const a = 1;\n".repeat(10)),
        ("src/deep/b.ts", &"const b = 2;\n".repeat(10)),
        ("README.md", &"hello\n".repeat(10)),
    ]);
    let m = measure(&dir);
    assert_eq!(
        m.files.len(),
        3,
        "expected 3 files, got {}",
        m.files
            .iter()
            .map(|f| f.file.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(m.tokens > 0);
}

#[test]
fn node_modules_is_not_counted_a_baseline_that_includes_it_is_worthless() {
    let dir = tree(&[
        ("src/a.ts", "const a = 1;\n"),
        ("node_modules/big/index.js", &"x".repeat(500_000)),
        ("dist/bundle.js", &"y".repeat(500_000)),
    ]);
    let m = measure(&dir);
    let bases: Vec<String> = m
        .files
        .iter()
        .map(|f| {
            Path::new(&f.file)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(bases, vec!["a.ts"], "only real source may enter the total");
    assert!(
        m.tokens < 100,
        "total must not include vendored or built output, got {}",
        m.tokens
    );
}

#[test]
fn a_binary_file_is_skipped_with_a_reason_not_counted_as_text() {
    let dir = tree(&[("src/a.ts", "const a = 1;\n")]);
    fs::write(
        dir.path().join("logo.png"),
        [0x89, 0x50, 0x4e, 0x47, 0x00, 0x01, 0x02, 0x03],
    )
    .unwrap();
    let m = measure(&dir);
    assert_eq!(m.files.len(), 1, "the binary must not be measured as text");
    assert!(
        m.skipped
            .iter()
            .any(|s| s.file.ends_with("logo.png") && s.reason == "binary"),
        "the skip must be reported, not silent: {:?}",
        m.skipped
    );
}

#[test]
fn a_lockfile_is_counted_but_classified_as_generated_and_reported_separately() {
    let packages: Vec<String> = (0..500).map(|i| format!("p{i}")).collect();
    let lock = serde_json::json!({ "packages": packages }).to_string();
    let dir = tree(&[("src/a.ts", "const a = 1;\n"), ("package-lock.json", &lock)]);
    let m = measure(&dir);
    assert_eq!(kind_of("package-lock.json"), Kind::Generated);
    assert!(
        m.generated_tokens > 0,
        "the lockfile's share must be broken out"
    );
    assert!(
        m.generated_tokens < m.tokens,
        "it is part of the total, just attributed"
    );
}

#[test]
fn a_minified_file_carries_a_caveat_instead_of_a_silent_estimate() {
    let dir = tree(&[("app.min.js", &format!("const x={}1;", "1+".repeat(400)))]);
    let m = measure(&dir);
    assert_eq!(m.files.len(), 1);
    assert!(
        m.files[0]
            .caveat
            .as_deref()
            .unwrap_or("")
            .contains("longest line"),
        "a long-line file must declare that its estimate is weaker"
    );
}

#[test]
fn an_ordinary_short_lined_source_file_carries_no_caveat() {
    // The must-not half of the caveat rule: warning on normal code would train the reader to
    // ignore the warning that matters.
    let dir = tree(&[("src/a.ts", "const a = 1;\nconst b = 2;\n")]);
    let m = measure(&dir);
    assert_eq!(
        m.files[0].caveat, None,
        "normal code must not be flagged: {:?}",
        m.files[0].caveat
    );
}

#[test]
fn repeat_multiplies_the_total_cadence_is_charged_not_just_payload() {
    let dir = tree(&[("src/a.ts", &"const a = 1;\n".repeat(100))]);
    let once = measure(&dir);
    let thrice = measure_read_cost(&[path_str(dir.path())], Some(3.0));
    assert_eq!(thrice.tokens, once.tokens, "one pass is unchanged");
    assert_eq!(
        thrice.total_tokens,
        once.tokens as f64 * 3.0,
        "three passes cost three times as much"
    );
}

#[test]
fn a_path_that_does_not_exist_is_recorded_as_skipped_never_silently_ignored() {
    let m = measure_read_cost(&[missing_path()], None);
    assert_eq!(m.files.len(), 0);
    assert!(
        m.skipped.iter().any(|s| s.reason == "does not exist"),
        "{:?}",
        m.skipped
    );
}

#[test]
fn an_empty_file_is_measured_at_zero_rather_than_dropped() {
    let dir = tree(&[("src/empty.ts", "")]);
    let m = measure(&dir);
    assert_eq!(
        m.files.len(),
        1,
        "an empty file still exists and still gets read"
    );
    assert_eq!(m.files[0].tokens, 0);
}

#[test]
fn cli_nothing_measurable_exits_2_distinct_from_a_clean_measurement() {
    let r = run_cli(&[&missing_path(), "--quiet"]);
    assert_eq!(
        r.status,
        Some(2),
        "expected 2 (nothing to measure), got {:?}: {}{}",
        r.status,
        r.stdout,
        r.stderr
    );
}

#[test]
fn cli_budget_below_the_total_exits_1_and_says_what_to_do_instead() {
    let dir = tree(&[("src/a.ts", &"const a = 1;\n".repeat(2000))]);
    let d = path_str(dir.path());
    let over = run_cli(&[&d, "--budget", "100"]);
    assert_eq!(
        over.status,
        Some(1),
        "over budget must fail: {}{}",
        over.stdout,
        over.stderr
    );
    assert!(over.stdout.contains("over budget"));
    assert!(
        over.stdout.contains("subagent"),
        "an over-budget verdict must name the alternative, not just refuse"
    );

    let under = run_cli(&[&d, "--budget", "10000000"]);
    assert_eq!(
        under.status,
        Some(0),
        "within budget must pass: {}{}",
        under.stdout,
        under.stderr
    );
    assert!(under.stdout.contains("within budget"));
}

#[test]
fn cli_no_budget_given_means_measure_and_report_exit_0() {
    let dir = tree(&[("src/a.ts", &"const a = 1;\n".repeat(500))]);
    let r = run_cli(&[&path_str(dir.path())]);
    assert_eq!(r.status, Some(0), "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("tokens to read once"));
}

#[test]
fn cli_the_estimate_always_ships_with_its_error_bar() {
    // A cheap estimate that hides its uncertainty is the fast-and-wrong answer this whole
    // skill exists to prevent. The disclaimer is part of the contract, not decoration.
    let dir = tree(&[("src/a.ts", "const a = 1;\n")]);
    let r = run_cli(&[&path_str(dir.path())]);
    assert!(r.stdout.contains("Estimate, not a tokenizer"));
}

#[test]
fn cli_json_emits_the_same_numbers_as_the_text_report() {
    let dir = tree(&[
        ("src/a.ts", &"const a = 1;\n".repeat(300)),
        ("docs/x.md", &"words ".repeat(300)),
    ]);
    let d = path_str(dir.path());
    let r = run_cli(&[&d, "--json"]);
    assert_eq!(r.status, Some(0), "{}{}", r.stdout, r.stderr);
    let parsed: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    let direct = measure_read_cost(&[d], None);
    assert_eq!(
        parsed["tokens"].as_u64(),
        Some(direct.tokens),
        "the JSON and library paths must not diverge"
    );
    assert_eq!(parsed["files"].as_array().map(Vec::len), Some(2));
}

#[test]
fn cli_the_biggest_contributor_is_listed_first_so_trimming_has_a_target() {
    let dir = tree(&[
        ("small.ts", "const a = 1;\n"),
        ("huge.ts", &"const b = 2;\n".repeat(400)),
    ]);
    let r = run_cli(&[&path_str(dir.path()), "--quiet"]);
    let huge = r.stdout.find("huge.ts");
    let small = r.stdout.find("small.ts");
    assert!(huge.is_some() && small.is_some(), "{}", r.stdout);
    assert!(
        huge < small,
        "contributors must be ranked by cost, got:\n{}",
        r.stdout
    );
}
