---
name: worktrees
description: >
  Use before running work that should be isolated from the main checkout — parallel
  implementer agents that would edit-conflict, a risky experiment, or a migration you want
  to keep off the working branch. Use when the user says "isolate this", "work in a
  worktree", "try this without touching main", or when orchestrate/migrate needs conflict-
  free parallel writes.
---

# Worktrees (isolation, done right)

```
IRON LAW: Detect existing isolation FIRST. Use the harness's native tool. Fall back to
          git only if there is none. Never fight the harness.
```

Isolation lets independent work happen without colliding — the enabling primitive for the
rare case where the fan-out law permits parallel writes (genuinely independent parts, no
shared state). Follow `/viby-toolkit:principles`.

## Order of operations

1. **Detect first.** Are you *already* in an isolated workspace (a worktree the harness or
   the user set up)? If so, just work here — do not create another. Check before acting;
   nested/duplicate worktrees are a mess to unwind.
   - Guard against false positives: being inside a git *submodule* can look like a separate
     worktree. Confirm it's actually a distinct working tree, not a submodule, before
     concluding you're isolated.
2. **Prefer the native tool.** This harness exposes worktree tools (e.g. EnterWorktree /
   ExitWorktree and related). Use them — they integrate with the session, clean up
   automatically, and won't confuse the harness's view of your working directory. Agents
   can also take `isolation: worktree` in frontmatter to run in a fresh worktree.
3. **Fall back to `git worktree` only if no native option exists.** Before creating a
   project-local worktree dir, `git check-ignore` the path (or place it outside the repo)
   so you don't add the worktree to the very repo you're isolating.

## Use it for

- **Parallel `implementer` agents** that would otherwise edit the same files — give each
  its own worktree so their writes can't conflict; you reconcile the diffs after.
- A **migration** batch you want to keep reviewable and revertible in isolation.
- A **risky experiment** you may throw away.

## Where the generated stuff lives — the question worktrees always raise

Every worktree starts with a clean tree, so nothing gitignored is inherited: no
`node_modules`, no venv, no build cache, no code index, no local DB. That is the whole point,
and it is also the cost — each new worktree either rebuilds them (minutes, × N) or shares
them (races). Decide deliberately, per artifact, before spawning parallel work:

| Option | Verdict | Trade-off |
|---|---|---|
| **One canonical copy, pinned to mainline, read-only from every worktree** | ✅ default for *read-only* artifacts (a code index, a symbol map, a package cache) | One build ever, zero per-worktree setup. Weak on symbols the branch just added — but those have no existing callers anyway, so a grep over the diff is the complete answer there. |
| **Per-worktree copy** | ⚠️ only for a long-lived branch | Exact on the branch's own new code; pays a cold build per throwaway worktree and N× disk. |
| **One copy symlinked into every worktree** | ❌ avoid | Concurrent writers race (last-writer-wins) and leave stale locks; and it only works at all if the artifact stores *relative* paths, which is usually undocumented. |

Two rules that follow:

- **Read-only sharing is fine; shared writes are the same trap as parallel writes.** If two
  worktrees would both *rebuild* the shared thing, they are parallel writers to one file — the
  fan-out law applies to caches too.
- **Name the mechanical assumption you have not verified, and check it before relying on it**
  — "this only works if the index stores relative paths; unconfirmed". An unstated assumption
  of this kind fails silently and looks like a tool bug for an hour. The default option above
  is the default precisely because it never has to be answered.

These artifacts are derived, so the rules in `/viby-toolkit:principles` §9 apply: regenerable
by command, never hand-patched, never committed, never cited as ground truth.

## Don't use it for

- Ordinary single-threaded work on the current branch — isolation you don't need is just
  overhead and cleanup risk.
- Anything where the "parallel" parts actually share state — that's the fan-out trap;
  isolation doesn't fix conflicting *decisions*, only conflicting *files*.

Always clean up: exit/remove the worktree when done (native tools auto-clean if unchanged).
