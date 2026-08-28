/**
 * viby-toolkit subagent contract check — guards the return-size contract, the mechanism
 * that makes fan-out worth doing at all.
 *
 * Subagents are the toolkit's context firewall: a scout can read a whole subsystem and the
 * caller only pays for a few lines back. Nothing enforced that split — a subagent whose body
 * never states a ceiling can (and, unwatched, will) return prose by the screenful and quietly
 * undo the mechanism the whole design depends on. This is the checkable half of the "four-part
 * subagent contract" in skills/principles/references/model-routing.md.
 *
 * It also catches a narrower but sharper bug: an agent with Write/Edit in its tools that
 * describes itself as safe to dispatch in parallel WITHOUT naming isolation (worktrees, or
 * "don't touch the same files"). Two writers on the same file in parallel is a known conflict
 * generator; a description that invites parallel writes without that caveat is a footgun
 * baked into the routing text itself.
 *
 * Usage:
 *   node check-agents.ts [agents-dir] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no agents found.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  check: string;
  severity: "P1" | "P2" | "P3";
  agent: string;
  message: string;
  detail?: string;
};

export type Agent = {
  file: string;
  name: string;
  description: string;
  tools: string[];
  hasFrontmatter: boolean;
  body: string;
};

function frontmatter(text: string): string {
  if (!text.startsWith("---")) return "";
  const end = text.indexOf("\n---", 3);
  return end === -1 ? "" : text.slice(3, end);
}

/** Pull a possibly-multiline YAML scalar (`key: >` folded blocks included). */
function yamlField(fm: string, key: string): string {
  const re = new RegExp(`^${key}:\\s*(>-?|\\|-?)?[ \\t]*(.*)$`, "m");
  const m = re.exec(fm);
  if (m === null) return "";
  if (m[1] !== undefined && m[1] !== "") {
    const start = fm.indexOf(m[0]) + m[0].length;
    const rest = fm.slice(start).split("\n");
    const body: string[] = [];
    for (const line of rest) {
      if (line.trim() === "") continue;
      if (!/^\s{2,}\S/.test(line)) break;
      body.push(line.trim());
    }
    return body.join(" ");
  }
  return (m[2] ?? "").trim();
}

