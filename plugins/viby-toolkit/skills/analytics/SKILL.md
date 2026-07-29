---
name: analytics
description: >
  Use when implementing or debugging the numbers themselves — "the numbers don't match", "write the
  query for this metric", "build the reporting pipeline", "why is this figure wrong", "the totals
  don't add up", "our dashboard disagrees with finance", "set up dbt models for these KPIs", "test
  the data". Implements a metric contract as queries and models, then proves the number with tests
  and a reconciliation. Distinct from /viby-toolkit:kpi, which defines what to measure and how to
  present it; come here once the definitions exist.
---

# Analytics (a number is not done until it reconciles)

```
IRON LAW: Every metric is implemented ONCE, in the transformation layer, and proved against an
          independent source before anyone sees it on a dashboard.
          A query that runs and returns a plausible number is the failure mode, not the success —
          nothing in the pipeline fails loudly when the number is merely wrong.
```

This is the counterpart to `/viby-toolkit:kpi`: it has the contracts, this makes them true. The
danger is specific to data work — a broken build is loud, a broken metric is a plausible number in
a tile that someone quotes in a board meeting six weeks later. Follow `/viby-toolkit:principles`.
Sources: `../kpi/references/methods.md`.

## 1. Refuse to start without a contract

If there is no formula, grain, time basis and filter list, go back to `/viby-toolkit:kpi` and get
them. Implementing an undefined metric means inventing the definition silently, and the client will
discover your invention by disagreeing with it. Five minutes of definition beats a week of
reconciliation.

## 2. Define it once, in the transformation layer

Not in the BI tool, not in a notebook, not in three places that are each right on their own. One
versioned definition that every consumer reads, so the same metric asked twice returns the same
number — and a change propagates rather than forking. Whatever the stack calls it (a semantic
layer, a metrics layer, a curated mart), the property that matters is **one definition, in git,
with a diff history**.

Layer the models so the grain is explicit at each step: raw → cleaned/typed → one row per business
event → aggregates per stated grain. Most "the totals don't add up" bugs are a grain change nobody
declared.

## 3. The traps that corrupt numbers silently

These are the mechanically checkable ones, and they all produce a plausible answer:

```bash
LINT=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/analytics/scripts/check-analytics-sql.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$LINT" models/          # exit 1 = findings
```

- **`BETWEEN` on time is closed at both ends.** The boundary row lands in two buckets, so monthly
  figures stop summing to the annual one. Always half-open: `>= start AND < end`.
- **A join that fans out multiplies your aggregate.** `COUNT(*)` after a one-to-many join counts
  the multiplication; `SUM` over it is inflated, and a trailing `DISTINCT` hides the duplicate rows
  without undoing the inflated sum. Aggregate the many-side to the right grain first.
- **Division with no guard**: a zero or NULL denominator either errors or yields NULL, which a
  dashboard draws as a gap or a zero. `NULLIF(denominator, 0)`, and decide what empty *displays* as.
- **Timezones decide which day a number lands in.** Store UTC, transform and join in UTC, convert
  only for display — and truncate with an explicit zone, because a UTC-day bucket labelled as the
  client's day is wrong by up to a day at every boundary. DST makes it worse: an hour repeats on
  fall-back and an hour does not exist on spring-forward.
- **`= NULL` matches nothing**, silently excluding exactly the rows you were filtering for.
- **Money in floating point** drifts by cents and stops reconciling with finance. `NUMERIC`.
- **`NOW()` in a definition** makes the figure unreproducible: last week's number cannot be
  re-derived, so it cannot be audited. Parameterise the window and stamp it on the output.
- **Plain `UNION` de-duplicates**, silently collapsing genuinely repeated events.

## 4. The traps a linter cannot see

- **Identity.** One human is an anonymous id, then a logged-in id, on two devices. Whatever rule
  you pick for stitching them, write it in the contract — "unique users" is otherwise unfalsifiable.
- **Late and out-of-order data.** Events arrive after the window closed, so yesterday's number
  changes tomorrow. Decide the restatement policy up front: how long a window stays open, and
  whether history is allowed to change. Then say so on the dashboard, because a figure that
  silently changes after being quoted destroys trust faster than a wrong one.
- **Event time vs ingest time.** Two different questions ("when did it happen" / "when did we learn
  about it"), and mixing them makes numbers move for no business reason.
- **Deleted, refunded, cancelled, test.** Every one is a filter decision with a business answer,
  not a technical default. Soft-deleted rows are the classic silent inflator.
- **Currency and units.** Mixed currency summed without conversion, cents added to pounds, and
  minutes added to seconds — none of which error.
- **Changing dimensions.** If a customer's plan or region changed, does history show what it was
  *then* or what it is *now*? Both are legitimate; only one is what they asked for.

## 5. Prove it — three layers, in this order

1. **Unit-test the transformation logic** against small hand-built fixtures where you know the
   answer, including the awkward rows: the boundary timestamp, the NULL, the refund, the duplicate,
   the one-to-many. Model logic can be tested with mock inputs — do that rather than eyeballing
   production output.
2. **Assert the rules the data must obey**: primary key uniqueness, not-null on the grain key,
   accepted values on enums, referential integrity, ranges (a conversion rate above 1 is a bug).
3. **Reconcile against an independent source.** This is the step that actually convinces a client:
   the revenue KPI against the invoicing system or the finance export, user counts against the
   application database. State the residual difference and *why* it exists (timing, timezone,
   refunds), because there is almost always one and an unexplained delta is where trust dies.

Then guard the pipeline against going quiet: **freshness** checks (with explicit warn/error
thresholds) catch the silent failure where a source stops updating and everything downstream keeps
running without an error, and **volume** checks catch abnormal shrink or growth. A dashboard whose
data stopped arriving looks exactly like one where nothing happened.

`/viby-toolkit:verify` applies here too, with a data-shaped twist: a green pipeline run says the
job succeeded, not that the numbers are right.

## 6. Make it survive

- **Backfill deliberately** and record what was restated and when.
- **Version the definitions**; when a definition changes, the old number does not become wrong
  retroactively — say which version produced which figure.
- **Watch the cost** — an unbounded scan over an event table gets slower and more expensive every
  day, and the query that was fine in the demo is the one that times out in month four.
- **Never put unnecessary PII in a reporting layer**: aggregate early, and remember that a
  dashboard is usually the least access-controlled thing you will build for a client.

## Output

- **The models**, layered with the grain stated at each step.
- **The tests** — unit, rule, freshness, volume — and what each protects.
- **The reconciliation**: source compared against, the delta, and the explanation for it.
- **The linter run**, clean or with each finding explained.
- **The restatement policy** and the refresh cadence, both written where a viewer sees them.
- **What is still unproven** — the metric you could not reconcile, and what it would take.
