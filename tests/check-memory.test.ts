/**
 * Contract tests for the memory auditor.
 *
 * Run: node --experimental-strip-types --test tests/check-memory.test.ts
 *
 * Both halves. Two of the must-NOT cases are real false positives this checker produced on the first
 * run against five real memory stores — it reported 23 stale references, almost all of them simply
 * resolved against the wrong project root.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { auditEntry, auditStore } from "../plugins/viby-toolkit/skills/brain/scripts/check-memory.ts";

const GOOD = [
  "---",
  "name: build-needs-node-22",
  "---",
  "",
  "The build fails on Node 20 because type stripping landed in 22.6. Verified 2026-07-29 by running",
  "`npm run check` on both. Outcome: pinning the engine field fixed it.",
  "",
].join("\n");

function store(files: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mem-"));
  for (const [n, body] of Object.entries(files)) fs.writeFileSync(path.join(dir, n), body);
  return dir;
}

test("a well-formed entry produces no findings", () => {
  assert.deepEqual(auditEntry("a.md", GOOD, os.tmpdir()).map((f) => f.check), []);
});

test("an undated entry is flagged — it can never be retired on evidence", () => {
  const f = auditEntry("a.md", "The build needs Node 22 because type stripping landed there.\n", os.tmpdir());
  assert.ok(f.some((x) => x.check === "undated"));
});

test("an entry with no provenance is flagged", () => {
  const f = auditEntry("a.md", "Deploys happen on Fridays. 2026-07-29\n", os.tmpdir()).map((x) => x.check);
  assert.ok(f.includes("no-provenance"), f.join());
});

test("an entry recording hearsay as fact is flagged as unverified", () => {
  const text = "2026-07-29 — the user said the API is rate limited, so avoid retries.\n";
  assert.ok(auditEntry("a.md", text, os.tmpdir()).some((x) => x.check === "unverified-claim"));
});

test("a stale path reference is P1", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "root-"));
  try {
    fs.mkdirSync(path.join(dir, "src"), { recursive: true });
    fs.writeFileSync(path.join(dir, "src", "real.ts"), "export const a = 1;\n");
    // One reference resolves, so the root is trusted; the other is genuinely gone.
    // (A reference needs a slash to count — a bare filename is too ambiguous to resolve.)
    const text = "2026-07-29, verified: see `src/real.ts` and `src/deleted.ts`.\n";
    const f = auditEntry("a.md", text, dir);
    const hit = f.find((x) => x.check === "stale-reference");
    assert.ok(hit, JSON.stringify(f));
    assert.equal(hit.severity, "P1");
    assert.match(hit.problem, /deleted\.ts/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("when NOTHING resolves, the root is wrong — say so instead of inventing stale findings", () => {
  // The real false positive: five stores audited against one repo root produced 23 "stale"
  // references, none of them stale. A comparison that could not happen must not read as a finding.
  const text = "2026-07-29, verified: see `src/a.ts` and `src/b.ts` and `lib/c.ts`.\n";
  const f = auditEntry("a.md", text, path.join(os.tmpdir(), "no-such-root")).map((x) => x.check);
  assert.ok(f.includes("root-unknown"), f.join());
  assert.ok(!f.includes("stale-reference"), "must not claim staleness it could not check");
});

test("a path that resolves against an ANCESTOR of the root is not stale", () => {
  // A memory about a project legitimately cites repo-root paths, package paths and home paths.
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "root-"));
  try {
    fs.mkdirSync(path.join(root, "pkg"), { recursive: true });
    fs.writeFileSync(path.join(root, "top.md"), "x");
    fs.mkdirSync(path.join(root, "docs"), { recursive: true });
    fs.writeFileSync(path.join(root, "docs", "top.md"), "x");
    const f = auditEntry("a.md", "2026-07-29, verified `docs/top.md`.\n", path.join(root, "pkg"));
    assert.ok(!f.some((x) => x.check === "stale-reference"), JSON.stringify(f));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("placeholders in examples are not treated as references", () => {
  const f = auditEntry("a.md", "2026-07-29, verified. Put it in `path/to/your-file.ts`.\n", os.tmpdir());
  assert.ok(!f.some((x) => x.check === "stale-reference"));
});

test("an entry that grew into a document is flagged", () => {
  const long = GOOD + "\n" + "word ".repeat(1000);
  assert.ok(auditEntry("a.md", long, os.tmpdir()).some((x) => x.check === "oversized-entry"));
});

test("an entry missing from the index is P1 — retrieval failure is where memory errors are", () => {
  const dir = store({ "MEMORY.md": "# Index\n\n- [a](a.md) — the thing\n", "a.md": GOOD, "b.md": GOOD });
  try {
    const f = auditStore(dir, os.tmpdir()).findings;
    const hit = f.find((x) => x.check === "not-indexed");
    assert.ok(hit, JSON.stringify(f.map((x) => x.check)));
    assert.equal(hit.severity, "P1");
    assert.equal(hit.file, "b.md");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("an index link pointing at nothing is P1", () => {
  const dir = store({ "MEMORY.md": "- [gone](gone.md) — hook\n- [a](a.md) — hook\n", "a.md": GOOD });
  try {
    assert.ok(auditStore(dir, os.tmpdir()).findings.some((x) => x.check === "index-dangling"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a store with entries and no index at all is P1", () => {
  const dir = store({ "a.md": GOOD });
  try {
    assert.ok(auditStore(dir, os.tmpdir()).findings.some((x) => x.check === "no-index"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("near-duplicate topics are flagged for merging", () => {
  const dir = store({
    "MEMORY.md": "- [a](build-needs-node-version.md)\n- [b](build-needs-node-engine.md)\n",
    "build-needs-node-version.md": GOOD,
    "build-needs-node-engine.md": GOOD,
  });
  try {
    assert.ok(auditStore(dir, os.tmpdir()).findings.some((x) => x.check === "duplicate-topic"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("distinct topics are NOT flagged as duplicates", () => {
  const dir = store({
    "MEMORY.md": "- [a](build-needs-node-22.md)\n- [b](deploy-window-fridays.md)\n",
    "build-needs-node-22.md": GOOD,
    "deploy-window-fridays.md": GOOD,
  });
  try {
    const f = auditStore(dir, os.tmpdir()).findings.map((x) => x.check);
    assert.ok(!f.includes("duplicate-topic"), f.join());
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("an empty store is nothing-to-check, not clean", () => {
  const dir = store({});
  try {
    assert.equal(auditStore(dir, os.tmpdir()).entries, 0);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("same-project notes on the same day are NOT duplicates", () => {
  // Measured on a real store: under a `project_<name>_<topic>_<date>` convention the shared project
  // and date tokens dominated the comparison, so two genuinely different notes looked identical.
  const dir = store({
    "MEMORY.md": "- [a](project_acme_audit_2026-04-27.md)\n- [b](project_acme_qa_discovery_2026-04-27.md)\n",
    "project_acme_audit_2026-04-27.md": GOOD,
    "project_acme_qa_discovery_2026-04-27.md": GOOD,
  });
  try {
    const f = auditStore(dir, os.tmpdir()).findings.map((x) => x.check);
    assert.ok(!f.includes("duplicate-topic"), `an audit and a QA discovery are different notes: ${f.join()}`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("but genuinely duplicated topics are still caught", () => {
  const dir = store({
    "MEMORY.md": "- [a](project_acme_deploy_window_2026-04-27.md)\n- [b](project_acme_deploy_schedule_window.md)\n",
    "project_acme_deploy_window_2026-04-27.md": GOOD,
    "project_acme_deploy_schedule_window.md": GOOD,
  });
  try {
    assert.ok(auditStore(dir, os.tmpdir()).findings.some((x) => x.check === "duplicate-topic"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a single-token filename is not a duplicate of a longer, specific one", () => {
  // Reviewer finding (P2): min() made `auth.md` score 1.0 against `auth-token-refresh-race.md`.
  const dir = store({
    "MEMORY.md": "- [a](auth.md)\n- [b](auth-token-refresh-race.md)\n",
    "auth.md": GOOD,
    "auth-token-refresh-race.md": GOOD,
  });
  try {
    assert.ok(!auditStore(dir, os.tmpdir()).findings.some((x) => x.check === "duplicate-topic"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a project path is not reported stale just because a sibling memory file resolved", () => {
  // Found by running this checker on the author's own memory store: the entry cited
  // `viby-toolkit-modules.md` (a real sibling) AND `plugins/viby-toolkit/lib/strip-noncode.ts` (real,
  // but in a repo this checker was never given). One resolving sibling made the root "look right", so
  // the project path was reported as a P1 stale-reference — a confidently wrong finding about a file
  // that exists. Two namespaces, one root assumption.
  const dir = store({
    "MEMORY.md": "- [a](a.md) — hook\n",
    "a.md": "See `other.md` and `src/deep/module.ts`.\n",
    "other.md": "x\n",
  });
  const rules = auditEntry("a.md", fs.readFileSync(path.join(dir, "a.md"), "utf8"), dir).map((f) => f.check);
  assert.ok(!rules.includes("stale-reference"), `must not claim stale, got ${rules.join()}`);
});
