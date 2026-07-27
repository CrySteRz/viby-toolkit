---
name: principles
description: >
  The operating contract every viby-code skill and agent follows — accuracy rules, the
  fan-out law, model-routing and escalation, context discipline, evidence-gating. Load it
  when another viby-code skill says "follow /viby-code:principles", before deciding whether
  to fan out subagents or which model to route work to, or when explaining how viby-code
  decides. Reference material — read it, don't "run" it.
---

# Viby-code Operating Principles

The shared contract. Every viby-code skill (`orchestrate`, `review-cluster`, `debug`,
`migrate`, `refactor`, `plan`, `verify`, `test`, `explore`, `secure`, `perf`, `release`, `schema`, `incident`, `observe`, `api`, `learn`) and agent (`scout`, `implementer`, `reviewer`,
`skeptic`, `debugger`) is built on these. It's a reference — read it, don't "run" it.

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
- **Know the shape of the degradation, because it is not what you expect.** Measured across
  formats (arXiv 2607.19257), recall holds at ceiling to roughly 64–128k tokens and then falls
  away sharply. And near the ceiling the dominant failure is **refusal, climbing to 79–90%** —
  not fabrication, which measured *exactly zero* across 5,760 absent-fact probes. So budget for
  "it declined to answer", not for "it made something up": if an agent starts refusing or
  hedging on work it managed earlier, suspect context pressure before suspecting the prompt.
  A 1M window does not exempt you — degradation was severe at 100k in windows many times that.
- **Compact with a ledger, not blindly.** Before a compaction/`/clear` decision, take
  stock of what's actually in context — the durable artifacts, the files still needed,
  and the large stale tool outputs — and evict the stale bulk first (raw tool output
  older than a few turns is worth replacing with its one-line conclusion; the subagent
  already returned that conclusion, so keep only it). Deciding what to drop from an
  explicit inventory beats hitting a blind threshold. Less context frequently *beats*
  full context on accuracy, not just cost — verbatim old tool spew distracts.
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
  context isolation improves quality. This is what viby-code's `scout`/`reviewer`/`skeptic`/
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
- **Effort ceilings — match fleet size to the task.** Simple fact-finding: 1 agent, a few
  tool calls. A comparison or multi-area map: 2–4 agents, ~10–15 calls each. 10+ agents
  only for a genuinely broad audit/migration. Spawning 50 subagents for a simple question
  is the classic waste.
- **The failure is at the seams, not inside the agents.** The MAST taxonomy (Cemri et al.,
  1,600+ annotated traces across 7 multi-agent frameworks) attributes **~36.9% of failures to
  inter-agent misalignment** — communication breakdown, context lost during handoff,
  conflicting outputs, format mismatch between agents. Not one of those is a reasoning failure;
  they are all interface failures, and they are the majority category. So spend your care on the
  brief and the return format, not on the agent's cleverness. Reconciling several agents'
  output is where *you* introduce bugs, which is why writes stay single-threaded.

## 4. Model-routing table

Route each unit of work to the cheapest model that can do it *correctly*. Agents declare
their model in frontmatter; the orchestrator picks per task.

Use the **whole lineup**, matched to need — Haiku 4.5 → Sonnet 5 → Opus 4.8 → Fable 5:

| Work type | Model | Why |
|---|---|---|
| Mechanical search, file-location, "does X exist", grep-and-report | **haiku** | Pattern-matching, no deep judgment. High rate-limit headroom. |
| Reading a subsystem and summarizing, single-file implementation to a clear spec, one review dimension, refuting one claim | **sonnet** | Solid reasoning at a fraction of the cost. The workhorse for read-only fan-out. |
| Planning, architecture, cross-cutting judgment, final synthesis, ambiguous root-cause, reconciling conflicting reports, all writes to shared code | **opus / inherit (main thread)** | Hard judgment where a wrong call is expensive. Never delegate the final decision to a cheap model. |
| The hardest, highest-stakes calls: authoring the reproduction test (the proven bottleneck), resolving conflicting validator verdicts, a subtle security/concurrency judgment, a gnarly root-cause that beat opus, the top rung of the escalation ladder | **fable** | The most capable tier. Reserve it for where being wrong is most expensive — it's the heaviest on rate-limit, so it's a scalpel, not a default. |

The main thread runs whatever model you've selected (it makes the final decisions); the
table governs what each **subagent** is dispatched with, and where to escalate.

- **Cheap models find; the strong main thread decides.** Most tokens spent cheaply,
  accuracy preserved where it matters.
- **Five cheap scouts beat one strong scout for coverage; one strong arbiter beats five
  cheap ones for the final call.** Cheap+parallel to gather, strong+single to decide.
- **Don't switch `/model` or `/effort` mid-task** — it invalidates the prompt cache
  (costs latency and rate-limit budget). On Max, cache hits don't count against your
  rate limit and aren't billed, so a stable prefix (stable CLAUDE.md, stable tool set) is
  pure headroom. Subagents run fresh contexts and don't inherit the parent's cache — one
  more reason to fan out only when the read-work justifies it.

