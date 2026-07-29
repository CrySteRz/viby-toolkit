# Changelog

## 2.6.0

The four gaps from the last review, closed — three of them fully, the fourth reduced to a
ten-minute task that only a human can do.

### 1. The agent that could not search (a verified defect, now fixed)

`study` instructed a multi-angle web-research fan-out while **every agent in the library was
filesystem-only** — `Read, Grep, Glob, Bash`, no web tools. The fan-out it described had nowhere to
go, so every search had to run on the main thread, dumping raw results into the context subagents
exist to protect.

- **New `researcher` agent** (`WebSearch`, `WebFetch`, sonnet, read-only): runs **one** search
  angle, opens primary sources, and returns findings that each carry a verbatim supporting quote,
  its URL and the fetch date, plus the searches it ran and its stopping reason. Its own Iron Law is
  the one from `study`: a working link is not evidence, because citations keep links valid >94% and
  on-topic >80% while only 39–77% support the claim. Wired into `study` §5 and `evaluate` §10.
- **`check-references.ts` now verifies capability, not just existence.** It checked that a named
  agent *exists*; it could not see that the agent *cannot do the thing*. It now maps each agent's
  granted tools and flags a skill that instructs a capability none of its named agents has.
  Verified non-vacuous: replaying the pre-fix state, the check fires.

### 2. Two coverage gaps in the skill library

- **`/viby-toolkit:ui`** — a claim about the interface needs an *observation*. The markup can be
  correct and the page still blank, and nothing in the diff shows it. Picks the driver by lifecycle
  stage rather than preference, charges cost as **payload × cadence** (a driver that re-sends the
  DOM after every click pays it per step), treats shipping real user data to a third-party service
  as a hard gate, and forces the states nobody builds first — empty, loading, error, too-much-data,
  two widths, keyboard-only, focus-after-action. Reports screenshot + console + network, or it did
  not happen.
- **`/viby-toolkit:extend`** — the meta-skill, written because five modules got built by hand this
  month and each one rediscovered the same rules. Earn the slot (a new skill needs a decision the
  others get *wrong*), write the description as real user phrasings with mutual cross-references,
  never state an exclusion in the excluded case's own words, then prove it routes **and** prove it
  doesn't shadow — expecting those two to fight, because adding trigger phrases *is* the shadowing
  mechanism.

### 3. First contact with real, non-toy repositories

Every previous dogfood target was this repo: TypeScript, zero dependencies, written by the same
process being tested. Pointed the checkers, read-only, at real work repositories — and the first
one produced a confident wrong answer about what a repo *is*:

- **A Kubernetes GitOps repo — 183 YAML manifests, one Python script — was reported as
  "Python 100%".** Technically true, and the worst output this tool can give. `detect-stack.ts` now
  censuses repo *shape* separately from language, names the infra flavour (kustomize, Helm,
  Terraform, Compose, Ansible), leads with `repo shape CONFIG/INFRA` when config outnumbers code
  4:1, and annotates the language line as "the code sliver only, not what this repo is". Both
  halves tested: an infra repo must be flagged, an ordinary TS project with a package.json and a CI
  file must not.
- **The test scanner ran clean on a real TS suite** (exit 0) — no false-positive storm, which is
  the precision work holding up outside its own fixtures.
- **The migration linter found 74 issues across 62 real migrations** with a healthy distribution —
  `index-without-concurrently` 36, `no-lock-timeout` 22, `ddl-and-backfill-together` 9,
  `unbounded-dml` 4, and the two irreversible ones (`drop-column`, `drop-table`) exactly once each.
  No single rule flooding is the signal that matters.

### 4. Live routing: reduced, not closed

Still the one honest gap, and still needs a human in fresh sessions. What changed is the friction:
`routing-probes.md` now has a **10-minute version** — the ten probes covering the pairs most likely
to collide and the skills that had no lexical hook before v2.5.0 — with the note that a single
logged row beats a perfect empty table.

### Also

Three probes added for the new skills (33 total, all ranking first), and a flaw fixed in the probe
scorer itself: it stoplisted "skill" and "toolkit", which are exactly `extend`'s distinguishing
vocabulary, making its own probe unscoreable. IDF already discounts common words, so the stoplist
only needs to remove English scaffolding.

