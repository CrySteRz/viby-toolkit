---
name: ui
description: >
  Use when a change has to be seen or driven to be believed — "does this render", "check the UI",
  "is the layout broken", "click through this flow", "the button does nothing", "take a
  screenshot", "does it work on mobile", "does this look right on mobile", "check accessibility",
  "the page is blank". Drives a real
  browser and reports what actually happened, with the screenshot and console attached. Distinct
  from /viby-toolkit:test, which designs the test suite, and from /viby-toolkit:perf, which
  measures speed.
---

# UI (see it, drive it, or you have not checked it)

```
IRON LAW: A claim about the interface needs an OBSERVATION of the rendered result — a screenshot
          or a driven interaction. Reading the component and concluding it looks right is the
          same error as claiming a test passes without running it.
```

The failure mode here is specific and common: the markup is correct, the change is correct, and
the page is blank — because of a build error, a failed request, a null guard, a CSS layer, or a
route that never mounts. None of that is visible in the diff. Follow `/viby-toolkit:principles`
§5: the evidence gate applies to pixels too.

## 1. Pick the tool by lifecycle stage, not by preference

Different stages have opposite constraints, so the same job wants different tools:

| Stage | Use | Why it wins here |
|---|---|---|
| Inner loop — many quick checks while building | the cheapest, fastest driver available | cadence dominates; correctness gets re-checked next step anyway |
| Behavioural or one-off investigation | full devtools access (console, network, DOM, coverage) | one-off cost, and being wrong here is expensive |
| Committed regression, running in CI | a real E2E framework | maintenance and flakiness dominate, not capability |

**Cost is payload × cadence** (`/viby-toolkit:principles` §2). A driver that re-sends the whole
accessibility tree or DOM snapshot after every click pays that cost *per step* — a 15k snapshot
across a six-step flow is 90k, and it loses to a cheaper tool taking one screenshot at the end.
Measure the flow you will actually run, not one call.

**Privacy is a hard gate, not a preference.** Never drive a page containing real user data,
credentials or payment details through a tool that ships page content to a third party for
processing. If a tool has a cloud mode, keep it off. State which mode you used.

## 2. Establish that it renders at all, before anything subtle

In order, because each step makes the next meaningful:

1. **Does the page load?** Navigate, then read the **console** and the **network log**. A 500 on
   a bundle or a failed API call explains more blank pages than any CSS theory.
2. **Is the thing you changed on screen?** Screenshot it. Not "the component tree contains it" —
   visible, in the viewport, with content.
3. **Does the flow complete?** Drive the actual path a user takes: click, type, submit, land.
   A form that renders and cannot submit is not working.
4. **Only then** judge layout, spacing and polish.

## 3. Check the states nobody builds first

Real interfaces fail on the states that are not the happy path, and they are cheap to force:

- **Empty** — no rows, no results, first-run. Does it explain itself or show a broken skeleton?
- **Loading** — is there one, and does it appear before the data or after?
- **Error** — kill the request (offline, or a forced 500) and see what the user is told. "Nothing
  happened" is the most common answer and the worst one.
- **Too much data** — a long name, 500 rows, a 40-character word with no spaces.
- **Two widths** — one narrow, one wide. Not every breakpoint; the two that would embarrass you.

## 4. Accessibility and keyboard, at the level that actually gets used

Not an audit — the four that catch most real breakage:

- **Keyboard only**: can you reach and operate every control with Tab and Enter, and is the focus
  ring visible? A modal you cannot escape or a button that is a `div` shows up immediately.
- **Focus after action**: where does focus go when a dialog opens and closes? Nowhere is wrong.
- **Names**: does every control have an accessible name a screen reader would read? Icon-only
  buttons are the usual offenders.
- **Contrast** on the text you changed, and never colour as the *only* signal of state — the
  white-on-white and red-means-error class of bug.

## 5. Report the observation, not the impression

Every claim carries its evidence:

- **Screenshot** of the state you are describing, per state you checked.
- **Console output** — errors and warnings, verbatim, or "clean".
- **Network** — the failed or slow requests, with status codes.
- **The steps you drove**, in order, so someone can repeat them.
- **What you did not check**, explicitly. An interface report that hides its gaps gets read as
  "everything works".

Then: durable regression coverage goes to `/viby-toolkit:test` (pick the level deliberately — an
E2E test per bug is how suites become slow and flaky), the ship/no-ship gate is
`/viby-toolkit:verify`, and anything about speed is `/viby-toolkit:perf`, which needs a
measurement rather than a feeling.

## Don't

- **Don't assert on implementation details** — class names, internal DOM structure, component
  names. They change without the interface changing, and that is a test that fails for no reason.
- **Don't snapshot the whole DOM** as a "test". It fails on every unrelated edit and gets
  regenerated blindly, which makes it worse than nothing.
- **Don't test the framework.** Whether the router routes is not your bug.
- **Don't report "looks good" without a screenshot.** That is the sentence this skill exists to
  make impossible.
