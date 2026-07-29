# What the best Claude skill collections do, and the one thing that changed my mind

> Status: **4 parallel researchers** on Superpowers (`obra/superpowers`), GStack (`garrytan/gstack`),
> the Awesome-Claude-Skills lists, and Anthropic's own authoring docs. Findings carry verbatim quotes
> and URLs, fetched 2026-07-29. **One finding was verified against the Claude Code binary rather than
> the docs, and it overturns a claim I made earlier in this repo.** Three mechanisms adopted, two
> rejected on a standing constraint, one methodology critique accepted.

## The finding that matters: the skill listing is budgeted, and overflow degrades routing silently

I measured 50% correct routing over 10 probes against ~90 installed skills and concluded my
descriptions needed to be more explicit. **That was wrong, and it was making the problem worse.**

From `strings` on the Claude Code binary (2.1.220) — not from the docs:

> "The skill listing is budgeted at ~1% of the context window; when summed descriptions exceed it,
> entries get truncated and skill routing degrades — so a bloated listing matters even before raw
> token cost does."

The binary also carries a `Skill listing over budget:` warning string and these constants:

| Constant | Value | Meaning |
|---|---|---|
| `skillListingBudgetFraction` | `0.01` | 1% of the context window |
| `bytesPerToken` | `4` | so the budget is `0.01 × ctx × 4` **characters** |
| `skillListingMaxDescChars` | `1536` | per-skill cap; the tail is cut silently |
| default context | `200_000` | → an **8,000-char** listing budget |

And the official docs supply the overflow *order*, which is the part that stings:

> "The listing always contains every skill name, but if you have many skills, Claude Code shortens
> descriptions to fit the listing's character budget, which can strip the keywords Claude needs to
> match your request… When the listing overflows, Claude Code drops descriptions starting with the
> skills you invoke least, so the skills you use most keep their full text."
> — code.claude.com/docs/en/skills, fetched 2026-07-29

### Measured on this machine

| | chars | vs 8,000 budget (200k ctx) |
|---|---|---|
| All 160 active installed skills | 62,644 | **7.8× over** |
| viby-toolkit alone, before this pass | 15,469 | 193% — **overflows on its own** |
| viby-toolkit alone, after this pass | 10,261 | 128% |

A fresh session has no invocation history, so *my* descriptions are among the first cut to
name-only. That is a complete mechanical explanation for the 50% figure, and it means every trigger
phrase I added over this project raised the overflow that strips those very phrases. **Longer
descriptions were the wrong direction.** The correction is in the code as a check, not just here.

## Adopted

**1. A listing-budget check (`check-skills.ts`).** Per-skill P1 over the 1,536 hard cap, P2 over a
400-char target, and a P3 informational total. The total is P3 *by name* rather than by severity: a
31-skill library can never clear it however well each line is written, and a permanently-red gate is
worse than no gate — but excluding it by severity would have silenced `shadowing-watch`, which is
what that test exists for.

**2. Descriptions are triggers, not summaries** — from Superpowers' own A/B result, which is the
most useful non-obvious thing in this entire survey:

> "Testing revealed that when a description summarizes the skill's workflow, an agent may follow the
> description instead of reading the full skill content. A description saying 'code review between
> tasks' caused an agent to do ONE review, even though the skill's flowchart clearly showed TWO
> reviews… When the description was changed to just 'Use when executing implementation plans with
> independent tasks' (no workflow summary), the agent correctly read the flowchart."
> — `obra/superpowers/skills/writing-skills/SKILL.md`, fetched 2026-07-29

So a workflow summary in a description is not merely wasted budget — it is a **shortcut that lets the
agent skip the body it was supposed to load**. Ten of my descriptions had one. All are gone; every
trigger phrase was kept. 34% saved on the 21 mechanically compressible ones.

**3. Numeric length budgets, verified against the primary source.** Anthropic's official tiering:

> "1. Metadata (name + description) — Always in context (~100 words) / 2. SKILL.md body — In context
> whenever skill triggers (<500 lines ideal) / 3. Bundled resources — As needed (unlimited, scripts
> can execute without loading)"
> — `anthropics/skills/skills/skill-creator/SKILL.md`, fetched 2026-07-29