## 2.5.0

Nine skill descriptions fixed, and a new pre-screen that found the problems. **Live routing is
still unverified** — `tests/routing-probes.md` still needs a human in fresh sessions — but the
descriptions are measurably better than they were, and one structural defect is gone.

### New — `tests/score-routing-probes.ts` (gate 17)

Scores all 30 routing probes against the shipped descriptions by IDF-weighted, length-normalised
word overlap. It is **a proxy and says so on every run**: real dispatch is a model reading the whole
listing, not word overlap, so this neither passes nor fails the routing claim. What it does catch is
the defect most likely to cause a mis-route and hardest to see by eye — a description that omits the
phrasing a user actually reaches for. The contract is "every probe's intended skill ranks first";
thin margins are advisory, since several probes are deliberately ambiguous.

**First run: 13 of 30 probes mis-ranked, and four skills scored zero on their own probe** —
`explore` had no hook for "where does X actually happen", `verify` none for "is this ready to ship",
`observe` none for "we can't tell what's happening", `test` none for "write tests for this module".

### The structural finding

**`brainstorm`'s description named the case where it must NOT fire** — "a clear, well-specified
ticket/spec" — which made it a lexical magnet for exactly those requests: it beat `orchestrate` on
`orchestrate`'s own probe ("build the CSV export from the ticket — spec is clear") by 5×. A negative
condition attracts the requests it is meant to repel. It is now phrased as "skip it entirely once
the WHAT is settled", and the specifying words moved to `orchestrate`, where they belong.

### Changed

Missing user phrasings added to `explore`, `verify`, `test`, `observe`, `handoff`, `worktrees`,
`debug`, `plan` and `learn`; `brainstorm` and `orchestrate` rebalanced as above. All 30 probes now
rank correctly.

### And the predicted side effect, caught by the repo's own gate

Adding trigger phrases raises description overlap, which *is* the shadowing mechanism — so the
skill-library check immediately flagged `test`+`verify` as newly confusable. Fixed the way the
doctrine says: their cross-reference was one-sided, and one-sided is not enough, so `verify` now
names `test` back ("designs and writes tests — this runs the project's real checks"). That both
clears the exemption and genuinely disambiguates the pair.

### Two limits worth keeping in view

