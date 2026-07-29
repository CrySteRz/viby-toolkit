# Routing probes — the one test that cannot be automated

Every other check in this repo verifies code. **This one verifies the thing the code cannot
reach: whether the 22 skill descriptions actually fire on the prompts they claim.**

`check-skills.ts` measures pairwise description similarity and flags trigger collisions. That
is a *proxy* — and by this repo's own doctrine a proxy is not the thing. Nothing here has ever
established that a real prompt in a real session loads the right skill. It could not, for a
mechanical reason: the installed plugin sat stale at 0.3.2 for the whole of development, so the
descriptions being tested were never the descriptions installed. That is fixed — the installed
plugin is current — so this test is now possible for the first time.

## How to run it

It is user-in-the-loop by construction: only the human can see which skill Claude loaded, and
asking the model to self-report its own routing is exactly the self-assessment this toolkit
treats as a weak signal. So:

1. Start a **fresh session** (routing is decided from the description listing at session start;
   a session where the skill is already loaded proves nothing).
2. Type the probe **verbatim**, as the first substantive message.
3. Note which skill loaded — right / wrong / none.
4. `/clear` between probes. A loaded skill biases the next selection.

You do not need to run all 30 in one sitting. Ten probes across three sessions beats a
one-off sweep, because the failures that matter are the ones that recur.

## Scoring

- **right** — the expected skill loaded.
- **wrong** — a different skill loaded. *Record which one.* This is the highest-value
  outcome: a wrong skill names the shadowing pair, which is a description fix.
- **none** — no skill loaded and Claude answered directly. For an ambiguous prompt this can be
  correct; for an explicit one it means the description does not cover its own trigger.

The bar: **≥ 80% right, and zero `wrong` on the same pair twice.** A repeated mis-route is a
shadowing defect even when `check-skills.ts` says the pair is clean — the measured metric tops
out around 0.13 on real libraries, so it cannot see everything a live selection does.

## The probes

Prompts are written the way work actually arrives — including the vague ones, because a skill
that only fires when you name it is a slash command, not a skill.

| # | Probe (type verbatim) | Expected |
|---|---|---|
| 1 | "I just cloned this repo, help me get oriented" | `explore` |
| 2 | "where does the auth check actually happen in here?" | `explore` |
| 3 | "I'm thinking about adding a notifications feature but I'm not sure what it should do" | `brainstorm` |
| 4 | "how would you approach splitting this service in two?" | `plan` |
| 5 | "build the CSV export from the ticket — spec is clear" | `orchestrate` |
| 6 | "this test passes locally and fails in CI, figure out why" | `debug` |
| 7 | "prod is down, 500s on every request" | `incident` |
| 8 | "rename `userId` to `accountId` everywhere" | `migrate` |
| 9 | "clean up this file, it's grown into a mess — same behaviour though" | `refactor` |
| 10 | "add a `deleted_at` column to the orders table" | `schema` |
| 11 | "is this ready to ship?" | `verify` |
| 12 | "write tests for the pricing module" | `test` |
| 13 | "are these tests any good?" | `test` |
| 14 | "review my changes before I open the PR" | `review-cluster` |
| 15 | "check this for security problems" | `secure` |
| 16 | "this endpoint takes 4 seconds, make it faster" | `perf` |
| 17 | "cut a release" | `release` |
| 18 | "should this be a major version bump?" | `release` |
| 19 | "we can't tell what's happening in this service at night" | `observe` |
| 20 | "design the API for the new billing service" | `api` |
| 21 | "is this a breaking change for our consumers?" | `api` |
| 22 | "should we use Playwright or something lighter for this?" | `evaluate` |
| 23 | "is this library worth adding as a dependency?" | `evaluate` |
| 24 | "remember that our migrations must never run in a transaction" | `learn` |
| 25 | "I'm out of context, write down where we are" | `handoff` |
| 26 | "run these three implementers without them stepping on each other" | `worktrees` |
| 27 | "how do you decide when to fan out subagents?" | `principles` |
| 28 | "make this faster" *(deliberately ambiguous — `perf` or a direct answer are both fine; `refactor` is a mis-route)* | `perf` / none |
| 29 | "the docs say one thing and the code does another" *(no skill owns this; `none` is the right answer)* | none |
| 30 | "why is this slow AND wrong?" *(genuinely two jobs — either `debug` or `perf`, but it must not silently pick one and drop the other)* | `debug` / `perf` |

## What to do with the results

- **A `wrong` result** → edit the *loser's* description to name the winner as the sibling to
  use instead, and vice versa. A mutual cross-reference is the disambiguation; `check-skills.ts`
  exempts mutually-referencing pairs from shadowing for exactly this reason.
- **A `none` on probes 1–27** → the description is missing the phrasing a real user reaches
  for. Add the phrasing, not more explanation.
- **Anything learned** → `/viby-toolkit:learn`, so the next round starts from the last one.
- Record the run below. An empty results table means this claim is still unverified, and the
  CHANGELOG should keep saying so.

## Results

| Date | Probes run | right | wrong (which skill fired) | none | Notes |
|---|---|---|---|---|---|
| — | — | — | — | — | **the live test has not been run** |

### Lexical pre-screen (a proxy, not this test)

`tests/score-routing-probes.ts` scores every probe against the shipped descriptions by
IDF-weighted, length-normalised word overlap. It cannot tell you what will fire — real dispatch is
a model reading the whole listing — but it finds the defect most likely to cause a mis-route: a
description that omits the phrasing a user actually reaches for.

Run 2026-07-29, before any live probing: **13 of 30 probes mis-ranked**, and four skills scored
**zero** on their own probe — `explore` had no hook for "where does X actually happen",
`verify` none for "is this ready to ship", `observe` none for "we can't tell what's happening",
`test` none for "write tests for this module". One structural finding: `brainstorm`'s description
named the case where it must *not* fire ("a clear, well-specified ticket/spec"), which made it a
lexical magnet for exactly those requests and beat `orchestrate` on its own probe. After adding the
missing phrasings and rewriting that negative condition, all 30 rank correctly with no new
shadowing. The check is now part of `npm run check`.

Two limits, stated so a green pre-screen is not mistaken for a passing routing test: probes naming
a specific product (#22, "Playwright") have no lexical hook in any description and are reported as
**proxy-blind** rather than as defects — stuffing product names into descriptions to satisfy the
script would make the library worse. And thin margins are advisory only, because several probes are
deliberately ambiguous.
