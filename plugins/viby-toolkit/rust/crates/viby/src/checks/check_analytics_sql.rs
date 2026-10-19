//! viby-toolkit analytics SQL linter — the executable half of /viby-toolkit:analytics.
//!
//! A KPI is a number a client makes decisions with, and the ways it goes quietly wrong are a short,
//! well-known list. None of them fail loudly: the query runs, returns a plausible number, and the
//! dashboard looks fine. That is the whole problem — a wrong KPI is indistinguishable from a right
//! one until someone reconciles it against the business, usually in a meeting.
//!
//! Usage:
//!   viby check-analytics-sql [paths...] [--all] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no SQL found.
//!
//! DESIGN, same as the migration linter: every rule names the danger AND the safe alternative,
//! because "don't do that" with no replacement gets ignored under deadline. Severity is by how
//! silently it corrupts a number: P1 = the number is wrong and nothing tells you, P2 = wrong under
//! conditions that will eventually occur, P3 = fragile or unreproducible.
//!
//! Engine behaviour differs (BETWEEN, DATE_TRUNC and timezone semantics vary), so findings say what
//! to verify rather than asserting a universal truth. Decide on parsed SQL, never raw text: strings
//! and `--` comments are blanked first, so a column named `revenue_between_dates` or a commented-out
//! draft query is not a finding.
//!
//! This file also holds the domain pieces the four linters in this family (analytics SQL,
//! migration, logging, skill safety) share: `Finding`, the collect-targets loop and the severity
//! order. The JavaScript-compatibility helpers live in `core::js` and `core::nodepath`.

use std::fs;

use regex::Regex;
use serde::Serialize;
use serde_json::json;

use crate::core::args::{flag, parse_or_exit};
use crate::core::js::{js_trim, locale_compare, read_text, rx, utf16_cmp};
use crate::core::nodepath::{extname, walk_files};
use crate::core::strip::strip_noncode;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: String,
    pub severity: String,
    pub danger: String,
    pub instead: String,
}

/// The targets of a run: the walk of `.` for `--all` or no paths, else each path (a directory is
/// walked, a file is taken as given, a missing one skipped), de-duplicated and sorted.
pub fn collect_targets(
    positionals: &[String],
    all: bool,
    walk: &dyn Fn(&str) -> Vec<String>,
) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    if all || positionals.is_empty() {
        targets.extend(walk("."));
    } else {
        for p in positionals {
            match fs::metadata(p) {
                Ok(m) if m.is_dir() => targets.extend(walk(p)),
                Ok(_) => targets.push(p.clone()),
                Err(_) => {}
            }
        }
    }
    targets.sort_by(|a, b| utf16_cmp(a, b));
    targets.dedup();
    targets
}

/// P1 before P2 before P3.
pub fn severity_order(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        _ => 2,
    }
}

// ---------------------------------------------------------------------------------------------
// The analytics rules.
// ---------------------------------------------------------------------------------------------

const SQL_EXT: &[&str] = &[".sql", ".dbt", ".ddl"];
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dbt_packages",
    "venv",
    ".venv",
    "dist",
    "build",
    "logs",
];

/// Looks like a date/time thing — a column name, a cast, or a date function.
fn temporal() -> &'static Regex {
    rx!(
        r"\b(\w*(?:date|time|timestamp|_at|_ts|day|month|week|year)\w*)\b|::\s*(?:date|timestamp\w*)|\b(?:current_date|now|getdate|sysdate)\b",
        "i"
    )
}

