---
name: plan
description: >
  Produce a concrete, reviewable implementation plan before writing code for a task
  that's ambiguous, cross-cutting, or high-stakes. Explores the codebase with cheap scout
  agents, weighs approaches, and returns an ordered change-list with the risky step and
  verification strategy called out. Use when the "how" isn't obvious or the user wants to
  approve direction first. Invoke with /forge:plan.
---

# Plan

A good plan is the cheapest place to fix a mistake. This skill turns a fuzzy task into an
ordered, file-anchored change-list you (or the user) can sanity-check before any code is
written. Follow `/forge:forge-principles`.

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
  Verify phase of `/forge:orchestrate`).
- What you're explicitly **not** doing.

Keep it tight and concrete. A plan full of "handle errors appropriately" is not a plan —
say which errors, where.

## 5. Hand off

If invoked standalone, present the plan for approval (use plan mode when the harness
supports it) and stop — don't start implementing until the direction is confirmed. If
invoked from `/forge:orchestrate`, return the plan to the orchestrator to execute.
