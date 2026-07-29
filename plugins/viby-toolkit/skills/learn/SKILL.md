---
name: learn
description: >
  Use after solving something non-obvious, discovering a build/test quirk or gotcha, having
  a review finding dismissed as unwanted, or finding a task failed because context got
  compacted away. Use when the user says "remember this", "remember that our migrations must never
  run in a transaction", "remember that we must never do that", "don't flag that again", "note for
  next time", "keep that in mind for this project". Records a reusable PROJECT FACT to memory so it
  compounds — not a settings or configuration change.
---

# Learn (the compounding loop)

```
IRON LAW: Record the lesson while context is fresh, or it's lost. One lesson per invocation.
          Only durable, reusable facts — not conversation trivia.
```

The first time you solve a problem it takes research; documented, the next occurrence
takes minutes. This is the compound-engineering move, adapted to Claude Code's **native
memory** so it's portable across machines and needs no separate files or tooling. Follow
`/viby-toolkit:principles`.

## What's worth recording (and what isn't)

**Record** (durable, reusable, non-obvious):
- A build/test/run command that isn't guessable from the repo.
- A gotcha that cost real time (a non-obvious coupling, an env quirk, an ordering
  constraint, a footgun API).
- A convention this project insists on that a reviewer or implementer would otherwise
  violate.
- A **rejected review finding** — a class of issue the user explicitly does *not* want
  flagged ("that's intentional", "our style", "stop suggesting X"), so `review-cluster`
  stops re-surfacing it (suppress direction).
- A **known past failure** for a module — "this area had an N+1 regression", "auth
  middleware here is easy to bypass". Future reviews cite it to *raise recall* (prime
  direction). Compounding runs both ways: suppress false positives AND surface known risks.
- A **"never compact away X"** lesson — when a task failed specifically because context
  was compacted/cleared and lost something load-bearing (not because of bad reasoning),
  record what must be preserved: "when doing X, always keep Y in context." Over time viby-toolkit
  learns what it must never evict, per project.
- A solved-problem summary: the symptom, the root cause, the fix, in a few lines.

- A **rejected option and the bar it failed** — "we evaluated X and dropped it because it
  ships data to a third party" (`/viby-toolkit:evaluate`). This is the highest-value entry per
  line here: without it the same option gets re-proposed and re-investigated every few months,
  and the reasoning has to be rebuilt from nothing.

**Don't record** (guidance from the memory system): anything derivable from the code,
git history, or CLAUDE.md; anything that only mattered to this one conversation; secrets
or client-identifying details (this knowledge may sync across machines).

**Record the authored layer, not the derived one** (`/viby-toolkit:principles` §9). A *why* —
a decision, a constraint, a rejected option, a trap — stays true and is expensive to
re-derive. A *what* — this function lives here, this module calls that one — is regenerable
by a command in seconds and goes stale silently, which makes recording it actively harmful:
a confidently wrong memory outlives the code it described.

## How

Use the memory layout the harness actually defines — check your own memory instructions for
the exact schema before writing, since it is the authority and this is a summary:
1. Write the lesson as a single focused file whose frontmatter carries `name`,
   `description`, and the type nested under `metadata` (`metadata.type`, **not** a
   top-level `type:`) — `project` for work/goals/gotchas, `reference` for external
   pointers, `feedback` for a rejected-finding preference (include **Why** and
   **How to apply**), `user` for who the user is.
2. Add a one-line pointer to `MEMORY.md` (the index loaded each session): `- [Title](file.md) — hook`.
3. Before writing, check for an existing memory that already covers it — **update that
   file** instead of duplicating. Delete memories that turn out to be wrong.
4. Link related memories with `[[slug]]`.

Keep each entry to one fact, tightly written — memory loads into every future session, so
the same leanness rule applies: would removing this line cause a mistake? If not, cut it.

## When it pays back

Next session, the recalled memory means: no re-researching that gotcha, no re-flagging
that accepted pattern, no re-deriving that build command. The reviewer's taste drifts
toward the user's; the orchestrator starts pre-warmed. That's the compounding — each
solved problem lowers the cost of the next.

Note: recalled memories reflect what was true when written. If one names a file, flag, or
command, verify it still exists before relying on it.
