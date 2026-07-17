---
name: review-cluster
description: >
  Review a diff or body of code with a parallel multi-dimension review cluster, then run
  an adversarial false-positive filter so only real, confirmed issues survive. Each raw
  finding is attacked by skeptic verifiers whose job is to refute it; findings that can't
  be refuted with concrete code-grounded counter-arguments are dropped. Use before
  shipping, on a PR, after implementing a feature, or whenever the user asks for a code
  review they can trust. Invoke with /forge:review-cluster.
---

# Review Cluster + False-Positive Filter

```
IRON LAW: A finding reaches the user only if a skeptic tried to REFUTE it and failed.
          Flag correctness/requirement gaps only — never taste. Crying wolf once
          teaches the user to ignore every future review.
```

Two stages. Stage 1 (the **review cluster**) maximizes *coverage* — many cheap reviewers,
each on a different dimension, finding everything plausibly wrong. Stage 2 (the **false
positive cluster**) maximizes *precision* — adversarial skeptics that try to kill each
finding. Only findings that survive the skeptics reach the user.

Reviewers and skeptics run on **fresh context** — an unbiased reviewer catches what the
author rationalizes. And a reviewer told to "find gaps" will always find some, which is
how false positives are born: so reviewers report only correctness and requirement gaps,
and the skeptic stage exists precisely to kill the rest.

This directly implements the accuracy-first + adversarial-verification rules in
`/forge:forge-principles`. The whole point is to **never cry wolf**: a review that
reports hallucinated bugs is worse than no review, because it trains you to ignore it.

## Determine the target

Default target is the current diff. Find it:
- Uncommitted work: `git diff` and `git diff --staged`.
- A branch/PR: `git diff <base>...HEAD` (find the base branch).
- If the user names files or a range, use that.

If there is genuinely nothing to review (docs-only, whitespace), say so and stop — don't
manufacture findings.

## Stage 1 — Review cluster (parallel `reviewer` agents, cheap model, coverage)

Spawn `reviewer` agents in parallel, **one per dimension**, each seeing the diff and
enough surrounding context. Standard dimensions (drop ones that don't apply, add
project-specific ones):

1. **Correctness** — logic errors, wrong conditions, off-by-one, unhandled cases, broken
   control flow, incorrect API/contract usage.
2. **Edge cases & error handling** — null/undefined/empty, boundaries, error paths,
   partial failure, resource cleanup, race conditions/concurrency.
3. **Security** — injection, authz/authn gaps, secret exposure, unsafe deserialization,
   SSRF/path traversal, missing validation on untrusted input.
4. **Data & state** — persistence correctness, migrations, transaction boundaries,
   caching/invalidation, state mutation bugs.
5. **Regression & integration** — does this break existing callers, contracts, or
   assumptions elsewhere? (This reviewer should check call sites, not just the diff.)
6. **(Optional) Performance** — only when the change is on a hot path; N+1, unnecessary
   work in loops, blocking I/O. Don't spawn this for a config tweak.

Each reviewer returns findings as structured items:
`{severity, file:line, one-sentence claim, concrete failure scenario (inputs → wrong
result)}`. A reviewer that can't tie a finding to a real failure scenario must drop it —
"this could be cleaner" is not a bug (route true cleanups to `/simplify`, not here).

**Dedup:** collect all findings, merge ones that are the same issue at the same location.
Now you have the raw candidate list. Do **not** show it to the user yet.

## Stage 2 — False-positive filter (adversarial `skeptic` agents, the kill stage)

For each surviving candidate, run adversarial verification. This is the part that makes
the whole thing trustworthy.

- Spawn **skeptic** verifiers per finding whose explicit instruction is: *try to refute
  this. Default to "not a real issue" unless the code proves otherwise.* Each skeptic
  independently reads the actual code around the finding and returns
  `{verdict: real | refuted, reason grounded in file:line, confidence}`.
- **Perspective-diverse voting** for findings that can fail in more than one way: give
  each skeptic a distinct lens rather than three identical ones —
  (a) *does it reproduce?* (construct the concrete input that triggers it),
  (b) *is it already handled?* (a guard/validation/caller-side check the reviewer missed),
  (c) *is the claim even correct about what the code does?*
- **Kill rule:** a finding is dropped if a majority of skeptics refute it (e.g. ≥2 of 3),
  or if any skeptic produces a *decisive* refutation (shows the exact guard that makes it
  impossible, or shows the reviewer misread the code). Ambiguous → keep, but downgrade
  severity and label it "unconfirmed — needs human check."
- Cheap model is fine for skeptics **because they cross-check each other**; that's the
  routing rule (many cheap voices that get reconciled). You (strong main thread) make the
  final keep/kill call when skeptics disagree.

## Report (only survivors)

Present confirmed findings ranked most-severe first. For each:
- `file:line` — one-sentence issue — concrete failure scenario — suggested fix.
- Mark each as **confirmed** (skeptics couldn't refute) or **unconfirmed** (kept but
  uncertain).

Then one line: how many raw candidates the cluster found and how many the filter killed
(e.g. "12 candidates → 4 confirmed, 1 unconfirmed, 7 killed as false positives"). This
transparency is the feature — it tells the user the filter is doing its job.

If everything got killed, say "no real issues found; N candidates were all false
positives" — that is a valid, valuable result. Don't invent something to report.

## Applying fixes

Only if asked (or if invoked from `/forge:orchestrate`'s self-review): fix confirmed
findings, then re-verify each fix. Never mark fixed without re-checking.

## Learn from rejections (compounding)

If the user dismisses a confirmed finding as *not something they want flagged* ("that's
our convention", "we intentionally do X", "stop suggesting Y"), that's a false-positive
class you should never surface again. Invoke `/forge:learn` to record the preference to
project memory. Over time the cluster stops re-flagging this project's accepted patterns —
the reviewer's "taste" compounds toward the user's, which is the whole point of a review
you keep rather than mute.

## Token discipline

Reviewers and skeptics run on cheap models in parallel; their file reads die with them.
You keep only the deduped candidate list and the verdicts. Scale the cluster to the diff:
a 20-line change needs 2–3 dimensions and single-vote skeptics; a 2000-line change
warrants the full dimension set and 3-vote adversarial verification. Don't run the heavy
cluster on a trivial diff.
