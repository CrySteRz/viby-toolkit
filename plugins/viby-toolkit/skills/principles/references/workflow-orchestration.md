# §3b — Stage multi-step fan-out with a script

Read before any fan-out.

## The decision rule: a workflow, or just dispatch?

Fanning out is the default either way (§3). What this file decides is *how*.

| The work is | Do this | Because |
|---|---|---|
| **One stage** — sweep an area, map a codebase, search N ways, run N detectors | **Dispatch 3–4 agents in parallel, in one message.** No script. | A script buys nothing when there is nothing to sequence, and "one agent is not a workflow" applies to one stage too. |
| **Staged** — fan out, then verify/refute each result, then synthesise | **Author a `Workflow`.** | The stages are the part that decays. Declared once, they run identically every time. |

The skills that are staged, and say so: `orchestrate` (research → plan → verify → review),
`review` (review → validate), `study` (research → verify → audit). Everything else —
`explore`, `debug`, `plan`, `migrate`, `secure`, `perf`, `adopt` — is one stage, and dispatches
directly.

When a one-stage sweep turns out to need a verification pass, that is the signal to promote it to a
workflow, not to bolt a second round of hand-dispatch onto the first.

## Why a script for the staged case

Evidence behind every rule here: `docs/studies/2026-08-27-workflow-orchestration-default.md`.

## Why a script and not a paragraph

Two measured reasons, not taste.

- **System design is the single largest failure category in multi-agent systems — 44.2%**, ahead of
  inter-agent misalignment at 32.3% (MAST, 1642 traces). Prose instructions are a design that gets
  re-improvised on every run, under whatever context pressure the session is in. A script is the
  design pinned down, identical on run 1 and run 40.
- **Centralized coordination contains error amplification to 4.4x, versus 17.2x for independent
  agents** (Kim et al., arXiv 2512.08296). A workflow is centralized coordination: every result
  returns through the orchestrator, and no agent hands work to another agent.

There is a third, practical reason. A skill's own instructions are a legitimate authorization to call
the `Workflow` tool, so a skill that says "author a workflow" gets one **without the user having to ask
for it** — no `ultracode`, no request from the user.

## The five shape rules

**1. Everything a workflow spawns is READ-ONLY. Writes stay on the main thread.**

This is the fan-out law (§3), and it is the rule the evidence is most one-sided about. Nine search
angles failed to find a single published ablation where parallel *writers* beat a single-agent
baseline on a coding benchmark. Meanwhile the winning scaffolds — Live-SWE-agent (79.2% SWE-bench
Verified), Meta's Confucius (74.6%), OpenHands, SWE-agent — are all single-threaded loops whose
subagents only read. Anthropic's guidance for this very platform: *"Two subagents editing the same
file in parallel is a recipe for conflict."*

So the pattern is **workflow → you → workflow**, not one workflow that writes:

```
phase('Research')  → N read-only agents  → you decide and WRITE inline
phase('Verify')    → N read-only agents  → you fix inline
```

The narrow exception is the mapped-partition case in `the-fan-out-law.md` — disjoint file ownership,
hubs taken by you first, `isolation: 'worktree'`. If you cannot name the partition, it does not apply.

**2. Cap a stage at 3–4 agents.**

*"per-agent reasoning capacity becomes prohibitively thin beyond 3-4 agents, creating a hard resource
ceiling."* More agents is not more coverage past that point; it is thinner reasoning each. Widen by
running another stage, not by widening the stage.

**3. Don't fan out what the main thread already does well.**

*"coordination yields diminishing or negative returns (beta=-0.408, p<0.001) once single-agent
baselines exceed ~45%."* Fan-out is a **coverage** tool for work whose baseline is weak — sweeping an
unfamiliar codebase, hunting an unknown-shaped bug, searching literature. For a task the main thread
handles reliably, a workflow makes the result *worse*, not merely slower. A known file and a known
symbol is still an inline read (§3), and no amount of orchestration changes that.

**4. `pipeline()` by default; a barrier only for a real cross-item need.**

Removing synchronization barriers measured a **3.5–4.9x** recovery (FlashEvolve). A barrier is correct
only when stage N genuinely needs all of stage N-1 at once — dedup across the full set, an early exit
on zero results, a prompt that references "the other findings". "I need to flatten first" is not a
cross-item need; do it inside a stage.

**5. Keep return schemas loose enough to reason in.**

Schema-forcing **measurably degrades reasoning, and worse under stricter constraints** (EMNLP 2024).
This cuts against the instinct to over-specify. Give every schema a free-text field — `reasoning`,
`verdict`, `notFound` — beside the structured ones, and never force a shape so tight the agent has to
abandon its argument to fit it. Structure the parts you will *machine-read*; leave prose for the parts
you will *judge*.

## The skeleton

```js
export const meta = {
  name: 'review-diff',
  description: 'Review a diff across dimensions, then adversarially verify each finding',
  phases: [{ title: 'Review' }, { title: 'Verify' }],
}

const FINDINGS = { /* structured fields + a free-text `reasoning` */ }

phase('Review')
const results = await pipeline(
  DIMENSIONS.slice(0, 4),                                    // rule 2
  d => agent(d.brief, { label: `review:${d.key}`, phase: 'Review',
                        agentType: 'viby-toolkit:reviewer',   // read-only by design — rule 1
                        schema: FINDINGS }),
  r => parallel((r?.findings ?? []).map(f => () =>            // no barrier between stages — rule 4
        agent(refuteBrief(f), { phase: 'Verify',
                                agentType: 'viby-toolkit:skeptic' })))
)
return { results: results.filter(Boolean) }                   // you reconcile — rule 1
```

Notes that save a re-run:

- `agent()` returns `null` when an agent is skipped or dies. **Always `.filter(Boolean)`** before use.
- `parallel()` never rejects; a thrown thunk becomes `null` in the array.
- Use the toolkit's own read-only agents by `agentType` — `viby-toolkit:scout`, `researcher`,
  `reviewer`, `skeptic`, `debugger`. They are read-only by construction, which is rule 1 enforced by
  the tool rather than by the prompt.
- No `Date.now()` / `Math.random()` in a script; they break resume. Vary by index instead.
- Concurrency is capped around 10 regardless of how many items you pass, so passing 40 items is safe —
  it is the *stage width per decision* that rule 2 constrains, not the queue length.
- The script is persisted to disk and the run is resumable — edit the file and re-invoke with
  `{scriptPath, resumeFromRunId}` and unchanged agents replay from cache instead of re-running.

## When NOT to author a workflow

Say so and work inline. A workflow has real setup cost and the 45% rule is unforgiving.

- A one-line change, a known file, a single known symbol.
- A strictly sequential chain — multi-agent variants degraded sequential-reasoning tasks by **39–70%**.
- Anything where you would spawn one agent. One agent is not a workflow; just read the file.
- Fewer than ~3 genuinely independent lines of inquiry. Two inline reads beat a two-agent workflow.

## Best-of-N: only behind a real verifier

Sampling K attempts and picking the best raises *oracle* coverage steeply (15.9% → 56% at K=250 on
SWE-bench Lite). But real selection is the bottleneck: at K=16 on SWE-bench Verified the oracle sits
11–25 points above what the best real verifier achieves, and with a weak verifier the compute-optimal
K collapses to **≤5, sometimes 0** — extra samples then *lower* accuracy by feeding false positives in.

So: best-of-N is worth it **only where an executed check decides the winner** — a test that runs, a
type error, a reproduced crash. Never where the selector is another model's opinion. This is the
justification for review's "execute, don't argue" rule, and the reason not to generalise it.
