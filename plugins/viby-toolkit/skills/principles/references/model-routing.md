# Model routing, the escalation ladder, and the subagent contract

Reference for `/viby-toolkit:principles` §4. Loaded on demand, not part of the always-on
contract — the measured instruction budget is what moved it here: a skill body is a list of
simultaneous directives, perfect compliance collapses to zero at N=80, and ~40 is a redesign
threshold (arXiv 2607.19257). Keeping the routing tables in the body pushed it toward that
line for material that is consulted, not obeyed continuously.

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
