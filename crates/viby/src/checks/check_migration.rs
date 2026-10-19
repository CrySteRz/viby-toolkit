//! viby-toolkit migration safety linter — the executable half of /viby-toolkit:schema.
//!
//! A bad schema migration is one of the very few genuinely unrecoverable software mistakes:
//! a dropped column takes the data with it, and a lock held on a hot table takes the service
//! down while it does. Almost all of it comes from a short list of operations whose danger is
//! well established and each of which has a safe alternative. This finds them.
//!
//! Usage:
//!   viby check-migration [paths...] [--all] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no migration files found.
//!
//! DESIGN: each rule names the DANGER and the SAFE ALTERNATIVE, because "don't do that" with
//! no replacement gets ignored under deadline. Severity is by irreversibility first and lock
//! impact second — a lock costs minutes of downtime, dropped data costs it permanently.
//!
//! Engine behaviour cited here (ACCESS EXCLUSIVE locks, table rewrites, CONCURRENTLY) is
//! standard documented PostgreSQL/MySQL behaviour, not a research claim. Specifics vary by
//! engine and version: treat every finding as "check this against YOUR engine", which is why
//! the messages say what to verify rather than asserting a universal truth.

use serde_json::json;

pub use super::check_analytics_sql::Finding;
use super::check_analytics_sql::{collect_targets, severity_order};
use crate::core::args::{flag, parse_or_exit};
use crate::core::js::{js_trim, locale_compare, read_text, rx};
use crate::core::nodepath::{extname, walk_files};
use crate::core::strip::strip_noncode;

const MIGRATION_EXT: &[&str] = &[
    ".sql", ".py", ".rb", ".ts", ".js", ".go", ".php", ".xml", ".yaml", ".yml",
];
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "venv",
    ".venv",
    "dist",
    "build",
    "target",
    "__pycache__",
];

/// Paths that hold schema migrations across the common tools.
fn is_migration_path(norm: &str) -> bool {
    rx!(&[
        r"(^|\/)migrations?\/", // django, alembic, knex, generic
        r"(^|\/)db\/migrate\/", // rails
        r"(^|\/)versions\/",    // alembic
        r"(^|\/)prisma\/migrations\/",
        r"(^|\/)supabase\/migrations\/",
        r"(^|\/)V\d+[\w.]*__.*\.sql$",          // flyway
        r"(^|\/)changelog.*\.(xml|ya?ml|sql)$", // liquibase
        r"(^|\/)schema\.rb$",
        r"(^|\/)structure\.sql$",
    ]
    .join("|"))
    .is_match(norm)
}

pub fn is_migration_file(p: &str) -> bool {
    if !MIGRATION_EXT.contains(&extname(p).to_lowercase().as_str()) {
        return false;
    }
    is_migration_path(p)
}

struct Rule {
    rule: &'static str,
    severity: &'static str,
    test: fn(line: &str, whole: &str) -> bool,
    danger: &'static str,
    instead: &'static str,
}

