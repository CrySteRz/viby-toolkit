---
name: plan
description: >
  Use when the HOW of a task isn't obvious, is cross-cutting or high-stakes, or the user
  wants to approve direction before any code is written. Use when the user says "plan
  this", "how would you approach", "what's the plan", or asks for a design/approach without
  asking you to build yet. (For deciding WHAT to build at all, use brainstorm first.)
---

# Plan

```
IRON LAW: Write the plan for an engineer with ZERO context and questionable taste.
          No unstated assumptions, no vague steps ("handle errors appropriately").
```

A good plan is the cheapest place to fix a mistake — a bad plan costs hundreds of bad
lines, a bad research phase thousands, so this is where scrutiny pays off most. This skill
turns a fuzzy task into an ordered, file-anchored change-list you (or the user) can
sanity-check before any code is written. Follow `/viby-code:principles`.

## 1. Understand the real request

Separate what's asked from what's needed. State the goal, the constraints, and what's out
of scope. Surface ambiguities that would change the design — resolve the load-bearing
ones with the user now; make a documented assumption for the minor ones.

## 2. Ground it in the actual code (cheap scouts, parallel)

Don't plan against an imagined codebase. Fan out `scout` agents to map the affected
areas: where the relevant logic lives, the existing patterns and conventions to follow,
the integration points, and the constraints (types, contracts, tests that will need
updating). Keep their conclusions, not their file dumps.

## 3. Weigh approaches (only when it matters)

If there's a real fork in the road, lay out the 2–3 viable approaches with their
trade-offs and **recommend one** — don't just enumerate. For a straightforward task, skip
this; a single obvious approach doesn't need a beauty pageant. When the solution space is
genuinely wide and the choice is expensive, consider a small judge-panel: sketch each
approach, evaluate against the constraints, pick the winner and graft the best ideas from
the runners-up.

## 4. Write the plan

An ordered list of steps, each as: `file:location → what changes → why`. Include:
- The **sequence** (what must happen before what, and why).
- The **risky step** — the one most likely to go wrong — and how you'll de-risk/verify it.
- The **verification strategy**: what tests/flows prove each part works (this feeds the
  Verify phase of `/viby-code:orchestrate`).
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
invoked from `/viby-code:orchestrate`, return the plan to the orchestrator to execute.
