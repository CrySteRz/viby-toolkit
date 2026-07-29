---
name: extend
description: >
  Use when adding to or changing this toolkit itself — "add a skill for X", "write a new
  module", "extend viby-toolkit", "add a checker", "improve this skill's description", "why
  isn't my skill triggering". The meta-skill: how a new capability gets built here so it
  routes, runs, and is verified rather than merely written. Not /viby-toolkit:plan.
---

# Extend (adding to this toolkit without degrading it)

```
IRON LAW: A new skill is not done when it reads well. It is done when its description ROUTES
          (probe added and ranking first), its executable half RUNS (contract tests pinning both
          halves), and the library-health gate is still clean.
```

Every module in here was built this way, and the same three defects recur every time, so they are
gates rather than advice. Follow `/viby-toolkit:principles`.

## 1. Earn the slot: is it a new skill at all?

Adding a skill is not free, but the cost is not size — it is **overlap**. Measured over a growing
library, pass rate fell up to 21% and the mechanism was **shadowing**: one description
semantically hides another, so the right skill stops being selected. The extra context cost was
indistinguishable from zero (`/viby-toolkit:principles` §8).

So before writing anything, answer: **which existing skill would a reasonable person expect to
handle this?** If the answer is "one of these three, sort of", you are looking at a section in an
existing skill, not a new one. A new skill needs a **decision or procedure the others get wrong**,
not a new topic label.

## 2. Write the description as trigger phrases, then prove it routes

The description is not documentation — it is the only thing dispatch sees. Three rules:

- **Quote the words a user actually says**, several of them, in their phrasing not yours. "Use when
  the user says 'is this ready to ship'" beats "for release readiness assessment".
- **Say what it is NOT for and name the sibling** to use instead. A **mutual** cross-reference is
  the disambiguation, and mutual matters: one-sided does not clear the shadowing check, by design.
- **Never state the case where it must not fire in that case's own words.** A negative condition is
  a lexical magnet: `brainstorm` said "do NOT use when there's a clear, well-specified ticket/spec"
  and thereby out-ranked `orchestrate` on exactly those requests, 5×. Phrase the exclusion in terms
  of the *other* skill instead.

Then prove it, in this order:

```bash
# 1. add a row to tests/routing-probes.md — a prompt in the user's words, and the expected skill
# 2. the intended skill must rank first (gate; a proxy, but it catches missing phrasings)
node --experimental-strip-types tests/score-routing-probes.ts
# 3. no shadowing or trigger collision introduced (gate)
node --experimental-strip-types plugins/viby-toolkit/skills/principles/scripts/check-skills.ts plugins/viby-toolkit/skills
```

Expect step 2 and step 3 to fight each other: adding trigger phrases raises overlap, which *is*
the shadowing mechanism. When they do, the fix is a mutual cross-reference, not deleting the
phrase.

## 3. Give it an executable half, or say why it cannot have one

A study of 2,853 repositories found that where skills are used at all they "typically rely on
static instructions rather than executable scripts". Prose degrades into advice nobody follows;
a script either passes or fails. So ask: **what in this skill is mechanically decidable?**

Rules for the checker, all learned by breaking them here:

- **Decide on parsed code, never raw text** — blank strings and comments first
  (`lib/strip-noncode.ts`). Four separate defects came from matching text that merely *mentioned*
  what the checker hunts. Exception with a rule: for constructs whose value lives *inside* a
  string (a module path, `__all__`), decide **where** from blanked code and read **what** from the
  raw text at the same offset.
- **Precision over coverage.** A check that fires on idiomatic code gets switched off, and then it
  protects nothing. Two checks were built, measured against real suites, and deleted for this.
- **Print `unknown` rather than guessing**, and exit `0` clean / `1` findings / `2` nothing to
  check. A comparison that could not happen must never read as a pass.
- **State the limits on every run.** Each checker here says what it cannot see — counts are not
  behaviour, form is not truth, syntax is not semantics.
- **Every rule names the safe alternative.** "Don't do that" with no replacement gets ignored.

## 4. Contract tests: both halves, or it is not tested

For every rule: a case that **must** flag and a case that **must not**. The must-not half is the
one that keeps the checker switched on, and it is where the real bugs are found — dates tripping a
figure check, hedging flagged inside a limitations section, a moved file read as deleted coverage.

- Add the suite to `tests/run-all.ts` with a `minPassing` count. **Raise it when you add cases;
  never lower it to make a run green** — that gate exists because `node --test` exits 0 for a file
  whose tests were emptied.
- **Validate a noisy heuristic against a large real corpus** before shipping it, not against your
  own fixtures. Fixtures agree with you.

## 5. Run it on something that is not this repo

This repo is a friendly target: TypeScript, zero dependencies, written by the same process being
tested. Point the new checker at a real, messy, polyglot repository read-only before believing it.
That step has found, on first contact: a Kubernetes repo reported as "Python 100%" off a single
stray script, and a base-ref typo reported as a suite that had grown from zero.

## 6. Wire it in completely

Easy to half-finish, and each omission is silent:

- `references/` for anything long — sources, tables, routing detail. The skill body carries
  doctrine; the reference carries evidence. Keep the body under the **directive budget** (~40
  bullets is a redesign threshold; `check-skills.ts` reports the heaviest skill).
- Cross-references: name the sibling skills, and check they resolve — `tests/check-references.ts`
  verifies that every `/viby-toolkit:<name>` exists **and is model-invocable**, that no skill body
  expands the hooks-only `CLAUDE_PLUGIN_ROOT` variable (it is empty in a skill body, so any path
  built from it silently breaks), and that a skill instructing a capability names an agent that
  actually has the tools for it.
- If the skill fans out, confirm an **agent exists that can do the work**. `study` once instructed
  a web-search fan-out when every agent was filesystem-only.
- Invoke scripts through the cache-path + `run.sh` pattern the other skills use, never a bare path.
- Bump the version in **all three** manifests, write the changelog entry, and let
  `check-api-surface.ts` decide major/minor from the actual surface diff.

## 7. Then the honest part

Run `npm run check` (all gates), and write down **what is still unverified**. Every release here
claims "code and contracts verified", never "live routing proven", because the second has never
been measured. Record durable lessons with `/viby-toolkit:learn`; a defect class you hit once
belongs in the checker, and a preference the user stated belongs in memory.
