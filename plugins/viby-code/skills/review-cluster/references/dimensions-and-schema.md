# Review dimensions, finding schema, and confidence rubric

Reference for the `review-cluster` skill. Loaded on demand — not part of the always-on
skill metadata.

## Dimensions (spawn by relevance to the diff, not all every time)

**Always-on (cheap, every review):**
- **correctness** — logic errors, wrong conditions, off-by-one, unhandled cases, broken
  control flow, incorrect API/contract usage, error propagation, intent compliance.
- **testing** — coverage gaps, weak/missing assertions, brittle tests, missing edge-case
  tests, tests that pass while the behavior is wrong.
- **maintainability** — structural quality only where it threatens correctness later:
  dangerous complexity, coupling, type-boundary leaks, dead code. (Pure taste →
  `/simplify`, not here.)

**Conditional (spawn when the diff touches the relevant surface):**
- **security** — auth/authz, public endpoints, untrusted input, permission checks,
  secrets, injection, SSRF, path traversal, unsafe deserialization.
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
- **performance** — only on hot paths: N+1, work in loops, blocking I/O, allocations.

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

## Confidence rubric (behavioral anchors, self-applied)

- **0** — false positive that doesn't survive light scrutiny.
- **25** — plausible but unverified; would need real digging.
- **50** — likely real; some uncertainty about trigger or impact.
- **75** — double-checked; will affect users/callers in normal usage.
- **100** — verifiable from the code itself: compile/type error or definitive logic bug.

**Gate:** suppress everything below **75** — EXCEPT a P0 (critical) at 50+ survives, so a
critical-but-uncertain issue is never silently dropped (it surfaces labeled "unconfirmed").

**Promotion:** when 2+ *independent* reviewers (fresh contexts) raise the same finding,
promote one anchor step (50→75→100). Agreement never bypasses the grounding gate — two
un-quoted findings cannot combine into a quoted-grade 75.
