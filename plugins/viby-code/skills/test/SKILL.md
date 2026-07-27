---
name: test
description: >
  Use when writing, designing, auditing, or repairing tests — unit, integration, e2e,
  contract, or property-based — and for QA strategy questions like "what should we test",
  "is this well tested", "add tests for this", "why didn't the tests catch this", "these
  tests are flaky", "improve our coverage". Also the test-authoring half of
  /viby-code:orchestrate. For *running* checks to prove a change works, use
  /viby-code:verify instead.
---

# Test (QA, by design)

```
IRON LAW: A test that cannot fail is worse than no test — it buys false confidence.
          Before you trust a new test, SEE IT FAIL for the right reason.
          Coverage says a line ran. Only a failing test says a bug would be caught.
```

Where the neighbours stop and this starts:

- `test` = **author, design, and audit the tests themselves** (this skill).
- `/viby-code:verify` = run the checks and prove *this change* works.
- `/viby-code:debug` = a bug exists; the reproduction test comes first.
- `/viby-code:review-cluster` = review someone else's tests inside a diff.

Follow `/viby-code:principles`. Test authoring is judgment work and a **write** — keep it on
the main thread. Auditing an existing suite is read-only, so fan that out.

## 1. Pick the level before writing anything

Push every test as far down as it can go while still proving the thing you care about. A
bug caught by a unit test costs seconds; the same bug caught by an e2e suite costs minutes
and a flake budget.

| Level | Proves | Use when |
|---|---|---|
| **unit** | one function/module's logic, all branches | pure logic, edge cases, error paths — the default |
| **integration** | two or more real components agree | persistence, queries, serialization, wiring |
| **contract** | a boundary's shape holds for its consumers | public APIs, events, exported types, cross-service |
| **e2e / smoke** | the critical user path works end to end | a handful of revenue/auth-critical flows only |
| **property / metamorphic** | an invariant holds across generated inputs | the expected output is hard to state but a *relation* is easy |

Test **observable behaviour at a boundary**, not internal structure. A test that asserts on
private fields, call order, or intermediate state fails on every refactor while catching
nothing — that's a change-detector, not a test.

**Property-based and metamorphic testing** are the escape hatch when you can't state the
expected output. You often still know a relation that must hold: sorting twice equals
sorting once, decode(encode(x)) == x, adding an item never decreases the total, a discount
never raises the price. Generate inputs, assert the relation. This finds edge cases no
hand-written example would.

## 2. Decide what actually deserves a test

**Test:** every branch and error path; boundaries (empty, zero, one, max, negative, null,
unicode, duplicate); invariants; the contract at each public boundary; **every fixed bug**
(a regression test is the cheapest test you will ever write, and the bug already told you
the input).

**Don't test:** framework internals, third-party library behaviour, trivial accessors,
generated code, or your own mocks. Tests that restate the implementation cost maintenance
and prove nothing.

## 3. Prove the test can fail — red before green

This is the single highest-value habit in this skill, and the one most often skipped:

1. Write the test.
2. **Make it fail** — run it against unimplemented or deliberately broken behaviour.
3. **Read the failure message.** If it wouldn't tell a stranger what broke, fix the message
   now, while you know the answer.
4. Only then make it pass.

For a test added to *existing* passing code, invert the check: change the code under test
so the behaviour is wrong, confirm this test goes red, then restore. A test never observed
failing is not known to test anything. This is mutation testing done by hand, and it is
where mutation-guided research consistently finds the fault-detection gains — a suite can
have high coverage while surviving nearly every mutation, meaning it would catch nearly
nothing.

**The weakest-test question.** After writing a test, ask: *what is the smallest change to
the production code that leaves this test green but the behaviour wrong?* If you can name
one, the test is too weak — tighten the assertion until you can't.

## 4. Mocking discipline

Coding agents over-mock measurably more than humans do — one 2026 study of real
repositories found agent test commits added mocks 36% of the time versus 26% for
non-agent commits, and agents used a single test-double type (`mock`) in 95% of cases while
humans varied across mocks, fakes, and spies. Over-mocking is seductive because a mocked
test is trivially easy to generate and always green. It is also how a suite passes while
production is broken: **a mock only holds while it matches the real implementation, and the
real implementation changes.**

- **Mock only what you must:** network calls, third-party services, real payments/emails,
  the clock, randomness, and genuinely slow or destructive operations.
