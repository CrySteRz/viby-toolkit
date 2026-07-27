---
name: debug
description: >
  Use when something is broken, failing, throwing, crashing, flaky, or behaving wrong and
  the cause isn't obvious. Use when the user says "why is this failing", "debug this",
  "it's broken", "this test is flaky", "root cause", or pastes an error/stack trace.
---

# Debug (root-cause, evidence-driven)

**If it is broken in production right now, start with `/viby-code:incident` instead.** That
skill deliberately inverts the rule below — reversible mitigation before diagnosis, because
users are losing service while you investigate — and then hands back here for the real
root-cause pass once service is restored.

```
IRON LAW: NO FIX WITHOUT A CONFIRMED ROOT CAUSE.
          A symptom fix is a failure. Systematic is faster than thrashing.
```

The failure mode of AI debugging is **plausible guessing**: changing code that "looks
suspicious" without proving it's the cause. Viby-code debugging is the opposite — every fix
is preceded by a reproduction and a confirmed causal chain. Follow
`/viby-code:principles`. Tempted to skip straight to a fix under time pressure? Don't —
thrashing through unverified guesses is slower than one disciplined pass.

## The loop

### 1. Reproduce first (non-negotiable when possible)

Establish a concrete, repeatable trigger: the exact input, command, request, or state
that produces the failure, and the exact observed symptom (error text, wrong output,
stack trace). If you can't reproduce it, say so — then your goal shifts to gathering
enough evidence to reproduce it, not to speculatively patching.

Capture the ground truth: the real error message, the actual stack trace, the failing
assertion. Don't paraphrase from memory — get the literal output.

**Make the reproduction a first-class deliverable — a failing test.** The strongest 2026
evidence across multiple independent studies is that *writing the correct reproduction
test is the hard part; once you have it, the fix is comparatively easy.* So: write a test
that reproduces the reported behavior, confirm it fails **for the right reason** (not a
typo or setup error), and keep it. `/viby-code:test` covers how to make it a *good* test —
in particular, don't mock away the component that actually contains the bug. This test is what proves the bug gone in step 6. Because
authoring a correct repro test is the hard, high-leverage step, do it on the **strong
model** (main thread, or escalate to the top tier / fable for a subtle one) even if you
later route the mechanical fix to a cheaper agent — inverting the usual routing intuition,
on purpose.

### 2. Localize (fan out `scout` / `debugger` agents for breadth)

Narrow *where* before reasoning about *why*. Spawn cheap parallel agents to gather
evidence, each returning a tight summary (not dumps):
- One traces the **code path** from the entry point to the failure site.
- One searches **recent changes** — `git log`/`git blame` on the implicated files, what
  changed around when this started failing. Regressions usually have a commit.
- One pulls **logs / error telemetry** around the failure if available.
- One checks **the obvious external causes** — config, env vars, dependency versions,
  data shape.

You keep the conclusions and assemble the picture. The bulk searching stays in the
subagents.

### 3. Hypothesize — explicitly

State the single most likely cause as a falsifiable hypothesis: "The failure is X because
Y; if that's true, I should see Z." Rank alternatives. **Do not skip to fixing.**

### 4. Test the hypothesis against evidence

Prove or kill the hypothesis *before* changing code:
- Add a targeted log/print, inspect state at the suspect point, write a failing test that
  isolates it, or trace the exact values. Confirm Z actually appears.
- If the evidence contradicts the hypothesis, discard it and take the next one. This is
  the discipline — you're not allowed to "fix" a cause you haven't confirmed.
- Beware the signal that pattern-matches to a known failure but has a different root
  cause. Confirm *this* instance, don't assume.

### 5. Fix at the root, minimally

Fix the actual cause, not the symptom. The smallest change that addresses the confirmed
root cause. If you find yourself suppressing a symptom (catch-and-ignore, bumping a
timeout, retry-until-it-works), stop — that's a signal you haven't found the root cause.

Trace bad values **backward** up the call stack to their origin and fix at the source, not
where the value surfaced. A wrong value caught three layers down should be corrected where
it was produced.

**Defense-in-depth (only when warranted).** If the bug let invalid state reach a dangerous
path, and either the pattern exists in 3+ places or the failure would be catastrophic,
don't patch one layer — guard several, each catching a *distinct* failure class:
1. **Entry validation** — reject bad input at the boundary (API/param parse).
2. **Invariant check** — enforce preconditions entry validation can't express.
3. **Environment guard** — refuse the dangerous op in the wrong context (e.g. refuse a
   destructive command outside a temp dir under test).
4. **Diagnostic breadcrumb** — log `{state, cwd, env, stack}` right before the risky op.

Don't apply this speculatively to ordinary bugs — it's for catastrophic or widespread
failure classes only. Each layer must catch something the others can't; don't duplicate
the same check four times.

### 6. Verify the fix closes the loop

Re-run the exact reproduction from step 1 (the failing test now passes) and show the
symptom is gone. Then check you didn't break the neighborhood — run the relevant tests via
`/viby-code:verify`, which also screens for the silent-pass modes that make a fix look
confirmed when it isn't. A fix isn't done until the original trigger is demonstrably
resolved *and* nothing adjacent regressed.

**Anti-flake rule:** if the bug or its test involves waiting (async, I/O, a server coming
up), never wait on an arbitrary `sleep`/timeout — wait on an **observable condition**
(poll for the state, await the event). Arbitrary timeouts are how flaky tests and
heisenbugs are born; a fix that relies on one hasn't found the real timing contract.

## Output

Report: the root cause (with the file:line and the evidence that confirmed it), the fix,
and the verification (repro before → repro after). If you couldn't fully confirm, say
exactly what's still uncertain and what evidence would settle it — don't dress up a guess
as a diagnosis.
