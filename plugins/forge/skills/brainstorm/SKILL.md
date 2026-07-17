---
name: brainstorm
description: >
  Use BEFORE building anything non-trivial, when what to build isn't fully pinned down, or
  when the user describes a goal/idea rather than a precise spec. Use when the user says
  "I want to build", "how should we approach", "I'm thinking about", "help me design",
  "should we". The design-before-code gate — decide WHAT and confirm it before any HOW.
---

# Brainstorm (design-before-code gate)

```
IRON LAW: Do NOT write code, scaffold, create files, or invoke any implementation skill
          until you have presented a design and the user has explicitly approved it.
          No exception for "this is simple" — simple tasks are where unexamined
          assumptions cost the most.
```

This is the gate `plan` and `orchestrate` sit behind for anything whose *shape* isn't
already agreed. `plan` decides HOW to build a known thing; `brainstorm` decides WHAT to
build and WHETHER this is the right thing. Follow `/forge:forge-principles`.

Why the hard gate: the most expensive mistakes are solving the wrong problem and baking in
an unstated assumption. Both are nearly free to fix here and very expensive to fix in code.
"Too simple to need a design" is the exact rationalization that produces them — refuse it.

## Process

1. **Understand the real goal.** Restate what the user actually wants and *why* — the
   underlying need, not just the stated feature. Separate the goal from one particular
   solution to it.
2. **Ask clarifying questions ONE AT A TIME.** Don't dump a questionnaire. Ask the single
   most decision-relevant question, use the answer to inform the next. Stop when the
   remaining unknowns wouldn't change the design. Use the AskUserQuestion tool for crisp
   either/or choices.
3. **Explore alternatives.** Propose **2–3 genuinely different approaches** with honest
   trade-offs (complexity, risk, effort, reversibility), and **recommend one** with a
   reason. Don't present a single option as if it were the only way; don't present a fake
   menu where only one is viable.
4. **Surface assumptions and non-goals explicitly.** State what you're assuming and what
   is deliberately out of scope, so the user can correct a wrong assumption now.
5. **Present the design for approval.** In sections: goal, chosen approach + why, key
   decisions, assumptions, out-of-scope, and the rough shape (components/files/interfaces
   at a high level — not code). Then **stop and ask for approval.**
6. **Self-check the design before presenting** — any placeholders, contradictions, or
   ambiguities you'd be embarrassed to hand an engineer? Fix them first.

## Hand-off (the only allowed exits)

- On approval → hand to `/forge:plan` (turn the design into an ordered change-list) or
  `/forge:orchestrate` (which will plan then build). Never jump straight to writing code.
- If the user wants to skip design ("just build it") on something genuinely tiny, comply —
  but for anything non-trivial, briefly note the top assumption you're making and proceed,
  so there's still a checkpoint on record.

Keep the whole exchange tight. Brainstorm is thinking made visible, not a ceremony — a
few sharp questions and a clear recommendation beat a long document.
