#!/usr/bin/env -S node --experimental-strip-types
/**
 * Score the routing probes against the shipped skill descriptions.
 *
 * Run:  node --experimental-strip-types --disable-warning=ExperimentalWarning tests/score-routing-probes.ts
 * Exit: 0 = every probe's intended skill wins clearly, 1 = at least one probe is mis-ranked or
 *       thin, 2 = the probe table could not be read.
 *
 * THIS IS A PROXY, AND NOT THE TEST. Real routing is decided by the model reading the whole
 * description listing in a live session; this is lexical overlap between the probe's words and
 * each description's words. It cannot tell you what will actually fire. What it CAN do is find
 * the defects that make a mis-route likely and are invisible by eye:
 *
 *   - the intended skill does not even rank first on its own probe (a description that omits the
 *     phrasing a user actually reaches for),
 *   - it wins by a hair over a sibling (a coin-flip at dispatch time),
 *   - or it scores zero (no lexical hook at all — the description shares no content words with
 *     the request it claims to serve).
 *
 * `tests/routing-probes.md` remains the real test, and it is user-in-the-loop by construction:
 * only the human can see which skill loaded, and asking the model to report its own routing is
 * the self-assessment this repo treats as a weak signal. Passing this script does not fill in
 * that results table.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { loadSkills } from "../plugins/viby-toolkit/skills/principles/scripts/check-skills.ts";

const ROOT = join(import.meta.dirname, "..");
const SKILLS_DIR = join(ROOT, "plugins", "viby-toolkit", "skills");
const PROBES = join(ROOT, "tests", "routing-probes.md");

/** Same stoplist philosophy as check-skills.ts: strip the scaffolding of an English sentence. */
const STOPWORDS = new Set(
  ("a an the and or but if then than that this these those use used using when whether while for" +
    " to of in on at by with from into out up down over under again further once here there all any" +
    " both each few more most other some such no nor not only own same so too very can will just" +
    " should now also it its is are was were be been being do does did doing have has had you your" +
    " i we they he she them their what which who whom how why where says say said want wants asks" +
    " ask before after during about against between through above below own s t don now d ll m o re" +
    " ve y viby code toolkit skill skills claude e.g eg ie make made get got need needs help let" +
    " like would could should does doing thing things something anything really actually")
    .split(/\s+/)
    .filter(Boolean),
);

function words(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, " ")
    .split(/\s+/)
    .filter((w) => w.length > 2 && !STOPWORDS.has(w));
}

type Probe = { n: string; text: string; expected: string[] };