const RULES: &[Rule] = &[
    // ---- irreversible: the data is gone
    Rule {
        rule: "drop-column",
        severity: "P1",
        test: |l, _| rx!(r"\bDROP\s+COLUMN\b", "i").is_match(l) || rx!(r"\bremove_column\b", "i").is_match(l),
        danger: "dropping a column destroys its data irreversibly, and any deployed code still selecting it starts erroring immediately",
        instead: "expand-contract: stop writing it, deploy, confirm nothing reads it (logs/metrics, not assumption), THEN drop in a later migration",
    },
    Rule {
        rule: "drop-table",
        severity: "P1",
        test: |l, _| rx!(r"\bDROP\s+TABLE\b", "i").is_match(l) || rx!(r"\bdrop_table\b", "i").is_match(l),
        danger: "dropping a table destroys the data irreversibly",
        instead: "rename it out of the way first and drop it a release later, once you can prove nothing references it",
    },
    Rule {
        rule: "truncate",
        severity: "P1",
        test: |l, _| rx!(r"\bTRUNCATE\b", "i").is_match(l),
        danger: "TRUNCATE deletes every row and is not transactional on all engines",
        instead: "if this is intentional in a migration, say so explicitly and take a verified backup first",
    },
    Rule {
        rule: "unbounded-dml",
        severity: "P1",
        // Checks the whole STATEMENT, not just the opening line: ordinary SQL style puts WHERE
        // on a later line, and the single-line version fired a false P1 on
        //   UPDATE foo\n  SET x = 1\n  WHERE id > 100;
        // A wrong P1 on safe, common code is the fastest way to get a linter switched off.
        test: |line, whole| {
            if !rx!(r"^\s*(UPDATE|DELETE)\b", "i").is_match(line) {
                return false;
            }
            let rest = match whole.find(line) {
                Some(start) => &whole[start..],
                None => line,
            };
            let statement = match rest.find(';') {
                Some(end) => &rest[..end],
                None => rest,
            };
            !rx!(r"\bWHERE\b", "i").is_match(statement)
        },
        danger: "an UPDATE/DELETE with no WHERE touches every row: one long transaction, held locks, and no way back",
        instead: "backfill in bounded batches with a WHERE on a key range, committing between batches",
    },
    // ---- locks: the service goes down while it runs
    Rule {
        rule: "index-without-concurrently",
        severity: "P1",
        test: |l, _| rx!(r"\bCREATE\s+(UNIQUE\s+)?INDEX\b", "i").is_match(l) && !rx!(r"\bCONCURRENTLY\b", "i").is_match(l),
        danger: "CREATE INDEX takes a lock that blocks writes for the whole build — minutes on a large table",
        instead: "CREATE INDEX CONCURRENTLY (postgres), outside a transaction; on MySQL confirm ALGORITHM=INPLACE applies",
    },
    Rule {
        rule: "drop-index-without-concurrently",
        severity: "P2",
        test: |l, _| rx!(r"\bDROP\s+INDEX\b", "i").is_match(l) && !rx!(r"\bCONCURRENTLY\b", "i").is_match(l),
        danger: "DROP INDEX takes an exclusive lock on the table",
        instead: "DROP INDEX CONCURRENTLY (postgres)",
    },
    Rule {
        rule: "add-not-null-column",
        severity: "P1",
        test: |l, _| {
            rx!(r"\bADD\s+COLUMN\b", "i").is_match(l)
                && rx!(r"\bNOT\s+NULL\b", "i").is_match(l)
                && !rx!(r"\bDEFAULT\b", "i").is_match(l)
        },
        danger: "adding NOT NULL without a default has to prove every existing row complies, and fails outright if the table is non-empty",
        instead: "add it nullable, backfill in batches, then add the NOT NULL constraint separately once the data is clean",
    },
    Rule {
        rule: "set-not-null",
        severity: "P2",
        test: |l, _| rx!(r"\bALTER\s+COLUMN\b.*\bSET\s+NOT\s+NULL\b", "i").is_match(l),
        danger: "SET NOT NULL scans the whole table under a lock to validate it",
        instead: "add a validated CHECK (col IS NOT NULL) NOT VALID, VALIDATE it separately, then SET NOT NULL (postgres 12+)",
    },
    Rule {
        rule: "column-type-change",
        severity: "P1",
        test: |l, _| rx!(r"\bALTER\s+(COLUMN\s+\w+\s+)?TYPE\b", "i").is_match(l) || rx!(r"\bMODIFY\s+COLUMN\b", "i").is_match(l),
        danger: "changing a column type can rewrite the entire table under an exclusive lock, and may silently truncate values",
        instead: "add a new column, dual-write, backfill in batches, switch reads, then drop the old one in a later migration",
    },
    Rule {
        rule: "constraint-without-not-valid",
        severity: "P2",
        test: |l, _| {
            rx!(r"\bADD\s+(CONSTRAINT\b.*)?(FOREIGN\s+KEY|CHECK)\b", "i").is_match(l)
                && !rx!(r"\bNOT\s+VALID\b", "i").is_match(l)
        },
        danger: "adding a FOREIGN KEY or CHECK validates every existing row while holding a lock",
        instead: "ADD ... NOT VALID first, then VALIDATE CONSTRAINT in a separate migration (takes a weaker lock)",
    },
    Rule {
        rule: "rename",
        severity: "P1",
        test: |l, _| {
            rx!(r"\bRENAME\s+(COLUMN|TO)\b", "i").is_match(l)
                || rx!(r"\brename_column\b", "i").is_match(l)
                || rx!(r"\brename_table\b", "i").is_match(l)
        },
        danger: "a rename is not backward compatible: code deployed before the migration breaks the instant it lands, and code after breaks until it lands",
        instead: "add the new name, dual-write, migrate readers, drop the old name a release later — never rename in place on a live system",
    },
    Rule {
        rule: "vacuum-full",
        severity: "P2",
        test: |l, _| {
            rx!(r"\bVACUUM\s+FULL\b", "i").is_match(l)
                || (rx!(r"\bREINDEX\b", "i").is_match(l) && !rx!(r"\bCONCURRENTLY\b", "i").is_match(l))
        },
        danger: "VACUUM FULL and plain REINDEX hold an exclusive lock for the duration",
        instead: "pg_repack, or REINDEX CONCURRENTLY",
    },
];

