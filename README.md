# viby-toolkit

My personal Claude Code toolkit, distributed as a public plugin marketplace. One repo,
installed once per machine at **user scope**, so it applies automatically to every
project — work and personal — and travels with me to any new computer.

It is public so it installs with no credentials and auto-updates without a token; it is still
built around how *I* work, so treat it as a working reference rather than a product. Nothing
here phones home, and no hook intercepts or blocks a command.

Marketplace: **`viby-toolkit`** · Plugin: **`viby-toolkit`**

---

## What's in it

`viby-toolkit` is an accuracy-first, token-disciplined set of engineering workflows. It's
stack-agnostic (detects the project at runtime; assumes nothing). Everything executable is
one Rust binary, `viby`, which the plugin fetches for itself — see
[How the executable half runs](#how-the-executable-half-runs).

### Skills (auto-trigger by context, or call with `/viby-toolkit:<name>`)

| Skill | What it does |
|---|---|
| `/viby-toolkit:brainstorm` | **Design-before-code gate.** Decides WHAT to build (and whether it's the right thing) with an Iron-Law hold on any implementation until you approve the design. Runs before plan/orchestrate for anything whose shape isn't settled. |
| `/viby-toolkit:orchestrate` | Drives a task end-to-end: scope → research → plan → implement → verify → self-review. Ships `viby check-diff-hygiene`, which audits the artifact the build phase produces: conflict markers and credentials being committed, debug prints and formatting churn mixed into a behavioural change, new TODOs and commented-out code — and **size**, because the largest study of code review found detection is best at 200–400 changed lines and falls to ~28% past 1,000, so a 2,000-line diff is not a bigger review but an unreviewed one. Fans out cheap scouts for discovery, keeps writes single-threaded, keeps main context clean. |
| `/viby-toolkit:review` | **Review cluster + false-positive filter.** Parallel per-dimension reviewers (incl. an adversarial chaos-engineer dimension) find candidates; a grounding gate drops anything that can't quote its own line; one fresh-context validator per finding confirms real/introduced/not-already-handled; a confidence gate suppresses below-threshold. Reports the full kill count. |
| `/viby-toolkit:explore` | **Understand an unfamiliar codebase.** Detects the stack mechanically first (languages, package manager, monorepo tool, real build/test commands with their source), then fans out scouts on specific questions, traces one path end to end, and writes a durable map. Scouts each language separately in a polyglot repo and names the cross-language seams. |
| `/viby-toolkit:secure` | **Security pass, ordered by what actually goes wrong.** Also audits the newest supply chain: agent skills themselves. Ships `viby check-skill-safety` — a credential path meeting a network call, `curl \| bash`, instructions to act without telling the user, instruction-override attempts, writes to the agent's own settings, encoded payloads, invisible bidi characters. An audit of 3,984 public skills found 36% with security flaws and prompt injection in 36%, so a skill is third-party code you execute with your own credentials. Credentials first (99.6% of critical findings in a 2026 study of 4,022 agent-assisted PRs), then supply chain and CI (82.3% by volume), then code surfaces judged by reachability. Confirms each candidate secret, because that study's own labelling was ~73% false positives. |
| `/viby-toolkit:verify` | **The evidence gate, executed.** Finds the project's real checks (CI config is authoritative), scopes them to the change, exercises the actual behavior — then screens the output for silent-pass modes, because a zero exit code with zero tests collected is not a pass. Fix the code, never the check. |
| `/viby-toolkit:test` | **QA and test design, with a scanner.** Picks the test level deliberately, insists every new test is *seen failing for the right reason* before it's trusted, and enforces mocking discipline (coding agents over-mock measurably more than humans). Ships an executable auditor — `viby scan-test-quality` finds no-assertion tests, tautologies, over-mocking, `.only`/`.skip` left in, sleep-waits and swallowed errors, with `file:line`. |
| `/viby-toolkit:perf` | **Measure, or it didn't happen.** Baseline → profile → one change → re-measure, with correctness as a gate. Exists because a 2026 study of 407 performance PRs found agents pick statistically indistinguishable optimisations to humans but validate them far less: 45.7% vs 63.6%, and 67.2% of validated agent PRs reasoned statically instead of benchmarking. The detector reports the repo's bench command and which profilers are actually installed. |
| `/viby-toolkit:refactor` | **Behaviour-preserving, and proven so.** Name the invariant, find what pins it, add characterization tests if nothing does, then transform in small verified steps. The same tests must pass before and after, unchanged. Never mixed with a behaviour change in one diff. |
| `/viby-toolkit:schema` | **The one change you cannot undo.** Every schema change must deploy while the OLD code still runs and be reversible without data loss — expand, migrate, contract, never rename in place. Ships `viby check-migration`: index without `CONCURRENTLY`, `NOT NULL` without a default, type changes, renames, unbounded `UPDATE`, constraints without `NOT VALID`, DDL mixed with a backfill, missing `lock_timeout`, no rollback. Every rule names the safe alternative. |
| `/viby-toolkit:incident` | **Stop the bleeding, then find out why.** Deliberately inverts `debug`: reversible mitigation *before* diagnosis, because users are losing service while you investigate — rollback, then flag, then shed load, then clear the blockage. Preserve evidence before mitigation destroys it. Never ship a speculative fix to production under pressure. Hands back to `debug` once service is restored. |
| `/viby-toolkit:debug` | Root-cause debugging by hypothesis and evidence — reproduce (as a failing test, routed to the strong model) → localize → confirm → fix → verify. No speculative patching. |
| `/viby-toolkit:migrate` | Wide mechanical changes (renames, upgrades, pattern sweeps): discover every site → transform in batches → verify each → final zero-remaining sweep. |
| `/viby-toolkit:plan` | Turns an agreed idea into an ordered, file-anchored change-list with the risky step and verification strategy called out, written as a **dispatchable task list** — each task naming the files it exclusively owns, how it is verified, and what it waits for. Ships `viby check-plan`, which makes the fan-out law's "name the partition" requirement mechanical: it fails when two tasks with no dependency between them own the same file, on dependency cycles, on files touched by 3+ tasks (structural hubs to take yourself), and on tasks with no files or no verification. The checkboxes double as the durable progress ledger. |
| `/viby-toolkit:release` | **The version number is a promise.** Decide major/minor/patch from the public-surface diff, never from the size of the change — a review of 97 studies found 67% of Maven artifacts violate SemVer, and that detection handles syntactic breaks well but behavioural ones poorly. Ships `viby check-release`: version drift across manifests, dirty tree, unpushed commits, tag collisions, stale changelog, debug artifacts left in. |
| `/viby-toolkit:observe` | **Instrument for the person reading it at 3am.** Ships `viby check-logging`, whose P1 is not a style question: personal data or credentials reaching the logs, and whole request/user objects being logged — a log line is the least access-controlled artifact a team produces. Plus the doctrine made mechanical: interpolated messages with no structured fields, a `catch` that logs without the caught error, identifier-shaped values used as metric labels, logs in tight loops, files with several events and no correlation key. CLI output and tests excluded. Log decisions and outcomes, never arrivals; structured fields, not sentences; correlation keys on every event. High cardinality is the point for logs and traces, and the cost trap for metric labels. Alert on symptoms, not causes — and verify the instrumentation by triggering the path and reading the real output. |
| `/viby-toolkit:api` | **Design the contract, because you cannot take it back.** Write the caller's code first, design inward from their use case rather than outward from the storage schema, and settle errors, pagination, idempotency and limits at design time — each is a breaking change if retrofitted. Expand-then-contract for evolution; diff the surface to decide if a change is breaking. Ships `viby check-api-surface`: added/removed/re-signatured exports between two git refs for TS/JS, Python, Go and Rust, honouring each language's own visibility rule — telling a positional parameter rename (P2) from a real signature break (P1), reporting every `export *` barrel it cannot follow rather than silently excluding it, and stating on every run that a behavioural break with an unchanged signature is invisible to it. |
| `/viby-toolkit:kpi` | **Define the number before anyone builds it.** Every KPI names the decision it serves, its exact formula at a stated grain/window/timezone, an owner, and **at least one guardrail** — because when a measure becomes a target it stops being a good measure, and if the target can improve while harm increases the definition is incomplete. Forces the ambiguous words (active, customer, churn, revenue) to be decided on paper with the client, because definition drift across tools is what destroys trust: each definition defensible alone, collectively nobody knows which dashboard is right. Then a dashboard of questions, each figure carrying a comparison. |
| `/viby-toolkit:analytics` | **A number is not done until it reconciles.** Implements a metric contract once, in the transformation layer, then proves it: unit tests on hand-built fixtures including the boundary/NULL/refund/duplicate rows, rules the data must obey, and a **reconciliation against an independent source** with the residual delta explained. Ships `viby check-analytics-sql` for the traps that return a plausible wrong number rather than an error — `BETWEEN` on time (closed at both ends, so boundary rows double-count), fan-out joins inflating `COUNT(*)`, division with no `NULLIF`, `DATE_TRUNC` without a timezone, `= NULL`, float money, `NOW()` in a definition, plain `UNION` de-duplicating. Plus freshness and volume checks, because a source that stops updating looks exactly like a quiet week. |
| `/viby-toolkit:docs` | **Write for one reader with one next action.** Ships `viby check-docs`: documented paths that don't exist, `npm run` scripts that aren't defined, dead relative links and dead anchors — the classic wrong-README, where the reader's first command fails. Resolves by suffix against the repo's real file index rather than guessing a base directory, and reports `root-unknown` instead of inventing staleness when nothing resolves. Picks the genre deliberately (README / how-to / reference / explanation / ADR / PR description / release notes) and refuses to mix them, runs every command it documents from a clean state, and documents the failure mode with the verbatim error — the most-read paragraph in any doc and usually the missing one. An ADR's value is entirely the alternatives rejected and the bar each failed. |
| `/viby-toolkit:deps` | **One at a time, changelog first, revert cheap.** Sorts the queue by *why* (reachable vulnerability → blocking → EOL → batched → never) rather than by how stale, reads the breaking changes of **every** major crossed rather than just the target, and treats a minor bump as evidence rather than proof — two thirds of artifacts in one large ecosystem study violate SemVer. Thirty bot PRs are not thirty tasks. A dependency you cannot upgrade is a recorded decision, not a failure. |
| `/viby-toolkit:ui` | **See it, drive it, or you have not checked it.** A claim about the interface needs an observation — a screenshot or a driven interaction — because the markup can be correct and the page still blank (build error, failed request, null guard, unmounted route), and none of that is in the diff. Picks the driver by lifecycle stage (inner loop → cheapest, investigation → full devtools, CI → real E2E), charges cost as payload × cadence, treats sending real user data to a third-party service as a hard no, and forces the states nobody builds first: empty, loading, error, too-much-data, two widths, keyboard-only. |
| `/viby-toolkit:extend` | **Adding to this toolkit without degrading it.** The meta-skill: earn the slot (a new skill needs a decision the others get wrong, not a new topic label — libraries degrade by overlap, not size), write the description as real user phrasings with mutual cross-references, never state the exclusion in the excluded case's own words, then prove it routes with a probe row and prove it doesn't shadow — expecting those two gates to fight each other. Plus the checker rules: parsed code not raw text, precision over coverage, `unknown` over guessing, both halves in every contract test, and run it on a repo that isn't this one. |
| `/viby-toolkit:adopt` | **Inherit foreign code, conform it, prove it still works.** Provenance gate first (licence, secrets in history, runtime behaviour) → capture behaviour with characterization tests *before* editing, and diff against the still-runnable original → a functionality matrix with a **held-out slice the agents never see** → Mikado steps, reverting on a blocked prerequisite so the graph becomes the partition parallel writes require → conform to instructions first, the language's own idiom second, project convention third. Built on measurement: the best model manages 41.58% on real behaviour-preserving refactorings, and monitoring for shortcuts cut hacked-resolved runs from 28.57% to 0.56% while raising clean resolved from 40.22% to 60.53%. Ships `viby check-test-drift`: deleted tests, removed assertions, new skips, `.only`, or a `sys.exit(0)` in the suite — the ways a green run gets faked. |
| `/viby-toolkit:study` | **Turn an idea into a study, protocol first.** You bring the question; it returns the protocol — competing answers, the observation that would exclude one, the search angles, the stopping rule, and what would change its mind — for approval *before* searching, then the document. Built on preregistration (a protocol is a promise to be transparent about deviating, not to obey), PICOC, strong inference, snowballing with a named stopping rule, AACODS source appraisal and GRADE's indirectness. Iron Law: every claim carries the quoted sentence that supports it, because deep-research citations keep links valid >94% and on-topic >80% while only 39–77% actually support the claim. Ships `viby check-study`: unsourced figures, hedges inside measured sections, missing falsifier or stopping rule, single-domain sourcing, undated citations. |
| `/viby-toolkit:evaluate` | **Decide what to adopt, by measuring it on a case you already know the answer to.** Establish the ground truth *before* installing anything, price the baseline you'd be replacing, then put cost and correctness in one table — because a cheap wrong answer beats an expensive right one on every column except the one that matters. Ships `viby measure-read-cost`: what a read set costs, `--repeat` for cadence (payload × frequency), `--budget` to gate it. Records the winner's failure case, the rejections with the bar each failed, and the back-out commands. |
| `/viby-toolkit:brain` | **The memory has to be maintained, or it starts lying.** Owns the architecture: episodic (`handoff`) / semantic (`learn`) / procedural (**the skills themselves**) — and most things people want to "remember" are episodic, which is why stores fill with things that used to be true. Every entry carries how it was established, when, and whether it worked: provenance is the documented defence against memory poisoning, and storing what the user *said* as fact is a benchmarked failure mode. Findability beats richness — across four memory benchmarks, answer errors concentrate in **retrieval** failure, with retrieval-succeeded-but-wrong only 5.8–13.7%. Ships `viby check-memory`: stale path references, undated or unsourced entries, hearsay, entries missing from the index, near-duplicates, entries grown into documents. |
| `/viby-toolkit:learn` | Records a reusable lesson (gotcha, build quirk, rejected finding, known past risk, "never compact X") to Claude's native project memory — the compounding loop, both suppressing false positives and raising recall on known risks. |
| `/viby-toolkit:handoff` | Serializes live task state (goal, decisions, next step) so a fresh session resumes mid-task without re-deriving it. Ephemeral, distinct from `learn`. |
| `/viby-toolkit:worktrees` | Isolates work (parallel implementers, risky experiments) — detect existing isolation first, prefer the native worktree tool, never fight the harness. |
| `/viby-toolkit:principles` | The operating contract everything follows: accuracy rules, the fan-out law, model-routing + escalation ladder, context discipline, the evidence gate. Read-only reference. |

### Commands

- `/viby-toolkit:ship <task>` — run the whole pipeline autonomously and don't stop until verified.
- `/viby-toolkit:browser [page]` — connect the `browser` tool to your own browser, guided, then prove it on a page.

### Browser control (MCP tool `browser`)

Drives your real Brave, Chrome, Edge or Firefox — your logins, your tabs — on macOS, Linux and
Windows, through a small extension and a native host. A port of JCode's bridge
(`plugins/viby-toolkit/browser/NOTICE.md`); no Playwright, nothing headless. First use is guided:
the first call opens your default browser on its extensions page with the folder to pick ready
(selected in Finder on macOS, on the clipboard elsewhere) and shows four steps in that browser's own
words (Developer mode, Load unpacked, pick the folder, Select); with no supported browser it opens
Brave's download page. Each
Claude session works in its own background tab (or its own window, or a private one). Snapshot,
click, type, forms and rich editors, uploads and drag-and-drop, screenshots, eval, reload; `batch`,
`try_until`, `fork` and `parallel` do several steps or pages in one call. Slow pages come back
usable instead of failing, a closed tab is replaced, and the extension updates itself. Passwords
are masked in what Claude reads and never typed by it.
Reference: `skills/ui/references/browser.md`.

### Code search (MCP tool `agentgrep`)

Wraps [1jehuang/agentgrep](https://github.com/1jehuang/agentgrep) (MIT), the structure-aware search
JCode uses: `grep` (matches grouped under their enclosing symbols), `find` (files ranked for a
topic), `outline` (a file's symbols with line ranges) and `trace` (follow a symbol, e.g.
`subject:parseConfig relation:called_from`, and get the regions in full). Like JCode, the session
remembers what it was shown and hands that to agentgrep, so a repeated `trace` comes back shortened
(about a third smaller, measured) until the file changes; `force: true` gets everything in full.
It is linked into `viby` (the `agentgrep` crate at v0.1.7), so there is nothing to install. It understands Rust, TypeScript/JavaScript and Python only;
on other languages it is plain grep. Measured against Grep/Read it did not save tokens, so nothing
routes agents to it yet: `docs/research/2026-10-19-agentgrep-measurement.md`.

### Agents (dispatched by the skills; cheap models by design)

`scout` (sonnet at low effort, read-only recon) · `researcher` (sonnet, one web-research angle) ·
`implementer` (sonnet) · `reviewer` (sonnet, one per review dimension) · `skeptic` (sonnet,
adversarial false-positive filter) · `debugger` (sonnet, evidence gathering) · `worker` (the
general `Workflow` stage agent: the stage's own model and effort, minus six tools workflow agents
never call, no turn cap) · `browser` (sonnet, owns one browser task in one tab and returns the trace —
JCode's handoff). Each is colour-coded in the transcript so a parallel fan-out is readable
at a glance.

### Hooks

One `SessionStart` hook injects the working-style defaults. Nothing intercepts or blocks
commands. See **Hooks** below.

---

## The token / rate-limit strategy (without losing accuracy)

Designed for a Claude Max subscription, where the scarce resources are the **main
thread's context window** and your **rate-limit budget** — not dollars.

1. **The fan-out law.** *Fan out for READ; keep WRITES single-threaded — unless you have
   mapped the dependency graph.* Parallel read-only subagents (search/explore/review) are a
   genuine win: they isolate verbose output and improve quality. Naive parallel *writing* is a
   trap — agents make conflicting decisions from partial context. But the trap is the naive
   partition, not parallelism itself: dependency-aware partitioning that isolates hub files
   measurably beat sequential on real repositories (see Provenance), so the operative rule is
   "don't parallelise writes across a boundary you haven't mapped". On Max every fan-out also
   burns rate-limit budget ~an order of magnitude faster, so each one is still gated on being
   genuinely parallel.
2. **Subagents are context firewalls.** Bulk reading (grep 40 files, read 10) happens in
   disposable subagents that return a ~200-token conclusion. The 30k tokens of file dumps
   die with the subagent and never touch main context.
3. **Model routing — the full lineup, by need.** `haiku` (4.5) for mechanical search;
   `sonnet` (5) for read-only reviewing/scouting fan-out; `opus` (4.8) for planning,
   synthesis, judgment, and all writes; `fable` (5), the most capable tier, reserved for
   the hardest, highest-stakes calls (authoring a repro test, resolving conflicting
   verdicts, subtle security/concurrency) and the top of the escalation ladder. Cheap
   models *find*; the strong model *decides*; escalate haiku → sonnet → opus → fable on low
   confidence. Fable is a scalpel (heaviest on rate-limit), not a default.
4. **Frequent intentional compaction.** Target 40–60% context utilization. Research and
   plan become durable markdown artifacts; the plan doubles as a checkpoint so a `/clear`
   loses no state. Context quality priority: Correctness > Completeness > Size.
5. **Evidence-gated completion.** Never claim done without running the check fresh and
   showing its output *and exit code*. The words "should / probably / seems" are the tell
   that you skipped verification — and a zero exit code is not a pass if zero tests ran,
   everything skipped, or the check was neutered by `|| true`. `/viby-toolkit:verify` runs this
   as a procedure rather than leaving it as an aspiration.
6. **Adversarial verification** keeps accuracy high while most tokens are spent cheaply —
   many cheap voices get cross-checked, so a single cheap voice being wrong doesn't sink
   the result.
7. **Compounding.** Each solved problem and each rejected review finding is recorded to
   native memory (`/viby-toolkit:learn`), so the next session is cheaper and the reviewer's taste
   drifts toward yours.

Full contract: `/viby-toolkit:principles`.

### Provenance

The workflows distill what's actually working in production agentic coding as of mid-2026,
keeping the mechanisms and discarding the marketing multipliers: Anthropic's
context-engineering, multi-agent-research, and Claude Code best-practices docs; humanlayer's
Advanced Context Engineering (frequent intentional compaction); obra/superpowers (the
Iron-Law skill format, trigger-only descriptions, TDD/verification/systematic-debugging);
Cognition (the read-vs-write fan-out rule); and Every's compound engineering (the learning
loop, multi-persona review, grounded findings schema).

The v0.3.0 review pipeline and reliability upgrades additionally draw on 2026 research:
the quote-the-line grounding gate and single-fresh-validator-over-panel design
(Refute-or-Promote; "Nine Judges, Two Effective Votes"); reproduction-test-as-the-bottleneck
(TDFlow and cogeneration-of-repro-test papers); the context-ledger idea (VISTA); failure-
driven "never-compact" lessons (ACON); and the escalation-ladder / cheap-model-danger-zones
model-routing guidance. Overstated single-number claims were deliberately dropped after an
adversarial fact-check; only convergent, credible mechanisms were kept.

The v0.5.0 testing module (`/viby-toolkit:test`) is grounded in four verified sources, each read
rather than taken from a summary:

- **Over-mocking is an agent-specific failure mode.** *Are Coding Agents Generating
  Over-Mocked Tests?* ([arXiv 2602.00409](https://arxiv.org/abs/2602.00409)) measured real
  repositories: agent test commits add mocks 36% of the time vs 26% for non-agent commits,
  and agents use the `mock` double in 95% of cases where humans spread across mocks (91%),
  fakes (57%) and spies (51%). The paper's own recommendation is to *"include guidance on
  mocking best practices and anti-patterns in agent configuration files"* — which is exactly
  what the skill's mocking section is.
- **Mutation beats coverage as the quality signal.** *Test vs Mutant: Adversarial LLM Agents
  for Robust Unit Test Generation* ([arXiv 2602.08146](https://arxiv.org/abs/2602.08146))
  runs a test-generator against a mutant-generator in an adversarial loop, reporting +8.56%
  fault detection over LLM baselines and +63.30% over EvoSuite on Defects4J; Meta's ACH
  deployed mutation-guided generation at scale with 73% of generated tests accepted. The
  skill folds in the cheap manual form — deliberately break the code, confirm the test goes
  red — plus the "weakest-test question", since most repos have no mutation tooling.
- **Named smells to hunt.** *Test smells in LLM-Generated Unit Tests*
  ([arXiv 2410.10628](https://arxiv.org/abs/2410.10628)) finds Assertion Roulette and Magic
  Number Test most prevalent across 20,505 generated suites. Assertion roulette became a
  scanner check; magic numbers stayed guidance only, because as a mechanical check it fires
  constantly on legitimate table-driven tests — precision over coverage, as everywhere else
  here.
- **Ship executable checks, not just prose.** *Harness Engineering for Agentic AI Coding
  Tools* ([arXiv 2602.14690](https://arxiv.org/abs/2602.14690)), a study of 2,853
  repositories, finds that where Skills are used at all they "typically rely on static
  instructions rather than executable scripts." That's the gap `viby scan-test-quality` and
  `tests/` close: guidance an agent can *run*, not only read.

The v0.8.0 additions:

- **Language-agnosticism made executable.** Every skill said "find the project's real
  commands"; nothing helped it do so. `viby detect-stack` now does, for ~30 languages and 16
  package managers, ranking **CI config above task-runner above convention** and printing
  `unknown` rather than inventing a command. Reporting on polyglot repos is a live weak spot
  for coding agents — cross-language dependency tracking is still an open problem — so the
  detector flags a polyglot repo explicitly instead of implying one test command covers it.
- **Security ordered by measured impact** ([arXiv 2607.12428](https://arxiv.org/abs/2607.12428),
  16,112 file changes across 4,022 agent-assisted PRs): hard-coded credentials were **99.6%
  of critical-severity findings**, **81.1% of leaked credentials reached integration
  undetected** by bots and humans alike, supply-chain/CI misconfiguration was **82.3% by
  volume**, and **67.6% of genuine leaks came from the human collaborator, not the agent** —
  so `/viby-toolkit:secure` checks the whole change, credentials first. The same paper's
  automated labelling had only a **27.2% validation rate**, which is why confirming each
  candidate is in that skill's Iron Law rather than a footnote.

The v0.9.0 additions:

- **Performance: the gap is measurement, not knowledge.** A study of 407 performance PRs
  ([arXiv 2512.21757](https://arxiv.org/abs/2512.21757) — 324 agent-authored, 83
  human-authored) found agents and humans choose *statistically indistinguishable*
  optimisations (χ²=6.10, p=0.636), but agents validate far less often: explicit performance
  validation in **45.7% vs 63.6%** of PRs (p=0.007), and of validated agent PRs **67.2%
  reasoned statically** while only **25% reported benchmarks** against **49%** for humans
  (χ²=12.43, p=0.006). `/viby-toolkit:perf` is built entirely around closing that gap — baseline
  first, profile to choose the target, one change per measurement — and the detector now
  reports the repo's bench command plus which profilers are actually on PATH, so "I couldn't
  measure" stops being the default. Corroborated by
  [PERFOPT-Bench](https://arxiv.org/abs/2607.07744), which scores agents on *verified* speedup
  behind hidden correctness tests and finds capability is workload-dependent rather than a
  property of the model.
- **Refactoring: models judge it by surface, not semantics.**
  [PROMISE 2026](https://homepages.dcc.ufmg.br/~figueiredo/publications/promise2026preprint.pdf)
  found a heuristic bias toward style and naming, plus *semantic overgeneralization* —
  penalising a legitimate refactor **because** it shortened the code, mistaking brevity for
  lost functionality. So `/viby-toolkit:refactor` requires the behaviour-pinning tests to pass
  unchanged before and after (adding characterization tests when nothing pins it), and the
  review cluster now carries the inverse caution: don't flag "this removed logic" from shape
  alone — name the input whose behaviour changed, or drop the finding.

The v0.10.0 additions:

- **Releases: the version number is the promise, and it is routinely broken.** A systematic
  review of **97 primary studies** across Maven, npm, Python, Web APIs and Linux distributions
  ([arXiv 2605.24397](https://arxiv.org/abs/2605.24397)) found **67% of Maven artifacts
  introduce at least one semantic-versioning violation**, and names *"the failure of semantic
  versioning as a trust mechanism"* a central open problem. Of its 43 surveyed detection
  approaches, the finding that shaped this skill is that they reach *"high accuracy on
  syntactic breaks but limited coverage on behavioral ones"* — so `/viby-toolkit:release` puts
  the mechanical checks in a script and spends its judgement on the behavioural half: changed
  defaults, narrowed inputs, new error types, reordered results. When unsure between minor and
  major, it is major, because the cost is asymmetric.
  *(Search results also offered per-release breakage percentages for Maven and npm; the paper
  did not confirm them in the text I fetched, so they are deliberately not cited here.)*
- **A shared strip-noncode module** (now `core/strip.rs`). Four separate times in this repo, a checker matched raw
  text and flagged a fixture, a comment, or a regex pattern that merely *mentioned* the thing
  it hunts. The blanking pass is now shared rather than reimplemented, and the release
  pre-flight uses it — which is exactly how its own `describe.only`-in-a-fixture false
  positive disappeared. Extracting it was itself a refactor done under the new skill's rules:
  62 scanner tests passed unchanged before and after.
- **The always-on injection got smaller, not bigger.** It used to enumerate all fourteen
  skills (~465 tokens) and grew with every addition. Skill *descriptions* are already loaded
  for discovery, so that list bought nothing. Now ~291 tokens of pure contract — evidence
  gate, fan-out law, one routing rule — and it no longer grows when a skill is added.

The v0.11.0 additions, chosen by cost-of-error rather than by frequency:

- **Schema changes are the rare unrecoverable mistake.** A code error is fixed by editing code;
  a dropped column takes its data with it, and a lock on a hot table is an outage while it
  runs. `viby check-migration` encodes the short list of operations behind most migration
  incidents, and each rule names the safe alternative — "don't" without "instead" gets ignored
  under deadline. The engine behaviour it relies on (ACCESS EXCLUSIVE locks, table rewrites,
  `CONCURRENTLY`, `NOT VALID`) is documented PostgreSQL/MySQL semantics rather than a research
  finding, and it is presented that way: every message says *check this against your engine*.
  Sources here were practitioner guides, not studies — stated plainly rather than dressed up as
  evidence.
- **Incidents invert the debugging rule, on purpose.** `/viby-toolkit:debug` forbids a fix without
  a confirmed root cause, which is right when you have time and wrong when users are losing
  service. `/viby-toolkit:incident` puts reversible mitigation first (rollback → flag → shed load
  → clear the blockage), insists on preserving evidence *before* mitigation destroys it, and
  forbids shipping a speculative code change to production under pressure. It then hands back
  to `debug` for the real root-cause pass. The two skills disagree about ordering by design,
  and each says so.
  *Search results for this one offered a "92.1% root-cause accuracy, 82% MTTD reduction" claim
  from a single unverified source. It is cited nowhere — that is exactly the kind of multiplier
  this section exists to discard.*

The v0.12.0 additions:

- **Skill libraries degrade by OVERLAP, not by size — and this corrects an earlier assumption
  here.** "More Skills, Worse Agents?" ([arXiv 2605.24050](https://arxiv.org/abs/2605.24050))
  measured pass rate falling up to **21% at 202 skills** (~8% at 52, ~14% at 102), with
  right-skill invocation dropping from **88% to 53%**. The mechanism is **skill shadowing**: a
  description that semantically overlaps another's hides it from selection, like variable
  shadowing. Critically, the cost of the extra *context* was **"statistically indistinguishable
  from zero"** — so the v0.10.0 note about trimming the always-on preamble was right about the
  redundancy and wrong about the reason. The paper's own recommended mitigation is *description
  disambiguation*, so that is now executable: `viby check-skills` measures pairwise description
  similarity, flags shared literal trigger phrases, and treats a mutually cross-referencing pair
  as already disambiguated — because "distinct from X" lowers real confusion while raising word
  overlap, and a metric that penalised the fix would reward vagueness.
  Its first thresholds were picked by feel (0.38/0.50) against a metric whose real-world maximum
  is ~0.13, so it could never have fired; recalibrated against the measured distribution
  (median 2.1%, most-adjacent legitimate pair 13%) it caught `migrate`/`refactor` drifting
  together on the first honest run, which is now fixed in both descriptions.
- **`observe` and `api`** fill the last two named gaps. Both are explicitly labelled **doctrine,
  not research**: the observability searches returned vendor material (including a "50-70% MTTR
  reduction" figure that is cited nowhere here), and the API-design searches returned no 2026
  research at all. The one genuinely useful framing kept from that reading is that most incident
  time is *human correlation* time, which is why `observe` judges every log line by whether it
  shortens that join rather than by whether it is information.

The v0.13.0 round sharpened existing doctrine instead of adding modules — three places where
newer evidence corrected or strengthened a claim already made here:

- **The failure mode near a context limit is refusal, not hallucination.**
  ([arXiv 2607.19257](https://arxiv.org/abs/2607.19257)) Recall holds at ceiling to roughly
  64–128k tokens then falls away sharply, and near the ceiling **refusal rates climb to 79–90%**
  while fabrication measured **exactly zero across 5,760 absent-fact probes**. So §2 now says to
  budget for "it declined" rather than "it invented", and to suspect context pressure when an
  agent starts hedging on work it managed earlier. A 1M window is no exemption — degradation was
  severe at 100k.
- **Multi-agent failures are interface failures.** The MAST taxonomy (Cemri et al., 1,600+
  annotated traces across 7 frameworks) attributes **~36.9% of failures to inter-agent
  misalignment** — communication breakdown, context lost at handoff, conflicting outputs, format
  mismatch. None of those is a reasoning failure. §3 now says to spend care on the brief and the
  return format rather than on the agent's cleverness, and the four-part subagent contract gained
  two rules earned in practice this week: a returned report can arrive **truncated** (treat that
  as an interface failure and ask for a self-contained re-send — never summarise a report you
  only partly received), and **demand negative results explicitly**, because otherwise you
  cannot distinguish "nothing there" from "never looked".
- **Self-review mostly manufactures confidence.** Measurements of LLM self-verification find
  rechecks are *overwhelmingly confirmatory rather than corrective*, and the assumed
  generation–verification gap does not reliably hold — a model is often no better at judging its
  own output than at producing it. That is now stated in `review` as the reason its
  fresh-context reviewer is load-bearing rather than stylistic.
- **Instruction budgets are now measured.** The same prompt-design paper found perfect compliance
  collapses to **zero at N=80 simultaneous instructions**, "a hard floor rather than a gradual
  asymptote", and recommends **~40 as a redesign threshold**. A skill body is exactly a list of
  simultaneous instructions, so `viby check-skills` now counts directives per skill and reports the
  heaviest. The largest here is `principles` at 32, so nothing fires — stated plainly rather than
  manufacturing a problem, and it now guards against drift.

The v0.14.0 round — one of these corrects a law this toolkit had stated absolutely:

- **The fan-out law was too strong, and here is the counter-evidence.** Co-Coder
  ([arXiv 2606.00953](https://arxiv.org/abs/2606.00953)) parallelised repository-level *coding*
  and beat sequential, file-based-parallel, **and Claude Code with Agent Teams** — by **+14.0%
  pass rate, up to 2.10× wall-clock, and −35% API cost** across 28 real tasks on DevEval and
  CodeProjectEval, with the largest gains on the most dependency-dense projects. What made the
  difference was not more agents but how the work was cut: dependency graph from static
  analysis, **structural hub files isolated**, partition by community boundary rather than by
  file, dependency-aware scheduling. So "never fan out to write" is now stated as what it
  actually is — *don't parallelise writes across a dependency boundary you have not mapped* —
  with single-threaded writes remaining the default because mapping is real work. `orchestrate`
  gained concrete criteria in place of "genuinely independent parts", and its Iron Law was
  updated to match, since nothing mechanical catches a doctrine that contradicts its own body.
- **Agent self-reported confidence is close to worthless, and the fix is adversarial framing.**
  ([arXiv 2602.06948](https://arxiv.org/abs/2602.06948)) *"All results exhibit agentic
  overconfidence: some agents that succeed only 22% of the time predict 77% success."* Two more
  findings shaped the response: a **pre-execution** estimate discriminated better than
  post-execution self-review despite having less information, and **reframing assessment as
  bug-finding achieved the best calibration**. That last one is exactly the `skeptic`'s shape —
  told to refute rather than to rate — so the review pipeline's design is now defended on
  evidence, and the escalation ladder is explicit that a subagent's `confidence` field is a weak
  input and the executed check is the strong one.
- **The instruction budget did its job on its author.** Adding the doctrine above pushed
  `principles` to 36 directives against a measured redesign threshold of ~40, so the
  model-routing table, escalation ladder and subagent contract moved to
  `references/model-routing.md` — consulted material rather than continuously-obeyed
  directives. The heaviest skill is now `test` at 32.

**Two fetches were discarded this round.** The PDF versions of both papers returned no
extractable text, and the fetcher answered from titles and metadata with "appears to", "likely"
and "the title suggests". Nothing from those responses is cited; the numbers above come from the
abstracts, re-fetched as HTML.

The v1.1.0 round has a different provenance from every round above: not papers, but **two
in-house tooling spikes** — a code-graph benchmark and a browser-automation decision — read in
full. Neither is published research, and both were run against a private codebase, so what is
taken is **method only**; no code, names or figures from either appear here. They earned their
place by being the best worked examples of a decision class this toolkit had no skill for:
*should we adopt this tool at all?*

- **The decisive result of a benchmark is a cheap tool being confidently wrong.** In the
  code-graph spike, the front-runner returned real 3–19× token savings on within-scope queries
  and **zero results** on the cross-module question that was the entire reason to adopt it —
  because it followed an import to a re-export barrel and stopped. The savings column alone would
  have recommended it. You only see that failure if you established the right answer *first*, on
  one labeled case. That is `/viby-toolkit:evaluate`'s Iron Law, and the new §1 line in
  `principles`: *a fast, cheap, wrong answer is worse than the slow one it replaced* — because it
  gets believed and kept, while the slow method got checked.
- **Cost is payload × cadence.** The browser spike's sharpest line was that cadence matters as
  much as size: a tool that re-sends its full page tree after every click pays its ~15k several
  times in one flow, so a "cheaper per call" option loses. Nothing in this toolkit measured
  either half, so `viby measure-read-cost` now prices a read set with `--repeat` for the repeats
  and `--budget` to gate it — and `explore` uses it to decide *read it myself vs send a scout*,
  a call previously made by feel.
- **Rank twice when the weightings disagree.** That spike ranked engines once by correctness and
  supply chain, then again by the team's operational criteria — and the leader changed. Both
  rankings shipped, side by side, which is the honest form: a single blended ranking is where an
  unstated preference hides. The corollary is that the winner is rarely best at everything, it is
  *the only option clearing every hard constraint* — and the recommendation is usually a **routing
  rule between tools** (graph to scope, grep to complete) rather than a tool.
- **Ship the winner's failure case, and correct your own earlier claims.** Both documents state
  where their own recommendation is incomplete, with the number attached ("20 of 61 files"), and
  route around it; one carries an explicit *"two claims this benchmark corrected"* paragraph
  against its own survey section. Both are now steps in `evaluate`, and the measured/inferred/
  not-tested labeling rule is in `principles` §5, at the point of the claim rather than in a
  closing caveat — unlabeled inferences get promoted to results by your own summary.
- **Rejections are the reusable part.** Each rejected candidate gets one line naming the bar it
  failed (a third-party egress, a runtime the machine doesn't have), which is what stops the same
  option being re-proposed next quarter. That pattern went into `evaluate` and into
  `/viby-toolkit:plan`, which previously recorded only the chosen approach.

**v2.1.0 closed the gaps in that round**, after auditing what had actually been taken versus
left: the **authored-vs-derived two-layer model** (`principles` §9 — the *why* is written,
reviewed and durable; the *what* is rebuilt by a command, disposable and never ground truth;
they join by stable reference, never by copying, and a heuristic derived artifact is a
planning aid rather than evidence), the **generated-artifact strategy across worktrees**
(`worktrees` — one canonical read-only copy pinned to mainline beats per-worktree rebuilds and
beats symlinking a single writable copy into N worktrees, which races; and the undocumented
mechanical assumption behind option three has to be named rather than assumed), the
**difference-only capability matrix** (a column of ✅ hides the two rows that decide it), and
the **verification status line** at the top of a decision record. Three of the four belong to
skills other than `evaluate`, which is why the first pass missed them.

Also adopted from them: pin exact versions and verify the publisher (one had a near-miss
typosquat slot next to the legitimate package); trial in a throwaway worktree with nothing added
to the project's manifests; **write the uninstall command down before running the install
command**; and a generalisation check — re-run the decision rule on a second, unrelated case,
because one case makes a champion and two make a rule.

---

## Hooks

- **SessionStart** injects the accuracy/fan-out defaults (~400 tokens) so the working style
  applies even when no skill is invoked. This is the only hook enabled by default.
- **Opt-in** (shipped, not enabled): `viby format-hook` (2.x settings that name its old `.ts` script still work) auto-formats edited
  files *only* when the formatter is already installed and the project uses it (never
  installs anything, never blocks). Enable by adding a `PostToolUse` matcher if you want it.

**No command-blocking guard.** An earlier version shipped a `PreToolUse` hook that vetoed
destructive Bash commands. It's gone: this is a single-user toolkit, and a veto that has to
be argued with costs more than it protects. Nothing here now intercepts or blocks a command
— permissions are left entirely to Claude Code itself. (The removed hook and its 110-case
contract are still in git history if it's ever wanted back.)

## Telemetry — prove the token strategy works

The context discipline is measurable, not just asserted:

- **Statusline** (`viby statusline`; opt-in; 2.x settings that name its old `.ts` script still work) — shows
  `model · ctx NN% · cache NN% · 5h NN% · 7d NN% · $cost`. `ctx` is
  `context_window.used_percentage` (input tokens: fresh + cache creation + cache read, as a
  share of the window) colour-banded green <60 / yellow <80 / red — watching it is how you
  hold the 40–60% target. `5h`/`7d` are **rate-limit consumption**, which is the resource
  that actually binds on Max, so they rank above the cost figure; they appear only for
  Pro/Max after the first API response. Absent or null fields are skipped rather than shown
  as `0`. Wire it in `~/.claude/settings.json` (or just use `bunx ccusage statusline`) —
  this form follows the newest installed version instead of hardcoding one. A 2.x line that
  runs `hooks/run.sh … statusline.ts` keeps working through 3.x:
  ```json
  { "statusLine": { "type": "command",
      "command": "sh \"$(ls -t \"${CLAUDE_CONFIG_DIR:-$HOME/.claude}\"/plugins/cache/viby-toolkit/viby-toolkit/*/launcher/viby | head -n 1)\" statusline" } }
  ```
- **OpenTelemetry** — set `CLAUDE_CODE_ENABLE_TELEMETRY=1` and export
  `claude_code.token.usage`; group by `query_source` (`main` vs `subagent`) and `agent.name`
  to measure fan-out ROI directly (main-thread tokens saved vs subagent tokens spent), and
  by `type=cacheRead` to confirm cache reuse.
- **ccusage** — `npx ccusage@latest blocks --live` for a zero-setup real-time view;
  `/cost` for an in-session check.

## Install on a new machine

Prereqs: none for the repo itself — it is public, so no `gh` login, token or SSH key is
needed to install or to auto-update.

**Runtime:** none to install. The skills, agents and prompts are plain markdown. The
executable parts (the checkers, the statusline, the browser and agentgrep servers) are one
binary, `viby`, which `launcher/viby` gets on its first call and caches per plugin version:

1. It downloads the build for this machine from the GitHub release of this version, and refuses
   it unless its SHA-256 matches `launcher/checksums.txt` in the plugin.
2. No build for this machine, or no network: it builds from the plugin's own source with cargo,
   once, in 1–2 minutes.
3. No cargo either: it prints the one command that installs Rust, so Claude can run it.

macOS (Apple silicon and Intel), Linux (x86_64 and arm64, static musl builds) and Windows
(x86_64, through Git Bash) have prebuilt binaries.

**Option A — via Claude Code (recommended):**

```
/plugin marketplace add CrySteRz/viby-toolkit
/plugin install viby-toolkit@viby-toolkit
```

Then confirm it's enabled at **user scope** so it applies to every project, and turn on
auto-update: in `/plugin`, open the `viby-toolkit` marketplace and choose **Enable auto-update**.
Claude Code leaves it off for every marketplace except Anthropic's own.

**Option B — via settings.json** (makes it declarative / reproducible). Add to
`~/.claude/settings.json`:

```jsonc
{
  "extraKnownMarketplaces": {
    "viby-toolkit": {
      "source": { "source": "github", "repo": "CrySteRz/viby-toolkit" },
      "autoUpdate": true
    }
  },
  "enabledPlugins": {
    "viby-toolkit@viby-toolkit": true
  }
}
```

Restart Claude Code. Verify with `/plugin` (should list `viby-toolkit` as enabled) and by typing
`/viby-toolkit:` (skills should autocomplete).

### Auto-update

Claude Code auto-updates only Anthropic's own marketplace unless told otherwise, so this one
needs `"autoUpdate": true` (Option B) or the `/plugin` toggle (Option A). With it on, Claude
Code refreshes the marketplace in the background a few minutes into a session and updates the
plugin on disk; the new version loads at the next launch or after `/reload-plugins`. The `viby`
binary follows on its own: the launcher keys its cache on the plugin version, so the first call
after an update fetches the matching build. To switch auto-update off, set `"autoUpdate": false`
or use the same toggle.

Background auto-update disables git credential helpers, which used to
make HTTPS pulls of this marketplace fail **silently** while it was private; a public repo needs
no credential, so that whole failure mode is gone.

One caveat if you cloned the marketplace over SSH (`gh` sets the git protocol to `ssh` for you,
so this is easy to end up with): the fetch then needs a loaded key in `ssh-agent`, reintroducing
the credential dependency the public repo just removed. `git -C ~/.claude/plugins/marketplaces/viby-toolkit remote -v`
shows which you have; an `https://` remote is the one that needs nothing.

Force a manual update anytime with `/plugin update viby-toolkit`. Set
`CLAUDE_CODE_PLUGIN_KEEP_MARKETPLACE_ON_FAILURE=1` to keep the working clone if a refresh
fails rather than losing it.

## Install on a machine without GitHub access (portable bundle)

The repo being public covers the no-account case, so this path is now for a machine with **no
network at all** — an air-gapped box, or a locked-down environment where GitHub is unreachable.
Install straight from a copy of this folder:

1. Copy the whole `viby-toolkit` folder to the target machine (USB, `scp`, a cloud drive,
   or Syncthing). A clean copy without git history:
   ```bash
   git archive --format=tar.gz -o viby-toolkit.tar.gz HEAD    # on a machine that has the repo
   # move the .tar.gz over, then on the target:  tar xzf viby-toolkit.tar.gz
   ```
2. On the target machine, from inside the folder, run:
   ```bash
   bash install.sh
   ```
   It registers this folder as a local marketplace and installs `viby-toolkit` at user scope.
   Restart Claude Code and type `/viby-toolkit:`.

   On a machine that already has `viby-toolkit` installed **from GitHub**, this refuses rather
   than proceeding, and says so. That is not caution for its own sake: `claude plugin marketplace
   add` *replaces* a same-named marketplace instead of failing, so without the guard this script
   silently repointed the marketplace at a local folder and every later auto-update came from that
   directory instead of GitHub — while printing "added". Override deliberately when you mean it:

   ```bash
   VIBY_INSTALL_FORCE_LOCAL=1 bash install.sh    # repoint at this folder on purpose
   ```

   To keep the GitHub source and merely refresh it:

   ```bash
   claude plugin marketplace update viby-toolkit && claude plugin update viby-toolkit@viby-toolkit
   ```

**Keep the folder** — the plugin loads from it. **To update later:** copy a newer copy of
the folder over the old one and re-run `bash install.sh` (it's idempotent). This trades
auto-update for total independence from GitHub.

> ⚠️ That re-run genuinely refreshes the install now; before, it silently did not. `claude plugin
> install` exits 0 as a no-op when the plugin is already present, so the script's update branch
> never ran — and `claude plugin update` compares version strings, so an edit made without bumping
> the version left the cached copy untouched. Since `~/.claude/plugins/cache` holds a *copy* rather
> than a symlink, the stale copy is what Claude Code loaded: the change sat in this folder, absent
> from the running agent, while the script reported success. `install.sh` now clears that cached
> copy first and verifies the plugin re-materialised.

---

## Editing the toolkit

Everything lives here as plain files:

```
.claude-plugin/marketplace.json      # marketplace manifest
plugins/viby-toolkit/
  .claude-plugin/plugin.json         # plugin manifest
  skills/<name>/SKILL.md             # the workflows
  agents/<name>.md                   # the subagents (model routing in frontmatter)
  commands/ship.md                   # the autonomous entry command
  hooks/hooks.json + session-start.sh   # SessionStart is the only default hook
  hooks/run.sh                       # 2.x compatibility: maps an old .ts path to its subcommand
  launcher/viby                      # gets this version's binary (download, verify, cache) and runs it
  launcher/checksums.txt             # SHA-256 per release asset, committed by the release
  browser/extension/                 # the browser extension (JavaScript, runs in the browser)
  rust/                              # the Cargo workspace
    crates/viby/src/main.rs          # `viby <command>`: one binary, one subcommand per tool
    crates/viby/src/checks/          # the checkers the skills run:
                                     #   scan-test-quality, detect-stack, check-release,
                                     #   check-migration, check-skills, measure-read-cost,
                                     #   check-api-surface, check-study, check-test-drift,
                                     #   check-analytics-sql, check-diff-hygiene,
                                     #   check-skill-safety, check-plan, check-memory,
                                     #   check-logging, check-docs, check-hooks, check-agents,
                                     #   check-orchestration
    crates/viby/src/core/strip.rs    # shared: match code, never raw text
    crates/viby/src/hooks/           # statusline, format-hook (opt-in)
    crates/viby/src/browser/         # browser MCP server, native host and its installer
    crates/viby/src/agentgrep.rs     # agentgrep MCP server
    crates/viby/tests/               # contract tests, one file per suite
    xtask/                           # `cargo xtask check` (the pre-push gate)
```

### How the executable half runs

Everything executable is one Rust crate built into one binary, `viby`, with a subcommand per
tool. The subcommands keep the old scripts' names and contracts: the same arguments, `--json`
and `--quiet`, exit `0` clean / `1` findings / `2` nothing to check. Skills run them as
`sh "$VIBY" <command>` after a one-line locator, and the plugin's MCP servers and the
statusline go through the same `launcher/viby`.

A plugin install copies only `plugins/viby-toolkit`, so the Cargo workspace lives inside it:
the launcher's cargo fallback needs the source to be there.

After editing, bump `version` in `plugin.json`, `marketplace.json` and `rust/Cargo.toml`
(all three carry it; the gate fails when they disagree), commit, push. **Pushing a new version is
the release:** `.github/workflows/release.yml` builds `viby` for the five targets, commits their
SHA-256 sums to `launcher/checksums.txt`, tags that commit and publishes the binaries, then
`launcher.yml` runs the launcher on a fresh machine of each OS. Machines with auto-update on pick
the new version up on their next session; others with `/plugin update viby-toolkit`.

### Verify before pushing

The toolkit holds itself to its own evidence gate — the hooks and scripts are executable
code, so they have contract tests. One command runs everything:

```bash
cd plugins/viby-toolkit/rust && cargo xtask check   # the gate, one verdict
```

Which runs:

```bash
claude plugin validate .          # manifests + skill/agent frontmatter (local only)
cargo test                        # every suite, each against a minimum pass count
cargo clippy -- -D warnings
# + the scanner on its own tests, versions (plugin.json = Cargo = not behind the newest tag),
#   every cross-reference resolves (see below), and the SessionStart JSON is valid
```

The same gate runs in CI on every push (`.github/workflows/check.yml`), because the other
machines auto-update from this repo's default branch — a broken push propagates.

**The reference check catches a class nothing else can.** Two shipped bugs were neither syntax
errors, type errors, nor failing tests — they were *instructions Claude could not follow*:
a keystone skill marked `disable-model-invocation` (making it user-only while nine skills
told Claude to load it), and a script path built from `CLAUDE_PLUGIN_ROOT`, which is set for
hooks but empty in a skill body. The check verifies every `/viby-toolkit:<name>` resolves to a
skill that exists *and is model-invocable*, that referenced agents and scripts exist, and
that no skill body expands a hook-only variable.

These are specifications, not smoke tests. Both halves of each contract are pinned: every
smell that must be flagged **and** every healthy test that must not be. If you add a rule,
add cases for both — a scanner that flags good tests gets ignored, and then it protects
nothing. Each test binary also has a minimum pass count in `MIN_PASSING` (`rust/xtask/src/check.rs`),
because `cargo test` exits 0 for a file whose tests were emptied.

The must-allow half earns its keep: the test scanner flagged 23 false positives on its own
fixtures until it learned to blank string literals before matching, then flagged them again
when it met multi-line template literals. Both times the root cause was the same — matching
raw text instead of parsed code.

**Secrets:** this repo syncs across work and personal machines — never commit tokens,
credentials, client names, or internal hostnames. `.gitignore` blocks the common ones;
keep skills/agents project-agnostic and put anything project-specific in that project's
own `.claude/`.
