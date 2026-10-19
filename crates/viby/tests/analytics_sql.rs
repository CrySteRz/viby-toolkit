//! Contract tests for the analytics SQL linter (port of `tests/check-analytics-sql.test.ts`).
//!
//! Both halves for every rule. The must-NOT half carries the weight: this linter runs over
//! analytics code, which is full of BETWEEN on numbers, division by literals, and DISTINCT used
//! legitimately. A rule that fires on correct SQL gets the whole checker switched off.

use std::fs;
use std::process::Command;

use viby::checks::check_analytics_sql::scan_sql;

fn rules(sql: &str) -> Vec<String> {
    scan_sql("q.sql", Some(sql))
        .into_iter()
        .map(|f| f.rule)
        .collect()
}

fn has(sql: &str, rule: &str) -> bool {
    rules(sql).iter().any(|r| r == rule)
}

#[test]
fn between_on_a_timestamp_range_is_p1_closed_closed_double_counts_the_boundary() {
    assert!(has(
        "SELECT COUNT(id) FROM orders WHERE created_at BETWEEN start_ts AND end_ts;",
        "between-on-time"
    ));
}

#[test]
fn between_on_numbers_is_not_flagged() {
    assert!(
        !has(
            "SELECT * FROM t WHERE score BETWEEN 1 AND 10;",
            "between-on-time"
        ),
        "numeric ranges are fine"
    );
}

#[test]
fn division_by_a_column_with_no_guard_is_p1() {
    assert!(has(
        "SELECT conversions / sessions AS rate FROM daily;",
        "divide-without-guard"
    ));
}

#[test]
fn division_wrapped_in_nullif_is_not_flagged() {
    assert!(!has(
        "SELECT conversions / NULLIF(sessions, 0) AS rate FROM daily;",
        "divide-without-guard"
    ));
}

#[test]
fn division_by_a_literal_is_not_flagged() {
    assert!(!has(
        "SELECT cents / 100 AS dollars FROM payments;",
        "divide-without-guard"
    ));
}

#[test]
fn count_star_in_a_joined_query_is_p1_a_fan_out_multiplies_it() {
    assert!(has(
        "SELECT COUNT(*) FROM users u LEFT JOIN orders o ON o.user_id = u.id;",
        "count-star-with-join"
    ));
}

#[test]
fn count_star_with_no_join_is_not_flagged() {
    assert!(!has("SELECT COUNT(*) FROM users;", "count-star-with-join"));
}

#[test]
fn now_in_a_metric_query_is_flagged_as_unreproducible() {
    assert!(has(
        "SELECT COUNT(id) FROM orders WHERE created_at > NOW() - INTERVAL '7 days';",
        "now-in-definition"
    ));
}

#[test]
fn equals_null_is_p1_it_silently_matches_nothing() {
    let f = scan_sql(
        "q.sql",
        Some("SELECT id FROM users WHERE deleted_at = NULL;"),
    );
    let hit = f.iter().find(|x| x.rule == "null-equality");
    assert!(hit.is_some());
    assert_eq!(hit.map(|h| h.severity.as_str()), Some("P1"));
}

#[test]
fn is_null_is_not_flagged() {
    assert!(!has(
        "SELECT id FROM users WHERE deleted_at IS NULL;",
        "null-equality"
    ));
}

#[test]
fn plain_union_is_flagged_for_silent_dedup_union_all_is_not() {
    assert!(has(
        "SELECT a FROM t1 UNION SELECT a FROM t2;",
        "union-dedupes"
    ));
    assert!(!has(
        "SELECT a FROM t1 UNION ALL SELECT a FROM t2;",
        "union-dedupes"
    ));
}

#[test]
fn date_trunc_without_a_timezone_is_flagged_with_at_time_zone_it_is_not() {
    assert!(has(
        "SELECT DATE_TRUNC('day', created_at) AS d, COUNT(id) FROM orders GROUP BY 1;",
        "date-trunc-without-zone"
    ));
    assert!(!has(
        "SELECT DATE_TRUNC('day', created_at AT TIME ZONE 'Europe/Bucharest') AS d FROM orders GROUP BY 1;",
        "date-trunc-without-zone"
    ));
}

#[test]
fn float_on_a_money_column_is_flagged_numeric_is_not() {
    assert!(has(
        "CREATE TABLE p (total_amount DOUBLE PRECISION);",
        "float-money"
    ));
    assert!(!has(
        "CREATE TABLE p (total_amount NUMERIC(12,2));",
        "float-money"
    ));
    assert!(
        !has("CREATE TABLE m (ratio DOUBLE PRECISION);", "float-money"),
        "a non-money float is fine"
    );
}

#[test]
fn select_star_is_flagged_but_count_star_is_not_mistaken_for_it() {
    assert!(has("SELECT * FROM orders;", "select-star"));
    assert!(!has("SELECT COUNT(*) FROM orders;", "select-star"));
}

