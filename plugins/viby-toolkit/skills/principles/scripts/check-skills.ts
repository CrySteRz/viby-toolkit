/**
 * viby-toolkit skill-library health check — guards against skill shadowing.
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
  directives: number; // simultaneous instructions the body asks the model to honour
  bodyLines: number;
  bodyWords: number;
  hasReferences: boolean;
};

/**
 * Instruction-count thresholds, measured rather than guessed.
 *
 * "Prompt Design at Scale" (arXiv 2607.19257) found perfect response rates collapse to ZERO
 * by N=80 simultaneous instructions for every model tested, regardless of format — "a hard
 * floor rather than a gradual asymptote" — and recommends treating ~40 as a REDESIGN
 * threshold rather than a tuning point.
 *
 * A skill body is exactly a list of simultaneous instructions, so this applies directly. For
 * reference, the largest skill in this repo sits at 32.
 */
const DIRECTIVES_REDESIGN = 40;
const DIRECTIVES_FLOOR = 80;

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
    " ve y viby code skill skills claude also e.g eg ie" +
    // The pushy-imperative scaffolding itself. Anthropic's guidance is to make descriptions
    // "a little bit pushy" to fight undertriggering, and this library measured that working. Once
    // every description opens "Always load ...", those words appear in all 31 and inflate every
    // pairwise similarity — the metric would punish the recommended phrasing and reward vagueness,
    // exactly what the mutual-cross-reference exemption below already refuses to do.
    " always load loading loads")
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

/**
 * Does `text` mention `name`? The name is escaped first: it comes from frontmatter, so a
 * name containing a regex metacharacter (`a+b`, `c#`, `f.sharp`) was previously spliced raw
 * into a RegExp — `\ba+b\b` means "one or more a, then b" and never matches the literal
 * "a+b". That silently broke the mutual-cross-reference exemption and produced a wrong P1.
 */