Superpowers goes further and verifies with `wc -w`: "getting-started workflows: <150 words each;
Frequently-loaded skills: <200 words total; Other skills: <500 words." My `principles` is 2,838 words
and is loaded by nearly every other skill — the worst possible ratio. Only 6 of 31 skills use
`references/`. That is the next piece of work, and it is real.

## The tension this pass discovered by walking into it

Compression is not free, and my own checkers caught both costs within one command:

1. **Shorter descriptions are more similar to each other.** Cutting 34% created two new
   `shadowing-watch` pairs (`evaluate ↔ plan`, `explore ↔ verify`). Shadowing is the *documented
   dominant cause* of routing failure at scale (88% → 53%), so the budget and the anti-shadowing
   requirement pull in opposite directions.
2. **I cut three trigger phrases that earlier probes had measured as necessary** — `#4`, `#12`, `#32`
   went red immediately.

The resolution is a rule worth keeping: **add distinguishing words as triggers, never as summaries.**
A user utterance distinguishes two skills *and* routes a request; a sentence describing what the
skill produces does neither, and invites the agent to skip the body. Both problems cleared without
re-inflating anything.

## Adopted as a critique of my own method

Superpowers tests skills the way this repo tests code, and it is stricter than my routing probes:

> "If you didn't watch an agent fail without the skill, you don't know if the skill teaches the right
> thing… One fresh-context sample per call… Always include a no-guidance control. If the control
> doesn't exhibit the failure, there is nothing to fix — stop, don't author the guidance… 5+ reps per
> variant. Single samples lie."

My routing measurement was **10 probes, one sample each, no control**. Its 50% number is directionally
real (a replication reproduced 4 of 5 failures) but it is not a rate I should quote as precise.
Anthropic now ships tooling for exactly this — `skill-creator` "generates should-trigger and
should-not-trigger prompts, measures the hit rate, and proposes description edits," with a 60/40
train/test split and 3 repetitions per query. **Use that instead of hand-rolling probes.**

## Rejected, with the bar each failed

- **GStack's `PreToolUse` blocking hooks** (`check-careful.sh` denies destructive Bash,
  `check-freeze.sh` denies edits outside a boundary, both fail-closed). Genuinely well built — and
  refused on a standing instruction for this repo: *"i dont want those guardrails, this plugin is
  only used by me."* Not a technical objection; the owner has decided, and it stays decided.
- **Non-standard frontmatter as a routing mechanism.** GStack ships `triggers:`, `preamble-tier:`,
  `benefits-from:`, `version:`. Anthropic's authoritative field list has no `triggers:` — the honoured
  set is `name`, `description`, `when_to_use`, `allowed-tools`, `disallowed-tools`,
  `disable-model-invocation`, `user-invocable`, `model`, `effort`, `context`, `agent`, `background`,
  `hooks`, `paths`, `argument-hint`, `arguments`, `shell`. Inventing fields the harness ignores
  *looks* like dispatch metadata while doing nothing. **`when_to_use` is the real field for this, and
  it counts against the same 1,536-char cap — so it is not free budget either.**
- **Anthropic's multi-agent numbers as justification for fan-out here.** "90.2% better than
  single-agent" and "15× more tokens" are from their internal *research* eval; the same post says
  multi-agent does poorly where work "requires all agents to share the same context or involves many
  dependencies between agents," and that "token usage by itself explains 80% of the variance." My
  domain is closer to their stated poor-fit case than to their success case. Fan-out stays justified
  here by the measured review results in `docs/reviews/`, not by their benchmark.

## Worth taking later, not taken today

- **`paths:` frontmatter** — glob-scoped activation. A filter that cuts false triggers *without*
  spending listing budget, which is the only lever found that is free on both axes.
- **Directory-scoped skills** (`apps/web:deploy`) — shrinks the effective routing set per task.
- **`skillOverrides` → `"name-only"`** in settings, to hand budget from skills I never invoke to the
  ones I do. This is a *user settings* change, not a repo change, so it is the owner's call.
