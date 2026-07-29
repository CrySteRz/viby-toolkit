# §3 in depth — the fan-out law, and the correction to it

Read before any fan-out that writes, and before choosing a fleet size.

> **Fan out for READ. Keep WRITES single-threaded — unless you have mapped the dependency graph
> and can name the partition.**

## The two halves

- **WIN — parallel read-only subagents:** search, explore, retrieve, analyze, review. They are
  independent, their verbose output would otherwise pollute your window, and context isolation
  improves quality. viby-toolkit's `scout`/`reviewer`/`skeptic`/`debugger` are all read-only by design.
- **TRAP — parallel writers:** two agents editing from partial context make conflicting decisions and
  produce incoherent results you then pay to reconcile. Use extra agents for *intelligence, not
  actions.*

## The correction — the trap is naive partitioning, not parallel writing as such

Stated flatly, "never fan out to write" is too strong, and honesty requires recording the
counter-evidence. Co-Coder (arXiv 2606.00953) parallelised repository-level *coding* and beat both
sequential and file-based-parallel baselines — and Claude Code with Agent Teams — by **+14.0% pass
rate, up to 2.10× wall-clock, and −35% API cost**, across 28 real tasks on DevEval and
CodeProjectEval, *with the largest gains on the most dependency-dense projects*.

What made the difference was not more agents but **how the work was cut**: build the dependency graph
by static analysis, **isolate the structural hub files**, partition along community boundaries rather
than by file or by feature name, and schedule dependency-aware. Framed properly it is a
graph-partitioning problem trading communication against computation. So the operative rule is:

> **Do not parallelise writes across a dependency boundary you have not mapped.**

Default to single-threaded writes, because you usually have not mapped it and mapping is work.
Parallelise writes only when you can name the partition and the hubs — and **take the hub files
yourself, sequentially**, since they are what everything else depends on.

## How to produce that partition

The operational half this law was missing: **decide the architecture once, scope each work item to
files it exclusively owns, and order the items into dependency waves.**

- One agreed architecture prevents *semantic* conflict between parallel agents (API style, data
  model, naming, error handling) — conflicts a merge cannot detect because every file is
  individually valid.
- Disjoint file ownership prevents *textual* conflict.
- Waves make the ordering explicit rather than hoped for.

With all three, fan-out needs no coordination; with any one missing, you pay reconciliation.

This is checkable rather than aspirational: `skills/plan/scripts/check-plan.ts` reads a task list and
fails if two tasks with no dependency between them own the same file, if the dependency graph has a
cycle, or if a task does not say what it owns and how it is verified.

## Sizing, and the cost of a seam

- **The rate-limit reality:** every fan-out spends against your Max limits roughly an order of
  magnitude faster (Anthropic measured multi-agent at ~15× a single chat). Gate each fan-out behind:
  *"is this genuinely parallel AND read-only?"* If no → do it inline.
- **Don't fan out for what you already know.** Known file, known symbol → read it inline. Agent
  spin-up plus its own context costs more than a direct read for a single known fact.
- **Effort ceilings.** Simple fact-finding: 1 agent, a few tool calls. A comparison or multi-area
  map: 2–4 agents, ~10–15 calls each. 10+ agents only for a genuinely broad audit or migration.
- **The failure is at the seams, not inside the agents.** The MAST taxonomy (Cemri et al., 1,600+
  annotated traces across 7 multi-agent frameworks) attributes **~36.9% of failures to inter-agent
  misalignment** — communication breakdown, context lost during handoff, conflicting outputs, format
  mismatch. Not one is a reasoning failure; they are all interface failures, and they are the
  majority category. Spend your care on the brief and the return format, not on the agent's
  cleverness. Reconciling several agents' output is where *you* introduce bugs, which is why writes
  stay single-threaded.

A caveat on importing anyone's multi-agent numbers, including Anthropic's own: their multi-agent
research post reports "90.2% better than single-agent" and "~15× the tokens" on an internal
*research* eval, and the same post says multi-agent does poorly where work "requires all agents to
share the same context or involves many dependencies between agents", and that token usage alone
explains 80% of the performance variance. Coding is closer to the stated poor-fit case. Justify
fan-out here with results measured on this repo (`docs/reviews/`), not with their benchmark.
