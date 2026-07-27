---
name: schema
description: >
  Use for any database schema or data change — "add a column", "write a migration", "change
  this table", "rename this field", "backfill", "add an index", "drop the old column", "change
  the column type", or reviewing a migration someone else wrote. Distinct from
  /viby-toolkit:migrate, which sweeps code, not data.
---

# Schema (the one change you cannot undo)

```
IRON LAW: Every schema change must be deployable while the OLD code is still running,
          and reversible without losing data. If it isn't both, it is at least two changes.
          Never drop or rewrite before you can PROVE nothing reads it.
          A lock on a hot table is an outage in progress.
```

Follow `/viby-toolkit:principles`. This skill has its own Iron Law because schema work breaks
the usual assumption behind everything else here: code mistakes are recoverable by editing
code, and data mistakes are not. A dropped column takes its data with it. A rewrite that
holds a lock takes the service down while it runs. Neither is fixed by a follow-up commit.

## 1. Lint it mechanically first

```bash
CHECK=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/schema/scripts/check-migration.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$CHECK" --all
```

It finds the short list of operations that cause almost all migration incidents, and for each
one names the **safe alternative** rather than just objecting: an index without
`CONCURRENTLY`, `NOT NULL` without a default, a column type change, a rename, an unbounded
`UPDATE`, a `FOREIGN KEY`/`CHECK` without `NOT VALID`, DDL mixed with a backfill, a missing
`lock_timeout`, a migration with no rollback, and the irreversible ones (`DROP COLUMN`,
`DROP TABLE`, `TRUNCATE`).

Engine and version change the specifics, so treat each finding as *check this against your
database*, not as a universal law. The linter says so too.

## 2. Make it backward compatible — expand, then contract

The deploy is never atomic. Old code and new schema coexist, in both orders, for at least a
few minutes — and for as long as a rollback window if you keep one. So:

**Expand** (safe, deploy any time): add a nullable column, add a table, add an index
concurrently, add a constraint as `NOT VALID`, start dual-writing.

**Migrate** (separate step): backfill in bounded batches, committing between them, at a rate
the database can absorb. Then validate the constraint. Then switch readers to the new shape.

**Contract** (last, and only with proof): stop writing the old shape, deploy that, *confirm
nothing reads it*, and only then drop it — ideally a release later.

The proof matters more than the sequence. "Nothing reads it" is a claim to verify with logs,
query stats, or a temporary error-on-read, not an assumption from grepping the repo — other
services, analytics jobs, dashboards and someone's saved query are all readers.

## 3. Never rename, never change a type in place

Both look like one small edit and are actually incompatible with any running code:

- **Rename** → add the new name, dual-write, migrate readers, drop the old name later.
- **Type change** → add a new column of the new type, dual-write, backfill, switch reads,
  drop the old column later.

Yes, it is four deploys instead of one. That is the cost of not being able to roll back.

## 4. Keep the lock window near zero

- Set `lock_timeout` (and a `statement_timeout`) so a migration that cannot acquire its lock
  **fails fast and retries** instead of queueing every request behind it. This is the single
  highest-value line in most migrations: without it, a five-second migration stuck behind one
  long-running query becomes a full outage.
- Build indexes concurrently, outside a transaction.
- Add constraints `NOT VALID` first, validate separately.
- Never put a backfill in the same transaction as DDL — the DDL lock is held for the whole
  backfill.
- Know the table size before you run anything. The same statement is instant on 1,000 rows
  and an outage on 100 million.

## 5. Rehearse, then verify

- **Run it against a realistic copy first** — production-scale data, not an empty dev
  database. Timing on empty tables tells you nothing about lock duration.
- **Write and actually test the rollback.** An untested down-migration is not a rollback plan.
  If a step is genuinely irreversible, say so explicitly and take a verified backup — and
  confirm the backup restores, because an unrestored backup is a hope.
- After running: verify the schema is what you intended, row counts are what you expected, the
  application works against it (`/viby-toolkit:verify`), and — for a backfill — that no rows were
  missed. Report the actual numbers.

## Output

- The change, split into expand / migrate / contract steps and which deploy each lands in.
- Linter findings and what you did about each.
- The lock impact you expect, and the table size you based that on.
- The rollback plan, and evidence you tested it.
- What you deliberately deferred to a later release, and the proof you will need before doing
  the contract step.
- If anything is irreversible: say it plainly, at the top.
