---
name: forge-principles
description: The operating contract for the forge toolkit — accuracy-first rules, the read-only-fan-out law, token/rate-limit discipline, frequent intentional compaction, and the model-routing table every forge skill and agent follows. Read this to understand how forge decides when to fan out, which model each subagent runs on, and how it keeps context clean.
disable-model-invocation: true
---

# Forge Operating Principles

The shared contract. Every forge skill (`orchestrate`, `review-cluster`, `debug`,
`migrate`, `plan`, `learn`) and agent (`scout`, `implementer`, `reviewer`, `skeptic`,
`debugger`) is built on these. It's a reference — read it, don't "run" it.

Synthesized from what actually works in production agentic coding (Anthropic's
context-engineering and multi-agent research, humanlayer's Advanced Context Engineering,
obra/superpowers, Cognition, Every's compound engineering) — keeping the mechanisms,
discarding the marketing multipliers.

## 1. Accuracy is the objective. Everything else is a constraint.

Token savings, speed, and elegance are secondary. The failure mode we optimize against is
**confident wrong output** — a hallucinated bug, a "fixed" claim that isn't verified, a
refactor that changed behavior. Never trade correctness for cost.

- **Never claim done without fresh evidence.** See the evidence gate in §5.
- **Ground every finding in `file:line`.** A claim you can't anchor is a hypothesis —
  label it as such.
- **Read the actual code over recalling how it "usually" works** — especially for
  Claude/Anthropic APIs, library versions, and config schemas.

## 2. Context is the master resource. Curate it deliberately.

The LLM is a stateless function; **the contents of the context window are the only lever
on output quality** (humanlayer/ACE). On a Max subscription the scarce resources are the
**main thread's context window** and your **rate-limit budget** — not dollars.

- **Context quality priority: Correctness > Completeness > Size.** Wrong context is
  worst; missing context second; excess tokens are the *least* damaging. Don't over-trim
  and drop something load-bearing to save tokens.
- **Target 40–60% context utilization** (Frequent Intentional Compaction). Reserve
  headroom for iteration and error handling. Don't run the window to overflow — recall
  degrades as it fills ("context rot"), and auto-compaction at ~90% produces noisy
  summaries. Compact *early*, at task boundaries, on purpose.
- **`/clear` liberally.** Between unrelated tasks, clear. **After two failed corrections
  on the same issue, `/clear` and rewrite the prompt** — a clean session with a better
  prompt beats a long session full of accumulated corrections.
- **Subagents are context firewalls.** A subagent that greps 40 files and reads 10
  returns a ~200-token conclusion; the 30k tokens of file dumps die with it and never
  touch main context. This is the single biggest lever.
- **Just-in-time, not dumps.** Hold references (paths, queries); load content on demand
  with targeted reads/grep/`head`/`tail`. Don't pre-load whole files.

## 3. The fan-out law — the rule that decides every delegation

> **Fan out for READ. Keep WRITES single-threaded.**

This is the one law to internalize. It's where both Anthropic and Cognition landed after
a year of production experience.

- **WIN — parallel read-only subagents:** search, explore, retrieve, analyze, review.
  They're independent, their verbose output would otherwise pollute your window, and
  context isolation improves quality. This is what forge's `scout`/`reviewer`/`skeptic`/
  `debugger` agents are — all read-only by design.
- **TRAP — parallel writers:** two agents editing in parallel from partial context make
  conflicting decisions and produce incoherent results you then pay to reconcile. Coding
  is far less parallelizable than research. **Keep writes on a single thread.** Use extra
  agents for *intelligence, not actions.*
- **The rate-limit reality:** every fan-out spends against your Max limits roughly an
  order of magnitude faster (Anthropic measured multi-agent at ~15× a single chat). Gate
  each fan-out behind: *"is this genuinely parallel AND read-only?"* If no → do it inline
  on the main thread.
- **Don't fan out for what you already know.** Known file, known symbol → read it inline.
  Agent spin-up + its own context costs *more* than a direct read for a single known fact.

## 4. Model-routing table

Route each unit of work to the cheapest model that can do it *correctly*. Agents declare
their model in frontmatter; the orchestrator picks per task.

| Work type | Model | Why |
|---|---|---|
| Mechanical search, file-location, "does X exist", grep-and-report | **haiku** | Pattern-matching, no deep judgment. High rate-limit headroom. |
| Reading a subsystem and summarizing, single-file implementation to a clear spec, one review dimension, refuting one claim | **sonnet** | Solid reasoning at a fraction of the cost. The workhorse for read-only fan-out. |
| Planning, architecture, cross-cutting judgment, final synthesis, ambiguous root-cause, reconciling conflicting reports, all writes to shared code | **opus / inherit (main thread)** | Hard judgment where a wrong call is expensive. Never delegate the final decision to a cheap model. |

- **Cheap models find; the strong main thread decides.** Most tokens spent cheaply,
  accuracy preserved where it matters.
- **Five cheap scouts beat one strong scout for coverage; one strong arbiter beats five
  cheap ones for the final call.** Cheap+parallel to gather, strong+single to decide.
- **Don't switch `/model` or `/effort` mid-task** — it invalidates the prompt cache
  (costs latency and rate-limit budget). On Max, cache hits don't count against your
  rate limit and aren't billed, so a stable prefix (stable CLAUDE.md, stable tool set) is
  pure headroom. Subagents run fresh contexts and don't inherit the parent's cache — one
  more reason to fan out only when the read-work justifies it.

## 5. Evidence-gated completion (the anti-"looks done" rule)

> **NO COMPLETION CLAIM WITHOUT FRESH VERIFICATION EVIDENCE.**

Claude stops when work *looks* done; without a check, "looks done" is the only signal.
Before claiming anything complete:

1. **IDENTIFY** the command that proves it (test, build, lint, repro, screenshot).
2. **RUN** it fresh and complete — not from memory of an earlier run.
3. **READ** the full output and exit code.
4. **VERIFY** it actually passed.
5. **CLAIM** only then — *with the evidence attached.*

Red-flag words that mean you're about to violate this: "should," "probably," "seems to,"
and a premature "Done!/Perfect!/Great!". If you didn't run the check, say so explicitly.
Claiming complete without verification is dishonesty, not efficiency.

TDAD nuance: when you verify, run the **specific tests relevant to the change**, named
explicitly — not a generic "do TDD" ritual. Telling an agent *which* tests to check cuts
regressions; a vague TDD lecture makes them worse.

## 6. Adversarial verification kills false positives

Any finding shown to the user as a "bug" must survive a skeptic pass whose explicit job
is to *refute* it, defaulting to "not real unless proven." A gap-hunting reviewer always
finds gaps — so reviewers flag **correctness/requirement issues only** (route taste and
cleanups to `/simplify`), and skeptics on fresh context cross-check before anything
surfaces. Prefer a fresh-context reviewer over pure self-review: models are weak at
judging their own output. Details in `review-cluster`.

## 7. Compounding — each solved problem makes the next cheaper

When you solve something non-obvious, or the user rejects a review finding as unwanted,
**record the lesson to the project's Claude memory** (see the `learn` skill) so future
sessions don't re-research it or re-flag it. This is the compound-engineering loop,
adapted to Claude's native memory so it's portable and needs no extra infrastructure.

## 8. Portability & secrets

This toolkit syncs across work and personal machines via a private Git marketplace.
- **Never hardcode secrets, tokens, internal hostnames, or client names** anywhere here.
- Behavior must be **project-agnostic** — detect the stack at runtime.
- Per-project overrides live in that project's `.claude/`, never in this repo.
- Keep any always-loaded text (this plugin's SessionStart injection, a project CLAUDE.md)
  lean: for each line ask *"would removing this cause a mistake?"* If not, cut it — a
  bloated always-on context makes Claude ignore the instructions that matter.
