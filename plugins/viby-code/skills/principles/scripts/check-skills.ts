/**
 * viby-code skill-library health check — guards against skill shadowing.
 *
 * "More Skills, Worse Agents?" (arXiv 2605.24050) measured what happens as a skill library
 * grows: pass rate drops up to 21% at 202 skills (~8% at 52, ~14% at 102), and the fraction
 * of runs that invoke the RIGHT skill falls from 88% to 53%. The important part is the cause:
 *
 *   - the dominant mechanism is SKILL SHADOWING — a skill whose description semantically
 *     overlaps another's hides it from selection, exactly like variable shadowing;
 *   - the cost of the extra CONTEXT is "statistically indistinguishable from zero".
 *
 * So the thing to police is not how many skills exist, nor how many tokens they cost. It is
 * how distinguishable their descriptions are. The paper's own recommended mitigation is
 * "description disambiguation" — which is what this script makes checkable.
 *
 * Usage:
 *   node check-skills.ts [skills-dir] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no skills found.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  check: string;
  severity: "P1" | "P2" | "P3";
  skills: string[];
  message: string;
  detail?: string;
};

export type Skill = {
  name: string;
  description: string;
  triggers: string[]; // quoted phrases a user might literally type
  modelInvocable: boolean;
};

/**
 * Words that carry no routing signal. Without stripping these, every description looks
 * similar to every other because they share the scaffolding of an English sentence.
 */
const STOPWORDS = new Set(
  ("a an the and or but if then than that this these those use used using when whether while for" +
    " to of in on at by with from into out up down over under again further once here there all any" +
    " both each few more most other some such no nor not only own same so too very can will just" +
    " should now also it its is are was were be been being do does did doing have has had you your" +
    " i we they he she them their what which who whom how why where says say said want wants asks" +
    " ask before after during about against between through above below own s t don now d ll m o re" +
    " ve y viby code skill skills claude also e.g eg ie")
    .split(/\s+/)
    .filter(Boolean),
);

function words(text: string): Set<string> {
  return new Set(
    text
      .toLowerCase()
      .replace(/[^a-z0-9\s-]/g, " ")
      .split(/\s+/)
      .filter((w) => w.length > 2 && !STOPWORDS.has(w)),
  );
}

/** Quoted phrases in a description — the literal utterances that route a request. */
function triggerPhrases(text: string): string[] {
  const out: string[] = [];
  for (const m of text.matchAll(/["“']([^"”']{4,60})["”']/g)) {
    const phrase = m[1]?.trim().toLowerCase();
    if (phrase !== undefined && /\s/.test(phrase)) out.push(phrase);
  }
  return out;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let shared = 0;
  for (const w of a) if (b.has(w)) shared += 1;
  return shared / (a.size + b.size - shared);
}

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
    // folded/literal block: take the indented lines that follow
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

export function loadSkills(dir: string): Skill[] {
  const out: Skill[] = [];
  let entries: string[];
  try {
    entries = fs.readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries.sort()) {
    const file = path.join(dir, entry, "SKILL.md");
    let text: string;
    try {
      text = fs.readFileSync(file, "utf8");
    } catch {
      continue;
    }
    const fm = frontmatter(text);
    const description = [yamlField(fm, "description"), yamlField(fm, "when_to_use")].filter(Boolean).join(" ");
    out.push({
      name: yamlField(fm, "name") || entry,
      description,
      triggers: triggerPhrases(description),
      modelInvocable: !/^disable-model-invocation:\s*true/m.test(fm),
    });
  }
  return out;
}

/**
 * CALIBRATED AGAINST REAL DATA, not intuition.
 *
 * Measured over a real 19-skill library: median pairwise similarity 2.1%, and the most
 * similar legitimate pair — `migrate` vs `refactor`, which genuinely are adjacent — sits at
 * 13%. Jaccard over short descriptions with stopwords removed simply does not produce large
 * numbers, so the first thresholds tried here (0.38 / 0.50, picked by feel) could never have
 * fired on any realistic library. That is worse than no check: it reports "clean" forever.
 *
 * So: watch just below the most-adjacent real pair, and high well above anything a
 * deliberately-distinguished library produces.
 */
const SHADOW_HIGH = 0.2; // a pair this close is near-duplicate framing; selection is a coin flip
const SHADOW_WATCH = 0.12; // adjacent enough that one disambiguating clause each is worth it

