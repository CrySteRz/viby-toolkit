---
name: incident
description: >
  Always load when something is broken RIGHT NOW in production or a shared environment — "the site
  is down", "we're getting paged", "users are reporting errors", "the deploy broke prod", "error
  rate spiked", "it's timing out in prod".

---

# Incident (stop the bleeding, then find out why)

```
IRON LAW: Restore service FIRST, using the most reversible action available.
          Diagnosis is not a prerequisite for a rollback — it is a prerequisite for a FIX.
          Preserve the evidence before you destroy it.
          Never make a speculative code change to production under time pressure.
```

Follow `/viby-toolkit:principles`, with one deliberate inversion: `/viby-toolkit:debug` forbids a fix
without a confirmed root cause, and that is right when you have time. In an incident, users
are losing service while you investigate, so **reversible mitigation comes before diagnosis** —
and then diagnosis comes before any *permanent* change. The two skills disagree on order on
purpose; this one applies only while something is actively broken.

## 1. Establish what is actually happening (2 minutes, not 20)

- **What is the user-visible symptom**, and what fraction of users or requests? "Errors" is
  not a symptom; "checkout returns 500 for about 20% of requests since 14:05" is.
- **When did it start**, and what changed near that time — a deploy, a config or flag change,
  a migration, a dependency or provider incident, a traffic spike, a certificate expiry, a
  disk or quota limit. Correlation with a change is the highest-value signal you have.
- **Is it getting worse?** A saturating resource behaves differently from a bad deploy, and
  the difference changes what you do next.

Write these down as you go. In an incident, memory is unreliable and the timeline is what you
will need for step 5.

## 2. Mitigate with the most reversible action that works

Prefer, in this order — earlier options are safer *and* usually faster:

1. **Roll back** the deploy, config, or migration. If the timing correlates with a change,
   this is almost always both the fastest and the most reversible action.
2. **Flip the flag off** / disable the feature.
3. **Shed or shift load** — scale out, rate-limit, drain a bad instance or region, fail over.
4. **Clear the specific blockage** — kill the long-running query holding the lock, restart the
   wedged process, expand the exhausted resource.

**What not to do:** write new code and ship it to production while the incident is open. A
speculative fix under time pressure is how a one-symptom incident becomes two, and it is not
reversible in the way a rollback is. If the only available fix is a code change, treat it with
the same review and verification you would normally require — the pressure is exactly when
that discipline pays.

Say explicitly whether an action is **reversible**, and confirm the symptom actually improved
after each one. If it did not, undo it before trying the next thing; stacked speculative
changes make the eventual diagnosis much harder.

## 3. Preserve the evidence before it disappears

Do this *as you mitigate*, because mitigation destroys evidence — a restart clears the state,
a rollback removes the broken code, logs roll over, and metrics age out of high resolution.

Capture: the error messages and stack traces verbatim, request IDs and a few affected
identifiers, the relevant dashboard window (screenshot or exported data), the deployed commit
SHA and config values at failure time, `git log` around the suspected change, and the exact
timeline of what you did and when.

An incident where service is restored but nothing was captured will recur, and the second
occurrence starts from zero.

## 4. Diagnose properly — after service is restored

Now the pressure is off, so switch to `/viby-toolkit:debug` and its discipline: reproduce it (the
captured inputs are your reproduction), form a falsifiable hypothesis, confirm it against
evidence, and fix at the root. A rollback restored service; it did not fix anything, and the
change you rolled back still needs to land eventually.

Two things worth stating in the write-up because they are so often skipped:

- **The trigger is not the cause.** "The deploy broke it" names the trigger. Why did the change
  break, why did tests and review not catch it, and why did it reach all users at once?
- **Watch for a hidden dependency.** A component that failed "because" of load may have failed
  because a retry storm amplified a small error, or a cache stampede followed an eviction.
  Cascades have an origin that is usually not where the alarm fired.

## 5. Close the loop

- **Write the timeline**: detection → mitigation → restoration → cause. Include how long each
  took, because that is what tells you whether to invest in detection or in response.
- **Fix the root cause** with normal rigour: a regression test that reproduces it
  (`/viby-toolkit:test`), the fix, verification (`/viby-toolkit:verify`).
- **Fix the detection.** If users told you before monitoring did, that is its own defect.
- **Record it** with `/viby-toolkit:learn` — the symptom, the cause, the mitigation that worked.
  A recurring incident diagnosed from scratch each time is the most expensive kind.
- Ask what made this *possible*, not just what made it happen: no rollback path, no flag, an
  unbounded retry, a migration that could not be reversed, a single point of failure.
  `/viby-toolkit:schema` and `/viby-toolkit:release` exist largely to prevent this class.

## Output

Lead with **current status**: is service restored, and is the mitigation reversible or
load-bearing? Then the timeline, the evidence captured, the cause if known (labelled as a
hypothesis if not confirmed), and the follow-ups with owners. Never report an incident as
resolved when only the symptom is suppressed — say "mitigated, cause not yet confirmed",
which is an honest and completely respectable state.
