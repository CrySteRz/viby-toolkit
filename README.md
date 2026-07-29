# viby-toolkit

My personal Claude Code toolkit, distributed as a private plugin marketplace. One repo,
installed once per machine at **user scope**, so it applies automatically to every
project — work and personal — and travels with me to any new computer.

Marketplace: **`viby-toolkit`** · Plugin: **`viby-toolkit`**

---

## What's in it

`viby-toolkit` is an accuracy-first, token-disciplined set of engineering workflows. It's
stack-agnostic (detects the project at runtime; assumes nothing). Everything executable is
TypeScript with zero runtime dependencies and no build step — see
[Why TypeScript](#why-typescript-and-how-it-runs-without-a-build-step).

### Skills (auto-trigger by context, or call with `/viby-toolkit:<name>`)

| Skill | What it does |
|---|---|
| `/viby-toolkit:brainstorm` | **Design-before-code gate.** Decides WHAT to build (and whether it's the right thing) with an Iron-Law hold on any implementation until you approve the design. Runs before plan/orchestrate for anything whose shape isn't settled. |
| `/viby-toolkit:orchestrate` | Drives a task end-to-end: scope → research → plan → implement → verify → self-review. Fans out cheap scouts for discovery, keeps writes single-threaded, keeps main context clean. |
| `/viby-toolkit:review-cluster` | **Review cluster + false-positive filter.** Parallel per-dimension reviewers (incl. an adversarial chaos-engineer dimension) find candidates; a grounding gate drops anything that can't quote its own line; one fresh-context validator per finding confirms real/introduced/not-already-handled; a confidence gate suppresses below-threshold. Reports the full kill count. |
| `/viby-toolkit:explore` | **Understand an unfamiliar codebase.** Detects the stack mechanically first (languages, package manager, monorepo tool, real build/test commands with their source), then fans out scouts on specific questions, traces one path end to end, and writes a durable map. Scouts each language separately in a polyglot repo and names the cross-language seams. |
| `/viby-toolkit:secure` | **Security pass, ordered by what actually goes wrong.** Credentials first (99.6% of critical findings in a 2026 study of 4,022 agent-assisted PRs), then supply chain and CI (82.3% by volume), then code surfaces judged by reachability. Confirms each candidate secret, because that study's own labelling was ~73% false positives. |
| `/viby-toolkit:verify` | **The evidence gate, executed.** Finds the project's real checks (CI config is authoritative), scopes them to the change, exercises the actual behavior — then screens the output for silent-pass modes, because a zero exit code with zero tests collected is not a pass. Fix the code, never the check. |
| `/viby-toolkit:test` | **QA and test design, with a scanner.** Picks the test level deliberately, insists every new test is *seen failing for the right reason* before it's trusted, and enforces mocking discipline (coding agents over-mock measurably more than humans). Ships an executable auditor — `scan-test-quality.ts` finds no-assertion tests, tautologies, over-mocking, `.only`/`.skip` left in, sleep-waits and swallowed errors, with `file:line`. |
| `/viby-toolkit:perf` | **Measure, or it didn't happen.** Baseline → profile → one change → re-measure, with correctness as a gate. Exists because a 2026 study of 407 performance PRs found agents pick statistically indistinguishable optimisations to humans but validate them far less: 45.7% vs 63.6%, and 67.2% of validated agent PRs reasoned statically instead of benchmarking. The detector reports the repo's bench command and which profilers are actually installed. |
| `/viby-toolkit:refactor` | **Behaviour-preserving, and proven so.** Name the invariant, find what pins it, add characterization tests if nothing does, then transform in small verified steps. The same tests must pass before and after, unchanged. Never mixed with a behaviour change in one diff. |
| `/viby-toolkit:schema` | **The one change you cannot undo.** Every schema change must deploy while the OLD code still runs and be reversible without data loss — expand, migrate, contract, never rename in place. Ships `check-migration.ts`: index without `CONCURRENTLY`, `NOT NULL` without a default, type changes, renames, unbounded `UPDATE`, constraints without `NOT VALID`, DDL mixed with a backfill, missing `lock_timeout`, no rollback. Every rule names the safe alternative. |
| `/viby-toolkit:incident` | **Stop the bleeding, then find out why.** Deliberately inverts `debug`: reversible mitigation *before* diagnosis, because users are losing service while you investigate — rollback, then flag, then shed load, then clear the blockage. Preserve evidence before mitigation destroys it. Never ship a speculative fix to production under pressure. Hands back to `debug` once service is restored. |
| `/viby-toolkit:debug` | Root-cause debugging by hypothesis and evidence — reproduce (as a failing test, routed to the strong model) → localize → confirm → fix → verify. No speculative patching. |
| `/viby-toolkit:migrate` | Wide mechanical changes (renames, upgrades, pattern sweeps): discover every site → transform in batches → verify each → final zero-remaining sweep. |
| `/viby-toolkit:plan` | Turns an agreed idea into an ordered, file-anchored change-list with the risky step and verification strategy called out. Plan doubles as a durable checkpoint. |
| `/viby-toolkit:release` | **The version number is a promise.** Decide major/minor/patch from the public-surface diff, never from the size of the change — a review of 97 studies found 67% of Maven artifacts violate SemVer, and that detection handles syntactic breaks well but behavioural ones poorly. Ships `check-release.ts`: version drift across manifests, dirty tree, unpushed commits, tag collisions, stale changelog, debug artifacts left in. |
| `/viby-toolkit:observe` | **Instrument for the person reading it at 3am.** Log decisions and outcomes, never arrivals; structured fields, not sentences; correlation keys on every event. High cardinality is the point for logs and traces, and the cost trap for metric labels. Alert on symptoms, not causes — and verify the instrumentation by triggering the path and reading the real output. |
| `/viby-toolkit:api` | **Design the contract, because you cannot take it back.** Write the caller's code first, design inward from their use case rather than outward from the storage schema, and settle errors, pagination, idempotency and limits at design time — each is a breaking change if retrofitted. Expand-then-contract for evolution; diff the surface to decide if a change is breaking. Ships `check-api-surface.ts`: added/removed/re-signatured exports between two git refs for TS/JS, Python, Go and Rust, honouring each language's own visibility rule — telling a positional parameter rename (P2) from a real signature break (P1), reporting every `export *` barrel it cannot follow rather than silently excluding it, and stating on every run that a behavioural break with an unchanged signature is invisible to it. |
| `/viby-toolkit:adopt` | **Inherit foreign code, conform it, prove it still works.** Provenance gate first (licence, secrets in history, runtime behaviour) → capture behaviour with characterization tests *before* editing, and diff against the still-runnable original → a functionality matrix with a **held-out slice the agents never see** → Mikado steps, reverting on a blocked prerequisite so the graph becomes the partition parallel writes require → conform to instructions first, the language's own idiom second, project convention third. Built on measurement: the best model manages 41.58% on real behaviour-preserving refactorings, and monitoring for shortcuts cut hacked-resolved runs from 28.57% to 0.56% while raising clean resolved from 40.22% to 60.53%. Ships `check-test-drift.ts`: deleted tests, removed assertions, new skips, `.only`, or a `sys.exit(0)` in the suite — the ways a green run gets faked. |
| `/viby-toolkit:study` | **Turn an idea into a study, protocol first.** You bring the question; it returns the protocol — competing answers, the observation that would exclude one, the search angles, the stopping rule, and what would change its mind — for approval *before* searching, then the document. Built on preregistration (a protocol is a promise to be transparent about deviating, not to obey), PICOC, strong inference, snowballing with a named stopping rule, AACODS source appraisal and GRADE's indirectness. Iron Law: every claim carries the quoted sentence that supports it, because deep-research citations keep links valid >94% and on-topic >80% while only 39–77% actually support the claim. Ships `check-study.ts`: unsourced figures, hedges inside measured sections, missing falsifier or stopping rule, single-domain sourcing, undated citations. |
| `/viby-toolkit:evaluate` | **Decide what to adopt, by measuring it on a case you already know the answer to.** Establish the ground truth *before* installing anything, price the baseline you'd be replacing, then put cost and correctness in one table — because a cheap wrong answer beats an expensive right one on every column except the one that matters. Ships `measure-read-cost.ts`: what a read set costs, `--repeat` for cadence (payload × frequency), `--budget` to gate it. Records the winner's failure case, the rejections with the bar each failed, and the back-out commands. |
| `/viby-toolkit:learn` | Records a reusable lesson (gotcha, build quirk, rejected finding, known past risk, "never compact X") to Claude's native project memory — the compounding loop, both suppressing false positives and raising recall on known risks. |
| `/viby-toolkit:handoff` | Serializes live task state (goal, decisions, next step) so a fresh session resumes mid-task without re-deriving it. Ephemeral, distinct from `learn`. |
| `/viby-toolkit:worktrees` | Isolates work (parallel implementers, risky experiments) — detect existing isolation first, prefer the native worktree tool, never fight the harness. |
| `/viby-toolkit:principles` | The operating contract everything follows: accuracy rules, the fan-out law, model-routing + escalation ladder, context discipline, the evidence gate. Read-only reference. |

### Command

- `/viby-toolkit:ship <task>` — run the whole pipeline autonomously and don't stop until verified.

### Agents (dispatched by the skills; cheap models by design)

`scout` (haiku, read-only recon) · `implementer` (sonnet) · `reviewer` (sonnet,
one per review dimension) · `skeptic` (sonnet, adversarial false-positive filter) ·
`debugger` (sonnet, evidence gathering). Each is colour-coded in the transcript so a
parallel fan-out is readable at a glance.

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
  instructions rather than executable scripts." That's the gap `scan-test-quality.ts` and
  `tests/` close: guidance an agent can *run*, not only read.

The v0.8.0 additions:

- **Language-agnosticism made executable.** Every skill said "find the project's real
  commands"; nothing helped it do so. `detect-stack.ts` now does, for ~30 languages and 16
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
- **A shared `lib/strip-noncode.ts`.** Four separate times in this repo, a checker matched raw
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
  runs. `check-migration.ts` encodes the short list of operations behind most migration
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
  disambiguation*, so that is now executable: `check-skills.ts` measures pairwise description
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
  own output than at producing it. That is now stated in `review-cluster` as the reason its
  fresh-context reviewer is load-bearing rather than stylistic.
- **Instruction budgets are now measured.** The same prompt-design paper found perfect compliance
  collapses to **zero at N=80 simultaneous instructions**, "a hard floor rather than a gradual
  asymptote", and recommends **~40 as a redesign threshold**. A skill body is exactly a list of
  simultaneous instructions, so `check-skills.ts` now counts directives per skill and reports the
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
  either half, so `measure-read-cost.ts` now prices a read set with `--repeat` for the repeats
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
- **Opt-in** (shipped, not enabled): `hooks/post-tool-use-format.ts` auto-formats edited
  files *only* when the formatter is already installed and the project uses it (never
  installs anything, never blocks). Enable by adding a `PostToolUse` matcher if you want it.

**No command-blocking guard.** An earlier version shipped a `PreToolUse` hook that vetoed
destructive Bash commands. It's gone: this is a single-user toolkit, and a veto that has to
be argued with costs more than it protects. Nothing here now intercepts or blocks a command
— permissions are left entirely to Claude Code itself. (The removed hook and its 110-case
contract are still in git history if it's ever wanted back.)

## Telemetry — prove the token strategy works

The context discipline is measurable, not just asserted:

- **Statusline** (`hooks/statusline.ts`) — shows
  `model · ctx NN% · cache NN% · 5h NN% · 7d NN% · $cost`. `ctx` is
  `context_window.used_percentage` (input tokens: fresh + cache creation + cache read, as a
  share of the window) colour-banded green <60 / yellow <80 / red — watching it is how you
  hold the 40–60% target. `5h`/`7d` are **rate-limit consumption**, which is the resource
  that actually binds on Max, so they rank above the cost figure; they appear only for
  Pro/Max after the first API response. Absent or null fields are skipped rather than shown
  as `0`. Wire it in `~/.claude/settings.json` (or just use `bunx ccusage statusline`) —
  this form survives version bumps instead of hardcoding one:
  ```json
  { "statusLine": { "type": "command",
      "command": "sh \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-toolkit/*/hooks/run.sh | tail -1)\" \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-toolkit/*/hooks/statusline.ts | tail -1)\"" } }
  ```
- **OpenTelemetry** — set `CLAUDE_CODE_ENABLE_TELEMETRY=1` and export
  `claude_code.token.usage`; group by `query_source` (`main` vs `subagent`) and `agent.name`
  to measure fan-out ROI directly (main-thread tokens saved vs subagent tokens spent), and
  by `type=cacheRead` to confirm cache reuse.
- **ccusage** — `npx ccusage@latest blocks --live` for a zero-setup real-time view;
  `/cost` for an in-session check.

## Install on a new machine

Prereqs: `gh` authenticated (`gh auth status`) or SSH access to this private repo.

**Runtime:** the skills, agents and prompts are plain markdown and need nothing. The
executable parts (statusline, test scanner) need **Node ≥22.6**, or bun, or
`tsx` — `hooks/run.sh` takes the first one it finds. With none of them present the hooks
no-op silently and the markdown half still works, so a machine without a JS runtime gets a
degraded-but-functional install rather than errors. Check with `node --version`.

**Option A — via Claude Code (recommended):**

```
/plugin marketplace add CrySteRz/viby-toolkit
/plugin install viby-toolkit@viby-toolkit
```

Then confirm it's enabled at **user scope** so it applies to every project.

**Option B — via settings.json** (makes it declarative / reproducible). Add to
`~/.claude/settings.json`:

```jsonc
{
  "extraKnownMarketplaces": {
    "viby-toolkit": {
      "source": { "source": "github", "repo": "CrySteRz/viby-toolkit" }
    }
  },
  "enabledPlugins": {
    "viby-toolkit@viby-toolkit": true
  }
}
```

Restart Claude Code. Verify with `/plugin` (should list `viby-toolkit` as enabled) and by typing
`/viby-toolkit:` (skills should autocomplete).

### Private-repo auto-update note

Background auto-update disables git credential helpers, so HTTPS pulls of a private
marketplace can fail silently. Either:
- use **SSH** (loaded key in `ssh-agent`) for the marketplace, or
- run `gh auth setup-git` once, or
- set `CLAUDE_CODE_PLUGIN_KEEP_MARKETPLACE_ON_FAILURE=1` to keep the working clone if a
  refresh fails.

Force a manual update anytime with `/plugin update viby-toolkit`.

## Install on a machine without GitHub access (portable bundle)

For a PC that can't reach the private repo (no account, no token), install straight from a
copy of this folder — no GitHub, no login, no network:

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

**Keep the folder** — the plugin loads from it. **To update later:** copy a newer copy of
the folder over the old one and re-run `bash install.sh` (it's idempotent). This trades
auto-update for total independence from GitHub.

---

## Editing the toolkit

Everything lives here as plain files:

```
.claude-plugin/marketplace.json      # marketplace manifest
package.json + tsconfig.json         # TS config; zero runtime dependencies
plugins/viby-toolkit/
  .claude-plugin/plugin.json         # plugin manifest
  skills/<name>/SKILL.md             # the workflows
  skills/test/scripts/scan-test-quality.ts   # executable test auditor
  skills/verify/scripts/detect-stack.ts      # language-agnostic toolchain detector
  skills/release/scripts/check-release.ts    # release pre-flight
  skills/schema/scripts/check-migration.ts   # migration safety linter
  skills/principles/scripts/check-skills.ts  # shadowing / trigger-collision check
  skills/evaluate/scripts/measure-read-cost.ts # read-set cost, cadence, budget gate
  skills/api/scripts/check-api-surface.ts    # public-surface diff -> major/minor/patch
  skills/study/scripts/check-study.ts        # research-document auditor (form, not truth)
  skills/adopt/scripts/check-test-drift.ts   # did the safety net shrink between two refs?
  lib/strip-noncode.ts                      # shared: match code, never raw text
  agents/<name>.md                   # the subagents (model routing in frontmatter)
  commands/ship.md                   # the autonomous entry command
  hooks/hooks.json + session-start.sh   # SessionStart is the only default hook
  hooks/run.sh                       # picks a TS runtime; no-ops if none exists
  hooks/statusline.ts + post-tool-use-format.ts   # opt-in, wired in settings.json
tests/*.test.ts                      # contract tests (node:test)
tests/run-all.ts                     # the pre-push gate
```

### Why TypeScript, and how it runs without a build step

Everything executable is TypeScript with **zero runtime dependencies** — only `node:`
builtins and the built-in `node:test` runner. There is no compile step and no
`node_modules`: Node ≥22.6 strips the types and runs the file directly.

Hooks are shell commands, so each one goes through `hooks/run.sh`, which picks the first
runtime it finds — node ≥22.6 (`--experimental-strip-types`), then bun, then `tsx` — and
**exits 0 silently when there is none**. A machine with no TypeScript runtime therefore
degrades to "no hook" rather than a broken session, which is the same fail-open rule the
guard itself follows. Because type stripping erases types rather than compiling them,
`tsconfig.json` sets `erasableSyntaxOnly` so the compiler rejects anything the runtime
cannot execute (no enums, no namespaces, no parameter properties).

After editing, bump `version` in `plugin.json` **and** in `marketplace.json` (both carry
it), commit, push. Machines pick up the change on next session (or `/plugin update
viby-toolkit`).

### Verify before pushing

The toolkit holds itself to its own evidence gate — the hooks and scripts are executable
code, so they have contract tests. One command runs everything:

```bash
npm run check                     # the gate: 8 checks, one verdict
```

Which runs:

```bash
claude plugin validate .          # manifests + skill/agent frontmatter (local only)
npm test                          # every tests/*.test.ts via the built-in node:test runner
#   statusline.test.ts  — payload shapes incl. the documented null cases
#   scanner.test.ts     — test-quality checks + file classification
npm run typecheck                 # tsc --noEmit (dev-only dep; skipped without node_modules)
npm run refs                      # every cross-reference resolves — see below
# + scanner self-audit, SessionStart JSON validity, and a runner-shim probe
```

The same gate runs in CI on every push (`.github/workflows/check.yml`), because the other
machines auto-update from this repo's default branch — a broken push propagates.

**`npm run refs` catches a class nothing else can.** Two shipped bugs were neither syntax
errors, type errors, nor failing tests — they were *instructions Claude could not follow*:
a keystone skill marked `disable-model-invocation` (making it user-only while nine skills
told Claude to load it), and a script path built from `CLAUDE_PLUGIN_ROOT`, which is set for
hooks but empty in a skill body. The check verifies every `/viby-toolkit:<name>` resolves to a
skill that exists *and is model-invocable*, that referenced agents and scripts exist, and
that no skill body expands a hook-only variable.

These are specifications, not smoke tests. Both halves of each contract are pinned: every
smell that must be flagged **and** every healthy test that must not be. If you add a rule,
add cases for both — a scanner that flags good tests gets ignored, and then it protects
nothing. Each suite also declares a `minPassing` count in `tests/run-all.ts`, because
`node --test` exits 0 for a file whose tests assert nothing.

The must-allow half earns its keep: the test scanner flagged 23 false positives on its own
fixtures until it learned to blank string literals before matching, then flagged them again
when it met multi-line template literals. Both times the root cause was the same — matching
raw text instead of parsed code.

**Secrets:** this repo syncs across work and personal machines — never commit tokens,
credentials, client names, or internal hostnames. `.gitignore` blocks the common ones;
keep skills/agents project-agnostic and put anything project-specific in that project's
own `.claude/`.