pub struct Rule {
    pub rule: &'static str,
    pub severity: &'static str,
    /// line = the blanked line, whole = the blanked file, raw = the original file.
    pub test: fn(line: &str, whole: &str, raw: &str) -> bool,
    pub danger: &'static str,
    pub instead: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        rule: "between-on-time",
        severity: "P1",
        // BETWEEN is closed-closed. Temporal binning needs a half-open interval or the boundary row is
        // counted in two periods — the classic "our monthly numbers don't add up to the year".
        test: |l, _, _| rx!(r"\bBETWEEN\b", "i").is_match(l) && temporal().is_match(l),
        danger: "BETWEEN on a date/time range is inclusive at BOTH ends, so a row exactly on the boundary lands in two buckets and monthly figures stop summing to the annual one",
        instead: "use a half-open interval: `ts >= '<start>' AND ts < '<next start>'` — never BETWEEN for time",
    },
    Rule {
        rule: "divide-without-guard",
        severity: "P1",
        // A rate whose denominator can be zero or NULL: either an error or, worse, a silent NULL that
        // a dashboard renders as a gap or a zero.
        test: |l, _, _| {
            if !l.contains('/') || rx!(r"^\s*--").is_match(l) {
                return false;
            }
            if rx!(r"\b(?:NULLIF|COALESCE|CASE|IFNULL|DIV0|SAFE_DIVIDE)\b", "i").is_match(l) {
                return false;
            }
            // Only when the denominator is an expression, not a literal (x / 100 is fine). The TS
            // wrote this as `\/\s*(?![0-9.]+\b)[a-z_(]`; the lookahead can never fail once the next
            // character is a letter, `_` or `(`, so it is dropped here (no lookaround in `regex`).
            rx!(r"/\s*[A-Za-z_(]").is_match(l)
        },
        danger: "division with a non-literal denominator: a zero denominator errors or returns NULL, and a NULL rate is rendered as a gap or a zero rather than 'no data'",
        instead: "wrap it — `numerator / NULLIF(denominator, 0)` — and decide explicitly what an empty denominator should display as",
    },
    Rule {
        rule: "count-star-with-join",
        severity: "P1",
        // A join that fans out multiplies the rows, and COUNT(*) counts the multiplication.
        test: |l, whole, _| {
            rx!(r"\bCOUNT\s*\(\s*\*\s*\)", "i").is_match(l)
                && rx!(r"\b(?:LEFT|RIGHT|INNER|FULL|CROSS)?\s*JOIN\b", "i").is_match(whole)
        },
        danger: "COUNT(*) in a query that joins: if any join matches more than one row, the count is multiplied and the metric is silently inflated",
        instead: "count the grain you mean — `COUNT(DISTINCT <primary key of the grain>)` — or aggregate the joined side in a subquery first",
    },
    Rule {
        rule: "now-in-definition",
        severity: "P2",
        // The word boundary goes only where a word actually ends. `\b` AFTER `NOW()` can never match —
        // `)` is not a word character, so there is no boundary to find, and the rule silently never
        // fired. Caught by its own contract test.
        test: |l, whole, _| {
            if !rx!(
                r"\bNOW\s*\(\s*\)|\bCURRENT_TIMESTAMP\b|\bCURRENT_DATE\b|\bGETDATE\s*\(|\bSYSDATE\b",
                "i"
            )
            .is_match(l)
            {
                return false;
            }
            // Only in a METRIC query. Reading the clock is correct and normal in a migration
            // (`created_at DEFAULT NOW()`, a dated backfill) and in DDL. Measured on a real repo of 62
            // migrations this rule fired 117 times — 93% of all findings, every one of them legitimate
            // SQL. A rule that floods on correct code takes the whole linter down with it.
            if rx!(
                r"\b(?:INSERT\s+INTO|UPDATE\s+\w|ALTER\s+TABLE|CREATE\s+(?:TABLE|INDEX)|DEFAULT\b)",
                "i"
            )
            .is_match(whole)
            {
                return false;
            }
            rx!(r"\bSELECT\b", "i").is_match(whole)
                && rx!(r"\b(?:SUM|COUNT|AVG|MIN|MAX|PERCENTILE\w*)\s*\(", "i").is_match(whole)
        },
        danger: "a metric that reads the clock cannot be reproduced: the same query returns a different number tomorrow, so nobody can verify last week's figure or re-run a report",
        instead: "take the window as a parameter (or a dbt var) so a run is reproducible, and record the window on the output",
    },
    Rule {
        rule: "select-star",
        severity: "P3",
        test: |l, _, _| {
            rx!(r"\bSELECT\s+\*", "i").is_match(l) && !rx!(r"\bCOUNT\s*\(\s*\*", "i").is_match(l)
        },
        danger: "SELECT * in a model or metric query silently changes shape when an upstream column is added, renamed or reordered",
        instead: "list the columns you depend on — the query then fails loudly when the contract changes, which is what you want",
    },
    Rule {
        rule: "union-dedupes",
        severity: "P2",
        test: |l, _, _| {
            rx!(r"\bUNION\b", "i").is_match(l) && !rx!(r"\bUNION\s+ALL\b", "i").is_match(l)
        },
        danger: "plain UNION removes duplicate rows, so genuinely repeated events (two identical purchases in a second) are silently collapsed and the count is under-reported",
        instead: "UNION ALL unless you can state why identical rows must be collapsed",
    },
    Rule {
        rule: "distinct-over-aggregate",
        severity: "P2",
        test: |l, whole, _| {
            rx!(r"\bSELECT\s+DISTINCT\b", "i").is_match(l)
                && rx!(r"\b(?:SUM|COUNT|AVG|MIN|MAX)\s*\(", "i").is_match(whole)
                && rx!(r"\bJOIN\b", "i").is_match(whole)
        },
        danger: "SELECT DISTINCT over a joined aggregate usually papers over a fan-out join rather than fixing it — it hides duplicate rows but the SUMs were already computed on them",
        instead: "find the join that fans out and aggregate it to the right grain first; DISTINCT at the end cannot undo an inflated SUM",
    },
    Rule {
        rule: "date-trunc-without-zone",
        severity: "P2",
        test: |l, _, _| {
            rx!(r"\bDATE_TRUNC\s*\(", "i").is_match(l)
                && !rx!(r"\bAT\s+TIME\s+ZONE\b", "i").is_match(l)
                && !rx!(r"\btimezone\s*\(", "i").is_match(l)
                && temporal().is_match(l)
        },
        danger: "truncating a timestamp without naming a timezone buckets by UTC day; a client in another timezone sees events shifted into the wrong day, and their 'yesterday' never matches yours",
        instead: "convert explicitly — `DATE_TRUNC('day', ts AT TIME ZONE '<the reporting zone>')` — and state the reporting timezone on the dashboard",
    },
    Rule {
        rule: "null-equality",
        severity: "P1",
        // Comparison only. `SET col = NULL` is an ASSIGNMENT and perfectly correct — flagging it fired
        // on real UPDATE statements that were doing exactly the right thing.
        test: |l, _, _| {
            rx!(r"(?:<>|!=)\s*NULL\b", "i").is_match(l)
                || (rx!(r"=\s*NULL\b", "i").is_match(l)
                    && !rx!(r"\bSET\b[^;]*=\s*NULL\b", "i").is_match(l))
        },
        danger: "comparing to NULL with = or <> is never true, so the filter silently matches nothing and the metric quietly excludes every row it should have caught",
        instead: "use IS NULL / IS NOT NULL",
    },
    Rule {
        rule: "float-money",
        severity: "P2",
        test: |l, _, _| {
            rx!(r"\b(?:FLOAT|REAL|DOUBLE(?:\s+PRECISION)?)\b", "i").is_match(l)
                && rx!(
                    r"\b(\w*(?:amount|price|revenue|cost|total|fee|balance|salary)\w*)\b",
                    "i"
                )
                .is_match(l)
        },
        danger: "binary floating point cannot represent decimal money exactly, so sums drift by cents and a revenue KPI stops reconciling with finance",
        instead: "use NUMERIC/DECIMAL with an explicit scale for anything monetary",
    },
    Rule {
        rule: "unbounded-fact-scan",
        severity: "P3",
        test: |l, whole, _| {
            if !rx!(
                r#"\bFROM\s+[\w."]*\b(?:events?|fact_\w+|\w+_events?|logs?|clicks?|pageviews?)\b"#,
                "i"
            )
            .is_match(l)
            {
                return false;
            }
            // The TS also tried `\bWHERE\b[\s\S]{0,400}?` first; the lazy tail matches empty, so it
            // only asks whether a WHERE exists, and the search below finds exactly that.
            if let Some(m) = rx!(r"\bWHERE\b", "i").find(whole) {
                if temporal().is_match(&whole[m.start()..]) {
                    return false;
                }
            }
            rx!(r"\b(?:SUM|COUNT|AVG)\s*\(", "i").is_match(whole)
        },
        danger: "an aggregate over an event/fact table with no time predicate scans the whole history: it gets slower every day and quietly changes meaning as old data ages in",
        instead: "always bound the window explicitly, even for an 'all time' figure — then 'all time' is a stated range rather than 'whatever exists today'",
    },
];

