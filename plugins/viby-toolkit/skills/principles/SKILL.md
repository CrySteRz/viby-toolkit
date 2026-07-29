---
name: principles
description: >
  The operating contract every viby-toolkit skill and agent follows — accuracy rules, the
  fan-out law, model routing and escalation, context discipline, evidence gating. Load it when
  another viby-toolkit skill says "follow /viby-toolkit:principles", or before deciding whether
  to fan out subagents. Reference material — read it, don't run it.
---

# viby-toolkit Operating Principles

The shared contract behind every skill and agent in this toolkit. **This file is the laws; the
`references/` files are the evidence and the operational detail.** It is deliberately short because
nearly every other skill loads it — read a reference only when you are acting on that section.

| § | Law | Depth |
|---|---|---|
| 1 | Accuracy over everything | — |
| 2 | Curate context deliberately | `references/context-discipline.md` |
| 3 | Fan out for READ; writes single-threaded | `references/the-fan-out-law.md` |
| 4 | Route to the cheapest model that is *correct* | `references/model-routing.md` |
| 5–6 | No claim without fresh evidence; verify adversarially | `references/evidence-gate.md` |
| 7 | Record lessons so they compound | — |
| 8 | Libraries degrade by overlap and listing budget | `references/skill-library-design.md` |
| 9 | Know authored from derived | `references/authored-vs-derived.md` |
| 10 | Portability and secrets | — |

## 1. Accuracy is the objective. Everything else is a constraint.

Token savings, speed and elegance are secondary. The failure mode to optimize against is **confident
wrong output** — a hallucinated bug, a "fixed" claim that isn't verified, a refactor that changed
behaviour. Never trade correctness for cost.

- **Never claim done without fresh evidence** (§5).
- **Ground every finding in `file:line`.** A claim you can't anchor is a hypothesis — label it one.
- **Read the actual code over recalling how it "usually" works** — especially Claude/Anthropic APIs,
  library versions, config schemas.
- **A fast, cheap, WRONG answer is worse than the slow one it replaced**, because it gets believed and
  kept while the slow method got checked. Rank any tool or shortcut on a case whose correct answer you
  established *first*, and put the correctness verdict in the same table as the cost
  (`/viby-toolkit:evaluate`).

## 2. Context is the master resource. Curate it deliberately.

The context window's contents are the only lever on output quality. **Correctness > Completeness >
Size** — wrong context is worst, missing context second, excess tokens least damaging, so don't
over-trim and drop something load-bearing.

- Target **40–60% utilization**; compact early, at task boundaries, on purpose.
- **Subagents are context firewalls** — the single biggest lever.
- **Cost is payload × cadence**, so measure the flow you will actually run, not one call.
- Near the ceiling the dominant failure is **refusal, not fabrication** — if an agent starts declining
  work it managed earlier, suspect context pressure first.
- `/clear` between unrelated tasks, and after two failed corrections on the same issue.

## 3. The fan-out law — the rule that decides every delegation

> **Fan out for READ. Keep WRITES single-threaded — unless you have mapped the dependency graph
> and can name the partition.**

- **WIN:** parallel read-only agents — search, explore, analyze, review. All of this toolkit's agents
  (`scout`, `reviewer`, `skeptic`, `debugger`, `researcher`) are read-only by design.
- **TRAP:** parallel writers make conflicting decisions from partial context, and you pay to
  reconcile. Use extra agents for *intelligence, not actions.*
- **The trap is naive partitioning, not parallel writing as such.** Parallel writes do work when the
  cut is right: one agreed architecture, each task owning files exclusively, ordered into dependency
  waves — and you take the hub files yourself, sequentially. `skills/plan/scripts/check-plan.ts`
  fails a plan whose parallel tasks share a file, so this is checkable rather than aspirational.
- **Don't fan out for what you already know.** Known file, known symbol → read it inline.
- **The failure is at the seams**, not inside the agents: ~36.9% of multi-agent failures are
  inter-agent misalignment. Spend your care on the brief and the return format.

