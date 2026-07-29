---
name: handoff
description: >
  Use when a task is mid-flight and the session is about to end, hit context limits, or be
  cleared — to serialize live state so a fresh session resumes without re-deriving it. Use
  when the user says "hand off", "save state", "continue later", "I'll pick this up
  tomorrow", "I'm out of context", "write down where we are", or context is running high
  mid-task. Ephemeral task state, not durable lessons.
---

# Handoff (survive a context break mid-task)

```
IRON LAW: Capture enough for a cold-start resume — goal, decisions, state, next step —
          and NOTHING that belongs in durable memory or the plan file.
```

Handoff preserves **ephemeral, task-specific live state** so an interrupted task can resume
in a fresh context. It's distinct from its neighbors — keep them from overlapping:

- `handoff` = *live state of THIS task* (in-flight, discarded once the task is done).
- `learn` = *durable, generalizable lessons* (persist across tasks and sessions).
- the `plan` file = the *ordered change-list + checkpoint* for a planned build.

If a plan file already exists for this work, prefer compacting status **into it** (that's
the orchestrate checkpoint pattern) rather than making a separate handoff. Use a standalone
handoff when there's no plan file, or for exploratory/debugging work.

## What to capture (the resume schema)

Write a compact note (to the plan file, a scratch `HANDOFF.md`, or wherever the user
wants) containing exactly:
- **End goal** — what "done" means for this task, in one or two sentences.
- **Current approach** — the strategy in flight, and any approach already ruled out (so
  the fresh session doesn't retry it).
- **Done so far** — what's completed and verified, with `file:line` anchors.
- **Current state / blockers** — what's in progress, what's failing, the last error.
- **Next step** — the single concrete next action to take on resume.
- **Minimal relevant snippets** — only the few lines a resumer must see; reference paths
  for the rest (don't paste files — that defeats the purpose).

This is the same shape a subagent uses to report back — compaction and delegation share
one schema.

## Rules

- **Compact errors** — distill a failure to its essential cause and the one relevant line,
  not the raw stack dump. Preserve reasoning capacity, don't refill the window.
- Keep it short enough to re-read cheaply; a handoff longer than the work it describes is
  a smell.
- On resume: read the handoff, verify the "done so far" claims still hold (code may have
  moved), then execute the next step. Don't trust the note blindly — it reflects a past
  state.
- Once the task is complete, the handoff is disposable — delete it, or fold any durable
  insight into `/viby-toolkit:learn`.
