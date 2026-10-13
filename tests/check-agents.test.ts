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
  grantsWriting,
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

test("REAL AGENTS: all seven shipped agent files pass clean", () => {
  const { agents, findings: f } = checkAgents(REAL_AGENTS_DIR);
  assert.equal(agents.length, 7, `expected 7 real agent files, got ${agents.length}`);
  assert.deepEqual(f, [], `real agents must pass clean, got ${JSON.stringify(f, null, 2)}`);
});

/** One scalar frontmatter field of a shipped agent, or undefined when the key is absent. */
function realField(agent: string, key: string): string | undefined {
  const text = fs.readFileSync(path.join(REAL_AGENTS_DIR, `${agent}.md`), "utf8");
  const fm = text.slice(3, text.indexOf("\n---", 3));
  return new RegExp(`^${key}:[ \\t]*(.*)$`, "m").exec(fm)?.[1]?.trim();
}

// The six tools default workflow agents never call (study5: 1 Artifact call in 171,522 tool calls,
// Skill twice in 4,088 agents, ListAgents 16 times, the other three never). Exactly these, so a
// widened list cannot quietly take ToolSearch, Bash or an MCP tool away from every workflow stage.
const WORKER_DENIED = ["Artifact", "Skill", "SendFeedback", "ShareOnboardingGuide", "ReportFindings", "ListAgents"];

test("WORKER: denies exactly the six never-called tools and keeps every other tool", () => {
  const worker = checkAgents(REAL_AGENTS_DIR).agents.find((a) => a.name === "worker");
  assert.ok(worker, "agents/worker.md must exist and be named worker");
  assert.deepEqual([...worker.disallowedTools].sort(), [...WORKER_DENIED].sort());
  assert.deepEqual(worker.tools, [], "an allowlist would drop MCP and deferred tools (ToolSearch) from every stage");
});

test("WORKER: no maxTurns and no model, so it neither dies at a cap nor overrides the stage's model", () => {
  assert.equal(realField("worker", "maxTurns"), undefined, "81 of 90 failed typed agents stopped exactly at maxTurns");
  assert.equal(realField("worker", "model"), undefined, "the script's per-stage model must decide");
});

test("WORKER: pins a moderate default effort, so a call that omits effort no longer inherits the session's", () => {
  const effort = realField("worker", "effort");
  assert.ok(effort !== undefined, "without it, 74% of workflow agents ran at the session's effort, often max");
  assert.ok(["low", "medium", "high"].includes(effort), `a default for every stage must not be xhigh or max, got ${effort}`);
});

// Floors from study6/M1 (1,455 typed-agent runs): ceil5(p99 of successful runs x 1.5). Every run cut
// at its cap stopped at exactly the cap, and most returned an empty result. Lowering one re-opens that.
const MAX_TURNS_FLOOR: Record<string, number> = {
  implementer: 60, researcher: 45, reviewer: 40, scout: 30, skeptic: 30, debugger: 25,
};

test("CAPS: no capped agent's maxTurns is below its measured floor", () => {
  for (const [agent, floor] of Object.entries(MAX_TURNS_FLOOR)) {
    const raw = realField(agent, "maxTurns");
    assert.ok(raw !== undefined, `${agent} must keep a maxTurns`);
    assert.ok(Number(raw) >= floor, `${agent} maxTurns ${raw} is below the measured floor ${floor}`);
  }
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

test("a denylist-only agent (disallowedTools, no tools) is NOT flagged as having no tools", () => {
  const agent = loadAgents(library({
    lean: "---\nname: lean\ndescription: Use this for a stage.\ndisallowedTools: Artifact, Skill\n---\n" + GOOD_BODY,
  }))[0];
  assert.ok(agent, "fixture agent should load");
  assert.deepEqual(agent.disallowedTools, ["Artifact", "Skill"]);
  assert.ok(!checkAgent(agent).some((x) => x.check === "no-tools"), "a denylist states what the agent may touch");
});

test("a denylist-only agent inherits Write/Edit, so an unisolated parallel claim is flagged P1", () => {
  const f = findings({
    lean: "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Artifact\n---\n" + GOOD_BODY,
  });
  const hit = f.find((x) => x.check === "parallel-write-without-isolation");
  assert.ok(hit, `expected parallel-write-without-isolation, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P1");
});

test("a denylist-only agent that denies Write, Edit and NotebookEdit is NOT held to the isolation rule", () => {
  const f = findings({
    lean: "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Write, Edit, NotebookEdit\n---\n" + GOOD_BODY,
  });
  assert.ok(!f.some((x) => x.check === "parallel-write-without-isolation"), `got ${JSON.stringify(f)}`);
});

test("a denylist-only agent that denies only Write and Edit still writes via NotebookEdit, so it is flagged P1", () => {
  const f = findings({
    lean: "---\nname: lean\ndescription: Dispatch several in parallel.\ndisallowedTools: Write, Edit\n---\n" + GOOD_BODY,
  });
  const hit = f.find((x) => x.check === "parallel-write-without-isolation");
  assert.ok(hit, `expected parallel-write-without-isolation, got ${JSON.stringify(f)}`);
  assert.equal(hit.severity, "P1");
});

test("grantsWriting: allowlist, denylist, both, and neither", () => {
  assert.equal(grantsWriting({ tools: ["Read", "Edit"], disallowedTools: [] }), true);
  assert.equal(grantsWriting({ tools: ["Read", "Edit"], disallowedTools: ["Edit"] }), false);
  assert.equal(grantsWriting({ tools: ["Read", "Bash"], disallowedTools: ["Write", "Edit"] }), false);
  assert.equal(grantsWriting({ tools: [], disallowedTools: ["Skill"] }), true);
  assert.equal(grantsWriting({ tools: [], disallowedTools: ["Write", "Edit"] }), true);
  assert.equal(grantsWriting({ tools: [], disallowedTools: ["Write", "Edit", "NotebookEdit"] }), false);
  assert.equal(grantsWriting({ tools: ["Read", "NotebookEdit"], disallowedTools: [] }), true);
  assert.equal(grantsWriting({ tools: [], disallowedTools: [] }), false);
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
