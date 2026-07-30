# Routing measurement — does the intended skill actually fire?

This replaces `tests/routing-probes.md`, which was a hand-run table: 10 probes, **one sample each,
no control**. It produced a "50%" figure that was directionally real but not a rate worth quoting.

## What this measures, and why it is ground truth

Each run is a fresh `claude -p` subprocess in a pristine copy of `fixture/`. That subprocess gets the
**real** skill listing — every installed plugin, plus Claude Code's own built-in skills — so the
measurement includes the competition that matters. The outcome is read from the actual `Skill` tool
call in the stream. **The model is never asked what it would do**, because self-reported routing is the
self-assessment this library treats as a weak signal.

```bash
REPS=5 PAR=6 MODEL=sonnet python3 tests/routing/drive.py     # run the matrix
python3 tests/routing/score.py                                # score it
```

`REPS=5` is the floor, not a default to lower: "5+ reps per variant. Single samples lie."
(`obra/superpowers/skills/writing-skills/SKILL.md`).

**Measure the SHIPPED descriptions.** A fresh subprocess reads the installed plugin cache, not this
working tree, so `git push --tags && claude plugin update viby-toolkit@viby-toolkit` first or you are
measuring the previous release.

## Three outcomes, not two

| | meaning | fix |
|---|---|---|
| `HIT` | the intended skill fired first | — |
| `WRONG` | a different skill fired first | description overlap / a competitor to differentiate against |
| `NONE` | **no skill fired at all** | the skill lost to the base system prompt, not to a sibling |

That third row is the one worth building the harness for. In the first real run, **every single failure
was `NONE` and there was not one `WRONG`** — which killed the hypothesis that sibling shadowing or
listing-budget truncation was to blame, and pointed at undertriggering instead.

## Two traps this harness has already fallen into

**Scoring mid-flight.** An unfinished run has no `Skill` call yet, so it looks exactly like `NONE`.
Scoring the same matrix at three different moments gave 42%, 58% and 17%. `score.py` now excludes any
run without a terminal `result` event and says loudly how many it dropped — a partial matrix is not a
result.

**A fixture that cannot answer the probe.** "review my changes" and "is this ready to ship" are
unanswerable in a clean checkout, so those probes scored `NONE` for reasons that had nothing to do with
routing. `verify` went **1/5 → 5/5** once the working tree had real uncommitted changes. If a probe
needs repo state, the fixture must have it, or the run measures the fixture.

## Results

| date | version | model | probes × reps | first-choice accuracy | notes |
|---|---|---|---|---|---|
| 2026-07-30 | 2.18.0 | sonnet | 10 × 5 = 50 | 58% | clean fixture; zero `WRONG`, all failures `NONE` |
| 2026-07-30 | 2.18.0 | sonnet | 4 × 5 = 20 | — | dirty-fixture arm; `verify` 1/5 → 5/5 |
| **2026-07-30** | **2.20.0** | **sonnet** | **29 × 5 = 145** | **83%** | full library, shipped descriptions, deterministic dirty fixture |

### Full library at 2.20.0 — 120/145, one `WRONG`, 24 `NONE`

**5/5 (20 skills):** `plan` `review-cluster` `test` `verify` `secure` `debug` `explore` `perf`
`orchestrate` `brainstorm` `study` `incident` `adopt` `observe` `deps` `api` `kpi` `release` `handoff`
`principles`

**4/5 (4):** `docs` `refactor` `migrate` `analytics` — one `NONE` each, run-to-run variance.

**2/5 (2):** `worktrees` (3 `NONE`), `extend` (2 `NONE`, and one loss to `skill-creator:skill-creator`).

**0/5 (3):** `schema`, `learn`, `evaluate`.

Two things worth naming about that list:

- **The only `WRONG` in 145 runs was to a plugin I installed today.** `skill-creator` beat `extend` on
  "add a new skill to the toolkit" — arguably correctly. Installing a plugin changes your routing, and
  a similarity check over your own library cannot see it coming.
- **`docs` went 0/5 → 4/5, and the description got *worse* in between.** It was 0/5 at 2.18.0 and 2.19.0
  on the clean fixture, and 4/5 at 2.20.0 after the pushy wording was **reverted**. The fixture changed
  in the same window, so the improvement cannot be credited to wording. Recorded as an unexplained
  confound rather than as evidence the revert helped — the matched-condition comparison (2.18.0 vs
  2.19.0, both clean) is the one that justified the revert, and it still stands.

Best estimate for 2.18.0, using the dirty-fixture result for the probes that needed repo state:
**68%** (34/50).