function yamlList(fm: string, key: string): string[] {
  const raw = yamlField(fm, key);
  if (raw === "") return [];
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

export function loadAgents(dir: string): Agent[] {
  const out: Agent[] = [];
  let entries: string[];
  try {
    entries = fs.readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries.sort()) {
    if (!entry.endsWith(".md")) continue;
    const file = path.join(dir, entry);
    const text = fs.readFileSync(file, "utf8");
    const fm = frontmatter(text);
    const bodyStart = text.indexOf("\n---", 3);
    const body = bodyStart === -1 ? text : text.slice(bodyStart + 4);
    out.push({
      file,
      name: yamlField(fm, "name") || entry.replace(/\.md$/, ""),
      description: yamlField(fm, "description"),
      tools: yamlList(fm, "tools"),
      hasFrontmatter: fm !== "",
      body,
    });
  }
  return out;
}

/**
 * A stated ceiling: a number tied to "lines" or "words" within the same short span as the
 * word "ceiling" or "cap". Loose enough to allow "Hard ceiling: **80 lines**" and "cap each
 * finding to ... 15 findings", tight enough that an unrelated number elsewhere in the body
 * (a maxTurns value, a severity list) can't satisfy it by accident.
 */
const CEILING_RE = /\b(?:ceiling|cap)\b[^\n]{0,60}?\b(\d+)\b[^\n]{0,20}?\b(lines?|words?|findings?)\b/i;

export function statedCeiling(body: string): number | undefined {
  const m = CEILING_RE.exec(body);
  return m?.[1] === undefined ? undefined : Number(m[1]);
}

/** "report negative results" instruction — the phrase this repo standardised on. */
export function hasCleanReportInstruction(body: string): boolean {
  return /found clean/i.test(body);
}

/** The two-tier escape hatch: write detail to a file, return headline + path. */
export function hasOverflowEscape(body: string): boolean {
  return /two-tier/i.test(body) || (/headline/i.test(body) && /\bpath\b/i.test(body));
}

/** Citation-first shape: cite a location/URL, don't paste content back. */
export function hasCitationFirstShape(body: string): boolean {
  const citesLocation = /file:line|file:start-end|`URL`|URL` plus/i.test(body);
  const forbidsPasting = /never paste|do not paste|not.*paste.*(back|content)/i.test(body);
  return citesLocation && forbidsPasting;
}

/** Claims safety running in parallel near the word "parallel" without naming isolation. */
export function parallelClaimLacksIsolation(text: string): boolean {
  const sentences = text.split(/(?<=[.!?])\s+/);
  for (const s of sentences) {
    if (/parallel/i.test(s) && !/isolat|worktree|different files|touch the same files/i.test(s)) {
      // Only a problem if no OTHER sentence in the file supplies the isolation caveat at all.
      if (!/isolat|worktree/i.test(text)) return true;
    }
  }
  return false;
}

const CITATION_FIRST_AGENTS = new Set(["scout", "researcher"]);

export function checkAgent(agent: Agent): Finding[] {
  const findings: Finding[] = [];
  const label = agent.name;

  if (!agent.hasFrontmatter) {
    findings.push({
      check: "no-frontmatter",
      severity: "P1",
      agent: label,
      message: "no frontmatter block parsed — the agent is likely unusable",
    });
    return findings;
  }
  if (agent.name.trim() === "") {
    findings.push({ check: "no-name", severity: "P1", agent: agent.file, message: "frontmatter has no `name`" });
  }
  if (agent.description.trim() === "") {
    findings.push({
      check: "no-description",
      severity: "P1",
      agent: label,
      message: "frontmatter has no `description` — dispatch has nothing to route on",
    });
  }
  if (agent.tools.length === 0) {
    findings.push({
      check: "no-tools",
      severity: "P1",
      agent: label,
      message: "frontmatter has no `tools` list — unclear what the agent is allowed to touch",
    });
  }

  const ceiling = statedCeiling(agent.body);
  if (ceiling === undefined) {
    findings.push({
      check: "no-return-ceiling",
      severity: "P1",
      agent: label,
      message:
        "body states no hard ceiling on its returned report — nothing stops it returning a full " +
        "file dump instead of a summary, which defeats the reason a subagent exists",
      detail: 'state one, e.g. "Hard ceiling: N lines" sized to this agent\'s job',
    });
  }

  if (!hasCleanReportInstruction(agent.body)) {
    findings.push({
      check: "no-clean-report-instruction",
      severity: "P2",
      agent: label,
      message:
        'body never instructs reporting what was checked and found "clean" — without it the caller ' +
        'cannot tell "nothing there" from "never looked"',
    });
  }

  if (!hasOverflowEscape(agent.body)) {
    findings.push({
      check: "no-overflow-escape",
      severity: "P2",
      agent: label,
      message:
        "body never says what to do when the real answer doesn't fit the ceiling — the failure mode " +
        "without this is silent truncation or a dump straight into the ceiling",
      detail: "state the two-tier return: write detail to a file, return the headline + path",
    });
  }

  if (CITATION_FIRST_AGENTS.has(agent.name) && !hasCitationFirstShape(agent.body)) {
    findings.push({
      check: "not-citation-first",
      severity: "P1",
      agent: label,
      message:
        "return shape is not citation-first — must cite file:line/URL plus a short clause and " +
        "explicitly forbid pasting file contents back, or this agent silently becomes a file dump",
    });
  }

  const grantsWriteOrEdit = agent.tools.some((t) => t === "Write" || t === "Edit");
  if (grantsWriteOrEdit && parallelClaimLacksIsolation(agent.description + "\n" + agent.body)) {
    findings.push({
      check: "parallel-write-without-isolation",
      severity: "P1",
      agent: label,
      message:
        "grants Write/Edit and discusses running in parallel without naming isolation (worktrees, or " +
        "not touching the same files) — two writers on one file in parallel is a known conflict source",
    });
  }

  return findings;
}

export function checkAgents(dir: string): { agents: Agent[]; findings: Finding[] } {
  const agents = loadAgents(dir);
  const findings: Finding[] = [];
  for (const a of agents) findings.push(...checkAgent(a));
  return { agents, findings };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });
  const dir = path.resolve(positionals[0] ?? path.join("plugins", "viby-toolkit", "agents"));
  const { agents, findings } = checkAgents(dir);

  if (agents.length === 0) {
    if (values.json) console.log(JSON.stringify({ agents: 0, findings: [] }));
    else if (!values.quiet) console.log(`no agents found under ${dir}`);
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify({ agents: agents.length, findings }, null, 2));
    return findings.some((f) => f.severity === "P1") ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`[${f.severity} ${f.check}] ${f.agent}`);
    console.log(`    ${f.message}`);
    if (f.detail !== undefined) console.log(`    fix: ${f.detail}`);
  }

  if (!values.quiet) {
    console.log("");
    console.log(`${agents.length} agents checked under ${dir}`);
    if (findings.length === 0) console.log("✓ every agent states a return-size contract");
  }

  return findings.some((f) => f.severity === "P1") ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