fn finding(file: &str, rule: &str, severity: &str, danger: &str, instead: &str) -> Finding {
    Finding {
        file: file.to_string(),
        line: 1,
        rule: rule.to_string(),
        severity: severity.to_string(),
        danger: danger.to_string(),
        instead: instead.to_string(),
    }
}

/// Whole-file checks, rather than per-line.
fn file_level(file: &str, text: &str, is_sql: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let ddl = rx!(
        r"\b(ALTER\s+TABLE|CREATE\s+TABLE|CREATE\s+INDEX|DROP\s+)",
        "i"
    )
    .is_match(text);
    let dml = rx!(r"^\s*(UPDATE|INSERT|DELETE)\b", "im").is_match(text);

    if ddl && dml {
        out.push(finding(
            file,
            "ddl-and-backfill-together",
            "P2",
            "schema change and data backfill in one migration means one long transaction: the DDL lock is held for as long as the backfill takes",
            "split them — ship the schema change, then backfill in batches in a separate migration or a job",
        ));
    }
    if is_sql && ddl && !rx!(r"\block_timeout\b", "i").is_match(text) {
        out.push(finding(
            file,
            "no-lock-timeout",
            "P2",
            "with no lock_timeout, a migration that cannot get its lock queues behind a long query and every request queues behind it — a short migration becomes an outage",
            "SET lock_timeout = '5s' (and a statement_timeout) so it fails fast and can be retried",
        ));
    }
    // Reversibility: only for tools that have an explicit down/rollback concept.
    let is_rails_or_alembic =
        rx!(r"(^|\/)(db\/migrate|versions)\/").is_match(file) || rx!(r"\.rb$").is_match(file);
    if is_rails_or_alembic
        && rx!(r"\b(def\s+up|def\s+change|def\s+upgrade)\b").is_match(text)
        && !rx!(r"\b(def\s+down|def\s+downgrade)\b").is_match(text)
        && !rx!(r"\bdef\s+change\b").is_match(text)
    {
        out.push(finding(
            file,
            "no-rollback",
            "P2",
            "no down/downgrade path, so this migration cannot be reversed if the deploy goes wrong",
            "write the inverse, or state in a comment why it is deliberately irreversible",
        ));
    }
    out
}

pub fn scan_migration(file_path: &str) -> Vec<Finding> {
    let Some(raw) = read_text(file_path) else {
        // Never report an unreadable migration as safe — see the same fix in
        // scan-test-quality.ts. Silence here would mean "no dangerous patterns found" for a
        // file nobody looked at, on the one class of change that cannot be undone.
        return vec![finding(
            file_path,
            "unreadable",
            "P2",
            "could not read this migration, so it was NOT checked for dangerous patterns",
            "fix the permissions or path and re-run — do not treat this run as clean for it",
        )];
    };
    let ext = extname(file_path).to_lowercase();
    // Match CODE, not raw text — a comment or a docstring describing a DROP COLUMN is not one.
    let text = strip_noncode(&raw, &ext);
    let mut findings = Vec::new();

    for (i, line) in text.split('\n').enumerate() {
        if js_trim(line).is_empty() {
            continue;
        }
        for r in RULES {
            if (r.test)(line, &text) {
                findings.push(Finding {
                    file: file_path.to_string(),
                    line: i + 1,
                    rule: r.rule.to_string(),
                    severity: r.severity.to_string(),
                    danger: r.danger.to_string(),
                    instead: r.instead.to_string(),
                });
            }
        }
    }
    findings.extend(file_level(file_path, &text, ext == ".sql"));
    findings
}

fn walk(root: &str) -> Vec<String> {
    walk_files(
        root,
        |name| SKIP_DIRS.contains(&name),
        |_, full, _| is_migration_file(full),
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
            println!("no migration files found (looked for migrations/, db/migrate/, versions/, flyway, prisma)");
        }
        return 2;
    }

    let mut findings: Vec<Finding> = targets.iter().flat_map(|t| scan_migration(t)).collect();

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
                "clean: {} migration file(s), no dangerous patterns found",
                targets.len()
            );
        } else {
            println!(
                "{} finding(s) across {} migration file(s)",
                findings.len(),
                targets.len()
            );
        }
        println!(
            "Engine and version change the details — verify each against YOUR database before acting,\n\
             and never run an unreviewed migration against production data. See /viby-toolkit:schema."
        );
    }
    i32::from(!findings.is_empty())
}
