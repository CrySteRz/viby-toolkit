---
name: refactor
description: >
  Use when restructuring code without changing what it does — "clean this up", "extract
  this", "split this file", "this function is too long", "reduce duplication", "improve the
  naming", "make this testable", "untangle this". Distinct from /viby-toolkit:migrate (a
  mechanical sweep across many files) and from a rewrite (which changes behaviour).
---

# Refactor (behaviour-preserving, and proven so)

```
IRON LAW: A refactor that changes behaviour is not a refactor — it is an unreviewed rewrite.
          Before restructuring, know what pins the behaviour. If nothing does, pin it first.
          Never mix a refactor and a behaviour change in one diff.
```

Follow `/viby-toolkit:principles`. Reading the code is fan-out work; the restructuring itself is
a write, so it stays single-threaded.

## Where this sits

- `refactor` — same behaviour, better structure. **This skill.**
- `/viby-toolkit:migrate` — one mechanical transformation applied across many files.
- `/viby-toolkit:perf` — behaviour preserved, but the *point* is a measured speed change.
- `/simplify` — small local cleanups of code you just wrote.
- A **rewrite** changes behaviour. That is a feature, and belongs in
  `/viby-toolkit:orchestrate` with a plan and tests, not here.

## 1. Say what must not change

Write it down before editing, in one or two lines: the public API, the observable outputs,
the error contract, the performance envelope, the side effects. This is the invariant the
rest of the skill defends. "It should behave the same" is not specific enough to check.

Also state the **reason** for the refactor. A refactor without a reason is churn: it costs
review time and git-blame clarity and buys nothing. Good reasons: a change you are about to
make is hard here; the same logic has drifted in three places; this cannot be tested as-is.

## 2. Find what currently pins the behaviour

Ask: **if I break this, what tells me?**

- Run the existing tests for the area and see them pass — that is your safety net's current
  state, and if they don't pass now, you cannot use them to judge your change.
- Check whether they actually cover the behaviour you are about to move, not just that they
  exist. `/viby-toolkit:test` covers judging that; the scanner will tell you if those tests
  assert nothing.

**If nothing pins it, pin it first — characterization tests.** Write tests that assert what
the code *currently does*, not what it should do. If current behaviour looks wrong, capture
it anyway and note it separately: preserving a bug during a refactor is correct, and fixing
it is a different, reviewable change. Without this step you are not refactoring, you are
editing hopefully.

## 3. Restructure in small, individually-verified steps

- **One transformation at a time**, each leaving the code working: extract a function, rename
  a symbol, introduce a parameter object, invert a dependency, move a module, replace a
  conditional with polymorphism, inline a needless indirection.
- **Run the tests after each step**, not at the end. A refactor that breaks something is
  cheap to diagnose one step back, and expensive after twenty.
- **Prefer the tool over the hand-edit.** An IDE or language-server rename, or a codemod, is
  exact where manual editing quietly misses a dynamic reference, a string-based lookup, a
  reflection call, a template, or a doc example.
- **Keep the diff boring.** No opportunistic behaviour tweaks, no drive-by fixes, no
  reformatting the whole file — those make the diff unreviewable and hide the one line that
  did change something. If you spot a real bug mid-refactor, note it and fix it separately.

## 4. Prove behaviour was preserved

This is the step that distinguishes a refactor from a hopeful edit, and models are
specifically weak here. A 2026 study of LLMs reviewing refactorings
([PROMISE 2026](https://homepages.dcc.ufmg.br/~figueiredo/publications/promise2026preprint.pdf))
found they judge by surface features rather than semantics: **heuristic bias toward style and
naming**, and **semantic overgeneralization** — penalizing legitimate refactorings *because*
they shortened the code, mistaking brevity for lost functionality. The lesson cuts both
ways: do not trust a code-shaped argument that behaviour is preserved, in either direction.
Check it.

- **The same tests pass before and after**, unchanged. If you had to edit a test to make it
  pass, behaviour changed — stop and decide deliberately whether that was intended.
- For a pure transformation, prefer an **executable equivalence check** where one is cheap:
  run both versions over the same realistic inputs and diff the outputs; snapshot the
  observable output before and after; compare emitted artifacts.
- Check the **non-test surface** the tests don't cover: exported type signatures, public
  docs, serialized shapes, log lines other systems parse, metric names on dashboards.
- Confirm the **call sites**, especially any reached dynamically — search for the old name as
  a string, not only as a symbol.

## 5. Land it

Commit the refactor **on its own**, with a message that says what was restructured and why,
and states that behaviour is unchanged and how that was verified. Then make the behaviour
change you actually wanted, in the next commit, against the now-cleaner code.

## Output

- What must not change (from step 1), and the reason for the refactor.
- What pinned the behaviour — existing tests, or characterization tests you added.
- The transformations applied, in order, at `file:line`.
- The preservation evidence: the same tests passing before and after, unchanged, plus any
  equivalence check you ran.
- Anything you deliberately left alone, and any bug you found and did **not** fix here.