fn finding(
    file: &str,
    line: usize,
    rule: &str,
    severity: &str,
    danger: &str,
    instead: &str,
) -> Finding {
    Finding {
        file: file.to_string(),
        line,
        rule: rule.to_string(),
        severity: severity.to_string(),
        danger: danger.to_string(),
        instead: instead.to_string(),
    }
}

pub fn scan_sql(file_path: &str, raw_input: Option<&str>) -> Vec<Finding> {
    let raw: String = match raw_input {
        Some(r) => r.to_string(),
        None => match read_text(file_path) {
            Some(r) => r,
            None => {
                return vec![finding(
                    file_path,
                    1,
                    "unreadable",
                    "P2",
                    "could not read this file, so it was NOT checked",
                    "fix the path and re-run — do not treat this run as clean for it",
                )]
            }
        },
    };
    // Decide on SQL, not on text: string literals and `--`/`/* */` comments are blanked.
    let code = strip_noncode(&raw, ".sql");
    let mut findings = Vec::new();
    for (i, line) in code.split('\n').enumerate() {
        if js_trim(line).is_empty() {
            continue;
        }
        for r in RULES {
            if (r.test)(line, &code, &raw) {
                findings.push(finding(
                    file_path,
                    i + 1,
                    r.rule,
                    r.severity,
                    r.danger,
                    r.instead,
                ));
            }
        }
    }
    findings
}

