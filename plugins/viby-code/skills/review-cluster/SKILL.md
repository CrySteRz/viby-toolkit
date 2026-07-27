---
name: review-cluster
description: >
  Use when reviewing a diff, a PR, or freshly written code before shipping — or whenever
  the user asks for a code review they can trust. Use after implementing a feature, as the
  self-review step of orchestration, or when the user says "review this", "check my
  changes", "is this correct", "find bugs".
---

# Review Cluster + False-Positive Filter

```
IRON LAW: A finding reaches the user only if (1) it quotes the exact line it's about,
          verified to exist, and (2) a fresh-context validator confirms it's real,
          introduced by this diff, and not already handled. Correctness only — taste
          goes to /simplify. Crying wolf once teaches the user to ignore every review.
```

Findings flow through gates, cheapest-first, so each gate shrinks the work for the next:

```
reviewers (coverage) → grounding gate → validator (precision) → confidence gate → report
```

Reviewers and validators run on **fresh context** (they never see the author's reasoning,
which is what makes them unbiased). This directly implements `/viby-code:principles`
§5–6, and it is the design decision most worth defending: measurements of LLM
self-verification find rechecks are **overwhelmingly confirmatory rather than corrective** —
they rarely identify an error or change the outcome — and the assumed generation–verification
gap does not reliably hold, i.e. a model is often no better at judging its own output than at
producing it. Self-review therefore mostly manufactures confidence. A fresh context that never
saw the reasoning is not a stylistic preference here; it is the only part of the pipeline that
can actually disagree. Dimensions, the finding schema, and the confidence rubric live in
`references/dimensions-and-schema.md` — read it when running a real review.

## 1. Target

Default is the current diff: `git diff` + `git diff --staged`, or `git diff <base>...HEAD`
for a branch/PR, or files the user names. If there's nothing substantive to review
(docs/whitespace only), say so and stop — don't manufacture findings.

## 2. Reviewers — coverage (parallel, cheap, read-only)

Spawn `reviewer` agents in parallel, **one per dimension relevant to this diff** (see the
reference; don't run every dimension on every diff — spawn security only if it touches an
auth/input surface, data-migration only if migration files are present, etc.). Always
include **adversarial** for anything non-trivial — it's the highest-value dimension.

Each reviewer returns findings in the schema, and each finding MUST carry `first_evidence`:
the verbatim line(s) at its `file:line`. A finding with no quotable evidence line is not a
finding.

## 3. Grounding gate — mechanical, before any validator burns tokens

For each candidate, **verify the quoted `first_evidence` actually exists at that
`file:line`** (read/grep it). If the quote doesn't match, the reviewer misread the code →
**drop it** (or demote to 25). This kills the largest false-positive class for near-zero
cost and shrinks the validator workload. Then **dedup**: merge findings with the same
`file` + line-bucket(±3) + normalized title.

## 4. Validator — precision (one fresh validator per surviving finding)

This replaces majority-vote refutation. Same-family model panels share blind spots, so a
2/3 vote can rubber-stamp a correlated hallucination (documented: 80 agents once
unanimously "confirmed" a vulnerability that only *execution* disproved). Instead, dispatch
**one `skeptic` (validator) per finding**, fresh context, given only the candidate claim
and the code — **not** the reviewer's reasoning. It answers three questions:

1. **Real** in the code as written?
2. **Introduced by this diff** (vs pre-existing)?
3. **Not already handled** elsewhere (a guard, validation, middleware, framework default)?

Conservative bias: **when in doubt, reject.** It returns `{validated, reason, confidence}`.

**Execute, don't argue, where you can.** If a finding is mechanically checkable — a failing
assertion, a type error, a reproducible crash — actually run it (or write the tiny repro)
rather than reasoning about whether it reproduces. An executed check beats any number of
agreeing opinions.

## 5. Confidence gate — the calibrated kill

Assign each surviving finding a confidence anchor (0/25/50/75/100 — see reference).
**Suppress everything below 75**, except a **P0 at 50+** survives (labeled "unconfirmed —
needs human check") so critical-but-uncertain never vanishes silently. Cross-reviewer
agreement promotes one anchor step, but never past the grounding gate.

## 6. Report — survivors only, plus a coverage line

Rank confirmed findings most-severe first. For each: `file:line` — the issue — the concrete
failure scenario — suggested fix — `[confirmed]` or `[unconfirmed]`, and its `action_class`
(route `advisory` to `/simplify`; `gated_auto`/`manual` stay here).

End with a **Coverage line** — the audit trail that proves the filter worked: e.g. "14
candidates → 3 grounding-dropped → 4 validator-killed → 2 below confidence → **5 confirmed,
1 unconfirmed**." If everything got killed, say "no real issues; N candidates were all
false positives" — a valid, valuable result. Never invent a finding to look useful.

## 7. Compounding (both directions)

- If the user **rejects** a confirmed finding as unwanted ("that's our convention"),
  `/viby-code:learn` it so the cluster stops re-flagging that class.
- If a reviewer surfaces a **known past failure** for this module (recorded via `learn`),
  cite it as prior context — past bugs raise recall, not just past FPs lowering noise.

## Scale to the diff

A 20-line change: 2–3 dimensions, grounding gate, single validator, skip N-run stability.
A large/high-risk diff: full dimension set incl. adversarial, and optionally **N-run
stability** (run the cluster k times; keep only findings appearing in ≥⌈k/2⌉ runs —
instability is itself a low-precision signal). Don't run the heavy path on a trivial diff.
