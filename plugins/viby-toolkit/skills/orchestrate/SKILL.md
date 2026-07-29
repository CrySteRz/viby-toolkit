---
name: orchestrate
description: >
  Use for any feature, fix, or change worth doing carefully — anything touching more than
  a file or two, ambiguous, or multi-step. Use when the user says "build", "implement",
  "add", "ship", "create", "refactor" something substantial, or hands you a well-specified
  ticket whose scope is already clear and just needs building. For a one-line change, skip
  this and just do it.
---

# Orchestrate

You own the plan and every real decision; you delegate **read-only breadth** to
subagents and keep **writes on your own single thread**. Follow `/viby-toolkit:principles`.

```
IRON LAW: Fan out to READ, freely.
          Never fan out to WRITE across a dependency boundary you have not mapped.
          Review the research and the plan harder than the diff — upstream errors compound.
```

Errors compound upstream: a bad research doc yields thousands of bad lines, a bad plan
hundreds, bad code just bad code. So the judgment goes in early and the human/strong-model
attention goes to the research and plan, not the diff.

## The pipeline

Run in order. **Scale to the task** — if you could describe the diff in one sentence, skip
straight to implementing. Don't ceremonially run six phases for a one-line change.

### Phase 0 — Scope (inline)

Restate the task: what "done" means, what's out of scope, the top risk. If ambiguity
would change the implementation, ask **one sharp question** before building. Otherwise
proceed.

If the **what** itself is unsettled (a fuzzy idea, not a spec), run `/viby-toolkit:brainstorm`
first. When you already have a clear, well-specified ticket, the what is decided — skip
brainstorm and go straight through research → plan → build.

### Phase 1 — Research (fan out read-only `scout` agents)

Build an accurate map without dragging file dumps into your context. **If the codebase (or
this corner of it) is unfamiliar, run `/viby-toolkit:explore` instead of improvising** — it
detects the stack mechanically and writes a durable map, which becomes this phase's output.

- One `scout` per independent area (by feature, by layer, by the files a change touches),
  **in parallel in a single message**. Each is read-only, cheap-model, returns a tight
  summary: relevant `file:line`, key functions/types, conventions to follow, gotchas.
- Keep only their conclusions. Don't scout what you already know — read that inline.
- Honor the **escalation ladder** (`/viby-toolkit:principles`): if a scout returns
  `escalate: true` or low confidence, re-run that area on a stronger model rather than
  building on a shaky map. For a big audit, have scouts write full notes to a file and
  return only the headline + path (two-tier return) so main context stays lean.
- For a substantial task, distill the findings into a short **research note** (which files
  are relevant, how data flows, candidate approaches). This is a durable artifact that
  survives compaction.

### Phase 2 — Plan (inline, strong model — judgment lives here)

Write a concrete plan and, for anything substantial, **save it to a file** (e.g.
`docs/plans/<date>-<topic>.md` or the project's convention). A good plan:
- Header: Goal / Constraints / Approach.
- An ordered list of steps, each `file:location → what changes → why`, sized bite-small
  (each step ~a few minutes of work carrying its own verification).
- The **risky step** called out with how you'll de-risk it.
- The **verification strategy**: the *specific* tests/flows that prove each part.
- What you're explicitly **not** doing.
- Aim ~200 lines, readable in ~10 minutes. Write it as if for an engineer with zero
  context and questionable taste — no unstated assumptions.

Present the plan for approval (plan mode) when the user wants to steer direction or the
change is risky. Use `/viby-toolkit:plan` if the planning itself is hard enough to warrant a
dedicated pass. **This plan file doubles as your checkpoint** (see Phase 3).

### Phase 3 — Implement (single-threaded by default)

- **Default: implement inline, one thread.** You have the plan and context; a subagent
  would re-learn it, and parallel writers make conflicting decisions. This is the fan-out
  law — writes stay single-threaded.
- **Exception, with real criteria:** parallel `implementer` agents (with
  `isolation: worktree` so they cannot conflict on disk) pay off when you can **name the
  partition and the hub files** — not merely when the parts *feel* separate. Concretely:
  map what depends on what, keep the structural hubs (the files many others import) on your
  own thread and do them first, then hand each independent community of files to one agent,
  and reconcile the diffs yourself. Dependency-aware partitioning of exactly this shape
  measured better than sequential on real repositories, with the largest gains where
  dependencies were densest (`/viby-toolkit:principles` §3). If you cannot draw that map, the
  parts are not independent — do them in sequence.
- Work phase-by-phase through the plan. **After each verified phase, compact its status
  back into the plan file** (done / current approach / blockers). The plan is now your
  durable state — if context is cleared, re-reading it restores where you are. This keeps
  utilization in the 40–60% band on long tasks.
- Match surrounding style, naming, idioms. Comments only for constraints the code can't
  show — never to narrate the diff.
- **Write the tests as part of the change, not after it.** Follow `/viby-toolkit:test`: pick the
  level deliberately, and see each new test **fail for the right reason** before making it
  pass. A test written only after the code is green is a test nobody has watched work.

### Phase 4 — Verify (mandatory for anything with runtime behavior)

Run `/viby-toolkit:verify` — it applies the evidence gate end to end: find the project's real
checks (CI config is authoritative), run the **specific tests relevant to the change** by
name, drive the actual flow for behavior changes (invoke the CLI, hit the endpoint, render
the component), then read the output for silent-pass modes before believing a zero exit
code. If it fails, fix the code — never the check — and re-run the same command. Never
surface a failing change as done.

### Phase 5 — Self-review (fan out `review-cluster`, then filter)

Run `/viby-toolkit:review-cluster` on your own diff — a fresh-context reviewer is unbiased toward
code it just wrote. It fans out dimension reviewers and the adversarial `skeptic` filter
so only confirmed issues survive. Fix confirmed findings; re-verify anything you change.

### Phase 6 — Compound (when something was non-obvious)

If you learned something reusable (a non-obvious gotcha, a build quirk, a pattern this
repo insists on), invoke `/viby-toolkit:learn` to record it to project memory so the next session
gets it for free.

## Output

Lead with the outcome: what changed (`file:line`), the verification evidence, any
confirmed issues you deliberately left and why, follow-ups. No process narration unless
it's load-bearing.