fn walk(root: &str) -> Vec<String> {
    walk_files(
        root,
        |name| SKIP_DIRS.contains(&name),
        |name, _, _| SQL_EXT.contains(&extname(name).to_lowercase().as_str()),
    )
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[flag("all"), flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let targets = collect_targets(&parsed.positionals, parsed.flag("all"), &walk);

    if targets.is_empty() {
        if json_out {
            println!("{}", json!({ "scanned": 0, "findings": [] }));
        } else if !quiet {
            println!("no .sql files found");
        }
        return 2;
    }

    let mut findings: Vec<Finding> = targets.iter().flat_map(|t| scan_sql(t, None)).collect();

    if json_out {
        let report = json!({ "scanned": targets.len(), "findings": findings });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return i32::from(!findings.is_empty());
    }

    findings.sort_by(|a, b| {
        severity_order(&a.severity)
            .cmp(&severity_order(&b.severity))
            .then_with(|| locale_compare(&a.file, &b.file))
    });
    for f in &findings {
        println!("{}:{}  [{} {}]", f.file, f.line, f.severity, f.rule);
        println!("    risk:    {}", f.danger);
        println!("    instead: {}", f.instead);
    }
    if !quiet {
        println!();
        if findings.is_empty() {
            println!(
                "clean: {} SQL file(s), none of the known silent-corruption patterns",
                targets.len()
            );
        } else {
            println!(
                "{} finding(s) across {} SQL file(s)",
                findings.len(),
                targets.len()
            );
        }
        println!(
            "None of these fail loudly — the query runs and returns a plausible number, which is why they\n\
             survive to production. Engine semantics differ, so verify each against YOUR warehouse, and\n\
             remember this checks syntax, not whether the metric answers the question. See\n\
             /viby-toolkit:analytics."
        );
    }
    i32::from(!findings.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_and_extname_follow_node_path() {
        use crate::core::nodepath::join;
        assert_eq!(join(&[".", "a"]), "a");
        assert_eq!(join(&["dir/", "a"]), "dir/a");
        assert_eq!(join(&["./x//y", "a"]), "x/y/a");
        assert_eq!(join(&["../x", "a"]), "../x/a");
        assert_eq!(join(&["/abs", "a"]), "/abs/a");
        assert_eq!(extname(".bashrc"), "");
        assert_eq!(extname("a.tar.GZ"), ".GZ");
        assert_eq!(extname("a."), ".");
        assert_eq!(extname("x/y.d/z"), "");
    }
}
