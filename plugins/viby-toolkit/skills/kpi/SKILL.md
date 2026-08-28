---
name: kpi
description: >
  Always load when deciding WHAT to measure and how to present it — "build them a dashboard",
  "what should we track", "which metrics matter", "define our KPIs", "what does 'active user' even
  mean here". Not /viby-toolkit:analytics.

---

# KPI (define the number before anyone builds it)

```
IRON LAW: Every KPI names the DECISION it serves, the exact FORMULA at a stated GRAIN and
          WINDOW, an OWNER, and at least one GUARDRAIL that would reveal it being gamed.
          A number nobody would act on differently is decoration — delete it, don't chart it.
```

Client dashboard work fails in a predictable order: the definitions were never written down, so
two dashboards disagree, so nobody trusts either, so the dashboard becomes a screenshot in a
monthly deck. All of that is decided *before* any code, which is why this skill comes first.
Follow `/viby-toolkit:principles`. Sources: `references/methods.md`.

## 1. Start from the decision, not the data

For each proposed KPI, answer in one line each: **who** looks at this, **how often**, **what do
they do differently** when it moves, and **what would they do if it moved the other way?** If the
answer to the last two is "nothing", it is context at best — put it in a footnote, not a tile.

Ask the client the question that saves the engagement: *"when this number is bad, what happens
next?"* If nobody can say, you are building a report, not a dashboard, and the difference is worth
naming out loud before you quote for it.

## 2. One north star, a handful of KPIs, a guardrail on each

Three roles, and mixing them is why dashboards sprawl:

- **North star** — *one* per business or product line: the enduring measure of delivered customer
  value, a leading indicator of revenue, phrased in plain language, tied to a customer behaviour.
  Multiple north stars dilute focus and make prioritisation impossible.
- **KPIs** — operational health. Dozens is fine. They answer *"is the engine running?"* while the
  north star answers *"are we going the right way?"*
- **Guardrails** — the counter-metric that catches damage done in pursuit of the target.

**Every KPI gets at least one guardrail: if the target can improve while harm increases, the
definition is incomplete.** This is not paperwork, it is the single highest-value thing in this
skill, because *when a measure becomes a target it stops being a good measure* and people optimise
exactly what you wrote down. The documented pattern: "reduce average handling time" works until it
rewards hanging up on customers; a target for number of nails produced yields tiny useless nails.
So: north star *transactions* → guardrail *cost per transaction*. Target *tickets closed* →
guardrail *reopen rate*. Target *signups* → guardrail *week-4 retention*.

Write the gaming path down explicitly: **"the laziest way to make this number better without
doing the real work is ___"**. That sentence usually writes the guardrail for you.

## 3. Write the metric contract — the artifact that prevents the disagreement

One per KPI, in version control, not in a BI tool's description field:

| Field | Why it exists |
|---|---|
| **Name** | plus every alias the client already uses for it, so their phrasing maps to yours |
| **Question it answers** | in the client's own words |
| **Formula** | numerator and denominator explicitly, including what's excluded |
| **Grain** | per what — user, account, session, order, day? Ambiguity here is the #1 cause of mismatched numbers |
| **Time basis** | which timestamp (event time, ingest time, invoice date?) and which **timezone** |
| **Window & comparison** | rolling 28d? calendar month? vs previous period or vs same period last year? |
| **Filters** | test accounts, internal users, refunds, cancelled, deleted — each one stated |
| **Owner** | a person who arbitrates when it's disputed |
| **Guardrail** | §2 |
| **Target / threshold** | what "good" is, and who agreed to it |

**Definitions drift, and that is what destroys trust.** The same metric defined once in the BI
tool, again in a notebook, again in a spreadsheet: each is defensible alone, and collectively
nobody knows which dashboard is right. Revenue means one thing to finance and another to
marketing; customer count includes trials in one place and not the other. Define **once**, in the
transformation layer, and have every consumer read that one definition
(`/viby-toolkit:analytics` §2).

**Decide the ambiguous words on paper, with the client, before implementing:** active, customer,
churn, revenue, session, new. Each has three plausible readings and they will not all agree.

## 4. Design the dashboard to answer a question per element

- **One question per chart**, and put the question in the title. "Are we selling more than last
  month?" beats "Revenue".
- **A number with no comparison is not information.** Every figure carries a baseline — prior
  period, target, or same period last year — because "£42,000" means nothing alone and
  "£42,000, +8% vs last month, target £40,000" is a decision.
- **Simplest graphical means that carries the information**, and size each region to its
  importance rather than filling the grid evenly. Decoration costs attention that the numbers need.
- **Lead with the north star and the exceptions**, not with everything you can compute. A
  dashboard that shows all 40 KPIs equally shows none of them.
- **Say when it was last refreshed, and in which timezone**, on the page. A stale dashboard that
  looks live is worse than one that says it is stale.
- **Avoid the two classic distortions**: dual axes (they let you imply any correlation you like)
  and truncated y-axes on anything the client will screenshot.
- **Every tile needs a drill-down path** — the first question after "why is that down?" is
  "which segment?", and a dashboard that cannot answer it generates a support request instead.

## 5. Cut it down, then hand off

Delete every tile that survived only because it was easy to compute. Then ship the contracts to
`/viby-toolkit:analytics` for implementation and testing, and the presentation to
`/viby-toolkit:ui` for verification that it actually renders and drills down.

## Output

- **The metric contract table** (§3), one row per KPI, in the repo.
- **The decision each serves**, and the ones you rejected as decoration, with why — a rejected
  metric is the most reusable part of this document.
- **Ambiguous terms resolved**, with the client's chosen reading recorded.
- **The dashboard layout**, as questions in reading order, not as a chart list.
- **Open disagreements** — where the client's usual figure will differ from yours, and by how much.
  Surface this *before* they notice it themselves; it is the moment trust is won or lost.
