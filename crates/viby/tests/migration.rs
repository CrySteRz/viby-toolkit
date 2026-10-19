//! Contract tests for the migration safety linter (port of `tests/check-migration.test.ts`).
//!
//! Both halves pinned: the dangerous pattern must be caught, AND its safe alternative must
//! NOT be flagged. The second half is what makes the linter usable — a linter that flags
//! `CREATE INDEX CONCURRENTLY` teaches you to skip the linter.

use std::fs;
use std::path::Path;

use viby::checks::check_migration::{is_migration_file, scan_migration, Finding};

fn write_in(dir: &Path, filename: &str, source: &str) -> String {
    let full = dir.join(filename);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(&full, source).unwrap();
    full.to_string_lossy().into_owned()
}

fn scan(source: &str, filename: &str) -> Vec<Finding> {
    let dir = tempfile::tempdir().unwrap();
    let full = write_in(dir.path(), filename, source);
    scan_migration(&full)
}

fn rules_in(source: &str, filename: &str) -> Vec<String> {
    scan(source, filename).into_iter().map(|f| f.rule).collect()
}

fn rules(source: &str) -> Vec<String> {
    rules_in(source, "migrations/001_change.sql")
}

fn has(found: &[String], rule: &str) -> bool {
    found.iter().any(|r| r == rule)
}

const SAFE_HEADER: &str = "SET lock_timeout = '5s';\n";

#[test]
fn dangerous_pattern_is_caught_safe_alternative_is_not() {
    // (name, dangerous, safe, rule)
    let cases = [
        (
            "index",
            "CREATE INDEX idx_users_email ON users (email);",
            "CREATE INDEX CONCURRENTLY idx_users_email ON users (email);",
            "index-without-concurrently",
        ),
        (
            "drop index",
            "DROP INDEX idx_users_email;",
            "DROP INDEX CONCURRENTLY idx_users_email;",
            "drop-index-without-concurrently",
        ),
        (
            "not null column",
            "ALTER TABLE users ADD COLUMN tier text NOT NULL;",
            "ALTER TABLE users ADD COLUMN tier text NOT NULL DEFAULT 'free';",
            "add-not-null-column",
        ),
        (
            "constraint",
            "ALTER TABLE orders ADD CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users (id);",
            "ALTER TABLE orders ADD CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users (id) NOT VALID;",
            "constraint-without-not-valid",
        ),
        (
            "unbounded update",
            "UPDATE users SET tier = 'free';",
            "UPDATE users SET tier = 'free' WHERE id BETWEEN 1 AND 1000;",
            "unbounded-dml",
        ),
        (
            "reindex",
            "REINDEX TABLE users;",
            "REINDEX TABLE CONCURRENTLY users;",
            "vacuum-full",
        ),
    ];
    for (name, dangerous, safe, rule) in cases {
        let flagged = rules(&format!("{SAFE_HEADER}{dangerous}"));
        assert!(
            has(&flagged, rule),
            "{name}: expected {rule}, got {}",
            flagged.join(",")
        );
        let clean = rules(&format!("{SAFE_HEADER}{safe}"));
        assert!(
            !has(&clean, rule),
            "{name}: safe form must not be flagged, got {}",
            clean.join(",")
        );
    }
}

#[test]
fn irreversible_operations_are_p1() {
    let found = scan(
        &format!("{SAFE_HEADER}ALTER TABLE users DROP COLUMN legacy_flag;\n"),
        "migrations/002_drop.sql",
    );
    let drop = found
        .iter()
        .find(|f| f.rule == "drop-column")
        .expect("drop-column must be found");
    assert_eq!(drop.severity, "P1", "destroying data is P1");
    assert!(
        drop.instead.to_lowercase().contains("expand-contract"),
        "must name the safe alternative, not just say no"
    );
}

#[test]
fn rename_is_flagged_as_not_backward_compatible() {
    assert!(has(
        &rules(&format!(
            "{SAFE_HEADER}ALTER TABLE users RENAME COLUMN email TO email_address;"
        )),
        "rename"
    ));
}

#[test]
fn column_type_change_is_flagged() {
    assert!(has(
        &rules(&format!(
            "{SAFE_HEADER}ALTER TABLE users ALTER COLUMN id TYPE bigint;"
        )),
        "column-type-change"
    ));
}

#[test]
fn truncate_and_drop_table_are_flagged() {
    assert!(has(
        &rules(&format!("{SAFE_HEADER}TRUNCATE events;")),
        "truncate"
    ));
    assert!(has(
        &rules(&format!("{SAFE_HEADER}DROP TABLE events;")),
        "drop-table"
    ));
}

#[test]
fn sql_comments_describing_a_danger_are_not_findings() {
    // strip_noncode must blank `--` comments, or documenting the plan trips the linter.
    let flagged = rules(&format!(
        "{SAFE_HEADER}-- We will DROP COLUMN legacy_flag in a later release, once nothing reads it.\n\
         -- Do NOT add an index without CONCURRENTLY here.\n\
         ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n"
    ));
    assert_eq!(
        flagged,
        Vec::<String>::new(),
        "comments must not be scanned as code, got {}",
        flagged.join(",")
    );
}

#[test]
fn a_string_literal_mentioning_a_danger_is_not_a_finding() {
    let flagged = rules(&format!(
        "{SAFE_HEADER}INSERT INTO audit (note) VALUES ('planned: DROP COLUMN legacy_flag next release');\n"
    ));
    assert!(!has(&flagged, "drop-column"), "got {}", flagged.join(","));
}

#[test]
fn ddl_mixed_with_a_backfill_in_one_migration_is_flagged() {
    let flagged = rules(&format!(
        "{SAFE_HEADER}ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n\
         UPDATE users SET tier = 'paid' WHERE subscribed = true;\n"
    ));
    assert!(
        has(&flagged, "ddl-and-backfill-together"),
        "got {}",
        flagged.join(",")
    );
}