### The escalation ladder (route down, escalate on doubt)

Cheap models are the default, but they fail *silently* on the wrong tasks. So:
- **Cheap subagents signal, not just answer.** Every cheap-model agent's output carries a
  `confidence: high | medium | low` and, when stuck, `escalate: true` + a one-line reason.
- **The orchestrator escalates** on `escalate: true`, low confidence, or a failed
  validation/check: re-dispatch the same task up one tier (haiku → sonnet → opus → fable).
  Escalating a hard case beats accepting a cheap wrong answer; fable is the last rung for
  the genuinely hardest calls.
- **Where cheap models are dangerous (route up):** multi-step chained reasoning,
  instruction drift in long prompts, interpretive/judgment calls, whole-system synthesis,
  and anything with no downstream check. `scout` (haiku) is safe only for narrow, explicit,
  single-hop retrieval; cross-file synthesis or judgment belongs on sonnet+.

### The four-part subagent contract

Every subagent prompt must specify four things or it will drift/duplicate work:
**objective**, **output format** (a schema), **tools/sources to use**, and **task
boundaries** (its lane, and what NOT to do). Hand it lightweight references (paths,
queries) and let it load data just-in-time; don't pre-stuff its context.

Given that interface failures are the largest category above, two additions earned in practice:

- **A returned report can arrive truncated** — you may receive only its tail, with the findings
  gone. Treat that as an interface failure, not a result: ask the agent to re-send a
  self-contained report, stating that you have none of its earlier context. Never summarise a
  report you only partially received.
- **Demand negative results explicitly.** Ask what the agent checked and found *clean*, not
  only what it found. Without that you cannot tell "nothing there" from "never looked", and
  those are very different inputs to your decision.

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

A zero exit code is not automatically a pass — zero tests collected, an all-skip run, a
`|| true`, or a cached result all exit 0. `/viby-code:verify` runs this gate as a
procedure: find the real checks, scope them to the change, exercise the actual behavior,
then screen the output for those silent-pass modes.

The same gate applies to the tests themselves: **a test never observed failing is not known
to test anything.** Coverage proves a line executed, not that a wrong value would be caught
— a suite can be fully covered and still survive nearly every mutation of the code. So see
each new test go red for the right reason before trusting it, and be suspicious of tests
that assert on mocks rather than outcomes (`/viby-code:test`).

## 6. Adversarial verification kills false positives

Any finding shown to the user as a "bug" must survive gates before it surfaces: it must
**quote the exact line** it's about (verified to exist), then a **single fresh-context
validator** — not a same-model majority vote — must confirm it's real, introduced by this
change, and not already handled, with a conservative reject-on-doubt bias. Same-family
model panels share blind spots, so a majority can rubber-stamp a correlated hallucination;
one independent validator that will *execute* a checkable claim beats N agreeing opinions.
A gap-hunting reviewer always finds gaps — so reviewers flag **correctness only** (taste →
`/simplify`), and validators see the claim, not the author's reasoning. Prefer a
fresh-context reviewer over self-review: models are weak at judging their own output.
Full protocol in `review-cluster`.

## 7. Compounding — each solved problem makes the next cheaper

When you solve something non-obvious, or the user rejects a review finding as unwanted,
**record the lesson to the project's Claude memory** (see the `learn` skill) so future
sessions don't re-research it or re-flag it. This is the compound-engineering loop,
adapted to Claude's native memory so it's portable and needs no extra infrastructure.

## 8. Skill libraries degrade by OVERLAP, not by size

Adding a skill is not free, but the cost is not where intuition puts it. Measured over a
growing skill library (arXiv 2605.24050), pass rate fell up to 21% at 202 skills — and the
mechanism was **skill shadowing**: a skill whose description semantically overlaps another's
hides it from selection, exactly like variable shadowing. The fraction of runs invoking the
right skill fell from 88% to 53%. Meanwhile the cost of the extra *context* was
"statistically indistinguishable from zero".

So the rule is not "few skills". It is **distinguishable descriptions**:

- Every description says what the skill is for **and what it is not for**, naming the sibling
  to use instead. A mutual cross-reference is the disambiguation.
- No two skills claim the same literal trigger phrase.
- Check it mechanically rather than by eye —
  `skills/principles/scripts/check-skills.ts` measures pairwise similarity and flags trigger
  collisions. It is part of the repo's own pre-push gate.

This also corrects a tempting mistake: trimming an always-on preamble is worth doing for
redundancy, but *not* on the theory that its tokens degrade selection. They do not measurably.
Overlapping descriptions do.

## 9. Portability & secrets

This toolkit syncs across work and personal machines via a private Git marketplace.
- **Never hardcode secrets, tokens, internal hostnames, or client names** anywhere here.
- Behavior must be **project-agnostic** — detect the stack at runtime.
- Per-project overrides live in that project's `.claude/`, never in this repo.
- Keep any always-loaded text (this plugin's SessionStart injection, a project CLAUDE.md)
  lean: for each line ask *"would removing this cause a mistake?"* If not, cut it — a
  bloated always-on context makes Claude ignore the instructions that matter.
