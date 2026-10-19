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
//! This file also holds the small JavaScript-compatibility helpers the four linters in this family
//! (analytics SQL, migration, logging, skill safety) share: a JS-flavoured regex builder, `path`
//! helpers, `localeCompare`, and the walk-and-collect loop that all four repeat.

use std::cmp::Ordering;
use std::fs;

use regex::Regex;
use serde::Serialize;
use serde_json::json;

use crate::core::args::{flag, parse_or_exit};
use crate::core::strip::{is_js_space, strip_noncode};

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: String,
    pub severity: String,
    pub danger: String,
    pub instead: String,
}

// ---------------------------------------------------------------------------------------------
// JavaScript-compatibility helpers (shared by the four linters of this family).
// ---------------------------------------------------------------------------------------------

/// JavaScript's `\s`, spelled out: Rust's `\s` also matches U+0085, and lacks the byte-order mark.
const JS_SPACE: &str = "\\t\\n\\x{b}\\x{c}\\r \\x{a0}\\x{1680}\\x{2000}-\\x{200a}\\x{2028}\\x{2029}\\x{202f}\\x{205f}\\x{3000}\\x{feff}";

/// Compile a JavaScript regex source (flags: `i`, `m`) with JavaScript's meaning: `\w`, `\d` and
/// `\b` are ASCII, `\s` is JS white space, and `.` stops at every JS line terminator.
/// Only for static patterns, so a bad one is a bug in this file and panics at first use.
pub fn js_re(pattern: &str, flags: &str) -> Regex {
    let mut out = String::new();
    if flags.contains('i') {
        out.push_str("(?i)");
    }
    if flags.contains('m') {
        out.push_str("(?m)");
    }
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('s') => out.push_str(&format!("[{JS_SPACE}]")),
                Some('S') => out.push_str(&format!("[^{JS_SPACE}]")),
                Some('w') => out.push_str("[A-Za-z0-9_]"),
                Some('W') => out.push_str("[^A-Za-z0-9_]"),
                Some('d') => out.push_str("[0-9]"),
                Some('D') => out.push_str("[^0-9]"),
                Some('b') if !in_class => out.push_str("(?-u:\\b)"),
                Some('B') if !in_class => out.push_str("(?-u:\\B)"),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            '[' => {
                in_class = true;
                out.push(c);
            }
            ']' => {
                in_class = false;
                out.push(c);
            }
            '.' if !in_class => out.push_str("[^\\n\\r\\x{2028}\\x{2029}]"),
            _ => out.push(c),
        }
    }
    // A static pattern that does not compile is a programming error, not input.
    Regex::new(&out).unwrap_or_else(|e| panic!("bad pattern {pattern:?}: {e}"))
}

/// A lazily compiled JS regex, one per call site: `rx!("\\bDROP\\b", "i")`.
macro_rules! rx {
    ($pat:expr) => {
        rx!($pat, "")
    };
    ($pat:expr, $flags:expr) => {{
        static R: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| $crate::checks::check_analytics_sql::js_re($pat, $flags));
        &*R
    }};
}
pub(crate) use rx;

/// JavaScript `String.prototype.trim`: white space and the BOM, but not U+0085.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| is_js_space(c) && c != '\u{85}')
}

/// `path.extname`: the text from the last `.` of the final component, unless that dot starts the
/// name (`.bashrc` has none) or the name is `..`.
pub fn js_extname(p: &str) -> String {
    let base = js_basename(p);
    if base == ".." {
        return String::new();
    }
    match base.rfind('.') {
        Some(i) if i > 0 => base[i..].to_string(),
        _ => String::new(),
    }
}

/// `path.basename`.
pub fn js_basename(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// Default `Array.prototype.sort` on strings: UTF-16 code-unit order.
pub fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// ICU root-collation order of the printable ASCII range, as `localeCompare` sees it: white space,
/// punctuation, symbols, digits, then letters (case-insensitive at the first level).
const COLLATION: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";

/// Latin-1 letters ICU files under a base letter: lower-case letter, base, accent rank (the order
/// ICU sorts the marks in: acute, grave, circumflex, ring, diaeresis, tilde, cedilla, stroke).
const ACCENTED: &[(char, char, u32)] = &[
    ('à', 'a', 2),
    ('á', 'a', 1),
    ('â', 'a', 4),
    ('ã', 'a', 7),
    ('ä', 'a', 6),
    ('å', 'a', 5),
    ('ç', 'c', 9),
    ('è', 'e', 2),
    ('é', 'e', 1),
    ('ê', 'e', 4),
    ('ë', 'e', 6),
    ('ì', 'i', 2),
    ('í', 'i', 1),
    ('î', 'i', 4),
    ('ï', 'i', 6),
    ('ñ', 'n', 7),
    ('ò', 'o', 2),
    ('ó', 'o', 1),
    ('ô', 'o', 4),
    ('õ', 'o', 7),
    ('ö', 'o', 6),
    ('ø', 'o', 10),
    ('ù', 'u', 2),
    ('ú', 'u', 1),
    ('û', 'u', 4),
    ('ü', 'u', 6),
    ('ý', 'y', 1),
    ('ÿ', 'y', 6),
];

/// `a.localeCompare(b)` for the default locale: ICU compares the base letters of the whole string
/// first, then the accents, then the case (lower before upper). Exact for ASCII and the Latin-1
/// accented letters (checked against Node's ICU); any other character sorts after them by code
/// point, which ICU would not do for, say, Greek or Cyrillic.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    /// (primary, accent, is upper case); `None` for an ignorable control character.
    fn key(c: char) -> Option<(u32, u32, bool)> {
        if c.is_ascii() {
            if (c as u32) < 0x20 || c as u32 == 0x7f {
                return None;
            }
            let p = COLLATION.find(c.to_ascii_lowercase())? as u32;
            return Some((p, 0, c.is_ascii_uppercase()));
        }
        let lower = c.to_lowercase().next().unwrap_or(c);
        if let Some(&(_, base, accent)) = ACCENTED.iter().find(|e| e.0 == lower) {
            let p = COLLATION.find(base)? as u32;
            return Some((p, accent, lower != c));
        }
        Some((1000 + c as u32, 0, false))
    }
    let (ka, kb): (Vec<_>, Vec<_>) = (
        a.chars().filter_map(key).collect(),
        b.chars().filter_map(key).collect(),
    );
    let level = |f: fn(&(u32, u32, bool)) -> u32| ka.iter().map(f).cmp(kb.iter().map(f));
    level(|k| k.0)
        .then_with(|| level(|k| k.1))
        .then_with(|| level(|k| u32::from(k.2)))
}