#[test]
fn a_missing_lock_timeout_is_flagged_on_sql_ddl() {
    assert!(has(
        &rules("ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n"),
        "no-lock-timeout"
    ));
}

#[test]
fn a_pure_backfill_migration_is_not_asked_for_a_lock_timeout() {
    let flagged = rules("UPDATE users SET tier = 'free' WHERE id < 1000;\n");
    assert!(
        !has(&flagged, "no-lock-timeout"),
        "no DDL here, got {}",
        flagged.join(",")
    );
}

#[test]
fn orm_helpers_are_recognised_not_just_raw_sql() {
    let rb = rules_in(
        "def up\n  remove_column :users, :legacy_flag\nend\n",
        "db/migrate/003_x.rb",
    );
    assert!(
        has(&rb, "drop-column"),
        "rails remove_column, got {}",
        rb.join(",")
    );
}

#[test]
fn an_alembic_migration_with_no_downgrade_is_flagged_as_irreversible() {
    let flagged = rules_in(
        "def upgrade():\n    op.execute('ALTER TABLE users ADD COLUMN tier text DEFAULT 1')\n",
        "versions/004_add_tier.py",
    );
    assert!(has(&flagged, "no-rollback"), "got {}", flagged.join(","));
}

#[test]
fn an_alembic_migration_with_a_downgrade_is_not_flagged() {
    let flagged = rules_in(
        "def upgrade():\n    op.add_column('users', 'tier')\n\ndef downgrade():\n    op.drop_column('users', 'tier')\n",
        "versions/005_add_tier.py",
    );
    assert!(!has(&flagged, "no-rollback"), "got {}", flagged.join(","));
}

#[test]
fn migration_file_detection_covers_the_common_tools_and_excludes_ordinary_code() {
    let yes = [
        "migrations/001_init.sql",
        "db/migrate/20260101_add_users.rb",
        "alembic/versions/abc123_add_col.py",
        "prisma/migrations/20260101_init/migration.sql",
        "supabase/migrations/001_init.sql",
        "V2__add_index.sql",
        "db/structure.sql",
    ];
    let no = [
        "src/users.ts",
        "tests/user.test.ts",
        "README.md",
        "src/migrations.md",
        "lib/migrate.go",
    ];
    for p in yes {
        assert!(is_migration_file(p), "{p} should be a migration file");
    }
    for p in no {
        assert!(!is_migration_file(p), "{p} should NOT be a migration file");
    }
}

#[test]
fn every_rule_names_a_safe_alternative() {
    // A rule that says "don't" without saying "instead" gets ignored under deadline.
    let found = scan(
        &[
            "CREATE INDEX i ON t (c);",
            "ALTER TABLE t DROP COLUMN c;",
            "ALTER TABLE t RENAME COLUMN a TO b;",
            "TRUNCATE t;",
        ]
        .join("\n"),
        "migrations/006_all.sql",
    );
    assert!(found.len() >= 4);
    for f in found {
        assert!(
            f.instead.chars().count() > 20,
            "{} must name a real alternative, got \"{}\"",
            f.rule,
            f.instead
        );
        assert!(
            f.danger.chars().count() > 20,
            "{} must explain the danger, got \"{}\"",
            f.rule,
            f.danger
        );
    }
}

#[test]
fn a_bounded_multi_line_update_is_not_flagged_where_on_a_later_line() {
    // Regression: unbounded-dml only inspected the opening line, so ordinary multi-line SQL
    // style produced a false P1 — the fastest way to get a linter switched off.
    let flagged = rules(&format!(
        "{SAFE_HEADER}UPDATE users\nSET tier = 'free'\nWHERE id > 100;\n"
    ));
    assert!(
        !has(&flagged, "unbounded-dml"),
        "bounded UPDATE must not fire, got {}",
        flagged.join(",")
    );
}

#[test]
fn a_genuinely_unbounded_multi_line_update_is_still_flagged() {
    let flagged = rules(&format!("{SAFE_HEADER}UPDATE users\nSET tier = 'free';\n"));
    assert!(has(&flagged, "unbounded-dml"), "got {}", flagged.join(","));
}

#[test]
fn a_sql_escaped_quote_literal_must_not_blank_the_rest_of_the_file() {
    // P0 regression: a legal escaped-quote default value began with three consecutive quotes,
    // which matched the triple-quote branch and opened an unbounded blanking region. Four P1
    // destructive operations became invisible and the file reported a single P2.
    let flagged = rules(
        "ALTER TABLE widgets ADD COLUMN q TEXT DEFAULT '''';\n\
         ALTER TABLE users RENAME COLUMN email TO email_address;\n\
         DROP TABLE legacy_accounts;\n\
         ALTER TABLE orders DROP COLUMN status;\n",
    );
    for rule in ["rename", "drop-table", "drop-column"] {
        assert!(
            has(&flagged, rule),
            "{rule} must still be visible, got {}",
            flagged.join(",")
        );
    }
}

#[cfg(unix)]
#[test]
fn an_unreadable_migration_is_reported_never_treated_as_safe() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let full = write_in(dir.path(), "migrations/001.sql", "DROP TABLE users;\n");
    fs::set_permissions(&full, fs::Permissions::from_mode(0o000)).unwrap();
    let found: Vec<String> = scan_migration(&full).into_iter().map(|f| f.rule).collect();
    fs::set_permissions(&full, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        has(&found, "unreadable"),
        "expected an unreadable finding, got {}",
        if found.is_empty() {
            "none".to_string()
        } else {
            found.join(",")
        }
    );
}
