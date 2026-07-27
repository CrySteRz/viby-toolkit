# Changelog

## 1.0.0

First tagged release. Everything before this shipped untagged from `main`.

`viby-code` is an accuracy-first Claude Code plugin: 21 skills covering the development
lifecycle, 5 subagents, and 5 executable checkers, in TypeScript with zero runtime
dependencies and no build step.

### What 1.0.0 means here, precisely

- **Verified:** every executable checker has a contract test pinning both halves — what it
  must flag and what it must not. 140 cases. `npm run check` runs 12 gates: manifests, all
  suites, typecheck under `strict` + `erasableSyntaxOnly` + `noUncheckedIndexedAccess`, the
  scanner's self-audit, cross-reference reachability, SessionStart JSON validity, and a
  runtime-shim probe. CI runs the same on every push.
- **Not verified:** live routing behaviour. Nothing here proves the 21 skills load, trigger on
  the prompts they claim, and don't mis-route in a real session. `check-skills.ts` measures
  description similarity as a *proxy* for that; a proxy is not the thing. Treat 1.0.0 as "the
  code and the contracts are verified", not "the routing is proven".

### Skills

- **Design & orientation** — `explore` (map an unfamiliar codebase, stack detected
  mechanically), `brainstorm` (decide WHAT), `plan` (decide HOW), `api` (contract-first design).
- **Build** — `orchestrate` (scope → research → plan → implement → verify → self-review),
  `refactor` (behaviour-preserving, proven so), `migrate` (wide mechanical sweeps),
  `schema` (database changes — the one class that cannot be undone).
- **Prove** — `verify` (the evidence gate as a procedure), `test` (QA and test design),
  `review-cluster` (parallel reviewers → grounding gate → adversarial validator → confidence
  gate), `secure` (credentials first, then supply chain, then code).
- **Operate** — `debug` (root-cause, repro-test-first), `incident` (mitigate before diagnose —
  deliberately inverts `debug`), `perf` (measure or it didn't happen), `observe`
  (instrument for the person reading it at 3am), `release` (the version number is a promise).
- **Compound** — `learn` (durable lessons), `handoff` (ephemeral state), `worktrees`
  (isolation), `principles` (the shared contract).

### Executable checkers

| Script | Catches |
|---|---|
| `scan-test-quality.ts` | no-assertion tests, tautologies, over-mocking, focused/skipped left in, timer waits, swallowed errors — with cross-file delegation resolved |
| `detect-stack.ts` | languages, package manager, monorepo tool, real build/test/bench commands ranked CI > task-runner > convention, installed profilers |
| `check-release.ts` | version drift across manifests, dirty tree, unpushed commits, tag collisions, stale changelog, debug artifacts |
| `check-migration.ts` | the short list of migration operations behind most outages, each with its safe alternative |
| `check-skills.ts` | skill shadowing, duplicate trigger phrases, instruction-budget overruns |

All five report `unknown` rather than guessing, and exit `0` clean / `1` findings /
`2` nothing-to-check.

### Design decisions worth knowing

- **Precision over coverage.** `assertion-roulette` and magic-number checks were built,
  measured against real suites, and deleted — they fired ~6×/file on idiomatic code. A checker
  that cries wolf gets switched off, and then it protects nothing.
- **Decide on parsed code, never raw text.** Four separate defects here came from matching
  text: fixtures, comments and regex patterns that merely *mentioned* what a checker hunts.
  The blanking pass is now shared in `lib/strip-noncode.ts`.
- **No command-blocking hook.** An earlier version vetoed destructive Bash commands. It was
  removed in 0.7.0: it blocked routine work — including a `git commit` whose only sin was
  containing the text `rm -rf` — and a veto you argue with costs more than it protects.
- **Research is cited only where verified.** Several claimed figures were fetched, found
  unverifiable, and deliberately discarded. The README's Provenance section says which.

### Known limitations

- Live skill routing is unvalidated (above).
- No mechanical check catches a skill body that contradicts its own Iron Law; one such
  contradiction was found by hand, and a second by review.
- `swallowed-error` is the least trustworthy check — 0 of 12 sampled were real on CPython
  before its exclusions were added. It is P2 and phrased as a question.
- Cross-file delegation cannot follow a dynamic import or a base class installed at runtime.
- JSX text nodes are not a string context, so prose inside `<div>…</div>` can still be flagged.