Per-probe at 2.18.0: `plan` `secure` `explore` `perf` 5/5 · `test` `debug` 4/5 · `verify` 5/5 (dirty) ·
`review-cluster` 1/5 · `schema` 0/5 · `docs` 0/5.

## What the failures taught

`docs` never fired **despite its description already containing the literal string "write the
README"**. So the failure is not keyword mismatch — it is undertriggering, which Anthropic documents
directly: *"Claude has a tendency to 'undertrigger' skills — to not use them when they'd be useful. To
combat this, please make the skill descriptions a little bit 'pushy'."* That is why those three
descriptions now open with an imperative (`Always load this before X — do not Y`) instead of a
descriptive `Use when`.

A pushy imperative is **not** the workflow summary that Superpowers' A/B test warns about: it says when
to load the skill, never what the skill will do, so it cannot act as a shortcut past the body.

`review-cluster` also has a **built-in competitor** — `code-review` won one run outright. My earlier
lexical proxy could not see this class of failure at all, because it only knew this library's own 31
skills and not Claude Code's built-ins. Another reason the proxy is not the test.

## v2.19.0 — the pushy fix, measured

Three descriptions were rewritten from a descriptive `Use when` to a pushy imperative
(`Always load this before X — do not Y`). Same probes, same 5 reps, freshly installed:

| probe | 2.18.0 | 2.19.0 |
|---|---|---|
| `review-cluster` | 1/5 | **5/5** |
| `schema` | 0/5 | 0/5 |
| `docs` | 0/5 | 0/5 |

**One of three.** Pushy phrasing fixed `review-cluster` outright — including beating the built-in
`code-review` skill that had won a run — and did nothing at all for the other two. Reported as a
partial result rather than as "the fix worked", because two thirds of it did not.

### A flaw in this harness, found while investigating the two that did not move

**59 of the first 66 runs ended in `error_max_turns`** at the default `--max-turns 4`. Dispatch almost
always happens on turn 1, so the headline accuracy mostly survives — but a skill that would have
loaded later is scored `NONE`, and no run gets far enough to judge output quality. Treat `MAX_TURNS=4`
as a dispatch-only measurement, and raise it before drawing any conclusion about what the agent
actually produced.

### The control changed the answer: two of the three "failures" were not failures

`MAX_TURNS=14` ruled out the turn cap — `schema` stayed 0/5 and `docs` reached only 1/5. With runs now
long enough to finish, the control question became answerable, and it inverted the conclusion:

- **`schema`, 5/5 unaided runs caught every hazard** in `migrations/002_add_status.sql` — the
  `ACCESS EXCLUSIVE`/rewrite risk of `ADD COLUMN NOT NULL DEFAULT`, the unbatched full-table `UPDATE`,
  and the `NOW()` problem. No skill loaded.
- **`docs`, unaided runs wrote a reasonable README** (123 words, sectioned: Modules / Database /
  Testing). No skill loaded.

> "Always include a no-guidance control. **If the control doesn't exhibit the failure, there is nothing
> to fix — stop, don't author the guidance.**" — `obra/superpowers/skills/writing-skills/SKILL.md`

So the pushy rewrite was **reverted for those two**. It measured no dispatch improvement, and keeping it
would spend listing budget the whole library shares in order to force a skill that adds nothing on that
request. `review-cluster` keeps its pushy version, because that one measured 1/5 → 5/5.

**This reframes what "58% routing accuracy" means.** A `NONE` is only a defect when the unaided run does
the job *worse*. Dispatch rate is not the objective; it was a proxy, and on two of ten probes the proxy
disagreed with the outcome. The open question this leaves is a strategic one, not a wording one:
whether `schema` and `docs` earn their share of the listing at all for requests the base model already
handles — which is a decision about the library, not a description to tune.

## The fixture-fit trap, third occurrence — now handled structurally

`ui` and `brain` both scored 0/5, and neither was a routing failure. The fixture contains **zero** UI
files (`find fixture -name '*.tsx' -o -name '*.jsx' -o -name '*.html' -o -name '*.css'` → 0) and **no
memory store**, so "does this look right on mobile" and "audit what you remember about this project"
had nothing to act on. Same trap as `review my changes` in a clean checkout, hit twice more.

They now live in `probes-unmeasurable.tsv` and are **excluded from the score** rather than counted as
misses. That is not hiding a failure — it is refusing to report a number that measures the fixture.
`ui` genuinely needs a browser and a dev server, which is out of scope here.

**Before adding a probe, check the fixture can answer it.** A probe the fixture cannot satisfy produces
a confident 0/5 that looks exactly like a description defect.
