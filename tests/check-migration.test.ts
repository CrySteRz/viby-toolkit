/**
 * Contract tests for the migration safety linter.
 *
 * Run: node --experimental-strip-types --test tests/check-migration.test.ts
 *
 * Both halves pinned: the dangerous pattern must be caught, AND its safe alternative must
 * NOT be flagged. The second half is what makes the linter usable — a linter that flags
 * `CREATE INDEX CONCURRENTLY` teaches you to skip the linter.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { scanMigration, isMigrationFile } from "../plugins/viby-toolkit/skills/schema/scripts/check-migration.ts";

function rules(source: string, filename = "migrations/001_change.sql"): string[] {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mig-"));
  try {
    const full = path.join(dir, filename);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, source);
    return scanMigration(full).map((f) => f.rule);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const SAFE_HEADER = "SET lock_timeout = '5s';\n";

test("dangerous pattern is caught, safe alternative is not", () => {
  const cases: Array<[name: string, dangerous: string, safe: string, rule: string]> = [
    [
      "index",
      "CREATE INDEX idx_users_email ON users (email);",
      "CREATE INDEX CONCURRENTLY idx_users_email ON users (email);",
      "index-without-concurrently",
    ],
    [
      "drop index",
      "DROP INDEX idx_users_email;",
      "DROP INDEX CONCURRENTLY idx_users_email;",
      "drop-index-without-concurrently",
    ],
    [
      "not null column",
      "ALTER TABLE users ADD COLUMN tier text NOT NULL;",
      "ALTER TABLE users ADD COLUMN tier text NOT NULL DEFAULT 'free';",
      "add-not-null-column",
    ],
    [
      "constraint",
      "ALTER TABLE orders ADD CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users (id);",
      "ALTER TABLE orders ADD CONSTRAINT fk_user FOREIGN KEY (user_id) REFERENCES users (id) NOT VALID;",
      "constraint-without-not-valid",
    ],
    [
      "unbounded update",
      "UPDATE users SET tier = 'free';",
      "UPDATE users SET tier = 'free' WHERE id BETWEEN 1 AND 1000;",
      "unbounded-dml",
    ],
    [
      "reindex",
      "REINDEX TABLE users;",
      "REINDEX TABLE CONCURRENTLY users;",
      "vacuum-full",
    ],
  ];
  for (const [name, dangerous, safe, rule] of cases) {
    const flagged = rules(SAFE_HEADER + dangerous);
    assert.ok(flagged.includes(rule), `${name}: expected ${rule}, got ${flagged.join() || "none"}`);
    const clean = rules(SAFE_HEADER + safe);
    assert.ok(!clean.includes(rule), `${name}: safe form must not be flagged, got ${clean.join()}`);
  }
});

test("irreversible operations are P1", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mig-"));
  try {
    const full = path.join(dir, "migrations/002_drop.sql");
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, SAFE_HEADER + "ALTER TABLE users DROP COLUMN legacy_flag;\n");
    const found = scanMigration(full);
    const drop = found.find((f) => f.rule === "drop-column");
    assert.ok(drop, "drop-column must be found");
    assert.equal(drop.severity, "P1", "destroying data is P1");
    assert.match(drop.instead, /expand-contract/i, "must name the safe alternative, not just say no");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("rename is flagged as not backward compatible", () => {
  assert.ok(rules(SAFE_HEADER + "ALTER TABLE users RENAME COLUMN email TO email_address;").includes("rename"));
});

test("column type change is flagged", () => {
  assert.ok(rules(SAFE_HEADER + "ALTER TABLE users ALTER COLUMN id TYPE bigint;").includes("column-type-change"));
});

test("truncate and drop table are flagged", () => {
  assert.ok(rules(SAFE_HEADER + "TRUNCATE events;").includes("truncate"));
  assert.ok(rules(SAFE_HEADER + "DROP TABLE events;").includes("drop-table"));
});

test("SQL comments describing a danger are not findings", () => {
  // stripNoncode must blank `--` comments, or documenting the plan trips the linter.
  const flagged = rules(
    SAFE_HEADER +
      "-- We will DROP COLUMN legacy_flag in a later release, once nothing reads it.\n" +
      "-- Do NOT add an index without CONCURRENTLY here.\n" +
      "ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n",
  );
  assert.deepEqual(flagged, [], `comments must not be scanned as code, got ${flagged.join()}`);
});

test("a string literal mentioning a danger is not a finding", () => {
  const flagged = rules(
    SAFE_HEADER + "INSERT INTO audit (note) VALUES ('planned: DROP COLUMN legacy_flag next release');\n",
  );
  assert.ok(!flagged.includes("drop-column"), `got ${flagged.join()}`);
});

test("DDL mixed with a backfill in one migration is flagged", () => {
  const flagged = rules(
    SAFE_HEADER +
      "ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n" +
      "UPDATE users SET tier = 'paid' WHERE subscribed = true;\n",
  );
  assert.ok(flagged.includes("ddl-and-backfill-together"), `got ${flagged.join()}`);
});

test("a missing lock_timeout is flagged on SQL DDL", () => {
  assert.ok(rules("ALTER TABLE users ADD COLUMN tier text DEFAULT 'free';\n").includes("no-lock-timeout"));
});

test("a pure backfill migration is not asked for a lock_timeout", () => {
  const flagged = rules("UPDATE users SET tier = 'free' WHERE id < 1000;\n");
  assert.ok(!flagged.includes("no-lock-timeout"), `no DDL here, got ${flagged.join()}`);
});

test("ORM helpers are recognised, not just raw SQL", () => {
  const rb = rules("def up\n  remove_column :users, :legacy_flag\nend\n", "db/migrate/003_x.rb");
  assert.ok(rb.includes("drop-column"), `rails remove_column, got ${rb.join()}`);
});

test("an alembic migration with no downgrade is flagged as irreversible", () => {
  const flagged = rules(
    "def upgrade():\n    op.execute('ALTER TABLE users ADD COLUMN tier text DEFAULT 1')\n",
    "versions/004_add_tier.py",
  );
  assert.ok(flagged.includes("no-rollback"), `got ${flagged.join()}`);
});

test("an alembic migration WITH a downgrade is not flagged", () => {
  const flagged = rules(
    "def upgrade():\n    op.add_column('users', 'tier')\n\ndef downgrade():\n    op.drop_column('users', 'tier')\n",
    "versions/005_add_tier.py",
  );
  assert.ok(!flagged.includes("no-rollback"), `got ${flagged.join()}`);
});

test("migration file detection covers the common tools and excludes ordinary code", () => {
  const yes = [
    "migrations/001_init.sql",
    "db/migrate/20260101_add_users.rb",
    "alembic/versions/abc123_add_col.py",
    "prisma/migrations/20260101_init/migration.sql",
    "supabase/migrations/001_init.sql",
    "V2__add_index.sql",
    "db/structure.sql",
  ];
  const no = ["src/users.ts", "tests/user.test.ts", "README.md", "src/migrations.md", "lib/migrate.go"];
  for (const p of yes) assert.ok(isMigrationFile(p), `${p} should be a migration file`);
  for (const p of no) assert.ok(!isMigrationFile(p), `${p} should NOT be a migration file`);
});

test("every rule names a safe alternative", () => {
  // A rule that says "don't" without saying "instead" gets ignored under deadline.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mig-"));
  try {
    const full = path.join(dir, "migrations/006_all.sql");
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(
      full,
      [
        "CREATE INDEX i ON t (c);",
        "ALTER TABLE t DROP COLUMN c;",
        "ALTER TABLE t RENAME COLUMN a TO b;",
        "TRUNCATE t;",
      ].join("\n"),
    );
    const found = scanMigration(full);
    assert.ok(found.length >= 4);
    for (const f of found) {
      assert.ok(f.instead.length > 20, `${f.rule} must name a real alternative, got "${f.instead}"`);
      assert.ok(f.danger.length > 20, `${f.rule} must explain the danger, got "${f.danger}"`);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a bounded multi-line UPDATE is not flagged (WHERE on a later line)", () => {
  // Regression: unbounded-dml only inspected the opening line, so ordinary multi-line SQL
  // style produced a false P1 — the fastest way to get a linter switched off.
  const flagged = rules(SAFE_HEADER + "UPDATE users\nSET tier = 'free'\nWHERE id > 100;\n");
  assert.ok(!flagged.includes("unbounded-dml"), `bounded UPDATE must not fire, got ${flagged.join()}`);
});

test("a genuinely unbounded multi-line UPDATE is still flagged", () => {
  const flagged = rules(SAFE_HEADER + "UPDATE users\nSET tier = 'free';\n");
  assert.ok(flagged.includes("unbounded-dml"), `got ${flagged.join()}`);
});

test("a SQL escaped-quote literal must not blank the rest of the file", () => {
  // P0 regression: a legal escaped-quote default value began with three consecutive quotes,
  // which matched the triple-quote branch and opened an unbounded blanking region. Four P1
  // destructive operations became invisible and the file reported a single P2.
  const flagged = rules(
    "ALTER TABLE widgets ADD COLUMN q TEXT DEFAULT '''';\n" +
      "ALTER TABLE users RENAME COLUMN email TO email_address;\n" +
      "DROP TABLE legacy_accounts;\n" +
      "ALTER TABLE orders DROP COLUMN status;\n",
  );
  for (const rule of ["rename", "drop-table", "drop-column"]) {
    assert.ok(flagged.includes(rule), `${rule} must still be visible, got ${flagged.join()}`);
  }
});

test("an unreadable migration is reported, never treated as safe", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mig-noperm-"));
  try {
    const full = path.join(dir, "migrations", "001.sql");
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, "DROP TABLE users;\n");
    fs.chmodSync(full, 0o000);
    const found = scanMigration(full).map((f) => f.rule);
    fs.chmodSync(full, 0o644);
    assert.ok(found.includes("unreadable"), `expected an unreadable finding, got ${found.join() || "none"}`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