function mentions(text: string, name: string): boolean {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(^|[^\\w-])${escaped}([^\\w-]|$)`).test(text);
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
    // A directive is one instruction the model must honour simultaneously: a bullet or a
    // numbered step. Prose sentences are context; bullets are the instruction budget.
    const bodyStart = text.indexOf("\n---", 3);
    const body = bodyStart === -1 ? text : text.slice(bodyStart + 4);
    let directives = 0;
    for (const line of body.split("\n")) {
      if (/^\s*[-*]\s+\S/.test(line) || /^\s*\d+\.\s+\S/.test(line)) directives += 1;
    }
    const bodyLines = body.split("\n").length;
    const bodyWords = body.split(/\s+/).filter(Boolean).length;
    let hasReferences = false;
    try {
      hasReferences = fs.readdirSync(path.join(dir, entry, "references")).some((f) => f.endsWith(".md"));
    } catch {
      hasReferences = false;
    }
    out.push({
      bodyLines,
      bodyWords,
      hasReferences,
      name: yamlField(fm, "name") || entry,
      description,
      triggers: triggerPhrases(description),
      modelInvocable: !/^disable-model-invocation:\s*true/m.test(fm),
      directives,
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

/**
 * Shadowing across LIBRARIES, not just within one.
 *
 * This checker only ever compared a directory against itself, which means it never saw the pairs that
 * matter most: dispatch competes across every installed plugin. Measured 2026-07-29 — a fresh agent
 * offered 90 skills picked a different plugin's `security-review` as runner-up for a probe this
 * library owns. Cross-library collisions cannot be fixed by editing the other plugin, so they are
 * reported at P2 with the only available remedy: make OUR description more specific.
 */
export function checkAcross(ourDir: string, otherDirs: string[]): Finding[] {
  const ours = loadSkills(ourDir).filter((s) => s.description !== "");
  const findings: Finding[] = [];
  const seen = new Set<string>();
  for (const dir of otherDirs) {
    const theirs = loadSkills(dir).filter((s) => s.description !== "");
    for (const a of ours) {
      for (const b of theirs) {
        if (a.name === b.name) continue;
        const key = `${a.name}|${b.name}`;
        if (seen.has(key)) continue;
        const sim = jaccard(words(a.description), words(b.description));
        if (sim >= SHADOW_HIGH) {
          seen.add(key);
          findings.push({
            severity: "P2",
            check: "cross-library-shadowing",
            skills: [a.name, `${b.name} (external)`],
            message: `${(sim * 100).toFixed(0)}% description overlap with an installed skill from another plugin — dispatch chooses between them and you can only edit yours`,
          });
        }
        for (const phrase of triggerPhrases(a.description)) {
          if (b.description.toLowerCase().includes(phrase)) {
            const k2 = `t:${a.name}|${b.name}|${phrase}`;
            if (seen.has(k2)) continue;
            seen.add(k2);
            findings.push({
              severity: "P1",
              check: "cross-library-trigger",
              skills: [a.name, `${b.name} (external)`],
              message: `both claim the literal trigger "${phrase}" — one of them wins and it is not decided by you`,
            });
          }
        }
      }
    }
  }
  return findings;
}

/**
 * The skill listing is budgeted, and overflowing it silently degrades routing. Verified against the
 * Claude Code binary (2.1.220), not the docs: `skillListingBudgetFraction` defaults to `0.01`,
 * `bytesPerToken` is `4`, the default context window is `200_000`, and `skillListingMaxDescChars`
 * caps a single description at `1536`. The binary's own words:
 *
 *   "The skill listing is budgeted at ~1% of the context window; when summed descriptions exceed
 *    it, entries get truncated and skill routing degrades."
 *
 * And the overflow order is the part that hurts: descriptions are dropped starting with the skills
 * you invoke LEAST. A fresh session has no invocation history, so a library's descriptions are the
 * first to be cut to name-only — which is why writing LONGER, more explicit descriptions to fix a
 * mis-route makes it worse. Every added character raises the overflow that strips the keywords.
 */
const LISTING_BYTES_PER_TOKEN = 4;
const LISTING_BUDGET_FRACTION = 0.01;
const LISTING_MAX_DESC_CHARS = 1536;
/**
 * The gate is PER-SKILL, not on the total, because a per-skill number is the actionable one.
 *
 * It used to say a 31-skill library "cannot fit" an 8,000-char budget however it is written. That
 * was wrong, and 2026-08-27 disproved it: rewriting all 31 as pure triggers — pushy opener, quoted
 * utterances, no summary of what the skill does — took the library 10,321 -> 7,370 chars, 129% ->
 * 92%, with zero shadowing findings. 258 chars each is tight, not impossible. What is
 * actually controllable is the length of each description, and the total follows from it.
 */
const DESC_TARGET = 400;

export function listingBudgetChars(contextTokens: number): number {
  return Math.floor(LISTING_BUDGET_FRACTION * contextTokens * LISTING_BYTES_PER_TOKEN);
}

export function checkListingBudget(skills: Skill[], contextTokens = 200_000): Finding[] {
  const findings: Finding[] = [];
  const total = skills.reduce((n, s) => n + s.description.length, 0);
  const budget = listingBudgetChars(contextTokens);
  const share = total / budget;

  for (const s of skills) {
    if (s.description.length > LISTING_MAX_DESC_CHARS) {
      findings.push({
        check: "description-over-cap",
        severity: "P1",
        skills: [s.name],
        message: `${s.description.length} chars — past the ${LISTING_MAX_DESC_CHARS}-char per-skill cap, so the tail is cut off and whatever routing keywords live there are simply gone`,
        detail: "the cut is silent; nothing warns you that the phrase you added is not in the listing",
      });
    } else if (s.description.length > DESC_TARGET) {
      findings.push({
        check: "description-too-long",
        severity: "P2",
        skills: [s.name],
        message: `${s.description.length} chars against a ${DESC_TARGET}-char target — it is spending listing budget that gets taken from the whole library`,
        detail: "cut the summary of what the skill DOES and keep only the triggers; the body says what it does",
      });
    }
  }
  if (share > 1) {
    findings.push({
      check: "listing-over-budget",
      // P3 deliberately: it is a library-wide symptom whose fix is spread across every description,
      // so the ACTIONABLE half is the per-skill P2. It IS clearable — the library sits at 92% — so
      // treat it as a real regression signal rather than as standing background noise.
      severity: "P3",
      skills: [],
      message:
        `${skills.length} descriptions total ${total} chars = ${(share * 100).toFixed(0)}% of the ` +
        `${budget}-char listing budget at ${contextTokens / 1000}k context, so entries get truncated ` +
        "and routing decides on less than you wrote",
      detail:
        "descriptions are dropped least-invoked-first, so in a fresh session these are the first cut " +
        "to name-only. Informational: with this many skills it is a standing cost, not a bug to fix.",
    });
  }
  return findings;
}

/**
 * Progressive disclosure. Anthropic's own guidance, fetched 2026-07-29: "Keep the SKILL.md body under
 * 500 lines for optimal performance. If your content exceeds this, split it into separate files using
 * the progressive disclosure patterns" — and, on nesting: "Claude may partially read files when
 * they're referenced from other referenced files… Keep references one level deep from SKILL.md."
 *
 * The word threshold is the softer signal and it is P3 on purpose: a long body is only a problem if
 * none of it has been moved out, and what counts as "too long" depends on how often the skill loads.
 */
const BODY_LINES_MAX = 500;
const BODY_WORDS_WATCH = 1_800;

export function checkProgressiveDisclosure(skills: Skill[]): Finding[] {
  const findings: Finding[] = [];
  for (const s of skills) {
    if (s.bodyLines > BODY_LINES_MAX) {
      findings.push({
        check: "body-over-500-lines",
        severity: "P2",
        skills: [s.name],
        message: `${s.bodyLines} lines — past Anthropic's stated ${BODY_LINES_MAX}-line limit for a SKILL.md body`,
        detail: "move the sections only needed at point of use into references/, one level deep",
      });
    } else if (s.bodyWords > BODY_WORDS_WATCH && !s.hasReferences) {
      findings.push({
        check: "no-progressive-disclosure",
        severity: "P3",
        skills: [s.name],
        message: `${s.bodyWords} words in one body with no references/ — every word loads whenever the skill fires`,
        detail: "split the parts a reader only needs once they are acting on that section",
      });
    }
  }
  return findings;
}