/// `path.join(dir, name)`: normalised, so `.` and `..` segments and doubled slashes disappear.
pub fn join_norm(dir: &str, name: &str) -> String {
    let joined = format!("{dir}/{name}");
    let absolute = joined.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(parts.last(), Some(l) if *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let body = parts.join("/");
    match (absolute, body.is_empty()) {
        (true, _) => format!("/{body}"),
        (false, true) => ".".to_string(),
        (false, false) => body,
    }
}

/// `readFileSync(path, "utf8")`: invalid UTF-8 becomes U+FFFD; a directory or an unreadable file
/// is an error the caller reports.
pub fn read_text(path: &str) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Every file under `root` that `want(name, full)` accepts, skipping directories `skip_dir(name)`
/// names. Symlinks are not followed (a `Dirent` says "symlink", not "directory"), as in Node.
pub fn walk_files(
    root: &str,
    skip_dir: &dyn Fn(&str) -> bool,
    want: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let full = join_norm(&dir, &name);
            let Ok(ty) = e.file_type() else { continue };
            if ty.is_dir() {
                if !skip_dir(&name) {
                    stack.push(full);
                }
            } else if want(&name, &full) {
                out.push(full);
            }
        }
    }
    out
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

/// Byte offset that is `units` UTF-16 units before byte `at` of `text` (or the start), the way
/// `text.slice(Math.max(0, at - units), at)` counts. `text` is the raw file: its char boundaries
/// are also the blanked file's, so the result slices both. An `at` inside a character is moved
/// back to its start.
pub fn back_utf16(text: &str, at: usize, units: usize) -> usize {
    let mut used = 0;
    let mut start = at.min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let at = start;
    for c in text[..at].chars().rev() {
        if used >= units {
            break;
        }
        used += c.len_utf16();
        start -= c.len_utf8();
    }
    start
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
    walk_files(root, &|name| SKIP_DIRS.contains(&name), &|name, _| {
        SQL_EXT.contains(&js_extname(name).to_lowercase().as_str())
    })
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

    /// The order Node's `localeCompare` (ICU) gives this vocabulary, ASCII and Latin-1 mixed.
    const ICU_ORDER: &[&str] = &[
        "_x",
        "!",
        "@x",
        "#x",
        "~x",
        "$x",
        "10",
        "9",
        "a",
        "A",
        "À",
        "a b",
        "a b/c",
        "a_b",
        "a-b",
        "a!",
        "a.sql",
        "a.SQL",
        "a/b",
        "A1",
        "ab",
        "Ab",
        "b",
        "B",
        "c",
        "ç",
        "d",
        "e",
        "E",
        "é",
        "e.sql",
        "é.sql",
        "f",
        "f.sql",
        "migrations/a.sql",
        "Migrations/a.sql",
        "migrations/B.sql",
        "n",
        "ñ",
        "o",
        "ö",
        "ø",
        "resume",
        "Resume",
        "résume",
        "résumé",
        "u",
        "ü",
        "x-y",
        "x.y",
        "X.y",
        "x/y",
        "y",
        "ÿ",
        "z",
        "zebra",
        "Zebra",
    ];

    #[test]
    fn locale_compare_orders_like_icu() {
        let mut shuffled: Vec<&str> = ICU_ORDER.iter().rev().copied().collect();
        shuffled.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(shuffled, ICU_ORDER);
    }

    #[test]
    fn join_norm_and_extname_follow_node_path() {
        assert_eq!(join_norm(".", "a"), "a");
        assert_eq!(join_norm("dir/", "a"), "dir/a");
        assert_eq!(join_norm("./x//y", "a"), "x/y/a");
        assert_eq!(join_norm("../x", "a"), "../x/a");
        assert_eq!(join_norm("/abs", "a"), "/abs/a");
        assert_eq!(js_extname(".bashrc"), "");
        assert_eq!(js_extname("a.tar.GZ"), ".GZ");
        assert_eq!(js_extname("a."), ".");
        assert_eq!(js_extname("x/y.d/z"), "");
    }
}