- **Never mock the thing under test.** If the test only exercises mocks, it tests nothing.
- **Prefer, in order:** the real object → an in-memory or local substitute (SQLite,
  testcontainer, temp dir, local fake server) → a hand-written fake → a mock.
- **Use the right double:** a *stub* returns canned data, a *fake* is a working lightweight
  implementation, a *spy* records calls, a *mock* asserts on interactions. Reaching for a
  mock every time is the smell.
- **Assert on outcomes, not call transcripts.** `expect(gateway.charge).toHaveBeenCalled()`
  passes even when nothing was charged. Assert the resulting state.
- **A mocked boundary needs a contract or integration test somewhere** that exercises the
  real thing, or the drift is invisible until production.

## 5. Assertions that diagnose

- Assert the **exact expected value**, not merely "not null" or "truthy".
- **One logical concern per test.** Name the test after the behaviour, not the method:
  `rejects_expired_token`, not `test_validate_2`.
- **Give assertions a message** when a bare failure wouldn't be self-explanatory. Four or
  more unexplained assertions in one test is *assertion roulette* — the top smell found in
  studies of LLM-generated tests: when it fails, nobody can tell which condition broke.
- **No unexplained magic numbers.** `assert total == 4550` teaches nothing; bind it to a
  named constant or derive it visibly from the inputs.
- Never assert something that cannot fail (`expect(true).toBe(true)`, `assert x == x`).

## 6. Determinism — flakes are bugs in the test

A flaky test is worse than a missing one: it trains the team to re-run until green, which
is the same as deleting the suite.

- **Never wait on a timer.** Poll for an observable condition or await the event. A
  `sleep(2)` is a bug that hasn't fired yet.
- Freeze the clock; seed the randomness; never depend on real network, real time-of-day,
  locale, timezone, or a fixed port.
- No shared mutable state between tests and no ordering dependence — each test sets up and
  tears down its own world. Unique temp dirs, fresh fixtures, isolated DB per test.
- Don't swallow exceptions in a test; the exception is often the thing under test.
- **A flaky test gets root-caused (`/viby-code:debug`) or quarantined with an owner — never
  blanket-retried.** `retry: 3` converts a real intermittent bug into invisible tech debt.

## 7. Audit a suite you didn't write

```bash
SCAN="${CLAUDE_PLUGIN_ROOT}/skills/test/scripts/scan-test-quality.ts"
sh "${CLAUDE_PLUGIN_ROOT}/hooks/run.sh" "$SCAN"            # test files changed vs HEAD
sh "${CLAUDE_PLUGIN_ROOT}/hooks/run.sh" "$SCAN" --all      # every test file in the repo
sh "${CLAUDE_PLUGIN_ROOT}/hooks/run.sh" "$SCAN" --json     # machine-readable
```

`run.sh` picks whatever TypeScript runtime the machine has (node ≥22.6, else bun, else
`tsx`) and no-ops silently if there is none, so this is safe to invoke anywhere.

Flags no-assertion tests, tautologies, assertion roulette, over-mocking, `.only`/`.skip`
left in, sleep-based waits, and swallowed errors — with `file:line`. Exit 1 on findings, so
it can gate. Every finding is a text heuristic: **confirm against the code before changing
anything**, exactly as the review cluster's grounding gate requires.

Then judge the suite on signal, not size:

- **Coverage is a floor, not a goal.** It tells you a line executed, not that a wrong value
  would be caught. 100% coverage with no assertions catches nothing. Use it to find
  *untouched* code, then ask what a mutation of that code would do.
- **Mutation score is the real measure** where tooling exists (`mutmut`, `cosmic-ray`,
  Stryker, PIT, `cargo-mutants`). Surviving mutants are precisely your blind spots — and
  feeding surviving mutants back as explicit test targets is what recent research finds
  most effective. Where no tooling exists, do §3 by hand on the riskiest function.
- **Check the suite actually runs.** Zero collected, all-skipped, and excluded directories
  all report green (see `/viby-code:verify` §4).

## 8. Report

- What you tested, at which level, and **why that level**.
- The **red-then-green evidence**: that each new test was seen failing for the right reason.
- Findings from the audit, grounded in `file:line`.
- What you deliberately did **not** test, and why (this is a real answer, not a gap).
- Anything that needs a contract/integration test because a boundary got mocked.

When a testing gotcha turns out to be project-specific — a fixture that must be ordered, a
suite that needs a service running, a flake with a known cause — record it with
`/viby-code:learn` so the next session doesn't rediscover it.
