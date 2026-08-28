/**
 * Contract tests for the skill-library health check.
 *
 * Run: node --experimental-strip-types --test tests/check-skills.test.ts
 *
 * The first version of this checker used thresholds picked by feel (0.38/0.50) against a
 * metric whose real-world maximum is ~0.13 — so it could never fire and reported "clean"
 * forever. These tests exist mainly to pin that it is NOT vacuous: a deliberately shadowed
 * pair must be caught, and a genuinely distinct pair must not be.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkListingBudget, checkProgressiveDisclosure, checkSkills, listingBudgetChars, loadSkills } from "../plugins/viby-toolkit/skills/principles/scripts/check-skills.ts";

function library(skills: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "skills-"));
  for (const [name, frontmatter] of Object.entries(skills)) {
    const d = path.join(dir, name);
    fs.mkdirSync(d, { recursive: true });
    fs.writeFileSync(path.join(d, "SKILL.md"), `---\n${frontmatter}\n---\n\n# ${name}\n\nbody\n`);
  }
  return dir;
}

function findings(skills: Record<string, string>): Array<{ check: string; skills: string[] }> {
  const dir = library(skills);
  try {
    return checkSkills(dir).findings.map((f) => ({ check: f.check, skills: f.skills }));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test("NOT VACUOUS: a deliberately shadowed pair is caught", () => {
  const f = findings({
    alpha: 'name: alpha\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs, and judging whether the change is safe to merge.',
    beta: 'name: beta\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs, and judging whether the change is safe to ship.',
  });
  const shadow = f.filter((x) => x.check === "shadowing");
  assert.equal(shadow.length, 1, `expected a shadowing finding, got ${JSON.stringify(f)}`);
  assert.deepEqual(shadow[0]?.skills.sort(), ["alpha", "beta"]);
});

test("genuinely distinct skills are NOT flagged", () => {
  const f = findings({
    perf: 'name: perf\ndescription: Use for speed, memory or cost — profiling a bottleneck and measuring a before and after number.',
    docs: 'name: docs\ndescription: Use when writing user-facing documentation, tutorials, or reference pages for an audience outside the team.',
  });
  assert.deepEqual(f, [], `distinct skills must be silent, got ${JSON.stringify(f)}`);
});

test("a trigger phrase claimed by two skills is a P1 collision", () => {
  const f = findings({
    one: 'name: one\ndescription: Use for auditing dependency licences. Trigger when the user says "check the licences" or asks about legal compliance.',
    two: 'name: two\ndescription: Use for scanning container images for outdated packages. Trigger when the user says "check the licences" before a release build.',
  });
  const dup = f.find((x) => x.check === "duplicate-trigger");
  assert.ok(dup, `expected duplicate-trigger, got ${JSON.stringify(f)}`);
  assert.deepEqual(dup.skills.sort(), ["one", "two"]);
});

test("a mutually cross-referencing pair is treated as already disambiguated", () => {
  // Adding "distinct from X" RAISES word overlap while LOWERING real confusion. Scoring it
  // as shadowing would penalise the fix and reward vagueness.
  const f = findings({
    migrate:
      'name: migrate\ndescription: Use for a wide mechanical sweep across many files, a rename everywhere or a codemod. Distinct from refactor, which restructures one area without changing behaviour.',
    refactor:
      'name: refactor\ndescription: Use when restructuring one area without changing behaviour, extracting a function or reducing duplication. Distinct from migrate, which is a wide mechanical sweep across many files.',
  });
  assert.ok(
    !f.some((x) => x.check === "shadowing" || x.check === "shadowing-watch"),
    `a mutually-disambiguated pair must not be flagged, got ${JSON.stringify(f)}`,
  );
});

test("a one-sided cross-reference is not enough to clear a shadowed pair", () => {
  const f = findings({
    aaa: 'name: aaa\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs, judging whether the change is safe to merge.',
    bbb: 'name: bbb\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs, judging whether the change is safe to ship. Distinct from aaa.',
  });
  assert.ok(
    f.some((x) => x.check === "shadowing"),
    `only one side references the other, so the pair is still confusable: ${JSON.stringify(f)}`,
  );
});

test("a missing description is P1 — nothing to route on", () => {
  const dir = library({ ghost: "name: ghost" });
  try {
    const { findings: fs_ } = checkSkills(dir);
    const miss = fs_.find((f) => f.check === "no-description");
    assert.ok(miss, "a skill with no description must be flagged");
    assert.equal(miss.severity, "P1");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("an over-long description is flagged as truncated by the listing", () => {
  const long = "Use when ".concat("handling an unusual and very specific situation ".repeat(40));
  assert.ok(long.length > 1536);
  const f = findings({ verbose: `name: verbose\ndescription: ${long}` });
  assert.ok(f.some((x) => x.check === "description-truncated"), `got ${JSON.stringify(f)}`);
});

test("folded YAML descriptions (`description: >`) are parsed, not skipped", () => {
  // The real skills use folded blocks; if the parser missed them every description would read
  // as empty and the whole check would silently pass.
  const dir = library({
    folded: "name: folded\ndescription: >\n  Use when the description spans several lines\n  and is written as a folded block.",
  });
  try {
    const skills = loadSkills(dir);
    assert.equal(skills.length, 1);
    assert.match(skills[0]?.description ?? "", /folded block/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a skill marked disable-model-invocation is reported as not model-invocable", () => {
  const dir = library({
    manual: "name: manual\ndescription: Reference material only.\ndisable-model-invocation: true",
  });
  try {
    const skills = loadSkills(dir);
    assert.equal(skills[0]?.modelInvocable, false);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("the real viby-toolkit library is clean", () => {
  // Dogfooding: this is the check that would have caught migrate/refactor drifting together.
  const dir = path.join(path.dirname(path.dirname(new URL(import.meta.url).pathname)), "plugins", "viby-toolkit", "skills");
  const { skills, findings: fs_ } = checkSkills(dir);
  assert.ok(skills.length >= 15, `expected the real library, found ${skills.length} skills`);
  // This used to exclude `listing-over-budget` by name and then assert it WAS reported, on the
  // belief that 31 skills could never fit. 2026-08-27 disproved that — rewriting every description
  // as pure triggers landed the library at 92% — so the assertion is inverted: no findings at all,
  // and being over budget is now a regression rather than the standing state.
  assert.deepEqual(
    fs_.map((f) => `${f.check}:${f.skills.join("+")}`),
    [],
    "the shipped library must have no shadowing, no trigger collisions, and must fit the listing budget",
  );
  const total = skills.reduce((n, s) => n + s.description.length, 0);
  assert.ok(
    total <= listingBudgetChars(200_000),
    `descriptions total ${total} chars, over the listing budget — the least-used skills get truncated to name-only first, which is exactly how a skill stops firing`,
  );
});

test("the listing budget matches the constants in the Claude Code binary, not the docs", () => {
  // Verified against Claude Code 2.1.220: skillListingBudgetFraction defaults to 0.01, bytesPerToken
  // is 4, and the default context window is 200_000. The binary's own string: "The skill listing is
  // budgeted at ~1% of the context window; when summed descriptions exceed it, entries get truncated
  // and skill routing degrades."
  assert.equal(listingBudgetChars(200_000), 8_000);
  assert.equal(listingBudgetChars(1_000_000), 40_000);
});

test("NOT VACUOUS: the aggregate budget check still fires when the library overflows", () => {
  // The real-library test no longer asserts this finding exists, because the library now fits. That
  // removed the only thing keeping the aggregate check honest, so it is pinned on a fixture instead.
  const many = Array.from({ length: 40 }, (_, i) => ({
    name: `s${i}`, description: "z".repeat(300), triggers: [], modelInvocable: true,
    directives: 0, bodyLines: 1, bodyWords: 1, hasReferences: false,
  }));
  const f = checkListingBudget(many);
  assert.ok(f.some((x) => x.check === "listing-over-budget"), "40 x 300 chars is 12,000 against an 8,000 budget");
});

test("and does NOT fire on a library that fits", () => {
  const few = Array.from({ length: 10 }, (_, i) => ({
    name: `s${i}`, description: "z".repeat(200), triggers: [], modelInvocable: true,
    directives: 0, bodyLines: 1, bodyWords: 1, hasReferences: false,
  }));
  assert.ok(!checkListingBudget(few).some((x) => x.check === "listing-over-budget"));
});

test("a description past the 1,536-char per-skill cap is a P1, because the tail is silently cut", () => {
  const long = { name: "x", description: "y".repeat(1_600), triggers: [], modelInvocable: true, directives: 0, bodyLines: 1, bodyWords: 1, hasReferences: false };
  const f = checkListingBudget([long]);
  assert.ok(f.some((x) => x.check === "description-over-cap" && x.severity === "P1"), "over-cap must be P1");
});

test("a skill name containing a regex metacharacter still gets the cross-reference exemption", () => {
  // Regression: the name was spliced unescaped into a RegExp, so the pattern for "a+b" meant
  // "one or more a, then b" and never matched the literal name. The mutual-cross-reference
  // exemption silently failed and the pair was reported as a P1 shadowing collision.
  const f = findings({
    "a+b":
      'name: a+b\ndescription: Use when reviewing a diff before shipping, checking correctness, finding bugs and judging whether the change is safe to merge. Distinct from other.',
    other:
      'name: other\ndescription: Use when reviewing a diff before merging, checking correctness, finding bugs and judging whether the change is safe to ship. Distinct from a+b.',
  });
  assert.ok(
    !f.some((x) => x.check === "shadowing" || x.check === "shadowing-watch"),
    `a mutually-disambiguated pair must be exempt regardless of metacharacters, got ${JSON.stringify(f)}`,
  );
});

test("progressive disclosure: a 500+ line body is P2, and references/ excuses a long-but-split one", () => {
  // Anthropic's stated limit, fetched 2026-07-29: "Keep the SKILL.md body under 500 lines."
  const base = { name: "x", description: "d", triggers: [], modelInvocable: true, directives: 0 };
  const huge = { ...base, bodyLines: 620, bodyWords: 5_000, hasReferences: true };
  assert.ok(
    checkProgressiveDisclosure([huge]).some((f) => f.check === "body-over-500-lines"),
    "past 500 lines is a finding even when references/ exists — the BODY is what loads",
  );
  const split = { ...base, bodyLines: 200, bodyWords: 2_500, hasReferences: true };
  assert.deepEqual(checkProgressiveDisclosure([split]), [], "a long body that HAS been split is fine");
  const unsplit = { ...base, bodyLines: 200, bodyWords: 2_500, hasReferences: false };
  assert.ok(
    checkProgressiveDisclosure([unsplit]).some((f) => f.check === "no-progressive-disclosure"),
    "long and unsplit is worth a nudge",
  );
});

test("a damaged frontmatter terminator yields an empty description, which is the P1", () => {
  // Found by accident while building the A/B harness: a bad edit ate the closing `---`, and the skill
  // then did not appear in `claude -p`'s available-skills list AT ALL — silently, no error. The
  // consequence is worse than "hard to route": the skill does not exist as far as the model is
  // concerned. This gate is what stands between that and shipping.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vt-fm-"));
  fs.mkdirSync(path.join(dir, "broken"));
  fs.writeFileSync(
    path.join(dir, "broken", "SKILL.md"),
    ["---", "name: broken", "description: >", "  a description with no closing fence", "", "# Body starts here"].join("\n"),
  );
  try {
    const { findings } = checkSkills(dir);
    assert.ok(findings.some((f) => f.check === "no-description" && f.severity === "P1"), "must be a P1");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("the pushy-imperative opener does not count toward similarity", () => {
  // Anthropic's documented fix for undertriggering is a pushy description, and this library
  // measured it working. Once every skill opens "Always load ...", those words are common to all
  // of them: if they counted, adopting the recommended phrasing library-wide would manufacture
  // shadowing findings between skills that share nothing but scaffolding.
  const f = findings({
    alpha: 'name: alpha\ndescription: Always load before writing a database migration — "add a column", "backfill", "add an index".',
    beta: 'name: beta\ndescription: Always load before clicking through a rendered page — "does this render", "take a screenshot", "the page is blank".',
  });
  assert.deepEqual(
    f.filter((x) => x.check === "shadowing" || x.check === "shadowing-watch"),
    [],
    `scaffolding counted as overlap: ${JSON.stringify(f)}`,
  );
});

test("STILL NOT VACUOUS: pushy openers do not hide a genuinely shadowed pair", () => {
  const f = findings({
    alpha: 'name: alpha\ndescription: Always load when reviewing a diff before shipping, checking correctness, finding bugs, judging whether the change is safe to merge.',
    beta: 'name: beta\ndescription: Always load when reviewing a diff before merging, checking correctness, finding bugs, judging whether the change is safe to ship.',
  });
  assert.equal(f.filter((x) => x.check === "shadowing").length, 1, `expected shadowing, got ${JSON.stringify(f)}`);
});
