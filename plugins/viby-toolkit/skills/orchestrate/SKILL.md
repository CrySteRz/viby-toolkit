---
name: orchestrate
description: >
  Always load for any feature, fix or change worth doing carefully — more than a file or two,
  ambiguous, or multi-step. "build", "implement", "add", "ship", "create", or a well-specified
  ticket that just needs building. One-line change: skip it.

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

- **Author a `Workflow` for this phase**, because it has two stages and the second one is the
  part people skip. Stage 1 runs one `scout` per independent area (by feature, by layer, by the
  files a change touches), **3–4 of them, not more**. Stage 2 is the **escalation ladder**
  (`/viby-toolkit:principles` §4) made automatic: any area whose scout returns `escalate: true` or
  low confidence is re-run on a stronger model before anything is built on it. Hand-dispatched,
  that second pass depends on you noticing a confidence field in a wall of returned text; declared
  as a stage, it cannot be forgotten. A single sweep with no escalation pass is not a workflow —
  dispatch it directly (§3b).
- Each scout is read-only, cheap-model, and returns a tight summary: relevant `file:line`, key
  functions/types, conventions to follow, gotchas.
- Keep only their conclusions. Don't scout what you already know — read that inline.
- For a big audit, have scouts write full notes to a file and return only the headline + path
  (two-tier return) so main context stays lean.
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
  law — writes stay single-threaded. **The workflow default of §3b does not reach this phase**:
  a workflow orchestrates reading, and you write between its calls. Every scaffold at the top of
  SWE-bench Verified is a single-threaded loop whose subagents only read, and Anthropic's guidance
  for this platform is blunt — *"Two subagents editing the same file in parallel is a recipe for
  conflict."*
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

### Phase 5 — Self-review (fan out `review`, then filter)

Run `/viby-toolkit:review` on your own diff — a fresh-context reviewer is unbiased toward
code it just wrote. It fans out dimension reviewers and the adversarial `skeptic` filter
so only confirmed issues survive. Fix confirmed findings; re-verify anything you change.

### Phase 5b — Check the diff is reviewable, before asking anyone to review it

The build phase produces a diff, and a diff can be correct and still unlandable. Audit the
artifact, not just the behaviour:

```bash
HYG=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/orchestrate/scripts/check-diff-hygiene.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$HYG"                      # working tree + staged
sh "$RUN" "$HYG" --base main          # the whole branch
```

It flags a conflict marker or a credential being committed (P1), debug prints and formatting
churn mixed into a behavioural change (P2), new TODOs and commented-out code (P3) — and
**size**, which is the one people rationalise. The largest study of code review found detection
is best at 200–400 changed lines and falls to roughly **28% past 1,000**: a 2,000-line diff is
not a bigger review, it is an unreviewed one. If you cannot split it, say so in the description
and point the reviewer at the risky part.

**Split in the order a reviewer can follow**: mechanical and formatting-only changes in their own
commit, then structural moves, then anything touching behaviour. Never mix them — an unreviewable
diff is where inherited bugs hide.

### Phase 6 — Compound (when something was non-obvious)

If you learned something reusable (a non-obvious gotcha, a build quirk, a pattern this
repo insists on), invoke `/viby-toolkit:learn` to record it to project memory so the next session
gets it for free.

## Output

Lead with the outcome: what changed (`file:line`), the verification evidence, any
confirmed issues you deliberately left and why, follow-ups. No process narration unless
it's load-bearing.