export function checkSkills(dir: string): { skills: Skill[]; findings: Finding[] } {
  const skills = loadSkills(dir);
  const findings: Finding[] = [...checkListingBudget(skills), ...checkProgressiveDisclosure(skills)];

  for (const s of skills) {
    if (s.description.trim() === "") {
      findings.push({
        check: "no-description",
        severity: "P1",
        skills: [s.name],
        message:
          "no description parsed — the skill is DROPPED FROM THE LISTING ENTIRELY, silently. " +
          "Observed 2026-07-30: a SKILL.md whose frontmatter terminator was damaged did not appear in " +
          "`claude -p`'s available skills at all, with no error and no warning. Usually a malformed " +
          "frontmatter block rather than a genuinely absent description — check the `---` fences first",
      });
    }
    if (s.directives >= DIRECTIVES_FLOOR) {
      findings.push({
        check: "instruction-overload",
        severity: "P1",
        skills: [s.name],
        message: `${s.directives} directives — at N=80 simultaneous instructions, perfect compliance measured zero for every model tested, regardless of format`,
        detail: "split it into a focused skill plus an on-demand reference file, as review does",
      });
    } else if (s.directives >= DIRECTIVES_REDESIGN) {
      findings.push({
        check: "instruction-heavy",
        severity: "P2",
        skills: [s.name],
        message: `${s.directives} directives — past ~40, treat this as a redesign threshold rather than something to tune`,
        detail: "move the reference material into references/ and keep the skill body to the decisions",
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
      const aNamesB = mentions(a.description, b.name);
      const bNamesA = mentions(b.description, a.name);
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
    options: {
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
      against: { type: "string", multiple: true },
    },
  });
  const dir = path.resolve(positionals[0] ?? path.join("plugins", "viby-toolkit", "skills"));
  const { skills, findings } = checkSkills(dir);
  // Dispatch competes across every installed library, not just within this one.
  const others = (values.against ?? []).filter((d) => d !== dir && fs.existsSync(d));
  if (others.length > 0) findings.push(...checkAcross(dir, others));

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
    const heaviest = [...skills].sort((a, b) => b.directives - a.directives)[0];
    if (heaviest !== undefined) {
      console.log(
        `heaviest skill: ${heaviest.name} at ${heaviest.directives} directives ` +
          `(redesign at ${DIRECTIVES_REDESIGN}, compliance floor at ${DIRECTIVES_FLOOR}).`,
      );
    }
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
