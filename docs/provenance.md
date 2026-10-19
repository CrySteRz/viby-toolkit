# Provenance

Where the doctrine in viby-toolkit came from, round by round: the papers, the in-house spikes,
and the claims that were checked and dropped. Moved out of the README on 2026-10-19; the text is
unchanged, so version numbers refer to the release that adopted each idea.

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
- **A shared strip-noncode module** (now `plugins/viby-toolkit/rust/crates/viby/src/core/strip.rs`). Four separate times in this repo, a checker matched raw
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
  `plugins/viby-toolkit/skills/principles/references/model-routing.md` — consulted material rather than continuously-obeyed
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
