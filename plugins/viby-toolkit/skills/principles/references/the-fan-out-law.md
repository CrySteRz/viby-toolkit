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
- **The failure is in the design and the seams, not inside the agents.** The MAST taxonomy (Cemri et
  al., **1642** annotated traces across 7 multi-agent frameworks) splits failures **System Design
  44.2% / Inter-Agent Misalignment 32.3% / Task Verification 23.5%**. Not one is a reasoning failure.
  Spend your care on the orchestration shape, the brief and the return format, not on the agent's
  cleverness. Reconciling several agents' output is where *you* introduce bugs, which is why writes
  stay single-threaded.

  > **Correction, 2026-08-27.** This file previously said "~36.9% ... inter-agent misalignment ... the
  > majority category". Both halves were wrong: the real figure is 32.3%, and the majority category is
  > System Design at 44.2%. The 36.9 traced to a **WebFetch summarization artifact** — a re-query of the
  > same render could not find any such number in the paper. Corrected against `pdftotext` output from
  > the v3 PDF. That the toolkit's own always-loaded contract carried a hallucinated citation for
  > months is the strongest argument there is for `/viby-toolkit:study`'s verbatim-quote rule.

- **Error amplification is topology-dependent, and this is the case for orchestrating rather than
  merely spawning:** independent agents amplify errors **17.2x** through unchecked propagation, while
  **centralized coordination contains it to 4.4x** (Kim et al., arXiv 2512.08296). Never let agents
  chain to each other; every result comes back through you.
- **Capability saturation — the rule that stops pointless fan-out.** "coordination yields diminishing
  or negative returns (beta=-0.408, p<0.001) once single-agent baselines exceed ~45%" (same paper). If
  the main thread already handles this class of task well, fanning out makes it *worse*, not slower.
- **A hard ceiling at 3–4 agents per stage:** "per-agent reasoning capacity becomes prohibitively thin
  beyond 3-4 agents, creating a hard resource ceiling". The "10+ agents for a broad audit" line above
  is the one exception, and only when each agent owns a genuinely disjoint slice.

A caveat on importing anyone's multi-agent numbers, including Anthropic's own: their multi-agent
research post reports "90.2% better than single-agent" and "~15× the tokens" on an internal
*research* eval, and the same post says multi-agent does poorly where work "requires all agents to
share the same context or involves many dependencies between agents", and that token usage alone
explains 80% of the performance variance. Coding is closer to the stated poor-fit case. Justify
fan-out here with results measured on this repo (`docs/reviews/`), not with their benchmark.
