---
name: plan
description: >
  Always load when the HOW is not obvious, is cross-cutting or high-stakes, or direction needs
  approval before code — "plan this", "how would you approach", "what's the plan", "break this
  into steps", "what order". Not /viby-toolkit:brainstorm.

---

# Plan

```
IRON LAW: Write the plan for an engineer with ZERO context and questionable taste.
          No unstated assumptions, no vague steps ("handle errors appropriately").
```

A good plan is the cheapest place to fix a mistake — a bad plan costs hundreds of bad
lines, a bad research phase thousands, so this is where scrutiny pays off most. This skill
turns a fuzzy task into an ordered, file-anchored change-list you (or the user) can
sanity-check before any code is written. Follow `/viby-toolkit:principles`.

## 1. Understand the real request

Separate what's asked from what's needed. State the goal, the constraints, and what's out
of scope. Surface ambiguities that would change the design — resolve the load-bearing
ones with the user now; make a documented assumption for the minor ones.

## 2. Ground it in the actual code (cheap scouts, parallel)

Don't plan against an imagined codebase. **Fan out `scout` agents in parallel, in a single
message** — 3–4 of them, read-only (`/viby-toolkit:principles` §3). Map the affected areas:
where the relevant logic lives, the existing patterns and conventions to follow,
the integration points, and the constraints (types, contracts, tests that will need
updating). Keep their conclusions, not their file dumps.

## 3. Weigh approaches (only when it matters)

If there's a real fork in the road, lay out the 2–3 viable approaches with their
trade-offs and **recommend one** — don't just enumerate. For a straightforward task, skip
this; a single obvious approach doesn't need a beauty pageant. When the solution space is
genuinely wide and the choice is expensive, consider a small judge-panel: sketch each
approach, evaluate against the constraints, pick the winner and graft the best ideas from
the runners-up.

**Record the rejections, each with the bar it failed** — "needs a migration we can't run
mid-quarter", "couples the two services we're trying to split". One line each. This is the
part of a plan that pays off months later, when the same option is proposed again and
nobody remembers why it was dropped; without it, the plan reads as if only one idea was
ever considered. If the choice is between existing *tools* rather than designs, that is
`/viby-toolkit:evaluate` — measure it, don't reason about it.

## 4. Write the plan as a dispatchable task list

Each task on one line, with what it **owns**, how it is **proved**, and what it **waits for**:

```markdown
- [ ] T1 — parse the CSV · files: src/parse.ts · verify: npm test -- parse · deps: —
- [ ] T2 — write the rows · files: src/write.ts · verify: npm test -- write · deps: —
- [ ] T3 — wire the route · files: src/route.ts · verify: npm test · deps: T1, T2
```

Then check it is actually dispatchable before anything is spawned:

```bash
VIBY_HOME=$(
  for d in "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/ "$HOME"/Projects/*/*/viby-toolkit/plugins/viby-toolkit/; do
    d=${d%/}
    [ -f "$d/hooks/run.sh" ] && [ -d "$d/skills" ] && { echo "$d"; break; }
  done
)

CP="$VIBY_HOME/skills/plan/scripts/check-plan.ts"
RUN="$VIBY_HOME/hooks/run.sh"
sh "$RUN" "$CP" docs/plans/<this-plan>.md
```

It fails on the things that turn a plan into a reconciliation problem: **two tasks with no
dependency between them owning the same file** (they can be dispatched in parallel and will
conflict), a dependency cycle, a file touched by three or more tasks (a structural hub — take it
yourself, sequentially), a task that does not name its files, and a task with no verification.
That is `/viby-toolkit:principles` §3's "name the partition" requirement, made mechanical.

The checkboxes are not decoration: this file is the durable progress ledger. Tick them as you go and
a cleared session resumes by re-reading it.

**Three things this format is deliberately not.** It is not a substitute for the prose below — a
task list with no reasoning is unreviewable. It is not a promise of parallelism; most plans should
run single-threaded (§3), and the checker only tells you *whether* you could. And it does not judge
whether the plan is any good — only whether it can be executed.

Alongside the task list, include:
- The **sequence** (what must happen before what, and why).
- The **risky step** — the one most likely to go wrong — and how you'll de-risk/verify it.
- The **verification strategy**: what tests/flows prove each part works (this feeds the
  Verify phase of `/viby-toolkit:orchestrate`).
- What you're explicitly **not** doing.

Keep it tight and concrete. A plan full of "handle errors appropriately" is not a plan —
say which errors, where. Aim for ~200 lines, readable in ~10 minutes.

For a substantial task, **save the plan to a file** (e.g. `docs/plans/<date>-<topic>.md`
or the project's convention). It then doubles as a durable checkpoint: as implementation
proceeds, status is compacted back into it, so if context is cleared, re-reading the plan
restores exactly where things stand.

## 5. Hand off

If invoked standalone, present the plan for approval (use plan mode when the harness
supports it) and stop — don't start implementing until the direction is confirmed. If
invoked from `/viby-toolkit:orchestrate`, return the plan to the orchestrator to execute.