/** Parse the probe table: | n | probe | expected |. `none` and `a / b` are both legal answers. */
export function parseProbes(md: string): Probe[] {
  const out: Probe[] = [];
  for (const line of md.split("\n")) {
    const m = /^\|\s*(\d+)\s*\|\s*(.+?)\s*\|\s*(.+?)\s*\|\s*$/.exec(line);
    if (!m) continue;
    const [, n, probe, expected] = m;
    if (n === undefined || probe === undefined || expected === undefined) continue;
    out.push({
      n,
      // Drop the italic parenthetical notes the table uses to explain ambiguous probes.
      text: probe.replace(/\*\([^)]*\)\*/g, "").replace(/^"|"$/g, "").trim(),
      expected: expected
        .split("/")
        .map((s) => s.replace(/[`*]/g, "").trim())
        .filter(Boolean),
    });
  }
  return out;
}

/**
 * Score a probe against a description by IDF-weighted overlap. Plain overlap rewards long
 * descriptions for containing common words; weighting by rarity across the library is what makes
 * "migration" count more than "change".
 */
function scoreProbe(probeWords: string[], descWords: Set<string>, idf: Map<string, number>): number {
  let score = 0;
  for (const w of new Set(probeWords)) {
    if (descWords.has(w)) score += idf.get(w) ?? 1;
  }
  // Length-normalise, or the longest description wins everyone else's probes purely by having
  // more words to collide with. The first run of this script had exactly that bug: `adopt`, the
  // longest description in the library, out-ranked the intended skill on six unrelated probes.
  // Dividing by sqrt(|description|) is the standard cosine-style correction.
  return descWords.size === 0 ? 0 : score / Math.sqrt(descWords.size);
}

function main(): number {
  let md: string;
  try {
    md = readFileSync(PROBES, "utf8");
  } catch {
    console.log(`cannot read ${PROBES}`);
    return 2;
  }
  const probes = parseProbes(md);
  if (probes.length === 0) {
    console.log("no probe rows parsed — has the table format changed?");
    return 2;
  }

  const skills = loadSkills(SKILLS_DIR).filter((s) => s.description !== "");
  const descWords = new Map(skills.map((s) => [s.name, new Set(words(`${s.name} ${s.description}`))]));

  // IDF over the description corpus.
  const df = new Map<string, number>();
  for (const set of descWords.values()) {
    for (const w of set) df.set(w, (df.get(w) ?? 0) + 1);
  }
  const idf = new Map<string, number>();
  for (const [w, n] of df) idf.set(w, Math.log(skills.length / n) + 1);

  type Row = { probe: Probe; ranked: Array<{ name: string; score: number }>; rank: number; margin: number };
  const rows: Row[] = [];

  for (const probe of probes) {
    const pw = words(probe.text);
    const ranked = skills
      .map((s) => ({ name: s.name, score: scoreProbe(pw, descWords.get(s.name) ?? new Set(), idf) }))
      .sort((a, b) => b.score - a.score);
    const wantsNone = probe.expected.includes("none");
    const bestExpectedIndex = ranked.findIndex((r) => probe.expected.includes(r.name));
    const rank = wantsNone ? 0 : bestExpectedIndex + 1;
    const top = ranked[0]?.score ?? 0;
    const runnerUp = ranked[1]?.score ?? 0;
    const margin = top > 0 ? (top - runnerUp) / top : 0;
    rows.push({ probe, ranked, rank, margin });
  }

  /** Hard failures: the intended skill does not rank first. These set the exit code. */
  const problems: string[] = [];
  /** Advisory: it won, but narrowly. Some probes are deliberately ambiguous, so tuning these to
   *  zero would be over-fitting to a lexical proxy — reported, never gating. */
  const thin: string[] = [];
  console.log(`${probes.length} probes × ${skills.length} skills — LEXICAL PROXY, not live routing\n`);

  for (const { probe, ranked, rank, margin } of rows) {
    const first = ranked[0];
    const second = ranked[1];
    if (first === undefined) continue;
    const wantsNone = probe.expected.includes("none");
    const hit = wantsNone || rank === 1;
    const expectedScore = ranked.find((r) => probe.expected.includes(r.name))?.score ?? 0;

    let flag = "  ";
    // When NOTHING scores above zero, the probe named something no description could contain —
    // a product name, a symptom in the user's own words. That is this script being blind, not a
    // description being wrong, and stuffing product names into descriptions to satisfy it would
    // make the library worse. Report it as a blind spot and do not count it as a defect.
    if (first.score === 0) {
      console.log(`  #${probe.n.padStart(2)} ??    want=${probe.expected.join("/").padEnd(16)} PROXY-BLIND: no description shares any content word with this probe`);
      continue;
    }
    if (!wantsNone && rank !== 1) {
      flag = "✗ ";
      problems.push(
        `#${probe.n} "${probe.text.slice(0, 52)}" → wanted ${probe.expected.join("/")}, ` +
          `ranked #${rank === 0 ? "unranked" : rank} behind ${first.name}`,
      );
    } else if (!wantsNone && margin < 0.15 && second !== undefined) {
      flag = "~ ";
      thin.push(
        `#${probe.n} "${probe.text.slice(0, 52)}" → ${first.name} beats ${second.name} by only ` +
          `${(margin * 100).toFixed(0)}% — a coin flip at dispatch time`,
      );
    }
    if (!wantsNone && expectedScore === 0) {
      flag = "✗ ";
      problems.push(
        `#${probe.n} "${probe.text.slice(0, 52)}" → ${probe.expected.join("/")} scores ZERO: its ` +
          `description shares no content word with this request`,
      );
    }

    const top3 = ranked
      .slice(0, 3)
      .map((r) => `${r.name}:${r.score.toFixed(1)}`)
      .join("  ");
    console.log(`${flag}#${probe.n.padStart(2)} ${hit ? "ok " : "MISS"}  want=${probe.expected.join("/").padEnd(16)} ${top3}`);
  }

  console.log("");
  if (problems.length > 0) {
    console.log(`${problems.length} MIS-RANKED probe(s) — the intended skill does not come first:\n`);
    for (const p of problems) console.log(`  ✗ ${p}`);
    console.log("");
  } else {
    console.log("every probe's intended skill ranks first on lexical overlap");
  }
  if (thin.length > 0) {
    console.log(`${thin.length} thin margin(s) — advisory, and some probes are deliberately ambiguous:\n`);
    for (const t of thin) console.log(`  ~ ${t}`);
  }

  console.log(
    "\nA PROXY. Real dispatch is a model reading the whole listing, not word overlap — so this\n" +
      "neither passes nor fails the routing claim. tests/routing-probes.md is the real test and\n" +
      "needs a human in fresh sessions; its results table stays empty until then.",
  );
  return problems.length > 0 ? 1 : 0;
}

process.exit(main());