export function checkSkills(dir: string): { skills: Skill[]; findings: Finding[] } {
  const skills = loadSkills(dir);
  const findings: Finding[] = [];

  for (const s of skills) {
    if (s.description.trim() === "") {
      findings.push({
        check: "no-description",
        severity: "P1",
        skills: [s.name],
        message: "no description, so the model has nothing to route on — it will effectively never be chosen",
      });
    }
    // Claude Code truncates description + when_to_use at 1,536 chars in the listing.
    if (s.description.length > 1536) {
      findings.push({
        check: "description-truncated",
        severity: "P2",
        skills: [s.name],
        message: `description is ${s.description.length} chars and is truncated at 1536 in the skill listing — the tail is invisible to routing`,
      });
    }
  }

  // Pairwise shadowing. Overlap, not count, is what the research found degrades selection.
  const invocable = skills.filter((s) => s.modelInvocable && s.description.trim() !== "");
  for (let i = 0; i < invocable.length; i += 1) {
    for (let j = i + 1; j < invocable.length; j += 1) {
      const a = invocable[i];
      const b = invocable[j];
      if (a === undefined || b === undefined) continue;
      const sim = jaccard(words(a.description), words(b.description));

      // An explicit cross-reference is the disambiguation, so it must not read as a problem.
      // A description saying "distinct from X — use X for Y" REDUCES the chance the model
      // confuses the pair while INCREASING their word overlap. Scoring that as shadowing
      // would penalise the fix and reward vagueness, so a mutually-referencing pair is
      // treated as already disambiguated.
      const aNamesB = new RegExp(`\\b${b.name}\\b`).test(a.description);
      const bNamesA = new RegExp(`\\b${a.name}\\b`).test(b.description);
      if (aNamesB && bNamesA) continue;

      if (sim >= SHADOW_HIGH) {
        findings.push({
          check: "shadowing",
          severity: "P1",
          skills: [a.name, b.name],
          message: `descriptions are ${(sim * 100).toFixed(0)}% similar — one will shadow the other and the model will pick between them arbitrarily`,
          detail: "add an explicit boundary to each: say what it is NOT for, and which sibling to use instead",
        });
      } else if (sim >= SHADOW_WATCH) {
        findings.push({
          check: "shadowing-watch",
          severity: "P3",
          skills: [a.name, b.name],
          message: `descriptions are ${(sim * 100).toFixed(0)}% similar — still distinguishable, but one disambiguating clause each would keep it that way`,
        });
      }

      // A literal trigger phrase claimed by two skills is the sharpest possible collision:
      // the user types exactly that, and routing is a coin flip.
      const shared = a.triggers.filter((t) => b.triggers.includes(t));
      if (shared.length > 0) {
        findings.push({
          check: "duplicate-trigger",
          severity: "P1",
          skills: [a.name, b.name],
          message: `both claim the same trigger phrase(s): ${shared.map((t) => `"${t}"`).join(", ")}`,
          detail: "give the phrase to exactly one skill, and have the other name it as an explicit exclusion",
        });
      }
    }
  }

  return { skills, findings };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  const dir = path.resolve(positionals[0] ?? path.join("plugins", "viby-code", "skills"));
  const { skills, findings } = checkSkills(dir);

  if (skills.length === 0) {
    if (values.json) console.log(JSON.stringify({ skills: 0, findings: [] }));
    else if (!values.quiet) console.log(`no skills found under ${dir}`);
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify({ skills: skills.length, findings }, null, 2));
    return findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`[${f.severity} ${f.check}] ${f.skills.join(" ↔ ")}`);
    console.log(`    ${f.message}`);
    if (f.detail !== undefined) console.log(`    fix: ${f.detail}`);
  }

  if (!values.quiet) {
    const blocked = skills.filter((s) => !s.modelInvocable).map((s) => s.name);
    console.log("");
    console.log(`${skills.length} skills (${blocked.length ? `${blocked.join(", ")} not model-invocable` : "all model-invocable"})`);
    // Honest framing: report where this library sits on the measured curve rather than
    // implying any count is inherently unsafe.
    const note =
      skills.length < 52
        ? "below the smallest library size where degradation was measured (52 skills, ~8% drop)"
        : skills.length < 102
          ? "in the range where a ~8-14% selection drop was measured"
          : "in the range where a 14-21% selection drop was measured — consider retrieval-based pre-filtering";
    console.log(`library size is ${note}.`);
    console.log(
      "Overlap matters more than count: the measured degradation is driven by shadowing, while\n" +
        "the cost of the extra context was indistinguishable from zero (arXiv 2605.24050).",
    );
    if (findings.length === 0) console.log("\n✓ no shadowing or trigger collisions found");
  }
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