- **Event-sourced decision log** (GStack: append-only `decisions.jsonl`, supersede/redact/compact,
  "works with gbrain OFF"). My `brain`/`learn` write Markdown memories with no supersede operation.
  Their split — durable decisions vs. ephemeral turns, with the semantic layer "an optional
  enhancement, never a dependency" — is the better architecture.
- **A ledger file for orchestration**, because compaction eats controllers: "controllers that lost
  their place have re-dispatched entire completed task sequences — the single most expensive failure
  observed." My `handoff` skill is the right home.
- **Coverage gaps the ecosystem lists reveal**: marketing/GTM (the largest independent-build category
  by volume), anti-AI-slop prose editing (4+ independent builds), and *skill-quality* tooling —
  NVIDIA `SkillSpector`, Snyk `agent-scan`, `SkilLock` all exist and my `secure` checker is a fourth
  independent build of the same idea, which is a decent sign it was worth building.

## Corrections this research forced

1. **"Make descriptions more explicit to fix mis-routing" was wrong** — it increases the overflow
   that strips the added keywords. Stated confidently earlier in this project; withdrawn.
2. **The 50% routing figure is under-powered**, not wrong: single samples, no control.
3. Anthropic publishes **no routing-accuracy curve versus skill count**. The docs assert 100+ skills
   work and give no number, so "measure it yourself" is the actual state of the art — and the
   listing-budget mechanism, not description wording, is the first thing to measure.

---

## Follow-through: the progressive-disclosure sweep

Done the same day, against the guidance quoted above.

| Skill | body before | after | cut | loaded by N siblings |
|---|---|---|---|---|
| `principles` | 2,774 w / 267 l | **1,248 w / 133 l** | **−55%** | **29 of 30** |
| `study` | 1,795 w / 179 l | 1,358 w / 138 l | −24% | 2 |
| `test` | 1,906 w / 201 l | 1,651 w / 179 l | −13% | 11 |
| `evaluate` | 1,820 w / 183 l | 1,643 w / 166 l | −10% | 6 |
| `adopt` | 1,777 w / 183 l | 1,668 w / 172 l | −6% | 2 |

`principles` was the whole point: it is loaded by 29 of the 30 other skills, so it was the worst
words-per-load ratio in the library by a wide margin. It is now the laws only, with a routing table at
the top pointing at six one-level-deep reference files that carry the evidence and the operational
detail. **Section numbers §1–§10 were preserved deliberately** — siblings cite `§9` six times, `§3`
three times, `§5` twice, plus `§2` and `§8`, and renumbering would have silently broken every one.

The design rule applied throughout: **an agent that reads only the SKILL.md must still behave
correctly.** References carry the *why*, the measured numbers and the worked procedure — never the
rule itself. So `references/the-fan-out-law.md` holds the Co-Coder counter-evidence and the MAST
figure; the law itself stays in the body.

New reference files: `context-discipline.md`, `the-fan-out-law.md`, `evidence-gate.md`,
`skill-library-design.md`, `authored-vs-derived.md` (principles), `scanner-confidence.md` (test),
`gates-and-decision-record.md` (evaluate), `appraisal-and-verification.md` (study),
`mikado-steps.md` (adopt). All 15 reference files across the library are pointed at from their own
SKILL.md — verified, no orphans — and all are one level deep, per: "Claude may partially read files
when they're referenced from other referenced files… Keep references one level deep from SKILL.md."

Gated so it cannot regress, in `check-skills.ts`:

- **P2 `body-over-500-lines`** — Anthropic's stated limit. Max in the library is now 179, so this is a
  guard against future growth rather than a to-do list. It fires *even when* `references/` exists,
  because the body is what loads.
- **P3 `no-progressive-disclosure`** — over 1,800 words with no `references/` at all.

Still unsplit and worth watching: `orchestrate` (1,237 w), `secure` (1,212 w), `analytics` (1,180 w) —
each under the watch threshold, none loaded often enough to be urgent.
