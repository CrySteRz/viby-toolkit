# Review dimensions, finding schema, and confidence rubric

Reference for the `review-cluster` skill. Loaded on demand — not part of the always-on
skill metadata.

## Dimensions (spawn by relevance to the diff, not all every time)

**Always-on (cheap, every review):**
- **correctness** — logic errors, wrong conditions, off-by-one, unhandled cases, broken
  control flow, incorrect API/contract usage, error propagation, intent compliance.
- **testing** — tests that pass while the behaviour is wrong. Hunt, in priority order:
  a test with **no assertion** or one that **cannot fail** (`expect(true).toBe(true)`,
  `assert x == x`); **over-mocking** — the test exercises mocks rather than code, or
  asserts a call transcript (`toHaveBeenCalled`) instead of the resulting state, so it stays
  green when the real implementation drifts; a **mocked boundary with no contract or
  integration test** anywhere behind it; `.only`/`.skip`/`@Ignore` left in, silently
  shrinking the suite; **sleep-based waiting** instead of an observable condition;
  **swallowed exceptions** in a test; missing tests for the branches and error paths this
  diff added; a **fixed bug with no regression test**; and assertion roulette (4+
  unexplained assertions — a failure that names no cause). Run
  the test-quality scanner on the diff's test files first — it finds most of these
  mechanically for near-zero cost, leaving you the judgment calls. `/viby-code:test` has the
  invocation (the path must be resolved by globbing the plugin cache; `CLAUDE_PLUGIN_ROOT`
  is not set for skill bodies) and the full guidance.
- **maintainability** — structural quality only where it threatens correctness later:
  dangerous complexity, coupling, type-boundary leaks, dead code. (Pure taste →
  `/simplify`, not here.)

**Conditional (spawn when the diff touches the relevant surface):**
- **security** — auth/authz (especially a missing *ownership* check, not just a missing
  login), public endpoints, untrusted input, permission checks, secrets, injection, SSRF,
  path traversal, unsafe deserialization. Judge by **reachability**: can untrusted input
  actually arrive here? For a dedicated pass over credentials, dependencies and CI config
  rather than one diff, use `/viby-code:secure`.
- **edge-cases** — null/undefined/empty, boundaries, partial failure, resource cleanup.
- **data-state** — persistence correctness, migrations, transaction boundaries,
  caching/invalidation, state-mutation bugs. (Spawn `data-migration` focus only when
  migration artifacts are actually present.)
- **reliability** — error handling, retries, timeouts, circuit breakers, background
  jobs, health checks, idempotency.
- **api-contract** — routes, serializers/interfaces, event schemas, exported type
  signatures, versioning, backward compatibility for existing callers.
- **regression / previous-comments** — does this break existing callers or assumptions
  elsewhere? On a PR, re-check prior review threads.
- **performance** — only on hot paths: N+1, work in loops, blocking I/O, allocations. A
  performance *claim* in the diff needs a measurement: agent-authored performance PRs
  validate by static reasoning 67.2% of the time and report benchmarks only 25% of the time
  (vs 49% for humans), so treat an unmeasured "this is faster" as unverified. Route real work
  to `/viby-code:perf`.

**Reviewing a refactor?** Models judge refactorings by surface features — they show a
heuristic bias toward style and naming, and *semantic overgeneralization*: penalising a
legitimate refactor because it shortened the code, mistaking brevity for lost functionality
(PROMISE 2026). So for a behaviour-preserving diff, do not flag "this removed logic" from
shape alone — name the specific input whose behaviour changed, or drop the finding.

**adversarial** — the highest-value non-obvious dimension. Not pattern-matching; a chaos
engineer that *constructs* failure scenarios in the space between the other reviewers:
1. **Assumption violation** — data-shape / timing / ordering / value-range assumptions.
2. **Composition failure** — contract mismatches, shared-state races, cross-boundary
   ordering, divergent error contracts.
3. **Cascade construction** — multi-step failure chains: retry storms, state-corruption
   propagation, recovery-induced failures.
4. **Abuse cases** — repetition, timing, concurrent mutation, boundary-walking.
5. **Silent-pass / verification-fidelity** — when the diff *is* a guard (CI gate,
   coverage/lint check, test mock), attack the "goes green while production is red" mode.

Each dimension has a "what I do NOT flag" boundary (its lane) so findings don't duplicate
across reviewers and inflate the dedup load.

## Finding schema

Each candidate finding is an object:

```
{
  "dimension":   "correctness | security | adversarial | ...",
  "file":        "path",
  "line":        123,
  "title":       "one-sentence statement of the defect",
  "first_evidence": "the VERBATIM line(s) at file:line that motivate this finding",
  "failure_scenario": "concrete inputs/state → wrong output or crash",
  "severity":    "P0 | P1 | P2 | P3",
  "confidence":  0 | 25 | 50 | 75 | 100,
  "action_class":"gated_auto | manual | advisory",
  "owner":       "downstream-resolver | human | release",
  "pre_existing": true | false
}
```

- **action_class** routes the fix: `advisory` (quality/taste) → send to `/simplify`, not
  here; `gated_auto` (clear correctness fix) → apply after judgment; `manual` → needs a
  human decision. It's a routing hint, not an auto-apply gate.
- **pre_existing**: true if the issue was already in the code before this diff — report
  separately; don't block the current change on it.

## Confidence rubric (behavioural anchors — EVIDENCE-BACKED, not self-assessed)

`/viby-code:principles` says a self-reported confidence must never be the gate, and agents are
measurably overconfident (some succeeding 22% of the time predict 77%). So this number is not
an opinion the validator forms about itself — it is a **summary of what was established**, and
each anchor names the evidence that earns it. A validator that cannot point to the evidence for
an anchor must use the lower one.

- **0** — refuted: the code does not do what the finding claims.
- **25** — plausible but nothing was verified. No quoted line, or the quote does not support it.
- **50** — the quoted line exists and supports the claim, but the trigger or impact is unconfirmed.
- **75** — requires MORE than reading: the call sites were checked, or the guard's absence was
  confirmed, or the path was traced. A "double-checked" feeling does not reach 75 — name what
  was checked.
- **100** — reproduced or mechanically proven: a failing assertion, a type error, an executed
  repro. If it was checkable and you did not run it, it is not 100.

**Anything at 75 or above must be able to state the evidence in one clause.** If it cannot, it
is 50 at best. That is what keeps this a gate on evidence rather than on a number the validator
chose for itself.

**Gate:** suppress everything below **75** — EXCEPT a P0 (critical) at 50+ survives, so a
critical-but-uncertain issue is never silently dropped (it surfaces labeled "unconfirmed").

**Promotion:** when 2+ *independent* reviewers (fresh contexts) raise the same finding,
promote one anchor step (50→75→100). Agreement never bypasses the grounding gate — two
un-quoted findings cannot combine into a quoted-grade 75.
