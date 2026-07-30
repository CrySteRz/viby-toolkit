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

| date | version | probes × reps | first-choice accuracy | notes |
|---|---|---|---|---|
| 2026-07-30 | 2.18.0 | 10 × 5 = 50 | **58%** | zero `WRONG`; all 21 failures were `NONE` |
| 2026-07-30 | 2.18.0 | 4 × 5 = 20 | — | dirty-fixture arm; `verify` 1/5 → 5/5 |

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
