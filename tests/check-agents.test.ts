/**
 * Contract tests for the subagent return-size-contract check.
 *
 * Run: node --experimental-strip-types --test tests/check-agents.test.ts
 *
 * Pins both halves: a checker that never flags anything is as useless as one that flags
 * everything. These tests build minimal fixture agents to prove each rule fires on a
 * deliberately broken input AND stays silent on a compliant one, then run the check against
 * the real shipped agents to prove it isn't merely passing on toy fixtures.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  checkAgent,
  checkAgents,
  hasCitationFirstShape,
  hasCleanReportInstruction,
  hasOverflowEscape,
  loadAgents,
  parallelClaimLacksIsolation,
  statedCeiling,
} from "../plugins/viby-toolkit/skills/principles/scripts/check-agents.ts";

const REAL_AGENTS_DIR = path.join("plugins", "viby-toolkit", "agents");

function library(agents: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "agents-"));
  for (const [name, content] of Object.entries(agents)) {
    fs.writeFileSync(path.join(dir, `${name}.md`), content);
  }
  return dir;
}

function findings(agents: Record<string, string>) {
  const dir = library(agents);
  try {
    return checkAgents(dir).findings;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const GOOD_BODY = `
You do the thing.

## Return-size contract

Hard ceiling: **50 lines**. Justified because this is a narrow job.
- Report what you checked and found clean, not only what you found.
- If it doesn't fit, write detail to a scratch file and return the headline plus the path
  (two-tier return).

## Output format
- **field**: value
`;

function fixture(opts: {
  name?: string;
  description?: string;
  tools?: string;
  body?: string;
}): string {
  const name = opts.name ?? "widget";
  const description = opts.description ?? "Use this to do the widget thing.";
  const tools = opts.tools ?? "Read, Grep, Glob, Bash";
  const body = opts.body ?? GOOD_BODY;
  return `---\nname: ${name}\ndescription: ${description}\ntools: ${tools}\n---\n${body}`;
}

test("REAL AGENTS: all six shipped agent files pass clean", () => {
  const { agents, findings: f } = checkAgents(REAL_AGENTS_DIR);
  assert.equal(agents.length, 6, `expected 6 real agent files, got ${agents.length}`);
  assert.deepEqual(f, [], `real agents must pass clean, got ${JSON.stringify(f, null, 2)}`);
});

test("NOT VACUOUS: a body with no stated ceiling is flagged P1", () => {
  const f = findings({
    widget: fixture({ body: "You do the thing.\n\n## Output\n- field: value\n" }),
  });
  const hit = f.find((x) => x.check === "no-return-ceiling");
  assert.ok(hit, `expected no-return-ceiling, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P1");
});

test("a compliant body with a stated ceiling is NOT flagged for it", () => {
  const f = findings({ widget: fixture({}) });
  assert.ok(
    !f.some((x) => x.check === "no-return-ceiling"),
    `must not flag a body with a stated ceiling, got ${JSON.stringify(f)}`,
  );
});

test("statedCeiling parses the number near 'ceiling'/'cap', not an unrelated number", () => {
  assert.equal(statedCeiling("Hard ceiling: **80 lines**. Some other detail: 25 turns."), 80);
  assert.equal(statedCeiling("maxTurns: 25\n\nNo ceiling stated here, just 25 turns."), undefined);
});

test("a body missing the clean-report instruction is flagged P2", () => {
  const f = findings({
    widget: fixture({
      body: "Hard ceiling: **50 lines**.\n- write detail to a file, headline plus the path (two-tier return)\n",
    }),
  });
  const hit = f.find((x) => x.check === "no-clean-report-instruction");
  assert.ok(hit, `expected no-clean-report-instruction, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P2");
});

test("hasCleanReportInstruction: present vs absent", () => {
  assert.equal(hasCleanReportInstruction("report what you checked and found clean"), true);
  assert.equal(hasCleanReportInstruction("just report your findings"), false);
});

test("a body missing the overflow escape is flagged P2", () => {
  const f = findings({
    widget: fixture({
      body: "Hard ceiling: **50 lines**.\n- report what you checked and found clean.\n",
    }),
  });
  const hit = f.find((x) => x.check === "no-overflow-escape");
  assert.ok(hit, `expected no-overflow-escape, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P2");
});

test("hasOverflowEscape: 'two-tier' and 'headline'+'path' both count, plain text does not", () => {
  assert.equal(hasOverflowEscape("use the two-tier return"), true);
  assert.equal(hasOverflowEscape("return the headline plus the path"), true);
  assert.equal(hasOverflowEscape("just truncate it"), false);
});

test("scout/researcher with no citation-first shape are flagged P1; a generic agent is not", () => {
  const f = findings({
    scout: fixture({ name: "scout", body: "Hard ceiling: **50 lines**. found clean. two-tier return.\n" }),
    widget: fixture({ name: "widget", body: "Hard ceiling: **50 lines**. found clean. two-tier return.\n" }),
  });
  const scoutHit = f.find((x) => x.check === "not-citation-first" && x.agent === "scout");
  assert.ok(scoutHit, `expected not-citation-first for scout, got ${JSON.stringify(f)}`);
  assert.equal(scoutHit.severity, "P1");
  assert.ok(
    !f.some((x) => x.check === "not-citation-first" && x.agent === "widget"),
    "a non scout/researcher agent must not be held to the citation-first rule",
  );
});

test("hasCitationFirstShape requires BOTH a location cite AND a no-pasting rule", () => {
  assert.equal(hasCitationFirstShape("cite file:line, never paste file contents back"), true);
  assert.equal(hasCitationFirstShape("cite file:line only"), false);
  assert.equal(hasCitationFirstShape("never paste content back"), false);
});

test("Write/Edit agent claiming parallel safety without isolation is flagged P1", () => {
  const f = findings({
    writer: fixture({
      name: "writer",
      tools: "Read, Edit, Write",
      description: "Dispatch several in parallel to build the pieces.",
      body: GOOD_BODY,
    }),
  });
  const hit = f.find((x) => x.check === "parallel-write-without-isolation");
  assert.ok(hit, `expected parallel-write-without-isolation, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P1");
});

test("Write/Edit agent that names worktree isolation for parallel dispatch is NOT flagged", () => {
  const f = findings({
    writer: fixture({
      name: "writer",
      tools: "Read, Edit, Write",
      description: "Dispatch several in parallel; use worktree isolation if they'd touch the same files.",
      body: GOOD_BODY,
    }),
  });
  assert.ok(
    !f.some((x) => x.check === "parallel-write-without-isolation"),
    `must not flag isolation-qualified parallel dispatch, got ${JSON.stringify(f)}`,
  );
});

test("a read-only agent (no Write/Edit) claiming parallel safety is NOT flagged", () => {
  const f = findings({
    reader: fixture({
      name: "reader",
      tools: "Read, Grep, Glob, Bash",
      description: "Dispatch several in parallel on different areas.",
      body: GOOD_BODY,
    }),
  });
  assert.ok(
    !f.some((x) => x.check === "parallel-write-without-isolation"),
    `must not apply the isolation rule to a read-only agent, got ${JSON.stringify(f)}`,
  );
});

test("parallelClaimLacksIsolation: direct unit behaviour", () => {
  assert.equal(parallelClaimLacksIsolation("Dispatch several in parallel."), true);
  assert.equal(parallelClaimLacksIsolation("Dispatch several in parallel with worktree isolation."), false);
  assert.equal(parallelClaimLacksIsolation("No mention of concurrency at all."), false);
});

test("missing frontmatter fields (name/description/tools) are each flagged P1", () => {
  const f = findings({
    broken: "---\nname: broken\n---\nHard ceiling: **50 lines**. found clean. two-tier return.\n",
  });
  assert.ok(f.some((x) => x.check === "no-description" && x.severity === "P1"));
  assert.ok(f.some((x) => x.check === "no-tools" && x.severity === "P1"));
  assert.ok(!f.some((x) => x.check === "no-name"));
});

test("checkAgent on a single fully-broken agent surfaces every relevant finding at once", () => {
  const agent = loadAgents(library({ bad: "---\nname: bad\n---\nno contract at all here.\n" }))[0];
  assert.ok(agent, "fixture agent should load");
  const f = checkAgent(agent);
  const checks = f.map((x) => x.check).sort();
  assert.deepEqual(checks, ["no-clean-report-instruction", "no-description", "no-overflow-escape", "no-return-ceiling", "no-tools"]);
});

test("no agents found in an empty/missing directory does not throw", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "agents-empty-"));
  try {
    const { agents, findings: f } = checkAgents(dir);
    assert.equal(agents.length, 0);
    assert.deepEqual(f, []);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
