/**
 * Contract tests for the analytics SQL linter.
 *
 * Run: node --experimental-strip-types --test tests/check-analytics-sql.test.ts
 *
 * Both halves for every rule. The must-NOT half carries the weight: this linter runs over
 * analytics code, which is full of BETWEEN on numbers, division by literals, and DISTINCT used
 * legitimately. A rule that fires on correct SQL gets the whole checker switched off.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { scanSql } from "../plugins/viby-toolkit/skills/analytics/scripts/check-analytics-sql.ts";

function rules(sql: string): string[] {
  return scanSql("q.sql", sql).map((f) => f.rule);
}

test("BETWEEN on a timestamp range is P1 — closed-closed double-counts the boundary", () => {
  assert.ok(rules("SELECT COUNT(id) FROM orders WHERE created_at BETWEEN start_ts AND end_ts;").includes("between-on-time"));
});

test("BETWEEN on numbers is NOT flagged", () => {
  assert.ok(!rules("SELECT * FROM t WHERE score BETWEEN 1 AND 10;").includes("between-on-time"), "numeric ranges are fine");
});

test("division by a column with no guard is P1", () => {
  assert.ok(rules("SELECT conversions / sessions AS rate FROM daily;").includes("divide-without-guard"));
});

test("division wrapped in NULLIF is NOT flagged", () => {
  assert.ok(!rules("SELECT conversions / NULLIF(sessions, 0) AS rate FROM daily;").includes("divide-without-guard"));
});

test("division by a literal is NOT flagged", () => {
  assert.ok(!rules("SELECT cents / 100 AS dollars FROM payments;").includes("divide-without-guard"));
});

test("COUNT(*) in a joined query is P1 — a fan-out multiplies it", () => {
  assert.ok(
    rules("SELECT COUNT(*) FROM users u LEFT JOIN orders o ON o.user_id = u.id;").includes("count-star-with-join"),
  );
});

test("COUNT(*) with no join is NOT flagged", () => {
  assert.ok(!rules("SELECT COUNT(*) FROM users;").includes("count-star-with-join"));
});

test("NOW() in a metric query is flagged as unreproducible", () => {
  assert.ok(rules("SELECT COUNT(id) FROM orders WHERE created_at > NOW() - INTERVAL '7 days';").includes("now-in-definition"));
});

test("= NULL is P1 — it silently matches nothing", () => {
  const f = scanSql("q.sql", "SELECT id FROM users WHERE deleted_at = NULL;");
  const hit = f.find((x) => x.rule === "null-equality");
  assert.ok(hit);
  assert.equal(hit.severity, "P1");
});

test("IS NULL is NOT flagged", () => {
  assert.ok(!rules("SELECT id FROM users WHERE deleted_at IS NULL;").includes("null-equality"));
});

test("plain UNION is flagged for silent dedup; UNION ALL is not", () => {
  assert.ok(rules("SELECT a FROM t1 UNION SELECT a FROM t2;").includes("union-dedupes"));
  assert.ok(!rules("SELECT a FROM t1 UNION ALL SELECT a FROM t2;").includes("union-dedupes"));
});

test("DATE_TRUNC without a timezone is flagged; with AT TIME ZONE it is not", () => {
  assert.ok(rules("SELECT DATE_TRUNC('day', created_at) AS d, COUNT(id) FROM orders GROUP BY 1;").includes("date-trunc-without-zone"));
  assert.ok(
    !rules("SELECT DATE_TRUNC('day', created_at AT TIME ZONE 'Europe/Bucharest') AS d FROM orders GROUP BY 1;").includes(
      "date-trunc-without-zone",
    ),
  );
});

test("FLOAT on a money column is flagged; NUMERIC is not", () => {
  assert.ok(rules("CREATE TABLE p (total_amount DOUBLE PRECISION);").includes("float-money"));
  assert.ok(!rules("CREATE TABLE p (total_amount NUMERIC(12,2));").includes("float-money"));
  assert.ok(!rules("CREATE TABLE m (ratio DOUBLE PRECISION);").includes("float-money"), "a non-money float is fine");
});

test("SELECT * is flagged, but COUNT(*) is not mistaken for it", () => {
  assert.ok(rules("SELECT * FROM orders;").includes("select-star"));
  assert.ok(!rules("SELECT COUNT(*) FROM orders;").includes("select-star"));
});

test("DISTINCT over a joined aggregate is flagged as papering over a fan-out", () => {
  assert.ok(
    rules("SELECT DISTINCT u.id, SUM(o.total) FROM users u JOIN orders o ON o.user_id = u.id GROUP BY 1;").includes(
      "distinct-over-aggregate",
    ),
  );
});

test("DISTINCT with no aggregate and no join is NOT flagged", () => {
  assert.ok(!rules("SELECT DISTINCT country FROM users;").includes("distinct-over-aggregate"));
});

test("an unbounded aggregate over an events table is flagged; a bounded one is not", () => {
  assert.ok(rules("SELECT COUNT(id) FROM events;").includes("unbounded-fact-scan"));
  assert.ok(
    !rules("SELECT COUNT(id) FROM events WHERE occurred_at >= '2026-01-01' AND occurred_at < '2026-02-01';").includes(
      "unbounded-fact-scan",
    ),
    "a stated window clears it",
  );
});

test("a pattern inside a comment or a string is NOT a finding", () => {
  // Decide on parsed SQL, never raw text — the rule this repo has re-learned four times.
  const sql = [
    "-- WHERE created_at BETWEEN a AND b  (the old version, kept for reference)",
    "SELECT label FROM t WHERE label = 'total_amount DOUBLE PRECISION';",
  ].join("\n");
  assert.deepEqual(rules(sql), [], `comments and strings must be inert, got ${rules(sql).join()}`);
});

test("a column merely NAMED like a keyword is not a finding", () => {
  assert.ok(!rules("SELECT revenue_between_dates FROM t;").includes("between-on-time"));
});

test("an unreadable file is reported, never silently clean", () => {
  const f = scanSql("/nope/missing.sql");
  assert.equal(f[0]?.rule, "unreadable");
});

test("CLI: findings exit 1, clean exits 0, nothing to scan exits 2", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "analytics", "scripts", "check-analytics-sql.ts",
  );
  const run = (args: string[], cwd: string): { status: number | null; stdout: string } => {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, ...args],
      { cwd, encoding: "utf8" },
    );
    return { status: p.status, stdout: p.stdout ?? "" };
  };

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sqlint-"));
  try {
    fs.writeFileSync(path.join(dir, "bad.sql"), "SELECT COUNT(id) FROM o WHERE created_at BETWEEN a AND b;\n");
    fs.writeFileSync(path.join(dir, "good.sql"), "SELECT COUNT(id) FROM o WHERE created_at >= a AND created_at < b;\n");
    assert.equal(run(["bad.sql", "--quiet"], dir).status, 1);
    assert.equal(run(["good.sql", "--quiet"], dir).status, 0);
    const empty = fs.mkdtempSync(path.join(os.tmpdir(), "sqlint-empty-"));
    try {
      assert.equal(run(["--all", "--quiet"], empty).status, 2);
    } finally {
      fs.rmSync(empty, { recursive: true, force: true });
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("CLI: the report says these failures are silent, and that syntax is not meaning", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "analytics", "scripts", "check-analytics-sql.ts",
  );
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "sqlint-"));
  try {
    fs.writeFileSync(path.join(dir, "a.sql"), "SELECT COUNT(id) FROM o WHERE d >= a AND d < b;\n");
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, "a.sql"],
      { cwd: dir, encoding: "utf8" },
    );
    assert.match(p.stdout ?? "", /None of these fail loudly/);
    assert.match(p.stdout ?? "", /not whether the metric answers the question/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("NOW() in a MIGRATION is not a finding — reading the clock there is correct", () => {
  // Measured: on a real repo of 62 migrations this rule fired 117 times, 93% of all findings, every
  // one legitimate SQL. Precision over coverage — a rule that floods takes the linter down with it.
  for (const sql of [
    "ALTER TABLE users ADD COLUMN created_at TIMESTAMPTZ DEFAULT NOW();",
    "UPDATE users SET migrated_at = NOW() WHERE migrated_at IS NULL;",
    "INSERT INTO audit (at) VALUES (NOW());",
  ]) {
    assert.ok(!rules(sql).includes("now-in-definition"), `must not fire on: ${sql}`);
  }
});

test("NOW() in a metric query IS still a finding", () => {
  assert.ok(
    rules("SELECT COUNT(id) FROM orders WHERE created_at > NOW() - INTERVAL '7 days';").includes("now-in-definition"),
    "the must-flag half of the same rule",
  );
});

test("SET col = NULL is an assignment, not a broken comparison", () => {
  assert.ok(!rules("UPDATE users SET deleted_at = NULL WHERE id = 1;").includes("null-equality"));
  assert.ok(rules("SELECT id FROM users WHERE deleted_at != NULL;").includes("null-equality"), "comparison still flagged");
});
