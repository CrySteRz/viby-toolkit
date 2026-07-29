# Where the `kpi` and `analytics` rules come from

Reference for `/viby-toolkit:kpi` and `/viby-toolkit:analytics`. Researched 2026-07-29. Labelled as
`/viby-toolkit:study` requires: **fetched** = page retrieved and the figure read off it;
**search-summary** = taken from a search result summarising the source, primary not opened.

Note on the shape of this evidence, stated plainly: almost all of it is **practitioner consensus,
not measurement**. Metric design has no equivalent of a controlled benchmark — there is no study
here reporting "dashboards designed this way produced better decisions N% of the time". These rules
are load-bearing because they are convergent across independent practitioners and because the
failure modes are concrete and observable, not because anyone has measured the effect size. Treat
them accordingly, and do not cite them as if they were measured.

## The metric hierarchy

Search-summary, 2026-07-29, convergent across several practitioner guides
([Growth Method](https://growthmethod.com/the-north-star-metric/),
[siftfeed on guardrails](https://siftfeed.com/guides/north-star-guardrail-metrics)):

- **One** north star per business or product line — "your single, enduring measure of customer
  value". The best ones are **leading indicators of revenue**, in plain language, tied to a customer
  behaviour rather than a vanity count. Multiple north stars "dilute focus and make it harder to
  prioritise".
- **KPIs** are operational health indicators; dozens is normal. The split quoted: KPIs answer *"is
  the engine running?"*, the north star answers *"are we going in the right direction?"*
- **Input / outcome / guardrail**: inputs show momentum, outcomes validate impact, guardrails
  maintain quality.
- The worked guardrail example: north star = monthly transactions → guardrail = **cost per
  transaction**. And from HEART: use the framework's dimensions as primary *and* guardrail pairs —
  improve Task Success without harming Happiness, lift Adoption without depressing Retention.

## Why guardrails are mandatory rather than nice

**Goodhart's law** — "when a measure becomes a target, it ceases to be a good measure"
(search-summary, 2026-07-29, [Splunk](https://www.splunk.com/en_us/blog/learn/goodharts-law.html),
[Psych Safety on Goodhart/Campbell/cobra effect](https://psychsafety.com/goodharts-law-campbells-law-and-the-cobra-effect/)).
The mitigation quoted directly, and the reason §2 makes it a rule rather than a suggestion:

> "Every KPI should have at least one counter-metric—if a target can improve while harm increases,
> it's incomplete."

Concrete gaming patterns from the same sources, useful because they are recognisable rather than
abstract: *"reduce average handling time" works until it starts rewarding premature call termination
and deflection*; a Soviet nail factory targeted on number of nails produced tiny useless ones;
public test scores as a target turn teaching into test-prep. "Adversarial Goodhart" is named for the
case where a metric allocates power and therefore attracts fraud, strategic compliance and loophole
discovery.

## Definition drift — the thing that actually kills client trust

Search-summary, 2026-07-29, semantic-layer write-ups
([dbt docs](https://docs.getdbt.com/docs/use-dbt-semantic-layer/dbt-sl), and practitioner posts).
The problem statement is quoted almost verbatim in the skill because it is exactly what happens on
client engagements:

> "Without a semantic layer, 'monthly active users' might be defined once in Looker, again in a Mode
> notebook, and a third time in a Jupyter analysis. Each definition is technically correct in
> isolation; collectively, they erode trust."

And: "Revenue means one thing in the finance dashboard and another thing in the marketing report.
Customer count includes trial users in one tool and excludes them in another. Everyone is technically
'correct' — they're just using different definitions."

The property that matters is not the vendor: **define once in the transformation layer, in version
control, and have every consumer read that definition**, so the same metric queried twice returns
the same number and one change propagates everywhere.

## The SQL traps, and why each is in the linter

Search-summary, 2026-07-29 ([Tinybird on timestamps/timezones](https://www.tinybird.co/blog/database-timestamps-timezones),
[DuckDB timestamp guide](https://duckdb.org/docs/current/guides/sql_features/timestamps),
[Red Gate on date failures](https://www.red-gate.com/simple-talk/databases/sql-server/t-sql-programming-sql-server/how-to-get-sql-server-dates-and-times-horribly-wrong/)):

- **BETWEEN is closed-closed.** Quoted: it "creates overlapping bins because it includes both
  endpoints … while temporal analytics almost always require half-open intervals
  (`start <= event_time < end`) to prevent double-counting edge cases."
- **Timezones**: the named failure modes are midnight boundaries, **repeated local times during
  fall-back**, and **missing local times during spring-forward**. Recommended posture: "store
  historical event timestamps in UTC, transform and join in that canonical form, and convert only
  when a consumer needs local display."
- **Late-arriving data**: out-of-order processing can produce correct results regardless of arrival
  order, and the cost "is typically related to the amount of late arriving data rather than how late
  it is" — which is why the restatement *policy* matters more than the lateness.

Fan-out joins, float money, `= NULL`, `UNION` dedup and unbounded fact scans are long-standing SQL
folklore rather than citable findings; they are in the linter because each is mechanically
detectable and each produces a plausible wrong number rather than an error. The linter's precision
was then measured on a real repository of 62 migrations, which is where two rules were narrowed —
see below.

## Testing data

Search-summary, 2026-07-29 (dbt testing guides, e.g.
[Datacoves](https://datacoves.com/post/dbt-test-options),
[SYNQ on monitoring strategy](https://www.synq.io/blog/monitoring-strategies)):

- The split that maps onto §5: **data tests** for known rules (primary key uniqueness, not-null,
  valid enum values), **unit tests** (dbt ≥1.8) that "verify transformation logic works correctly
  with mock data", and **source freshness** with `warn_after` / `error_after` thresholds.
- The freshness rationale, quoted, because it is this repo's silent-pass doctrine in data form:
  freshness checks "solve silent failure modes where a source stops updating but the pipeline keeps
  running without errors."
- **Volume monitors** "detect abnormal table shrink or growth"; anomaly monitoring covers
  distribution shifts and null-rate drift where a fixed threshold is not knowable in advance.
- Reconciliation across source systems is named as a dbt responsibility: express expectations of the
  data, and reconcile to ensure consistency.

## Dashboard presentation

Stephen Few, *Information Dashboard Design* (search-summary of the book and
[Perceptual Edge materials](https://www.perceptualedge.com/library.php), 2026-07-29 — the book
itself was not read). What is safely attributable: it catalogues **thirteen common dashboard design
mistakes**, argues for conveying "a maximum amount of information with the simplest graphical means
possible", and for sizing regions "to visually reinforce their relative importance".

**Deliberately not attributed to Few**: "one question per chart", "every number needs a comparison",
"no dual axes", "no truncated y-axis". These are conventional practice, consistent with his
argument, but they were not verified against his text. They are in the skill on their merits, and
labelled here so nobody later cites a book that was not opened.

## Measured on real code (this repo's own doing, 2026-07-29)

Measured here, against a real client-work data repo — 62 SQL files — the first version of
`check-analytics-sql.ts` produced **126 findings, 117 of them one rule** (`now-in-definition`),
93% of the total. Every one was legitimate: `NOW()` in a migration default or a dated backfill is
correct SQL. `null-equality` also fired on `SET col = NULL`, which is assignment, not comparison.
Both rules were narrowed and the same repo now yields **8 findings**. Precision over coverage, and
this is why the doctrine says validate a heuristic against a large real corpus rather than against
your own fixtures — the fixtures agreed with the author.
