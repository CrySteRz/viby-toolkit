# §8 in depth — how a skill library actually degrades

Read before adding a skill, renaming one, or trying to fix a mis-route.

## Overlap, not size

Measured over a growing skill library (arXiv 2605.24050), pass rate fell up to 21% at 202 skills
(~8% at 52, ~14% at 102) — and the mechanism was **skill shadowing**: a skill whose description
semantically overlaps another's hides it from selection, exactly like variable shadowing. The fraction
of runs invoking the right skill fell from **88% to 53%**. Meanwhile the cost of the extra *context*
was "statistically indistinguishable from zero".

So the rule is not "few skills", it is **distinguishable descriptions**. Checked mechanically by
`skills/principles/scripts/check-skills.ts`, which measures pairwise similarity and flags trigger
collisions as part of the pre-push gate.

This corrects a tempting mistake: trimming an always-on preamble is worth doing for redundancy, but
*not* on the theory that its tokens degrade selection. They do not measurably. Overlapping
descriptions do.

## But there IS a hard mechanical limit, and it is not in that paper

Verified against the Claude Code binary (2.1.220) rather than documentation. Its own string:

> "The skill listing is budgeted at ~1% of the context window; when summed descriptions exceed it,
> entries get truncated and skill routing degrades — so a bloated listing matters even before raw
> token cost does."

Constants in that binary: `skillListingBudgetFraction=0.01`, `bytesPerToken=4`,
`skillListingMaxDescChars=1536` (per-skill cap; the tail is cut silently), default context `200_000`.
So the budget is `0.01 × context_tokens × 4` **characters** — 8,000 at 200k, 40,000 at 1M.

The overflow *order* is the part that hurts (code.claude.com/docs/en/skills): descriptions are dropped
**starting with the skills you invoke least**. A fresh session has no invocation history, so a
library's descriptions are among the first cut to name-only.

**The consequence is counterintuitive and it inverts the obvious fix.** Writing a longer, more
explicit description with more trigger phrases makes mis-routing *worse*, because every added
character raises the overflow that strips those phrases. Measured on this machine 2026-07-29: 160
installed skills totalled 62,644 chars — 7.8× over an 8,000-char budget — and viby-toolkit alone was
15,469, overflowing the entire listing by itself.

## So: a description is a trigger, not a summary

From `obra/superpowers`' own A/B testing:

> "Testing revealed that when a description summarizes the skill's workflow, an agent may follow the
> description instead of reading the full skill content. A description saying 'code review between
> tasks' caused an agent to do ONE review, even though the skill's flowchart clearly showed TWO
> reviews."

A workflow summary in a description is therefore doubly bad: it spends budget the whole library shares,
*and* it hands the agent a shortcut past the body it was supposed to load.

These two forces pull against each other — the budget wants brevity, anti-shadowing wants
distinctness — and the rule that satisfies both is:

> **Add distinguishing words as TRIGGERS (things a user would literally say), never as summaries.**

An utterance both distinguishes two skills and routes a request. A sentence describing what the skill
produces does neither.

## Levers, cheapest first

- `paths:` frontmatter — glob-scoped activation. The only lever that costs no listing budget.
- Directory-scoped skills (`apps/web:deploy`) — shrinks the effective routing set per task.
- `skillOverrides: "name-only"` in settings — hands budget from skills you never invoke to ones you do.
- `when_to_use:` — the real field for trigger phrases, but it counts against the same 1,536-char cap,
  so it is not free budget.
- Diagnostics: `/context` (Skills row), `/doctor`, `claude --debug`.

## Measure routing properly, or not at all

Hand-rolled probes are under-powered. From `superpowers/skills/writing-skills/SKILL.md`: "Always
include a no-guidance control. If the control doesn't exhibit the failure, there is nothing to fix —
stop… 5+ reps per variant. Single samples lie." Anthropic's `skill-creator` plugin does this properly —
generates should-trigger and should-not-trigger prompts, 60/40 train/test split, 3 repetitions per
query — and should be preferred to a bespoke probe table.
