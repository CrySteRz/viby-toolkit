/**
 * Contract tests for the plan validator.
 *
 * Run: node --experimental-strip-types --test tests/check-plan.test.ts
 *
 * Both halves. The rule that earns this script's existence is `unpartitioned-file`: it makes the
 * fan-out law's "you must be able to name the partition" checkable instead of aspirational.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { checkPlan, parseTasks } from "../plugins/viby-toolkit/skills/plan/scripts/check-plan.ts";

function plan(...lines: string[]): string {
  return ["# Plan", "", "## Tasks", "", ...lines, ""].join("\n");
}
function checks(text: string): string[] {
  return checkPlan(text).findings.map((f) => f.check);
}

const GOOD = plan(
  "- [ ] T1 — add the parser · files: src/parse.ts · verify: npm test -- parse · deps: —",
  "- [ ] T2 — add the writer · files: src/write.ts · verify: npm test -- write · deps: —",
  "- [ ] T3 — wire them up · files: src/index.ts · verify: npm test · deps: T1, T2",
);

test("a well-formed plan parses into tasks with files, verify and deps", () => {
  const t = parseTasks(GOOD);
  assert.equal(t.length, 3);
  assert.deepEqual(t[0]?.files, ["src/parse.ts"]);
  assert.equal(t[2]?.verify, "npm test");
  assert.deepEqual(t[2]?.deps, ["T1", "T2"]);
  assert.equal(t[0]?.done, false);
});

test("a well-formed plan produces no findings", () => {
  assert.deepEqual(checks(GOOD), [], `expected dispatchable, got ${checks(GOOD).join()}`);
});

test("a completed task is parsed as done", () => {
  const t = parseTasks(plan("- [x] T1 — done thing · files: a.ts · verify: npm test · deps: —"));
  assert.equal(t[0]?.done, true);
});

test("a task with no files is P1 — it cannot be partitioned", () => {
  const f = checkPlan(plan("- [ ] T1 — do the thing · verify: npm test · deps: —")).findings;
  const hit = f.find((x) => x.check === "no-files");
  assert.ok(hit);
  assert.equal(hit.severity, "P1");
});

test("a task with no verification is P1 — 'done' would be a feeling", () => {
  assert.ok(checks(plan("- [ ] T1 — do it · files: a.ts · deps: —")).includes("no-verify"));
});

test("THE check: two independent tasks owning the same file is P1", () => {
  // This is the partition principles §3 demands before any parallel write, made mechanical.
  const f = checkPlan(
    plan(
      "- [ ] T1 — add a field · files: src/model.ts · verify: npm test · deps: —",
      "- [ ] T2 — rename a field · files: src/model.ts · verify: npm test · deps: —",
    ),
  ).findings;
  const hit = f.find((x) => x.check === "unpartitioned-file");
  assert.ok(hit, JSON.stringify(f));
  assert.equal(hit.severity, "P1");
  assert.match(hit.problem, /src\/model\.ts/);
});

test("the same two tasks ORDERED by a dependency are fine", () => {
  // Shared ownership is only a problem when they could run at the same time.
  const f = checks(
    plan(
      "- [ ] T1 — add a field · files: src/model.ts · verify: npm test · deps: —",
      "- [ ] T2 — rename it · files: src/model.ts · verify: npm test · deps: T1",
    ),
  );
  assert.ok(!f.includes("unpartitioned-file"), `an ordered pair is safe: ${f.join()}`);
});

test("a transitive dependency also counts as ordered", () => {
  const f = checks(
    plan(
      "- [ ] T1 — first · files: src/a.ts · verify: t · deps: —",
      "- [ ] T2 — middle · files: src/b.ts · verify: t · deps: T1",
      "- [ ] T3 — last, touches a.ts again · files: src/a.ts · verify: t · deps: T2",
    ),
  );
  assert.ok(!f.includes("unpartitioned-file"), `T1 → T2 → T3 is an order: ${f.join()}`);
});

test("a file owned by three or more tasks is flagged as a structural hub", () => {
  const f = checkPlan(
    plan(
      "- [ ] T1 — a · files: src/hub.ts · verify: t · deps: —",
      "- [ ] T2 — b · files: src/hub.ts · verify: t · deps: T1",
      "- [ ] T3 — c · files: src/hub.ts · verify: t · deps: T2",
    ),
  ).findings;
  const hit = f.find((x) => x.check === "hub-file");
  assert.ok(hit, JSON.stringify(f.map((x) => x.check)));
  assert.match(hit.fix, /take the hub yourself/);
});

test("a dependency cycle is P1 — no execution order exists", () => {
  const f = checks(
    plan(
      "- [ ] T1 — a · files: a.ts · verify: t · deps: T2",
      "- [ ] T2 — b · files: b.ts · verify: t · deps: T1",
    ),
  );
  assert.ok(f.includes("dep-cycle"), f.join());
});

test("a dependency on a task that does not exist is P1", () => {
  assert.ok(checks(plan("- [ ] T1 — a · files: a.ts · verify: t · deps: T9")).includes("unknown-dep"));
});

test("a plan with no task list is exit-2 territory, not clean", () => {
  const r = checkPlan("# Plan\n\nWe will refactor the parser and then ship it.\n");
  assert.equal(r.tasks.length, 0);
  assert.deepEqual(r.findings, []);
});

test("prose that merely looks like a checkbox is not parsed as a task", () => {
  // Must-not: ordinary markdown checklists in a document are not dispatchable tasks.
  const t = parseTasks("- [ ] remember to tell the client\n- [x] coffee\n");
  assert.equal(t.length, 0, `needs an id and a title separator, got ${JSON.stringify(t)}`);
});

test("both `·` and `|` separate fields, and `deps: none` is empty", () => {
  const t = parseTasks(plan("- [ ] T1 — a | files: a.ts | verify: npm test | deps: none"));
  assert.deepEqual(t[0]?.deps, []);
  assert.deepEqual(t[0]?.files, ["a.ts"]);
});

test("CLI: a dispatchable plan exits 0 and reports how many are ready now", () => {
  const script = path.join(path.dirname(import.meta.dirname), "plugins", "viby-toolkit", "skills", "plan", "scripts", "check-plan.ts");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "plan-"));
  try {
    const p = path.join(dir, "plan.md");
    fs.writeFileSync(p, GOOD);
    const r = spawnSync(process.execPath, ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, p], { encoding: "utf8" });
    assert.equal(r.status, 0, r.stdout);
    assert.match(r.stdout, /2 ready now/);
    assert.match(r.stdout, /not that it is a good plan/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("CLI: a conflicting plan exits 1", () => {
  const script = path.join(path.dirname(import.meta.dirname), "plugins", "viby-toolkit", "skills", "plan", "scripts", "check-plan.ts");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "plan-"));
  try {
    const p = path.join(dir, "plan.md");
    fs.writeFileSync(p, plan("- [ ] T1 — a · files: x.ts · verify: t · deps: —", "- [ ] T2 — b · files: x.ts · verify: t · deps: —"));
    const r = spawnSync(process.execPath, ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, p, "--quiet"], { encoding: "utf8" });
    assert.equal(r.status, 1);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