- Probes naming a specific product (#22, "Playwright") have no lexical hook in any description and
  are reported as **proxy-blind**, not as defects. Stuffing product names into descriptions to
  satisfy the script would make the library worse.
- A green pre-screen is not a passing routing test. The results table in `routing-probes.md` is
  still empty.

## 2.4.0

New module: **`/viby-toolkit:adopt`** — taking on code you did not write and do not trust yet,
conforming it to instructions and to the target language's idiom with agents, and proving at the
end that the required functionality is still there. Researched 2026-07-29; sources labelled
fetched vs search-summary in `skills/adopt/references/methods.md`, including one paywalled paper
recorded as unread rather than cited.

### The two measurements the module is built on

- **Repository-level refactoring fails more often than it succeeds.** On *SWE-Refactor* — 1,099
  developer-written, behaviour-preserving refactorings from 18 Java projects — the best model
  managed **41.58%**, and an agent **39.4%** on compound cases. Roughly six attempts in ten fail.
  Multi-agent workflows helped more than any other single strategy.
- **Watching for shortcuts makes agents better, not just honester.** Trajectory-level behaviour
  monitoring "reduces average hacked-resolved rate from **28.57% to 0.56%**, while improving clean
  resolved rate from **40.22% to 60.53%**" (arXiv 2606.26300, fetched). Without it, ~28.6% of
  solutions that passed the verifier had reached green through a shortcut channel.

So: fan out aggressively, gate mechanically. Under that much failure pressure the cheapest path to
green is to edit the check rather than the code — documented shortcuts include deleting the test
file and inserting `sys.exit(0)` to leave the harness successful.

### The pipeline

1. **Provenance gate first** — licence (may you even ship it?), secrets in history (rotate: they
   are compromised on arrival), runtime behaviour, dependency typosquats. Stop and report rather
   than refactoring around an unresolved licence.
2. **Map it and find the seams**, assuming there are no usable tests — and if some exist, prove
   they fail when the code is broken before trusting them.
3. **Capture behaviour before editing**: characterization / golden-master / approval tests that
   pin what it *did*, explicitly without judging whether that is correct; real recorded inputs over
   invented ones; differential testing against the still-runnable original, which gives an oracle
   without a spec — precisely what inherited code lacks; sprout and wrap before cutting.
4. **The functionality matrix** — every requirement with its verification method and pass
   criterion, traceable both ways — and **a held-out slice the agents never see**, the split
   benchmarks use to catch agents that overfit to visible tests.
5. **Mikado steps**: attempt naively, and when it breaks record the prerequisite and **revert**.
   The discarded attempt bought the dependency graph, which is also the partition the fan-out law
   requires before any parallel writes.
6. **Conform in priority order**: explicit instructions > the target language's own idiom >
   existing project conventions — and never carry one language's patterns into another. Mechanical,
   structural and behavioural changes in separate commits.
7. **Agents with a monitored trajectory**, then the acceptance gate: characterization green, drift
   check clean, held-out suite green, every matrix row demonstrated, `verify` over the lot.

### New — `check-test-drift.ts` (22 contract cases, 16 gates)

Compares the test suite between two git refs and answers the question a green run cannot: **did
the safety net shrink?** Test files gone, cases gone, assertions gone, skips added, `.only` added,
or a zero-status exit inserted. Counts across TS/JS, Python, Go, Rust and JUnit-style suites.
Reports on this repo's own history: 145 → 194 tests, 182 → 262 assertions since v2.2.0.

It states its own limits on every run: an assertion weakened in place keeps the count, and nothing
is executed, so a clean result proves the net was not cut — never that behaviour was preserved.

**A move is not a deletion.** Suite-wide counts decide severity, so reorganising files is silent
while genuinely removed coverage is P1.

### Found by running the new tools on this release

- **A silent-pass mode in the drift checker**, caught by pointing it at a tag that doesn't exist:
  an unresolvable base ref produced an empty baseline and reported "0 → 198 tests, the net grew" —
  a perfect score for a typo. Now exit 2 with "NOTHING was compared".
- **Overlapping assert patterns double-counted** `self.assertEqual(...)`, and bare Python/Rust
  `assert x == 1` wasn't counted at all. A miscount that differs between refs invents drift that
  isn't there or hides drift that is.
- **Three false-positive classes in `check-study.ts`**, found by auditing this release's own
  research notes: a section headed **"Not verified"** matched the "measured" pattern, so the most
  honest paragraph in the document was flagged as a dressed-up result; only a paragraph's *first*
  line was kept as its citation, so a source on the second line read as unsourced; and multi-line
  blockquotes were split one block per line, cutting a quotation off from the citation that
  introduced it. Attribution now also extends to the paragraph *directly* above — bounded there, so
  one URL still cannot source a whole section — and dates are matched per block, since a wrapped
  citation carries "fetched <date>" on its second line.

## 2.3.0

New module: **`/viby-toolkit:study`** — you bring an idea, it produces the protocol first
(question, competing answers, the observation that would exclude one, the stopping rule) for
approval, then the study document. Version decided by the tool again: five added exports,
nothing removed → minor.

Researched 2026-07-29 across nine sources, of which four were fetched and read directly and the
rest are search-summaries — labelled individually in `skills/study/references/methods.md`,
including one **fetch failure recorded rather than hidden** (the Garousi MLR paper: PDF returned
unparseable binary, abstract page had no criteria, so only its definition is cited).

### The finding that shaped the whole module

Deep-research citations keep **link validity above 94%** and topical **relevance above 80%**
while achieving only **39–77% factual accuracy** — the cited page exists, is on-topic, and does
not say what it was cited for ([arXiv 2605.06635](https://arxiv.org/html/2605.06635), fetched).
Reported citation-hallucination rates across deployed models run **11–57%**.

So checking that a link resolves is nearly worthless as verification, and the Iron Law is: every
claim carries the **quoted sentence** that supports it, with the date it was fetched. That is the
same grounding gate `review-cluster` already applies to code findings, pointed at prose.

### What the skill is built on

- **Decision before study** — value-of-information reasoning: perfect information is worth only
  what it changes, so if every answer implies the same action, don't run the study. Depth scales
  to the cost of being wrong.
- **PICOC** (Kitchenham & Charters) to turn an idea into an answerable question. Comparison and
  Context are the two people skip, and the two that make an answer transferable to *your* case.
- **Protocol before searching**, preregistration-style — and the framing that makes it usable:
  *a protocol is not a promise to follow it, it is a promise to be transparent about where you
  deviated*, with an explicit Exploratory section. This is the anti-HARKing device: a hypothesis
  formed after the evidence but presented as the motivation is how false conclusions get shipped.
- **Strong inference** (Platt 1964, on Chamberlin 1890): list 2–4 competing answers, then design
  the observation that **excludes** one. If no finding could exclude anything, it isn't a study
  design. Chamberlin's stated motive is the reason it's in there — you fall in love with a
  favourite hypothesis and fit all evidence to it, which feels exactly like doing research.
- **Search on several angles, then snowball** (backward through references, forward through
  citers), and **stop by a rule you named**: saturation, effort-bounded (state the N), or
  exhaustion. "I got tired" is effort-bounded — label it that way.
- **One query that argues the opposite**, because leading phrasing skews retrieval and pulls a
  model toward the stance it was handed.
- **AACODS** for source appraisal (Authority, Accuracy, Coverage, Objectivity, Date,
  Significance) — built for grey literature, which is where engineering knowledge actually lives.
  Then **GRADE**'s four transferable downgrade domains, with **indirectness** promoted to the big
  one: a number measured on someone else's stack is the default situation, not an edge case.
- **Vendor numbers are a claim about a different question.** Documented gap: best systems ~14%
  Precision@1 on an independent benchmark versus 98%+ claimed by vendors on nominally the same
  task. A vendor figure is a hypothesis to reproduce, never a result to cite.
- **Audit the trajectory, not the answer** (PING: propagation, intent, noise-induced, grounding).
  A mis-framed sub-question reaches the conclusion looking clean.
- **Sources rot** — 38% of 2013 pages gone, 25% of pages that existed 2013–2023 gone, 54% of
  Wikipedia pages carrying a dead reference — so quote and date-stamp at fetch time.

### New — `check-study.ts` (28 contract cases, 15 gates)

Audits a research document's form: unsourced figures, hedged language inside a section presented
as measured, missing status line / falsifier / stopping rule / evidence labels, single-domain
sourcing, undated citations. It deliberately does **not** score quality — LLM-defined rubrics
misalign with expert judgement, are coarse, and push the judge onto unverifiable knowledge — and
it prints on every run that it checks **form, not truth**, since it cannot tell whether a quote
is real, which is the check that matters most.

Two design decisions worth recording:

- **Mode is inferred from path and title, never from the document's own fields.** A study missing
  its status line must not thereby opt out of the other checks — one absent field disarming the
  gates meant to catch it is a defect class this repo has shipped once already.
- **In a table, every row carries its own source.** Strict on purpose: that rule caught a
  genuinely unsourced row in this release's own evidence table, which whole-table attribution
  would have passed.

### Found by running it on its own documents

Five real defects, four in the checker and one in the prose:

1. A `/g` regex used with `.test()` — `lastIndex` advances between calls, so the same input
   alternates true and false.
2. `2026` is a four-digit number, so **every date-stamped citation tripped the unsourced-figure
   check** — the checker punished the exact habit it exists to encourage.
3. Mode inference was **backwards**: the reference file was held to the full study contract
   because its title contains the word "study", while the real decision record got the weaker
   audit.
4. Blank lines were treated as *starting* a block, so no paragraph after a blank line was ever
   recognised as a block start and "the sentence introducing this list" never resolved — every
   properly-introduced list of figures reported one finding per bullet.
5. The intro sentence then persisted document-wide, so one early URL silently sourced every list
   and table below it. Now bounded to the block directly beneath it.

## 2.2.0

Closes the three items that were still open: the missing checker, the unvalidatable claim, and
`evaluate` never having been run on a real decision. **This is the first release whose version
number was decided by a tool rather than by eye** — `check-api-surface.ts --base v2.1.0` reports
five added exports and nothing removed, so: minor.

### New — `check-api-surface.ts` (the missing half of `release`)

`release` says the version number is a promise about the public surface, and then asked for a
judgement it gave no tool for. This computes the input: added / removed / re-signatured exports
between two git refs, for TS/JS, Python, Go and Rust.

- Tells a **positional parameter rename (P2)** from a **real signature change (P1)** — calling a
  rename "MAJOR" is how a differ gets switched off, and then it protects nothing.
- Respects each language's own visibility rule: Python `_private` and an explicit `__all__`, Go
  capitalisation, Rust `pub`. `export { a as b }` records the **alias**, since that is what a
  caller imports.
- **Reports every `export * from` barrel it could not follow** instead of silently excluding it.
  Following a re-export needs module resolution, so this has the same blind spot as a
  tree-sitter code graph — and a surface report that quietly omits part of the surface is worse
  than no report, because it gets trusted.
- Prints on every run that it sees **syntax only**: a signature that held while its meaning
  changed is a major break it will call a patch. A `major` verdict is authoritative; a
  `minor`/`patch` still owes the behavioural read. Wired into `release` §2 and `api` §5.
- 20 contract cases; 14 gates now.

### New — `tests/routing-probes.md`, the one test that cannot be automated

30 prompts written the way work actually arrives, each paired with the skill that should fire,
plus a scoring rubric (≥80% right, zero repeat mis-routes on the same pair) and what to do with
each failure mode. It is user-in-the-loop by construction: only the human can see which skill
loaded, and asking the model to self-report its own routing is precisely the self-assessment
this toolkit treats as a weak signal. Three probes are deliberately ambiguous, including one
where `none` is the correct answer.

This closes a gap that was **mechanically impossible to test until now**: the installed plugin
sat stale at 0.3.2 for the whole of development, so the descriptions under test were never the
descriptions installed. The results table ships empty and honest — until it has rows, live
routing remains unverified and the limitations section keeps saying so.

### New — the first real `evaluate` run

`docs/decisions/2026-07-28-api-surface-extraction.md` is the skill applied to the decision the
differ itself required (regex over blanked code vs the TS Compiler API vs diffing export lines),
not a hypothetical. It doubles as the worked example the skill lacked.

**It refuted its author.** The design assumption was that extracting on blanked code is strictly
correct, per this repo's rule *decide on parsed code, never raw text*. Two constructs carry their
value **inside** a string literal — a re-export's module path and Python's `__all__` — which the
blanking pass erases: the barrel path came out as ten NUL bytes, and `__all__` produced an
**empty surface**, which would have reported every symbol as removed. The rule needed splitting,
not abandoning: **decide WHERE from the blanked code, read WHAT from the raw text at the same
offset** — which works only because the blanking pass preserves offsets, a property that was
incidental and is now a commented contract. Both bugs were caught by tests written from the
oracle before the fix.

### Changed

- **`evaluate`** — make the oracle **executable** where you can (fixture + assertion, not a
  paragraph). Added because the run above only caught the refutation thanks to executable
  fixtures, and that should be a step rather than a lucky habit.

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

**Breaking: the plugin was renamed to `viby-toolkit`** (from its one previous name). Every
skill is now invoked `/viby-toolkit:<name>`, and the installed plugin id is
`viby-toolkit@viby-toolkit`. Nothing else changed — no skill was added, removed or rewritten
in this release. Major, not minor, because renaming every entry point is the definition of a
breaking public-surface change, and this repo's own `release` skill says the version number is
a promise about that surface rather than about the size of the diff.

The old prefix resolves to nothing rather than failing loudly, so a stale reference in a
project `CLAUDE.md` shows up as a skill that mysteriously never fires.

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

- Live skill routing is unvalidated (above). As of 2.2.0 there is a way to test it —
  `tests/routing-probes.md` — but its results table is still empty, so the claim stands.
- No mechanical check catches a skill body that contradicts its own Iron Law; one such
  contradiction was found by hand, and a second by review.
- `swallowed-error` is the least trustworthy check — 0 of 12 sampled were real on CPython
  before its exclusions were added. It is P2 and phrased as a question.
- Cross-file delegation cannot follow a dynamic import or a base class installed at runtime.
- JSX text nodes are not a string context, so prose inside `<div>…</div>` can still be flagged.