## 4. Model routing and delegation

Route each unit of work to the cheapest model that can do it *correctly*: **haiku** for mechanical
search and file-location, **sonnet** for read-only fan-out, **opus/inherit** for planning, synthesis,
judgement and all writes to shared code, **fable** for the hardest calls — a reproduction test,
conflicting verdicts, a subtle security or concurrency judgement. Cheap models find; the strong main
thread decides. **Escalate on doubt, but never trust a self-reported confidence number** — prefer an
executed check.

## 5. Evidence-gated completion (the anti-"looks done" rule)

> **NO COMPLETION CLAIM WITHOUT FRESH VERIFICATION EVIDENCE.**

1. **IDENTIFY** the command that proves it. 2. **RUN** it fresh. 3. **READ** the full output and exit
code. 4. **VERIFY** it actually passed. 5. **CLAIM** only then, with the evidence attached.

A zero exit code is not automatically a pass — zero tests collected, an all-skip run, a `|| true`, or
a cached result all exit 0. **Label every claim measured, inferred, or not tested** at the point it
appears. If the diff *is* a guard, verify it fails when it should. `/viby-toolkit:verify` runs this
gate as a procedure.

## 6. Adversarial verification kills false positives

A finding reaches the user only if it **quotes the exact line** (verified to exist) and a **single
fresh-context validator** — not a same-model majority vote — confirms it is real, introduced by this
change, and not already handled, with a reject-on-doubt bias. Reviewers flag **correctness only**
(taste → `/simplify`); validators never see the author's reasoning. Full protocol in `review-cluster`.

## 7. Compounding — each solved problem makes the next cheaper

When you solve something non-obvious, or the user rejects a review finding as unwanted, **record the
lesson to memory** (`/viby-toolkit:learn`) so future sessions don't re-research or re-flag it.
Compounding runs both ways: suppress known false positives *and* surface known past risks.

## 8. Skill libraries degrade by OVERLAP and by listing budget

Adding a skill is not free, but the cost is not where intuition puts it. Two distinct mechanisms:

- **Shadowing.** A description that semantically overlaps another's hides it from selection. Measured:
  right-skill invocation fell 88% → 53% as a library grew. The extra *context* cost was
  indistinguishable from zero. So the rule is distinguishable descriptions, not few skills.
- **The listing budget.** Claude Code fits all descriptions into ~1% of the context window and, on
  overflow, truncates them **starting with the skills you invoke least**. So a longer, more explicit
  description makes mis-routing *worse*.

Both point one way: **a description is a trigger, not a summary** — and when two skills look alike,
add distinguishing words as *triggers*, never as summaries. Checked by
`skills/principles/scripts/check-skills.ts` in the pre-push gate.

## 9. Authored vs derived — know which of your artifacts is which

**Authored** is the *why* — decisions, lessons, contracts, plans: reviewed, durable, the source of
truth. **Derived** is the *what* — maps, indexes, scan output, caches: rebuilt by a command,
disposable, never a source of truth. Keep them layered, connected by stable reference, never by
copying — a copy is what goes silently stale.

- **Stamp every derived artifact with its provenance** and the command that rebuilds it.
- When derived disagrees with its source, **the source wins** — regenerate, don't hand-patch.
- **A heuristic derived artifact is a planning aid, not evidence** (§5).

## 10. Portability & secrets

This toolkit syncs across work and personal machines via a private Git marketplace.

- **Never hardcode secrets, tokens, internal hostnames, or client names** anywhere here.
- Behaviour must be **project-agnostic** — detect the stack at runtime.
- Per-project overrides live in that project's `.claude/`, never in this repo.
- Keep any always-loaded text lean: for each line ask *"would removing this cause a mistake?"* If not,
  cut it — a bloated always-on context makes Claude ignore the instructions that matter.
