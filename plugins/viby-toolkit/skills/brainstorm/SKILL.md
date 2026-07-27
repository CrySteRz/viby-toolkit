---
name: brainstorm
description: >
  Use when WHAT to build isn't settled — a fuzzy idea, a vague goal, or several possible
  directions. Use when the user says "I want to build", "I'm thinking about", "should we",
  "help me figure out what". Do NOT use when there's already a clear, well-specified
  ticket/spec with unambiguous scope — go straight to /viby-toolkit:plan (the how) or
  /viby-toolkit:orchestrate. This gate is about deciding WHAT, not planning HOW, and not which
  existing tool to adopt (that is /viby-toolkit:evaluate).
---

# Brainstorm (design-before-code gate)

```
IRON LAW (once you're here): Do NOT write code, scaffold, create files, or invoke an
          implementation skill until you have presented a design and the user has approved
          it. If the WHAT is genuinely unsettled, the size of the task is no excuse to skip
          the design — unexamined assumptions cost the most on "simple" things.
```

## When to SKIP brainstorm

This gate is about deciding **WHAT** to build. If the what is already settled, skip it:

- **You have a clear, well-specified ticket or spec** — unambiguous scope, known
  acceptance criteria, no real fork in the road. → Go straight to `/viby-toolkit:plan` (design the
  HOW) or `/viby-toolkit:orchestrate` (which plans then builds). The person who wrote the ticket
  already did the brainstorm.
- **The change is obvious and you could describe the diff in a sentence.** → Just do it.

Reach for brainstorm only when the *what* is fuzzy: a vague goal, an idea rather than a
spec, several viable directions, or a ticket whose intent is unclear enough that building
the literal text would risk solving the wrong problem. When in doubt on a ticket, a single
clarifying question beats a full brainstorm.

`plan` decides HOW to build a known thing; `brainstorm` decides WHAT to build and whether
it's the right thing. Follow `/viby-toolkit:principles`. The mistakes this prevents —
solving the wrong problem, baking in an unstated assumption — are nearly free to fix here
and expensive to fix in code, which is why it exists; but a decided ticket has already
paid that cost.

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

- On approval → hand to `/viby-toolkit:plan` (turn the design into an ordered change-list) or
  `/viby-toolkit:orchestrate` (which will plan then build). Never jump straight to writing code.
- If the user wants to skip design ("just build it"), that is their call and it overrides this
  skill — but keep the two cases distinct, because the Iron Law above still governs the second:
  - **genuinely tiny** → comply, no ceremony.
  - **non-trivial** → state the single decision you are guessing at and the assumption you are
    making, in one or two sentences, and ask them to confirm just that one point. This is not
    the full design pass; it is the minimum checkpoint the Iron Law exists to protect. If they
    say go, go — and record the assumption in the output so it is on file.

Keep the whole exchange tight. Brainstorm is thinking made visible, not a ceremony — a
few sharp questions and a clear recommendation beat a long document.
