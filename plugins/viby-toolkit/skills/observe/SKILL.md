---
name: observe
description: >
  Use when adding or fixing instrumentation — logging, metrics, tracing, error reporting — or
  when an incident revealed you could not see what happened. Triggers: "add logging", "why
  can't we see this", "add a metric", "instrument this", "we had no visibility", "we can't tell
  what's happening in this service", "what should we alert on", "this log is useless".
---

# Observe (instrument for the person reading it at 3am)

```
IRON LAW: Instrument DECISIONS and OUTCOMES, not arrivals. "Entered function" tells nobody
          anything; "rejected order 8412: card declined, retry 2 of 3" ends an investigation.
          Every line must carry the identifiers needed to correlate it with everything else.
          Never log a secret, a token, or personal data you would not put in a screenshot.
```

Follow `/viby-toolkit:principles`. This is a doctrine skill: the guidance below is established
engineering practice rather than a research finding, and the sourcing on it is mostly vendor
material, so it is stated as practice and not dressed up as evidence.

The framing that matters: most of the time spent resolving an incident is **human time spent
correlating** — reading logs, then traces, then deploy history, then a dashboard, trying to
line them up. So the question for every log line and every metric is not "is this
information?" but **"does this shorten that correlation?"** Instrumentation that cannot be
joined to anything else is decoration.

## 0. Audit what is already there before adding more

```bash
LOG=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/observe/scripts/check-logging.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$LOG" src/
```

P1 is the part that is not a style question: **personal data or a credential reaching the logs**, and
whole request/user objects being logged, where whatever the object contains next month goes with it. A
log line is the least access-controlled artifact a team produces — it fans out to aggregators,
alerting, screenshots and third-party dashboards, and it is retained long after the request.

Then the doctrine below, made mechanical: interpolated messages with no structured fields, a `catch`
that logs without the caught error, arrival logs, identifier-shaped values used as **metric labels**
(the documented cost trap), logs in tight loops, and files with several log calls and no correlation
key. Tests and CLI output are excluded — a command-line tool's stdout is its interface, not telemetry.

It reads field *names*, so it cannot know that a variable called `data` holds an email address, and it
cannot tell whether your aggregator redacts. A clean run is not a privacy review.

## 1. Decide what question you are trying to answer

Instrument backwards from the question, never forwards from the code:

- "Which users were affected, and when did it start?" → needs a user/tenant identifier and a
  timestamp on the error path.
- "Is it one dependency or all of them?" → needs the downstream target as a field.
- "Is it slow or is it failing?" → needs latency *and* outcome on the same event.
- "Did the deploy cause it?" → needs the version/commit on every event.

If you cannot name the question, you are about to add noise. A log nobody will query is a
cost with no benefit: it slows the search that matters and it bills you monthly.

## 2. Structured fields, not sentences

Emit events as key/value data, not prose. `"payment failed for user 8412 after 3 tries"` can
only be grepped; `{event: "payment.failed", user_id: 8412, attempts: 3, reason: "card_declined"}`
can be counted, grouped, and sliced.

- **High cardinality is the point**, not the problem — for logs and traces. Request id, user
  id, tenant, order id: these are what let you go from "something is wrong" to "these 40
  requests are wrong, and they share a tenant". Beware the inverse for **metric labels**,
  where each distinct value multiplies stored series and the bill; put identifiers in
  logs/traces, keep metric labels low-cardinality (status, route template, region).
- **Include the correlation keys on every event**: trace/request id, service, version/commit,
  environment. This is what makes the 3am join possible.
- **One event per decision**, with everything about it, rather than five lines you must
  reassemble in your head.
- **Log the reason, not just the failure.** `reason: "card_declined"` versus a generic
  "payment failed" is the difference between a five-minute and a two-hour investigation.

## 3. Levels that mean something

- **ERROR** — someone must act; it broke and we could not recover. If nobody would act on it,
  it is not an error. An ERROR log that fires constantly has trained everyone to ignore the
  real one, which is the same crying-wolf failure this toolkit fights everywhere else.
- **WARN** — degraded but handled: a retry succeeded, a fallback was used, a limit is near.
- **INFO** — the decisions and state transitions a reader needs to reconstruct what happened.
- **DEBUG** — detail for development, off in production by default, sampled if on.

Never swallow an exception without logging it (`/viby-toolkit:test` flags this in tests for the
same reason). Log the error *with* its stack and the inputs that produced it, once, at the
level that owns the decision — not at every layer as it propagates.

## 4. Metrics and traces

- **Metrics answer "how much/how often", not "why".** Track rate, errors, duration for each
  meaningful operation, and saturation for each bounded resource. Use histograms for latency
  and read p95/p99 — a mean latency hides exactly the tail users complain about.
- **Traces answer "where did the time go".** Span the boundaries: inbound request, each
  outbound call, each queue hop, each database query. Propagate the trace context across
  every boundary, including async ones, or the trace ends where the interesting part starts.
- **Prefer vendor-neutral instrumentation** (OpenTelemetry) so the backend can change without
  re-instrumenting everything.

## 5. Alert on symptoms, not causes

Alert on what the user experiences — error rate, latency, success of a critical flow — plus
saturation of anything that will fail when full. Do not alert on individual causes: they are
unbounded in number, and the alert that fires is rarely the one that matters.

Every alert needs an owner and an action. An alert with no action is a notification, and a
notification that fires often is training people to ignore the page.

## 6. Verify the instrumentation actually works

Same evidence gate as everything else: **trigger the path and look at the output.** An
untested log line is as likely to be missing its most useful field as not.

- Cause the failure deliberately and confirm the event appears, with the fields you intended,
  in the place you will actually look.
- Confirm you can answer the step-1 question with the query you would run in an incident.
- Check for leaks in what you just added: tokens, credentials, personal data, full request
  bodies, entire objects logged by default.
- Confirm the cost: an event per request at scale is a large bill and a slow search. Sample
  the high-volume paths, keep the errors unsampled.

## Output

- The question this instrumentation answers, and the query that answers it.
- What was added, at `file:line`, with the field schema.
- The evidence: the path triggered, and the real emitted output.
- Cardinality and cost note: what is per-request, what is sampled, what is a metric label.
- What you deliberately did not instrument, and why.
