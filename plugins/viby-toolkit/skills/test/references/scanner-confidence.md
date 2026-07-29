# How much to trust each check in `scan-test-quality.ts`

Read when you have run the scanner and are deciding which findings to act on. Every finding is a
heuristic; this file is what separates the ones you can act on directly from the ones that are only
a question worth asking.

**Trust the checks unequally — these are measured numbers, not guesses.** Audited against
real suites (CPython's ~1,100 files, plus vuejs/core and vitejs/vite):

| Check | Confidence | Notes |
|---|---|---|
| `tautology`, `focused-or-skipped` | **high** | clean in every sample; act on these directly |
| `over-mocking`, `sleep-wait` | medium | real signal, but judgement calls — a zero-delay `setTimeout` tick-flush and a poll's timeout guard are excluded as legitimate |
| `no-assertion` | medium | it now follows delegation into base classes and mixins in other files, which was the largest false-positive source; the remaining classes are below |
| `swallowed-error` | **lowest** | 0 of 12 sampled were real on Python. Kept because empty `catch {}` in JS is a genuine smell; treat every hit as a question |

**Known blind spots — a clean scan is not proof.** The scanner matches text, so it cannot
see: assertions inside a **context manager's `__exit__`**; assertions in a **native
extension** or an external script; and **JSX text nodes**, which are not a string context,
so prose inside `<div>…</div>` mentioning `it.skip` can still be flagged.

**Cross-file delegation is handled, with limits.** It resolves relative imports, dotted
Python package paths, and shared-infrastructure modules in the same directory, then indexes
which of their declarations provably assert — so `self.checkParam(...)` asserting via a
mixin in another module is recognised. Two deliberate constraints: imported names count
**only when called through a receiver** (`self.x()`, `this.x()`, `cls.x()`), since that is
how inherited helpers are always invoked and the restriction stops a generically-named
shared helper from excusing unrelated calls; and resolution stops at 12 related files per
scanned file. A helper reached some other way — a deep package alias, a dynamic import, a
base class installed at runtime — still won't resolve.

So: every finding is a heuristic. **Confirm against the code before changing anything**,
exactly as the review cluster's grounding gate requires — and never report a clean scan as
evidence that the tests are good, only that these specific defects were not found.
