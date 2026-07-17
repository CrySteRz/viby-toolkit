---
name: forge-principles
description: The operating contract for the forge toolkit — accuracy-first rules, token/rate-limit discipline, and the model-routing table every forge skill and agent follows. Read this to understand how forge decides when to fan out subagents, which model each subagent runs on, and how it keeps the main thread's context clean.
disable-model-invocation: true
---

# Forge Operating Principles

This is the shared contract. Every other forge skill (`orchestrate`, `review-cluster`,
`debug`, `migrate`, `plan`) and every forge agent (`scout`, `implementer`, `reviewer`,
`skeptic`, `debugger`) is built on these rules. It is a reference — read it, don't "run" it.

## 1. Accuracy is the primary objective. Everything else is a constraint.

Token savings, speed, and elegance are secondary. If a cheaper path risks a wrong
answer, take the expensive path. The failure mode we optimize against is **confident
wrong output** — a hallucinated bug, a "fixed" claim that isn't verified, a refactor
that changes behavior. Never trade correctness for cost.

Concrete rules:
- **Never claim done without evidence.** "Tests pass" requires showing the run. "Bug
  fixed" requires reproducing the bug first, then showing it gone. If you didn't verify
  it, say so explicitly.
- **Ground every finding in a file:line.** A claim you can't anchor to real code is a
  hypothesis, not a finding — label it as such.
- **Prefer reading the actual code over recalling how it "usually" works.** Especially
  for anything naming Claude/Anthropic APIs, library versions, or config schemas.

## 2. Context hygiene is how forge saves tokens without losing accuracy.

The expensive resource on a Max subscription is the **main thread's context window** and
your **rate-limit budget**. The strategy is not "use a dumber model" — it's "keep the
smart main thread's context clean, and push bulk reading into disposable subagents."

- **Subagents are context firewalls.** A subagent that greps 40 files and reads 10 of
  them returns a 200-token conclusion; the 30k tokens of file dumps die with the
  subagent and never touch main context. This is the single biggest lever. Fan out for
  *breadth* (searching many files, checking many call sites) and keep only the
  conclusion.
- **Don't fan out for things you already know.** If you know the file and the symbol,
  read it directly. Spawning an agent to fetch one known fact costs *more* than doing it
  inline (agent spin-up + its own context). Fan-out pays off when the search space is
  wide or unknown.
- **One finding, one home.** Don't restate a subagent's full output in main context —
  summarize the load-bearing part and move on.

## 3. Model-routing table (the token/rate-limit lever)

Route each unit of work to the cheapest model that can do it *correctly*. Subagents
declare their model in frontmatter; the orchestrator picks per-task.

| Work type | Model | Why |
|---|---|---|
| Mechanical search, file-location, "does X exist", grep-and-report | **haiku** | Pattern-matching, no deep judgment. Fast, cheap, high rate-limit headroom. |
| Reading a subsystem and summarizing, single-file implementation to a clear spec, one review dimension, adversarial refutation of one claim | **sonnet** | Solid reasoning at a fraction of the cost. The workhorse for parallel fan-out. |
| Planning/architecture, cross-cutting judgment, final synthesis, ambiguous root-cause, resolving conflicting subagent reports | **opus / inherit (main thread)** | Hard judgment where a wrong call is expensive. Never delegate the final decision to a cheap model. |

Rules of thumb:
- **The main thread stays on the strong model and makes the decisions.** Cheap models
  gather and propose; the strong model judges. This preserves accuracy while most tokens
  are spent cheaply.
- **A single strong reviewer beats five cheap ones for final judgment**, but five cheap
  scouts beat one strong one for *coverage*. Use cheap+parallel to find, strong+single
  to decide.
- When unsure whether a task needs the strong model, ask: "if this subagent is subtly
  wrong, does the whole result break?" If yes → strong model. If it's one voice among
  many that get cross-checked → cheap model is fine.

## 4. When NOT to spawn agents

- Trivial or single-step tasks — just do it inline.
- Anything needing tight back-and-forth with the current context — subagents can't see
  your conversation; re-establishing context costs more than it saves.
- When you'd spawn an agent and then immediately need to re-read everything it read
  anyway.

## 5. Adversarial verification kills false positives

Any finding that will be shown to the user as a "bug" or "problem" must survive a
skeptic pass whose explicit job is to *refute* it. Default the skeptic to "not a real
issue unless proven." A finding survives only if the skeptic cannot refute it with a
concrete counter-argument grounded in the code. This is how forge avoids crying wolf.
Details in the `review-cluster` skill.

## 6. Portability & secrets

This toolkit syncs across work and personal machines via a private Git marketplace.
- **Never hardcode secrets, tokens, internal hostnames, or client names** into any
  skill/agent/hook. The repo is transferable; treat it as if it could leak.
- Behavior must be **project-agnostic** — detect the stack at runtime, don't assume it.
- Per-project overrides live in that project's `.claude/`, never here.
