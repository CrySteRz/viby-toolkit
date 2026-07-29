---
name: principles
description: >
  The operating contract every viby-toolkit skill and agent follows — accuracy rules, the
  fan-out law, model routing and escalation, context discipline, evidence gating. Load it when
  another viby-toolkit skill says "follow /viby-toolkit:principles", or before deciding whether
  to fan out subagents. Reference material — read it, don't run it.
---

# Viby-code Operating Principles

The shared contract. Every viby-toolkit skill (`orchestrate`, `review-cluster`, `debug`,
`migrate`, `refactor`, `plan`, `verify`, `test`, `explore`, `secure`, `perf`, `release`, `schema`, `incident`, `observe`, `api`, `evaluate`, `learn`) and agent (`scout`, `implementer`, `reviewer`,
`skeptic`, `debugger`) is built on these. It's a reference — read it, don't "run" it.

Synthesized from what actually works in production agentic coding (Anthropic's
context-engineering and multi-agent research, humanlayer's Advanced Context Engineering,
obra/superpowers, Cognition, Every's compound engineering) — keeping the mechanisms,
discarding the marketing multipliers.

## 1. Accuracy is the objective. Everything else is a constraint.

Token savings, speed, and elegance are secondary. The failure mode we optimize against is
**confident wrong output** — a hallucinated bug, a "fixed" claim that isn't verified, a
refactor that changed behavior. Never trade correctness for cost.

- **Never claim done without fresh evidence.** See the evidence gate in §5.
- **Ground every finding in `file:line`.** A claim you can't anchor is a hypothesis —
  label it as such.
- **Read the actual code over recalling how it "usually" works** — especially for
  Claude/Anthropic APIs, library versions, and config schemas.
- **A fast, cheap, WRONG answer is worse than the slow one it replaced** — because it gets
  believed and kept, while the slow method got checked. This is the rule for adopting any
  tool, index or shortcut: rank it on a case whose correct answer you established *first*,
  and put the correctness verdict in the same table as the cost. A savings number with no
  correctness column beside it is not a result (`/viby-toolkit:evaluate`).

## 2. Context is the master resource. Curate it deliberately.

The LLM is a stateless function; **the contents of the context window are the only lever
on output quality** (humanlayer/ACE). On a Max subscription the scarce resources are the
**main thread's context window** and your **rate-limit budget** — not dollars.

- **Context quality priority: Correctness > Completeness > Size.** Wrong context is
  worst; missing context second; excess tokens are the *least* damaging. Don't over-trim
  and drop something load-bearing to save tokens.
- **Target 40–60% context utilization** (Frequent Intentional Compaction). Reserve
  headroom for iteration and error handling. Don't run the window to overflow — recall
  degrades as it fills ("context rot"), and auto-compaction at ~90% produces noisy
  summaries. Compact *early*, at task boundaries, on purpose.
- **Know the shape of the degradation, because it is not what you expect.** Measured across
  formats (arXiv 2607.19257), recall holds at ceiling to roughly 64–128k tokens and then falls
  away sharply. And near the ceiling the dominant failure is **refusal, climbing to 79–90%** —
  not fabrication, which measured *exactly zero* across 5,760 absent-fact probes. So budget for
  "it declined to answer", not for "it made something up": if an agent starts refusing or
  hedging on work it managed earlier, suspect context pressure before suspecting the prompt.
  A 1M window does not exempt you — degradation was severe at 100k in windows many times that.
- **Compact with a ledger, not blindly.** Before a compaction/`/clear` decision, take
  stock of what's actually in context — the durable artifacts, the files still needed,
  and the large stale tool outputs — and evict the stale bulk first (raw tool output
  older than a few turns is worth replacing with its one-line conclusion; the subagent
  already returned that conclusion, so keep only it). Deciding what to drop from an
  explicit inventory beats hitting a blind threshold. Less context frequently *beats*
  full context on accuracy, not just cost — verbatim old tool spew distracts.
- **`/clear` liberally.** Between unrelated tasks, clear. **After two failed corrections
  on the same issue, `/clear` and rewrite the prompt** — a clean session with a better
  prompt beats a long session full of accumulated corrections.
- **Subagents are context firewalls.** A subagent that greps 40 files and reads 10
  returns a ~200-token conclusion; the 30k tokens of file dumps die with it and never
  touch main context. This is the single biggest lever.
- **Just-in-time, not dumps.** Hold references (paths, queries); load content on demand
  with targeted reads/grep/`head`/`tail`. Don't pre-load whole files.
- **Cost is payload × cadence.** A 15k-token payload re-sent after every step of a six-step
  flow is 90k, and loses to a 40k one-shot read — so measure the *flow you will actually
  run*, not one call. This is what makes a tool that "returns less per call" the more
  expensive option, and it is invisible until you count the repeats.
  `skills/evaluate/scripts/measure-read-cost.ts` prices a read set (`--repeat` for cadence,
  `--budget` to gate it) so this stays a measurement rather than a feeling.

## 3. The fan-out law — the rule that decides every delegation

> **Fan out for READ. Keep WRITES single-threaded — unless you have mapped the
> dependency graph and can name the partition.**

This is the one law to internalize. It's where both Anthropic and Cognition landed after
a year of production experience.

- **WIN — parallel read-only subagents:** search, explore, retrieve, analyze, review.
  They're independent, their verbose output would otherwise pollute your window, and
  context isolation improves quality. This is what viby-toolkit's `scout`/`reviewer`/`skeptic`/
  `debugger` agents are — all read-only by design.
- **TRAP — parallel writers:** two agents editing in parallel from partial context make
  conflicting decisions and produce incoherent results you then pay to reconcile. Coding
  is far less parallelizable than research. **Keep writes on a single thread.** Use extra
  agents for *intelligence, not actions.*
- **CORRECTION — the trap is naive partitioning, not parallel writing as such.** Stated
  flatly, "never fan out to write" is too strong, and honesty requires recording the
  counter-evidence. Co-Coder (arXiv 2606.00953) parallelised repository-level *coding* and
  beat both sequential and file-based-parallel baselines — and Claude Code with Agent Teams —
  by **+14.0% pass rate, up to 2.10× wall-clock, and −35% API cost**, across 28 real tasks on
  DevEval and CodeProjectEval, *with the largest gains on the most dependency-dense projects*.
  What made the difference was not more agents but **how the work was cut**: build the
  dependency graph by static analysis, **isolate the structural hub files**, partition along
  community boundaries rather than by file or by feature name, and schedule
  dependency-aware. Framed properly it is a graph-partitioning problem trading communication
  against computation — which is the same trade-off the MAST finding below measures the cost
  of. So the operative rule is:

  > **Do not parallelise writes across a dependency boundary you have not mapped.**

  Default to single-threaded writes, because you usually have not mapped it and mapping is
  work. Parallelise writes only when you can name the partition and the hubs — and take the
  hub files yourself, sequentially, since they are what everything else depends on.
- **HOW to produce that partition** — the operational half this law was missing, and the one
  genuinely transferable idea from the spec-driven agile frameworks: **decide the architecture
  once, scope each work item to files it exclusively owns, and order the items into dependency
  waves.** One agreed architecture is what prevents *semantic* conflict between parallel agents
  (API style, data model, naming, error handling) — conflicts a merge cannot detect because every
  file is individually valid. Disjoint file ownership is what prevents *textual* conflict. Waves
  are what make the ordering explicit rather than hoped for. With all three, fan-out needs no
  coordination; with any one missing, you pay reconciliation.

  This is now checkable rather than aspirational: `skills/plan/scripts/check-plan.ts` reads a task
  list and fails if two tasks with no dependency between them own the same file, if the dependency
  graph has a cycle, or if a task does not say what it owns and how it is verified.
- **The rate-limit reality:** every fan-out spends against your Max limits roughly an
  order of magnitude faster (Anthropic measured multi-agent at ~15× a single chat). Gate
  each fan-out behind: *"is this genuinely parallel AND read-only?"* If no → do it inline
  on the main thread.
- **Don't fan out for what you already know.** Known file, known symbol → read it inline.
  Agent spin-up + its own context costs *more* than a direct read for a single known fact.
- **Effort ceilings — match fleet size to the task.** Simple fact-finding: 1 agent, a few
  tool calls. A comparison or multi-area map: 2–4 agents, ~10–15 calls each. 10+ agents
  only for a genuinely broad audit/migration. Spawning 50 subagents for a simple question
  is the classic waste.
- **The failure is at the seams, not inside the agents.** The MAST taxonomy (Cemri et al.,
  1,600+ annotated traces across 7 multi-agent frameworks) attributes **~36.9% of failures to
  inter-agent misalignment** — communication breakdown, context lost during handoff,
  conflicting outputs, format mismatch between agents. Not one of those is a reasoning failure;
  they are all interface failures, and they are the majority category. So spend your care on the
  brief and the return format, not on the agent's cleverness. Reconciling several agents'
  output is where *you* introduce bugs, which is why writes stay single-threaded.

## 4. Model routing and delegation

Route each unit of work to the cheapest model that can do it *correctly*: **haiku** for
mechanical search and file-location, **sonnet** for read-only fan-out (scouting, one review
dimension, refuting one claim), **opus/inherit** for planning, synthesis, judgement and all
writes to shared code, **fable** for the hardest, highest-stakes calls — authoring a
reproduction test, resolving conflicting verdicts, a subtle security or concurrency judgement.
Cheap models find; the strong main thread decides.

**Escalate on doubt, but do not trust the doubt signal.** Agents are systematically
overconfident: measured, some that succeed only **22%** of the time predict **77%** success
(arXiv 2602.06948). So a subagent's self-reported `confidence` is a weak input — never the
gate. Prefer an **executed check** over any stated confidence, and prefer an **adversarial
framing** when you must elicit a judgement, because reframing assessment as bug-finding
measured the best calibration of the methods tried. Counterintuitively, a **pre-execution**
estimate ("can this be done, and what would make it fail?") discriminated better than
post-execution self-review, despite having less information.

Full routing table, the escalation ladder, and the four-part subagent contract:
`references/model-routing.md`.

## 5. Evidence-gated completion (the anti-"looks done" rule)

> **NO COMPLETION CLAIM WITHOUT FRESH VERIFICATION EVIDENCE.**

Claude stops when work *looks* done; without a check, "looks done" is the only signal.
Before claiming anything complete:

1. **IDENTIFY** the command that proves it (test, build, lint, repro, screenshot).
2. **RUN** it fresh and complete — not from memory of an earlier run.
3. **READ** the full output and exit code.
4. **VERIFY** it actually passed.
5. **CLAIM** only then — *with the evidence attached.*

Red-flag words that mean you're about to violate this: "should," "probably," "seems to,"
and a premature "Done!/Perfect!/Great!". If you didn't run the check, say so explicitly.
Claiming complete without verification is dishonesty, not efficiency.

**Label every claim measured, inferred, or not tested** — in the table or line where it
appears, not in a caveat at the end. "Equivalent to the one we did test" is a legitimate
finding *and* an inference; written as one, a reader can weight it. The reason to label at
the point of the claim is that unlabeled inferences get promoted to results by your own
summary three paragraphs later, and by then nothing distinguishes them.

TDAD nuance: when you verify, run the **specific tests relevant to the change**, named
explicitly — not a generic "do TDD" ritual. Telling an agent *which* tests to check cuts
regressions; a vague TDD lecture makes them worse.

A zero exit code is not automatically a pass — zero tests collected, an all-skip run, a
`|| true`, or a cached result all exit 0. `/viby-toolkit:verify` runs this gate as a
procedure: find the real checks, scope them to the change, exercise the actual behavior,
then screen the output for those silent-pass modes.

The same gate applies to the tests themselves: **a test never observed failing is not known
to test anything.** Coverage proves a line executed, not that a wrong value would be caught
— a suite can be fully covered and still survive nearly every mutation of the code. So see
each new test go red for the right reason before trusting it, and be suspicious of tests
that assert on mocks rather than outcomes (`/viby-toolkit:test`).

## 6. Adversarial verification kills false positives

Any finding shown to the user as a "bug" must survive gates before it surfaces: it must
**quote the exact line** it's about (verified to exist), then a **single fresh-context
validator** — not a same-model majority vote — must confirm it's real, introduced by this
change, and not already handled, with a conservative reject-on-doubt bias. Same-family
model panels share blind spots, so a majority can rubber-stamp a correlated hallucination;
one independent validator that will *execute* a checkable claim beats N agreeing opinions.
A gap-hunting reviewer always finds gaps — so reviewers flag **correctness only** (taste →
`/simplify`), and validators see the claim, not the author's reasoning. Prefer a
fresh-context reviewer over self-review: models are weak at judging their own output.
Full protocol in `review-cluster`.

## 7. Compounding — each solved problem makes the next cheaper

When you solve something non-obvious, or the user rejects a review finding as unwanted,
**record the lesson to the project's Claude memory** (see the `learn` skill) so future
sessions don't re-research it or re-flag it. This is the compound-engineering loop,
adapted to Claude's native memory so it's portable and needs no extra infrastructure.

## 8. Skill libraries degrade by OVERLAP, not by size

Adding a skill is not free, but the cost is not where intuition puts it. Measured over a
growing skill library (arXiv 2605.24050), pass rate fell up to 21% at 202 skills — and the
mechanism was **skill shadowing**: a skill whose description semantically overlaps another's
hides it from selection, exactly like variable shadowing. The fraction of runs invoking the
right skill fell from 88% to 53%. Meanwhile the cost of the extra *context* was
"statistically indistinguishable from zero".

So the rule is not "few skills". It is **distinguishable descriptions**:

- Every description says what the skill is for **and what it is not for**, naming the sibling
  to use instead. A mutual cross-reference is the disambiguation.
- No two skills claim the same literal trigger phrase.
- Check it mechanically rather than by eye —
  `skills/principles/scripts/check-skills.ts` measures pairwise similarity and flags trigger
  collisions. It is part of the repo's own pre-push gate.

This also corrects a tempting mistake: trimming an always-on preamble is worth doing for
redundancy, but *not* on the theory that its tokens degrade selection. They do not measurably.
Overlapping descriptions do.

## 9. Authored vs derived — know which of your artifacts is which

Work produces two kinds of artifact, and treating one as the other is how a session ends up
confidently reasoning from a stale map. **Authored** artifacts are the *why* — decisions,
lessons, requirements, contracts, plans (`learn`, `plan`, `api`): written with the user,
reviewed, durable, the source of truth. **Derived** artifacts are the *what* — maps, indexes,
scan output, caches, generated clients: rebuilt from the code by a command, disposable, and
never a source of truth. Keep them layered rather than merged, so neither has to pretend to
be the other; they connect by stable reference (`file:line`, an ID), never by copying each
other's content, because a copy is what goes silently stale.

- **Stamp a derived artifact with its provenance** — the commit or date it was built from and
  the command that rebuilds it. Without those, staleness is invisible and the thing can only
  be believed, not checked. An `explore` map is derived: stamp it. And when derived disagrees
  with its source, the source wins — regenerate rather than hand-patch, because patching
  quietly converts it into an authored file no command can reproduce.
- **Don't commit what a command can rebuild** unless rebuilding is expensive; commit the
  *guidance* on when to trust it and when to verify it, which is the genuinely authored part.
- **A heuristic derived artifact is a planning aid, not evidence.** Fine for "where should I
  look"; never the proof in an evidence-gated claim (§5), and never the basis for a change to
  auth, payments or migrations without verifying the specific edge you relied on.

## 10. Portability & secrets

This toolkit syncs across work and personal machines via a private Git marketplace.
- **Never hardcode secrets, tokens, internal hostnames, or client names** anywhere here.
- Behavior must be **project-agnostic** — detect the stack at runtime.
- Per-project overrides live in that project's `.claude/`, never in this repo.
- Keep any always-loaded text (this plugin's SessionStart injection, a project CLAUDE.md)
  lean: for each line ask *"would removing this cause a mistake?"* If not, cut it — a
  bloated always-on context makes Claude ignore the instructions that matter.
