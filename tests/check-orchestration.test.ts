/**
 * Contract tests for the delegation-defaults checker.
 *
 * Run: node --experimental-strip-types --test tests/check-orchestration.test.ts
 *
 * Both halves pinned, as always: what must be flagged, and — the half that keeps a checker
 * trustworthy — what must NOT be. Several negative cases are false positives this checker actually
 * produced against the real library before being narrowed, including one whole rule that had to be
 * replaced when the library moved to the hybrid.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkOrchestration, checkStagedSkillsExist } from "../plugins/viby-toolkit/skills/principles/scripts/check-orchestration.ts";

function checks(skills: Record<string, string>): string[] {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orch-"));
  try {
    for (const [name, body] of Object.entries(skills)) {
      fs.mkdirSync(path.join(dir, name), { recursive: true });
      fs.writeFileSync(path.join(dir, name, "SKILL.md"), body);
    }
    return checkOrchestration(dir).map((f) => `${f.skill}:${f.check}`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test("fan-out phrased as a permission is flagged", () => {
  const found = checks({ explore: "You may dispatch several `scout` agents in parallel if it helps.\n" });
  assert.deepEqual(found, ["explore:hedged-fanout"]);
});

test("'optionally' on a fan-out line is flagged", () => {
  const found = checks({ explore: "Optionally, run one `reviewer` per dimension.\n" });
  assert.deepEqual(found, ["explore:hedged-fanout"]);
});

test("fan-out phrased as a directive is NOT flagged", () => {
  const found = checks({ explore: "Dispatch several `scout` agents in parallel, in one message.\n" });
  assert.deepEqual(found, []);
});

test("a single-stage skill need not name Workflow — this is the hybrid", () => {
  // Regression against the rule this checker used to have. Requiring Workflow everywhere flagged
  // explore/debug/plan/migrate/adopt, none of which has stages to declare.
  const found = checks({ debug: "Dispatch several `debugger` agents in parallel, in one message.\n" });
  assert.deepEqual(found, []);
});

test("a staged skill that never names Workflow is flagged", () => {
  const found = checks({ "review": "Run one `reviewer` per dimension, then one `skeptic` per finding.\n" });
  assert.deepEqual(found, ["review:unstaged-pipeline"]);
});

test("a staged skill that names Workflow is not flagged", () => {
  const found = checks({
    "review": "Author a `Workflow`: one `reviewer` per dimension, then one `skeptic` per finding.\n",
  });
  assert.deepEqual(found, []);
});

test("all three staged skills are covered by the rule", () => {
  const bare = "Run one `reviewer` per dimension.\n";
  const found = checks({ orchestrate: bare, study: bare, "review": bare });
  assert.deepEqual(found.filter((f) => f.endsWith("unstaged-pipeline")).sort(), [
    "orchestrate:unstaged-pipeline",
    "review:unstaged-pipeline",
    "study:unstaged-pipeline",
  ]);
});

test("a retrospective mention of fan-out with no named agent is NOT flagged", () => {
  const found = checks({ extend: "It described a web-search fan-out when every agent was filesystem-only.\n" });
  assert.deepEqual(found, []);
});

test("dispatching a SINGLE agent is NOT a fan-out", () => {
  const found = checks({ evaluate: "Dispatch a `researcher` rather than reading repos inline.\n" });
  assert.deepEqual(found, []);
});

test("a fenced example script does not count as an instruction", () => {
  const found = checks({
    demo: "Read inline.\n\n```js\nparallel(D.map(d => agent(d, {agentType: 'viby-toolkit:reviewer'})))\n```\n",
  });
  assert.deepEqual(found, []);
});

test("parallelising an implementer without worktree isolation is flagged", () => {
  const found = checks({ build: "Dispatch several `implementer` agents in parallel, one per node.\n" });
  assert.deepEqual(found, ["build:unisolated-parallel-write"]);
});

test("parallelising an implementer WITH worktree isolation is not flagged", () => {
  const found = checks({
    build: "Dispatch several `implementer` agents in parallel, one per node, each in its own worktree.\n",
  });
  assert.deepEqual(found, []);
});

test("principles itself is exempt — it defines the laws rather than consuming them", () => {
  const found = checks({ principles: "You may dispatch several `scout` agents in parallel.\n" });
  assert.deepEqual(found, []);
});

test("a directory with no SKILL.md is skipped rather than crashing", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orch-"));
  try {
    fs.mkdirSync(path.join(dir, "empty"), { recursive: true });
    assert.deepEqual(checkOrchestration(dir), []);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a bare 'Workflow' inside a URL does not satisfy a staged skill", () => {
  // Regression: the staged check tested the RAW file while every other rule used stripped prose, so
  // any occurrence of the word — a link, a path — hollowly satisfied it.
  const found = checks({
    study: "Run one `researcher` per angle.\n\nSee https://example.com/Workflow-diagram for background.\n",
  });
  assert.deepEqual(found, ["study:unstaged-pipeline"]);
});

test("a 'Workflow' mentioned only inside a fenced code block does not satisfy a staged skill", () => {
  const found = checks({
    study: "Run one `researcher` per angle.\n\n```js\n// no Workflow needed here\n```\n",
  });
  assert.deepEqual(found, ["study:unstaged-pipeline"]);
});

test("a backticked `Workflow` in prose does satisfy a staged skill", () => {
  const found = checks({ study: "Author a `Workflow` that runs one `researcher` per angle.\n" });
  assert.deepEqual(found, []);
});

test("a hedged fan-out is caught even when the agent name is NOT backticked", () => {
  // Regression: the hedge rule only scanned lines with a BACKTICKED agent, so formatting alone
  // exempted a softened directive from detection.
  const found = checks({ explore: "You may optionally fan out scout agents in parallel if you want more coverage.\n" });
  assert.deepEqual(found, ["explore:hedged-fanout"]);
});

test("loosening the agent match did NOT start flagging retrospective asides", () => {
  const found = checks({ extend: "It described a web-search fan-out when every agent was filesystem-only.\n" });
  assert.deepEqual(found, []);
});

test("an unrelated 'worktree' elsewhere in the file does not suppress an unisolated parallel write", () => {
  // Regression: isolation was matched against the whole document, so any stray mention counted.
  const found = checks({
    build: "Dispatch several `implementer` agents in parallel, one per node.\n" +
           "\n\n\n\n\n\nUnrelated aside: worktree isolation is discussed elsewhere.\n",
  });
  assert.deepEqual(found, ["build:unisolated-parallel-write"]);
});

test("worktree isolation alongside the instruction does suppress it", () => {
  const found = checks({
    build: "Dispatch several `implementer` agents in parallel, one per node.\nEach runs in its own worktree.\n",
  });
  assert.deepEqual(found, []);
});

test("a STAGED name with no matching skill directory fails loudly", () => {
  // `review-cluster` was renamed to `review` mid-development; a stale entry must not silently
  // retire the rule.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orch-"));
  try {
    fs.mkdirSync(path.join(dir, "orchestrate"), { recursive: true });
    fs.writeFileSync(path.join(dir, "orchestrate", "SKILL.md"), "Author a `Workflow`.\n");
    const found = checkStagedSkillsExist(dir).map((f) => `${f.skill}:${f.check}`);
    assert.ok(found.includes("review:staged-skill-missing"), `expected a missing-skill finding, got ${found.join(", ")}`);
    assert.ok(found.includes("study:staged-skill-missing"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("the real skill library passes its own rules", () => {
  const real = path.join(path.dirname(import.meta.dirname), "plugins", "viby-toolkit", "skills");
  const found = checkOrchestration(real);
  assert.deepEqual(found, [], `library regressed: ${found.map((f) => `${f.skill}:${f.check}`).join(", ")}`);
});
