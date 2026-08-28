---
name: migrate
description: >
  Always load for a change that is mechanical but wide — a rename, an API upgrade, a framework
  bump, a pattern sweep across many files. "rename X everywhere", "migrate", "replace all",
  "codemod". Not /viby-toolkit:deps or /viby-toolkit:schema.

---

# Migrate (wide, mechanical, verified)

For restructuring one area rather than sweeping many files, use `/viby-toolkit:refactor`; for a
change whose point is a measured speedup, `/viby-toolkit:perf`.
For a database schema or data change, use `/viby-toolkit:schema` — data mistakes are not
recoverable by editing code.

```
IRON LAW: No batch is done until it is verified. No migration is done until a fresh
          discovery sweep finds ZERO remaining old-pattern sites.
```

Big sweeps fail in two ways: **missed sites** (incomplete migration that half-breaks) and
**silent behavior drift** (a "mechanical" change that wasn't). This pipeline defends
against both. Follow `/viby-toolkit:principles`. Discovery and per-site checking are
read-only — fan those out. The transforms are writes — keep them single-threaded per
batch (or in isolated worktrees for genuinely independent batches), never parallel writers
on shared code.

## 1. Discover the full work-list (before transforming anything)

Enumerate **every** site that must change. Be exhaustive — a migration that covers 90% of
sites is often worse than none. **Dispatch cheap `scout` agents in parallel, in a single message**,
each searching a different way so nothing hides:
- by **symbol/identifier** (the thing being renamed/replaced),
- by **import/usage** of the old API,
- by **string/pattern** (config, templates, docs, generated code),
- by **file type/location** the first passes might miss.

Merge into one deduplicated, categorized work-list: sites that are pure-mechanical vs.
sites that need judgment (ambiguous, edge-shaped, or where the old and new APIs aren't
1:1). **Log the count and the categories.** If you cap or sample anything, say so out
loud — silent truncation reads as "done" when it isn't.

## 2. Define the transform + the invariant

Write the exact transformation rule (old → new) and, critically, the **invariant that
must hold**: "behavior is identical except X." State how you'll check it. If the change
is *not* purely behavior-preserving (an upgrade with semantic differences), flag which
sites need human decisions and handle those separately — don't auto-apply a rule that
doesn't hold.

## 3. Transform in batches

- Group sites into coherent batches (by module/package). Process each batch, applying the
  transform.
- For genuinely independent batches that would edit-conflict if parallelized, dispatch
  `implementer` agents with `isolation: worktree` — each works on an isolated copy, you
  reconcile the diffs. For a sequential sweep, just do it directly.
- Keep each batch's diff reviewable. Resist bundling unrelated cleanups into the
  migration — a migration diff should be boring and uniform, which is what makes it
  reviewable.

## 4. Verify every batch (the gate)

No batch is done until it passes (`/viby-toolkit:verify` finds the real commands and screens
the output):
- Build/typecheck/lint on the touched area.
- The relevant tests — and for behavior-preserving migrations, ideally the *same* tests
  pass before and after.
- A spot-check of a few transformed sites by eye to confirm the invariant held.

If a batch fails, fix within the batch before moving on. Don't let failures accumulate
across batches — you'll lose track of which change broke what.

## 5. Final sweep

Re-run discovery (step 1) to confirm **zero** remaining old-pattern sites. This catches
anything created mid-migration or missed. Then run the full test suite once. Report:
total sites, batches, what was migrated, verification evidence, and any sites
deliberately left (with why).

## Token discipline

Discovery and per-site checking are cheap-model parallel work — that's where the leverage
is. You (strong model) own the transform rule, the invariant, batch boundaries, and the
final go/no-go. Don't read every file into main context; let scouts report and keep the
work-list.
