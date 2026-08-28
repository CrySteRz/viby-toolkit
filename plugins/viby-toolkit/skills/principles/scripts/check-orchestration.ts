#!/usr/bin/env -S node --experimental-strip-types
/**
 * Verify the delegation defaults are actually written into the skills, not merely believed.
 *
 * Run:  sh hooks/run.sh skills/principles/scripts/check-orchestration.ts
 * Exit: 0 = clean, 1 = at least one P1.
 *
 * §3 says fanning out is a DEFAULT and §3b says staged fan-out is declared as a script. Prose
 * decays: a skill gets edited, a directive softens into a suggestion, and nothing notices because
 * no test reads intent. This does — the same reason check-plan.ts exists for the partition rule.
 *
 * Three rules, each deliberately narrow so it reports an omission rather than a style preference:
 *   P1 hedged-fanout            a fan-out instruction phrased as a permission. "You may fan out"
 *                               is the phrasing that produced zero fan-outs; that is the whole
 *                               reason these defaults were rewritten.
 *   P1 unstaged-pipeline        a skill whose work is genuinely staged does not instruct a Workflow,
 *                               so its stages get re-improvised on each run.
 *   P1 unisolated-parallel-write  a WRITING agent is parallelised without worktree isolation.
 */
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const HERE = import.meta.dirname;
const SKILLS = join(HERE, "..", "..");

export type Finding = { skill: string; check: string; message: string };

/**
 * The skills whose work is staged — fan out, then act on each result, then synthesise. These are
 * the ones a script buys something for; everything else is a single sweep and dispatches directly.
 * An explicit list rather than a heuristic: "how many stages does this prose describe" is not
 * mechanically decidable, and a checker that guesses at it would cry wolf.
 *
 * Every name here is asserted to exist below, because a rename would otherwise silently retire the
 * rule — `review-cluster` was renamed to `review` while this file was being written.
 */
const STAGED = ["orchestrate", "review", "study"];

/** Phrases that mean "run several agents at once". Matched per line. */
const FANOUT =
  /\b(in parallel|fan out|fan-out|dispatch (?:one|several|\d+)|one \S+ per (?:dimension|angle|area|question|subsystem|node))\b/i;
/** A toolkit agent, backticked — the strict form, used where a false positive would be costly. */
const AGENT = /`(scout|reviewer|skeptic|debugger|researcher|implementer)`/;
/**
 * The same agents unbackticked. Used ONLY for the hedge rule: formatting must not decide whether a
 * softened directive is caught, and "You may optionally fan out scout agents" is exactly the
 * sentence that has to fire. Still requires a NAMED agent, so a retrospective aside like "a
 * web-search fan-out when every agent was filesystem-only" stays clean.
 */
const AGENT_LOOSE = /\b(scouts?|reviewers?|skeptics?|debuggers?|researchers?|implementers?)\b/i;
/** Softeners that turn a directive back into the permission nobody acts on. */
const HEDGE =
  /\b(you may|optionally|if you (?:want|like|prefer)|feel free|consider (?:dispatching|fanning|spawning)|could (?:dispatch|fan|spawn))\b/i;
/** The agent that writes. Reading agents are safe to parallelise; this one is not. */
const WRITER = /`?implementers?`?/i;
/**
 * Backticked deliberately: a bare occurrence of the word satisfies nothing. A URL containing
 * "Workflow", or a fenced sample mentioning `.github/workflows`, is not an instruction to author one.
 */
const WORKFLOW = /`Workflow`/;
const ISOLATION = /worktree/i;
/** How far from a parallel-write instruction the isolation guidance may sit and still count. */
const ISOLATION_WINDOW = 3;

/** Strip fenced code so a sample script isn't mistaken for an instruction. */
function prose(text: string): string {
  return text.replace(/```[\s\S]*?```/g, "");
}

/**
 * Assert every STAGED name still resolves to a real skill. Kept SEPARATE from checkOrchestration
 * because that function must stay usable against an arbitrary fixture directory — folding this in
 * made every fixture report three spurious missing-skill findings, which is how the test suite
 * caught it. This one is for the real library, and main() runs it.
 */
export function checkStagedSkillsExist(skillsDir: string): Finding[] {
  const present = new Set(readdirSync(skillsDir));
  return STAGED.filter((name) => !present.has(name)).map((name) => ({
    skill: name,
    check: "staged-skill-missing",
    message: `STAGED names "${name}" but no such skill directory exists — a rename retired this rule silently`,
  }));
}

export function checkOrchestration(skillsDir: string): Finding[] {
  const findings: Finding[] = [];
  const present = new Set(readdirSync(skillsDir));

  for (const name of [...present].sort()) {
    const file = join(skillsDir, name, "SKILL.md");
    if (!existsSync(file)) continue;

    // principles itself defines the laws; it is the reference, not a consumer of them.
    if (name === "principles") continue;

    const lines = prose(readFileSync(file, "utf8")).split("\n");
    const body = lines.join("\n");

    if (lines.some((l) => FANOUT.test(l) && AGENT_LOOSE.test(l) && HEDGE.test(l))) {
      findings.push({
        skill: name,
        check: "hedged-fanout",
        message: "fan-out is phrased as a permission rather than a default — that phrasing is why it never happened",
      });
    }

    if (STAGED.includes(name) && !WORKFLOW.test(body)) {
      findings.push({
        skill: name,
        check: "unstaged-pipeline",
        message: "staged work that never instructs a `Workflow` — its stages will be re-improvised each run (§3b)",
      });
    }

    const unisolated = lines.some((l, i) => {
      if (!(FANOUT.test(l) && AGENT.test(l) && WRITER.test(l))) return false;
      const near = lines.slice(Math.max(0, i - ISOLATION_WINDOW), i + ISOLATION_WINDOW + 1).join("\n");
      return !ISOLATION.test(near);
    });
    if (unisolated) {
      findings.push({
        skill: name,
        check: "unisolated-parallel-write",
        message: "parallelises an `implementer` (a writing agent) with no worktree isolation alongside the instruction",
      });
    }
  }
  return findings;
}

function main(): number {
  const findings = [...checkStagedSkillsExist(SKILLS), ...checkOrchestration(SKILLS)];
  for (const f of findings) {
    console.log(`[P1 ${f.check}] ${f.skill}`);
    console.log(`    ${f.message}`);
  }
  if (findings.length === 0) {
    console.log("✓ fan-out is stated as a default everywhere, and staged skills declare their stages");
    return 0;
  }
  console.log(`\n${findings.length} finding(s).`);
  return 1;
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  process.exit(main());
}
