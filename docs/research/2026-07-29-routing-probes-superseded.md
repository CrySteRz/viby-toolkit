> **SUPERSEDED 2026-07-30 — kept for the record, do not run.** Replaced by `tests/routing/`, which
> is executable: a fresh `claude -p` per run against the real installed listing, with the outcome read
> from the actual `Skill` tool call instead of a human watching which skill loaded.
>
> **Its numbers should not be cited.** The method here is 10 probes, **one sample each, no control** —
> and both flaws mattered. Re-measuring properly gave a different figure, and more importantly showed
> that on two of ten probes the metric itself was wrong: `schema` and `docs` never fire, and the
> unaided model does both jobs well, so those were never defects. A dispatch-rate table with no
> control arm cannot tell "the wrong skill fired" from "no skill was needed".
>
> The document is preserved because the *reasoning* about why routing needed measuring at all is still
> right, and because the shape of its failure is worth remembering.

---

# Routing probes — the one test that cannot be automated

Every other check in this repo verifies code. **This one verifies the thing the code cannot
reach: whether the 30 skill descriptions actually fire on the prompts they claim.**

`check-skills.ts` measures pairwise description similarity and flags trigger collisions. That
is a *proxy* — and by this repo's own doctrine a proxy is not the thing. Nothing here has ever
established that a real prompt in a real session loads the right skill. It could not, for a
mechanical reason: the installed plugin sat stale at 0.3.2 for the whole of development, so the
descriptions being tested were never the descriptions installed. That is fixed — the installed
plugin is current — so this test is now possible for the first time.

## The 10-minute version

If the full set is never going to happen, run **these ten**. They cover the pairs most likely to
collide and the three skills that had no lexical hook at all before v2.5.0, so a miss here is
worth more than a miss anywhere else. One fresh session each, `/clear` between:

```
1.  is this ready to ship?                                  → verify
2.  are these tests any good?                               → test
3.  we can't tell what's happening in this service at night → observe
4.  build the CSV export from the ticket — spec is clear    → orchestrate
5.  I'm thinking about adding notifications, not sure what  → brainstorm
6.  review my changes before I open the PR                  → review-cluster
7.  check this for security problems                        → secure
8.  prod is down, 500s on every request                     → incident
9.  should we use Playwright or something lighter?          → evaluate
10. remember that our migrations must never run in a txn    → learn
```

Log it as one line in the results table: date, `10`, how many right, which skill fired for each
wrong one. **A single row beats a perfect empty table** — with ten results the routing claim stops
being unverified, and the misses point directly at the descriptions to fix.

## How to run it

It is user-in-the-loop by construction: only the human can see which skill Claude loaded, and
asking the model to self-report its own routing is exactly the self-assessment this toolkit
treats as a weak signal. So:

1. Start a **fresh session — and specifically one begun AFTER the last `/plugin update`.** This is
   mechanical, not a preference: the skill registry is captured at session start and does not refresh
   mid-session. Measured on 2026-07-29 by dispatching a probe to a fresh subagent from a long-running
   session: it reported the skills as `viby-code:ship` and `viby-code:review-cluster` — names retired
   several releases earlier. A subagent inherits the parent session's frozen listing, so **no probe
   run from a stale session measures the current descriptions**, however fresh the agent's context is.
   That single fact is why this table stayed empty for so long.
2. Type the probe **verbatim**, as the first substantive message.
3. Note which skill loaded — right / wrong / none.
4. `/clear` between probes. A loaded skill biases the next selection.

You do not need to run them all in one sitting. Ten probes across three sessions beats a
one-off sweep, because the failures that matter are the ones that recur.

## The other thing that probe revealed

The subagent could see **90 skills**, because every installed plugin contributes to the listing. Its
runner-up for "is this ready to ship?" was `security-review` — a skill from a different plugin
entirely.

So the competition for a trigger phrase is not among this library's 31 descriptions; it is among all
90 installed. `check-skills.ts` measures shadowing *within* a directory, which means it has never
seen the pairs that matter most. Run it across the whole installed set as well:

```bash
node --experimental-strip-types plugins/viby-toolkit/skills/principles/scripts/check-skills.ts \
  plugins/viby-toolkit/skills --against "$HOME"/.claude/plugins/cache/*/*/*/skills
```

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
| 31 | "the page is blank after my change, check the UI" | `ui` |
| 32 | "does this look right on mobile?" | `ui` |
| 33 | "add a skill to the toolkit for handling database seeds" | `extend` |
| 34 | "the client wants a KPI dashboard, what should we track" | `kpi` |
| 35 | "our dashboard revenue doesn't match what finance says" | `analytics` |
| 36 | "write the README for this repo" | `docs` |
| 37 | "dependabot opened 30 PRs, deal with them" | `deps` |

## What to do with the results

- **A `wrong` result** → edit the *loser's* description to name the winner as the sibling to
  use instead, and vice versa. A mutual cross-reference is the disambiguation; `check-skills.ts`
  exempts mutually-referencing pairs from shadowing for exactly this reason.
- **A `none` on an explicit probe** → the description is missing the phrasing a real user reaches
  for. Add the phrasing, not more explanation.
- **Anything learned** → `/viby-toolkit:learn`, so the next round starts from the last one.
- Record the run below. An empty results table means this claim is still unverified, and the
  CHANGELOG should keep saying so.

## Results

| Date | Probes run | right | wrong (which skill fired) | none | Notes |
|---|---|---|---|---|---|
| 2026-07-29 | 10 | **5** | 3 (`release` for verify · `review-cluster` for test · external `security-review` for secure) | 2 (evaluate, learn) | **50% — FAILS the 80% bar.** Fresh-context agent dispatch, current descriptions, one agent per probe. See below. |

**What the first real run established.** 5/10, against a lexical pre-screen that said all 37 probes
ranked first — so the proxy was wrong about half of them, and every failure had a specific cause:

| Probe | Fired | Wanted | Cause | Fix applied |
|---|---|---|---|---|
| is this ready to ship? | `release` | `verify` | both own the word "ship" | `verify` leads with the phrase; `release` no longer says "ship a release"; mutual cross-reference |
| are these tests any good? | `review-cluster` | `test` | "any good" is review vocabulary | `review-cluster` now states it reviews the change, not the suite |
| check this for security problems | **`security-review`** (another plugin) | `secure` | **cross-plugin competition** — 90 skills are in the listing, not 31 | `secure` claims the literal phrase and states what it covers that a diff review does not |
| should we use Playwright or something lighter? | none | `evaluate` | description used placeholders (`X or Y`) where users name products | concrete product-shaped phrasings added |
| remember that our migrations must never run in a transaction | none (runner-up `update-config`) | `learn` | read as a config change | `learn` now owns that phrasing and says it records a project fact, not a setting |

Three findings that outlive this run:

1. **The registry is frozen at session start.** A probe dispatched from a long-running session reported
   the skills under names retired several releases earlier. Subagents inherit the parent's frozen
   listing, so a stale session cannot measure current descriptions however fresh the agent is.
2. **Dispatch competes across all installed plugins.** One failure was a loss to an official plugin's
   skill. `check-skills.ts` only ever compared this library against itself — hence the new
   `--against` mode.
3. **The intended skill was runner-up in every mis-route.** The descriptions were close but not close
   enough, which is exactly the band a lexical proxy cannot resolve.

**Replication run, same day, 5 probes, one fresh agent each — and it measured the PRE-FIX
descriptions**, because the session registry had been reloaded before the fixes were written. Result:
**4 of 5 failures reproduced exactly** (`verify`→`release`, `secure`→external `security-review`,
`evaluate`→none, `learn`→none/`update-config`). The fifth drifted: `are these tests any good?` went
from `review-cluster` to **none**, with `test` as runner-up — an unstable probe, which is its own
finding, because instability means those two descriptions are close enough that dispatch is a coin
flip rather than a decision.

So the failures are **reproducible, not noise** — worth knowing before spending effort on them.

**The fixes remain unvalidated, and the reason is the same mechanical fact:** the registry froze at
the reload, so agents spawned afterwards see the descriptions as they were then, not as they are on
disk. Validating requires `/reload-skills` (or a new session) and then re-running these five. The
installed copy already carries the fixes — verified: `verify`'s description now contains the readiness
phrase and `release`'s no longer does.

**Method that works, for next time:** one fresh general-purpose agent per probe, launched in
parallel, each given the probe text plus a meta-instruction to report `would_invoke` / `runner_up`
and do no work. Ten probes cost about four seconds of wall-clock each and roughly 29k tokens per
agent — none of which touches the main context. This is the fan-out law working exactly as stated:
pure read work, no shared state, and the raw reasoning dies with the agent.

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