#[test]
fn distinct_over_a_joined_aggregate_is_flagged_as_papering_over_a_fan_out() {
    assert!(has(
        "SELECT DISTINCT u.id, SUM(o.total) FROM users u JOIN orders o ON o.user_id = u.id GROUP BY 1;",
        "distinct-over-aggregate"
    ));
}

#[test]
fn distinct_with_no_aggregate_and_no_join_is_not_flagged() {
    assert!(!has(
        "SELECT DISTINCT country FROM users;",
        "distinct-over-aggregate"
    ));
}

#[test]
fn an_unbounded_aggregate_over_an_events_table_is_flagged_a_bounded_one_is_not() {
    assert!(has("SELECT COUNT(id) FROM events;", "unbounded-fact-scan"));
    assert!(
        !has(
            "SELECT COUNT(id) FROM events WHERE occurred_at >= '2026-01-01' AND occurred_at < '2026-02-01';",
            "unbounded-fact-scan"
        ),
        "a stated window clears it"
    );
}

#[test]
fn a_pattern_inside_a_comment_or_a_string_is_not_a_finding() {
    // Decide on parsed SQL, never raw text — the rule this repo has re-learned four times.
    let sql = [
        "-- WHERE created_at BETWEEN a AND b  (the old version, kept for reference)",
        "SELECT label FROM t WHERE label = 'total_amount DOUBLE PRECISION';",
    ]
    .join("\n");
    assert_eq!(
        rules(&sql),
        Vec::<String>::new(),
        "comments and strings must be inert, got {}",
        rules(&sql).join(",")
    );
}

#[test]
fn a_column_merely_named_like_a_keyword_is_not_a_finding() {
    assert!(!has(
        "SELECT revenue_between_dates FROM t;",
        "between-on-time"
    ));
}

#[test]
fn an_unreadable_file_is_reported_never_silently_clean() {
    let f = scan_sql("/nope/missing.sql", None);
    assert_eq!(f.first().map(|x| x.rule.as_str()), Some("unreadable"));
}

fn run(args: &[&str], cwd: &std::path::Path) -> (Option<i32>, String) {
    let p = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-analytics-sql")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn viby");
    (
        p.status.code(),
        String::from_utf8_lossy(&p.stdout).into_owned(),
    )
}

#[test]
fn cli_findings_exit_1_clean_exits_0_nothing_to_scan_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("bad.sql"),
        "SELECT COUNT(id) FROM o WHERE created_at BETWEEN a AND b;\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("good.sql"),
        "SELECT COUNT(id) FROM o WHERE created_at >= a AND created_at < b;\n",
    )
    .unwrap();
    assert_eq!(run(&["bad.sql", "--quiet"], dir.path()).0, Some(1));
    assert_eq!(run(&["good.sql", "--quiet"], dir.path()).0, Some(0));
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(run(&["--all", "--quiet"], empty.path()).0, Some(2));
}

#[test]
fn cli_the_report_says_these_failures_are_silent_and_that_syntax_is_not_meaning() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("a.sql"),
        "SELECT COUNT(id) FROM o WHERE d >= a AND d < b;\n",
    )
    .unwrap();
    let (_, out) = run(&["a.sql"], dir.path());
    assert!(out.contains("None of these fail loudly"), "{out}");
    assert!(
        out.contains("not whether the metric answers the question"),
        "{out}"
    );
}

#[test]
fn now_in_a_migration_is_not_a_finding_reading_the_clock_there_is_correct() {
    // Measured: on a real repo of 62 migrations this rule fired 117 times, 93% of all findings, every
    // one legitimate SQL. Precision over coverage — a rule that floods takes the linter down with it.
    for sql in [
        "ALTER TABLE users ADD COLUMN created_at TIMESTAMPTZ DEFAULT NOW();",
        "UPDATE users SET migrated_at = NOW() WHERE migrated_at IS NULL;",
        "INSERT INTO audit (at) VALUES (NOW());",
    ] {
        assert!(!has(sql, "now-in-definition"), "must not fire on: {sql}");
    }
}

#[test]
fn now_in_a_metric_query_is_still_a_finding() {
    assert!(
        has(
            "SELECT COUNT(id) FROM orders WHERE created_at > NOW() - INTERVAL '7 days';",
            "now-in-definition"
        ),
        "the must-flag half of the same rule"
    );
}

#[test]
fn set_col_equals_null_is_an_assignment_not_a_broken_comparison() {
    assert!(!has(
        "UPDATE users SET deleted_at = NULL WHERE id = 1;",
        "null-equality"
    ));
    assert!(
        has(
            "SELECT id FROM users WHERE deleted_at != NULL;",
            "null-equality"
        ),
        "comparison still flagged"
    );
}
