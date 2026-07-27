# Changelog

## 2.1.0

The four patterns from the source spikes that 1.1.0 left on the table — audited and named
after the fact, because "did you take *all* the best parts" turned out to have the answer
"no, four are missing", and three of them belong in skills other than `evaluate`.

- **`principles` §9 — authored vs derived artifacts.** The strongest idea in the longer
  spike, generalised: authored artifacts are the *why* (decisions, lessons, contracts —
  reviewed, durable, the source of truth), derived are the *what* (maps, indexes, scan
  output, caches — rebuilt by a command, disposable, never ground truth). Keep them layered
  and joined by stable reference, never by copying. Stamp derived output with the commit it
  was built from; when derived disagrees with its source, regenerate rather than hand-patch,
  because patching converts it into an authored file no command can reproduce. A heuristic
  derived artifact is a planning aid, never the proof in an evidence-gated claim.
  (Portability & secrets moves to §10; no other section renumbered.)
- **`worktrees` — where the generated stuff lives.** Every worktree starts with nothing
  gitignored inherited, so each one either rebuilds the index/cache/venv (minutes × N) or
  shares it (races). Now an explicit options table: one canonical read-only copy pinned to
  mainline (default) · per-worktree (only for a long-lived branch) · symlinked into every
  worktree (❌ — concurrent writers race, and it only works if the artifact stores relative
  paths, which is usually undocumented). Two rules follow: shared *writes* to a cache are
  parallel writes and the fan-out law applies; and name the mechanical assumption you have
  not verified, because that class of assumption fails silently and looks like a tool bug.
- **`evaluate` — difference-only matrices, and the lifecycle-split routing table.** A
  capability matrix with a column of ✅ all the way down teaches nothing and hides the two
  rows that decide it: state the shared baseline in a sentence, table only the differences.
  And the routing rule now has the shape that made it land — by lifecycle stage (inner loop
  → cheap and fast, deep investigation → heavy and accurate, committed in CI → boring and
  stable), because the loser overall often wins one row outright.
- **`evaluate` — a status line at the top of the decision record.** What was actually done,
  before anything is read: candidates tried, how many were measured versus rejected on a
  stated bar, tasks checked against ground truth, and how many of your own earlier claims
  the run refuted. The same audit trail `review-cluster` ends with. A document that cannot
  fill it honestly is a survey, and should say so instead of dressing itself up.
- **`explore`** stamps its map as derived; **`learn`** records the authored layer only — a
  *why* stays true and is expensive to re-derive, a *what* is regenerable in seconds and goes
  stale silently, which makes recording it actively harmful.

`principles` hit 38 directives against its own redesign threshold of 40, so §9's definitions
were rewritten as prose (a definition is not an instruction) and its rules merged — 34 now,
with nothing dropped. The checker caught its author again.

## 2.0.0

**Breaking: the plugin is renamed `viby-code` → `viby-toolkit`.** Every skill is now invoked
`/viby-toolkit:<name>`, and the installed plugin id is `viby-toolkit@viby-toolkit`. Nothing
else changed — no skill was added, removed or rewritten in this release. Major, not minor,
because renaming every entry point is the definition of a breaking public-surface change,
and this repo's own `release` skill says the version number is a promise about that surface
rather than about the size of the diff.

**To update:** `/plugin update viby-toolkit`. The old `viby-code@viby-toolkit` entry no
longer exists in the marketplace, so `~/.claude/settings.json` needs its `enabledPlugins`
key changed to `"viby-toolkit@viby-toolkit"` — done on this machine already. Any project
`CLAUDE.md` or note that says `/viby-code:` must be updated by hand; the old prefix resolves
to nothing rather than failing loudly.

## 1.1.0

Adds the decision class the library was missing — **should we adopt this at all?** — taken
from two in-house tooling spikes (a code-graph benchmark, a browser-automation decision),
read in full. Method only: neither is published research, both were run against a private
codebase, and no code, names or figures from either appear here. See the README's Provenance
section for what each contributed and why.

### New

- **`/viby-toolkit:evaluate`** — choosing a tool, library, dependency or MCP server before
  adopting it. Iron Law: *rank a candidate only on a case whose correct answer you
  established first; a cost number with no correctness verdict beside it is not a result.*
  Establish the oracle and price the baseline before installing anything; name the
  operational criteria before looking at candidates; trial in isolation with pinned versions
  and the uninstall command written down first; measure cost **and** correctness in one
  table; label every cell measured / inferred / not-tested; rank twice when the weightings
  disagree and show both; state the winner's failure case with its number and route around
  it; check the rule generalises on a second unrelated case; record the rejections with the
  bar each failed and what the measurement proved wrong about your own research.
- **`measure-read-cost.ts`** — prices a read set: what grep-and-read actually costs, which
  files dominate it, `--repeat N` for cadence, `--budget N` as a gate (exit 1 over budget,
  naming the alternative), `--json`. Excludes `node_modules`/`dist`/vendored trees and
  attributes lockfile content separately, because a ratio computed against an inflated
  denominator is a flattering fiction. Skips binaries with a reason rather than silently.
  18 contract cases; 13 gates now.
- **`explore`** uses it to decide *read it myself vs send a scout* before reading a
  subsystem — a call previously made by feel, and a directory's cost is not visible from its
  file count.

### Changed

- **`principles` §1** — *a fast, cheap, wrong answer is worse than the slow one it
  replaced*, because it gets believed and kept while the slow method got checked. With the
  rule that follows: rank a shortcut on a case whose answer you established first.
- **`principles` §2** — cost is **payload × cadence**. A 15k payload re-sent every step of
  a six-step flow is 90k and loses to a 40k one-shot read; measure the flow you will run.
- **`principles` §5** — label every claim **measured / inferred / not tested** at the point
  it appears, not in a closing caveat: unlabeled inferences get promoted to results by your
  own summary a few paragraphs later.
- **`plan`** — record the rejected approaches, each with the bar it failed. Previously only
  the chosen approach survived, which reads as if one idea was ever considered.

### Honest limits of the read-cost meter

It is an **estimate, not a tokenizer** — characters divided by a per-kind ratio, non-ASCII
charged at ~1 token/char, roughly ±15% on ordinary source and worse on minified or
non-Latin content, both of which it flags per file. Fit for "is this 7k or 87k", "does this
fit the remaining budget", "is that savings claim real". Not fit for a precise published
number; the tool prints its own error bar on every run, because a cheap estimate that hides
its uncertainty is exactly the fast-and-wrong answer `evaluate` exists to prevent.

## 1.0.0

First tagged release. Everything before this shipped untagged from `main`.

`viby-toolkit` is an accuracy-first Claude Code plugin: 21 skills covering the development
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
