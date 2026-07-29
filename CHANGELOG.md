# Changelog

## 2.14.0

`docs` gets its executable half. 16 checkers, 24 gates — every skill that has a mechanically
checkable claim now has a checker behind it.

### New — `check-docs.ts` (12 contract cases)

`docs` says every command, path and claim must be one you actually ran, because a wrong command in a
README costs more than ten missing pages — a stale doc is *trusted*. This automates the checkable
part: documented paths that do not exist, `npm run` scripts that are not defined, dead relative links,
and dead anchors (a heading that was renamed).

**76 findings on this repo's own docs, and every single one was wrong.** Three distinct bugs, each
worth the fix:

- **`make` matched English prose** — "make it readable", "make a decision" — 48 times. Only explicit
  run-forms (`npm run`, `pnpm run`, `yarn run`, `bun run`, `make -C dir target`) count now.
- **A regex flag in backticks looked like a path.** `` `/g` `` starts with a slash, so it resolved as
  an absolute path and failed. A reference now needs a real segment before the slash and an extension.
- **28 "stale" paths were real files one directory deeper.** Guessing a base directory cannot work: a
  document legitimately cites paths relative to the repo root, a package subdirectory, or its own
  folder. Resolution is now by **suffix against an index of the repo's real files** — the same
  self-calibration `check-memory.ts` needed, arrived at from the same failure.

After the fixes: this repo is clean, and on four real repositories two are clean while a docs-heavy
one surfaced **84 genuine problems** — dead links to transcript files that no longer exist, a renamed
heading, and references to paths that live in a sibling repository. Exactly the class where a reader
follows a link, finds nothing, and stops trusting the rest.

## 2.13.0

`observe` gets its executable half, and the memory stores get repaired. 15 checkers, 23 gates.

### New — `check-logging.ts` (17 contract cases)

`observe` was pure doctrine. Its P1 is the part that is not a style question at all: **personal data
or a credential reaching the logs**, and whole request/user objects being logged, where whatever the
object contains next month goes with it. A log line is the least access-controlled artifact a team
produces — it fans out to aggregators, alerting, screenshots and third-party dashboards, and it is
retained long after the request is gone. Nothing errors when an email address lands in it.

Then the existing doctrine, made mechanical: interpolated messages with no structured fields, a
`catch` that logs without the caught error (the stack is the whole value of the line at 3am),
arrival logs, identifier-shaped values used as **metric labels** — the cost trap `observe` already
warned about — logs in tight loops, and files with several events and no correlation key.

**Benchmarked on this repo and three real ones, and it was wrong twice before it was right.**

- **44 findings on this repo across 17 files, every one false.** They were checkers printing their
  own results in a loop: a CLI's stdout *is* its interface, not telemetry, and a CLI has no request
  to correlate. Rules that are meaningless in a command-line tool are now skipped there — PII rules
  still apply, because printing a secret is a disclosure wherever it happens. 44 → 0.
- **The single `sensitive-in-log` on a real payments codebase was also false**, and worth being
  precise about: it was a **Stripe checkout session id** — a resource identifier that debugging a
  payment requires, not an auth credential. `sessionId` is no longer treated as sensitive;
  `session_token`, `session_secret` and cookies still are. Conflating a resource id with a credential
  is exactly how a rule teaches its reader to ignore it.

And the same shape as five previous defects, met a sixth time: two rules read *inside* the string
literal — the interpolation markers of an unstructured message, the word "entering" in an arrival log
— which the blanking pass removes. Resolved the documented way: locate on parsed code, read the value
from the raw line, and only offer the raw line to a rule when the *blanked* line proves it is live
code. Comments and fixtures stay inert.

### Repaired the memory stores it audited last release

Acting on v2.12.0's findings across five real stores: **4 entries were missing from their index** and
have been indexed using their own frontmatter descriptions as hooks — by the retrieval finding, an
unindexed entry effectively does not exist. All five stores are now free of findability defects.

And the reported duplicate pair turned out to be **another false positive in the checker**: under a
`project_<name>_<topic>_<date>` naming convention the shared project and date tokens dominate the
comparison, so two genuinely different notes about the same project on the same day looked identical.
Now compares only the distinguishing tokens.

## 2.12.0

New module: **`/viby-toolkit:brain`** — the memory system as an architecture with maintenance, rather
than a place `learn` writes to. 31 skills, 14 checkers, 22 gates. Researched 2026-07-29; sources in
`skills/brain/references/methods.md`.

### The taxonomy, and what it revealed about this library

The agent ecosystem converged on the taxonomy cognitive science has used for decades — **episodic /
semantic / procedural** — and mapping it onto what already existed here was the clarifying part:
`handoff` is episodic, `learn` is semantic, and **the skills are procedural memory**. Two consequences
are now doctrine: editing a skill is a memory operation (so it belongs to `extend`), and **most things
people want to "remember" are episodic** — which is the single biggest cause of a store full of things
that used to be true.

### Three findings that shaped the rules

- **Retrieval failure is where the errors are.** Across LongMemEval, LoCoMo, STALE and PersonaMem,
  answer errors concentrate in cases where retrieval failed; retrieval succeeding while the answer is
  still wrong accounts for only **5.8–13.7%**. So the lever is findability — naming, indexing, entry
  size — not richer entries. An entry nobody retrieves has no effect at all.
- **Staleness is the hard, under-measured failure.** A benchmark exists for exactly the question "can
  agents know when their memories are no longer valid?", and the gap it names is recognising that a
  previously valid memory has been **rendered obsolete by a structurally related but differently
  worded observation**. In a coding project a useful slice of that is mechanical: the memory cites a
  path that no longer exists.
- **Poisoning is defended with provenance.** The proposed mechanism is reliability-conditional
  updating with a **provenance cap** — an entry's influence bounded by how well it is sourced. You
  cannot cap what is not recorded, so provenance, date and outcome are now mandatory per entry. And
  storing what the user *said* as though it were established is its own benchmarked failure
  (sycophantic memory).

### New — `check-memory.ts` (15 contract cases)

Audits a store: stale path references, undated entries, entries with no provenance, hearsay recorded
as fact, entries missing from `MEMORY.md` (and index links pointing at nothing), near-duplicate
topics, and entries that have grown into documents.

**Benchmarked on five real memory stores, 53 entries — and the benchmark corrected it twice.**

1. The first run reported **23 stale references**. Almost all were false: a store's relative paths
   belong to *its* project, and every store had been resolved against one repo root. Now
   self-calibrating — if nothing in a file resolves, it reports `root-unknown` instead of inventing
   findings. 23 → 6.
2. Those 6 included two in this repo's own memory that were *also* root artifacts: a memory about a
   project legitimately cites paths from the repo root, a package subdirectory and home. Fixed by
   trying the root and two ancestors. 6 → 0 false.

Real signals across the five stores: **24 undated, 34 without provenance**, 4 entries missing from
their index, 1 duplicate pair — i.e. exactly the two fields the poisoning defence requires are the
two the existing store mostly lacked.

### Acted on its findings here

This repo's own memory entry was **1,641 words** — a document, not a memory, which drags the whole
thing into context on any partial match. Split into a core entry (560 words) and a linked inventory,
both indexed; the undated entry now carries when and how it was established. The inventory is *still*
flagged as oversized and is **kept deliberately, with the reason recorded in the entry**: it is a
lookup table retrieved on purpose, and the rule is right for narrative memories and wrong for an index.

## 2.11.0

A second, deeper pass over the ecosystem — this time the famous *planning* frameworks (BMAD, the
Kiro-style spec-driven workflows, agent-os) rather than the marketplaces. Two ideas were worth
taking, one of them completes a law this toolkit has stated since v0.1 without ever explaining how
to satisfy it. 13 checkers, 21 gates.

### The idea that completes the fan-out law

`principles` §3 has always said: parallelise writes only when "you can name the partition and the
hubs". It never said **how to produce one**. The agile-AI frameworks answer it directly, and the
formulation is worth quoting because it is the whole recipe:

> a shared architecture, stories scoped to owned files, a dependency-ordered wave plan … One
> architecture prevents semantic conflicts (API style, data model, naming, security) across agents.
> Stories scoped to disjoint files, dependency-ordered into parallel waves, prevent file/merge
> conflicts.

Three ingredients, each preventing a different failure: **one agreed architecture** stops *semantic*
conflict — the kind a merge cannot detect because every file is individually valid; **disjoint file
ownership** stops *textual* conflict; **dependency waves** make the ordering explicit instead of
hoped for. §3 now carries that recipe.

### New — `check-plan.ts` (16 contract cases)

And it makes the requirement mechanical instead of aspirational. `plan` now writes a dispatchable
task list — each task naming the files it exclusively owns, its verification, and its dependencies —
and the checker fails on:

- **`unpartitioned-file` (P1)** — two tasks with no dependency between them owning the same file.
  They can be dispatched in parallel and they will conflict. This one check is why the script exists.
- **`hub-file` (P2)** — a file touched by three or more tasks: a structural hub, which §3 says to
  take yourself, sequentially.
- **`dep-cycle` / `unknown-dep` (P1)** — no execution order exists, or a task depends on nothing real.
- **`no-files` / `no-verify` (P1)** — a task that cannot be partitioned, or whose "done" is a feeling.

Transitive ordering counts, so `T1 → T2 → T3` sharing a file is fine; only genuinely concurrent
tasks are flagged. The checkboxes double as the durable progress ledger a cleared session resumes from.

### The second idea: gates exist so review happens THERE

The spec-driven frameworks' real insight is not the documents, it is *where* review happens:
"instead of reviewing every individual edit during implementation, you could review at structured
phase gates", which reduces approval overhead and gives predictable intervention points. `plan` now
frames its output that way.

### Deliberately rejected, with the bar each failed

- **21 agents, 50+ workflows, personas, "party mode".** Contradicts `principles` §8 — measured
  shadowing cut correct skill selection from 88% to 53%, and this library's whole thesis is that
  overlap is the cost, not count. Six agents with explicit model routing already cover the roles.
- **Multiple agents conferring in one conversation.** Attractive, but the MAST finding is that ~36.9%
  of multi-agent failures are inter-agent misalignment; putting more agents in one context is the
  reconciliation problem, not a fix for it.
- **"Regenerate the code from the spec when it changes."** Not credible on a real codebase, and it
  contradicts `adopt` and `refactor`, which exist because you cannot regenerate what you cannot
  characterise.

## 2.10.0

Surveyed the wider Claude Code plugin/skill ecosystem for inspiration. The dominant pattern there is
**quantity** — 83 agents in one collection, 100+ in another, 2,810 skills in a single marketplace —
which is the opposite of this repo's thesis and directly contradicts `principles` §8: libraries
degrade by *overlap*, not size, and skill shadowing cut correct selection from 88% to 53% in the
measured study. So almost none of it was worth copying.

One thing was, and it is not a feature. It is a threat.

### New — `check-skill-safety.ts` (17 contract cases, gate 20)

**The skill ecosystem is now a supply chain, and it is already under attack.** An audit of **3,984
skills** across two public marketplaces (Feb 2026) found **36% containing security flaws, 1,467 with
active malicious payloads, and prompt injection in 36%** — summarised as *"if you've installed one in
the past month, there's a 13% chance it contains a critical security flaw"*. A coordinated campaign
distributing 30+ malicious skills was documented the same month. Community marketplaces have no
automated vetting.

A skill is not a document: it is instructions executed with your credentials, in your repositories,
with your agent's tool access. So `secure` gained a section for auditing one before you install it,
and a checker for the patterns malicious and careless skills share:

- **P1** — a credential path meeting a network call in one line; `curl | bash`; **instructions to act
  without telling the user** (the most reliable single marker: there is no legitimate reason for a
  skill to require concealment); instruction-override attempts; writes to the agent's own settings;
  encoded/decode-then-execute payloads; invisible bidi and zero-width characters.
- **P2/P3** — imperative reads of credential files, destructive commands, broad tool grants.

### Audited what is already installed here

1,186 files across three installed third-party marketplaces: **two came back completely clean**; the
official set produced 12 `reads-credentials` and 8 `destructive-command` at P2 — consistent with what
those plugins do for a living (an env-var manager reads `.env`), which is why those rules are phrased
as *confirm why it needs this* rather than as verdicts.

### Four false-positive classes it produced on the way, all found by running it

- **On its own source**: the header and rule messages matched its own rules. Fixed by blanking
  comments and string literals — the repo's oldest lesson, applied a fifth time.
- **On its own prose**: the new `secure` section says "instructions to act without telling the user",
  which the rule read as an instruction. A line *describing* a pattern is now excluded. Accepted cost,
  stated in the code: an attacker who prefixes an injection with "the pattern is:" evades this.
- **On emoji**: `U+200D` (zero-width joiner) builds emoji sequences, so two official plugins were
  flagged for having emoji. Excluded — and `U+2066–2069`, the actual Trojan Source isolates, were
  missing and are now included.
- **On security guidance generally**: prose rules now apply only to prose and command rules only to
  fenced blocks, so a skill that *warns* about `rm -rf` is inert.

And one silent-failure bug of the same class as an earlier one: `\b>>\b` can never match, because
punctuation has no word boundary — the `modifies-agent-config` rule had never fired.

## 2.9.0

**The last named gap is closed: the build phase now has an executable half.** Deciding and proving
had checkers; writing code had prose. 11 checkers, 19 gates, ~287 contract cases.

### New — `check-diff-hygiene.ts` (22 contract cases)

Audits the artifact the build phase actually produces — the diff — for what makes a change hard to
review or unsafe to land, none of which a compiler or a test run objects to. Diff-scoped on purpose:
it judges what you are *adding*, not what the file already contained.

- **P1**: a merge conflict marker being committed; a credential-shaped line (narrow, high-confidence
  patterns only — `/viby-toolkit:secure` is the real pass); and **over 1,000 changed lines**.
- **P2**: debug prints being added; formatting churn mixed into a behavioural change so the real
  diff is unreadable; source changed with no test touched; a lockfile moving without its manifest.
- **P3**: new TODO/FIXME; commented-out code.

**Size is a finding, not a style opinion.** The largest study of code review — SmartBear at Cisco,
2,500 reviews over 3.2 million lines — found detection is best at **200–400 changed lines** and
falls from ~87% under 100 lines to **~28% past 1,000**. A 2,000-line diff is not a bigger review; it
is an unreviewed one.

### Benchmarked on real repositories before shipping, and it mattered twice

Run across four real repositories at three commit ranges each:

- **Clean on three of four single commits** — the property that decides whether anyone leaves it
  switched on — and proportionate on wide ranges, with no rule flooding.
- **False positive #1: every one of the 20 `debug-added` hits was wrong.** They were all
  `console.log` progress output in CLI, migration and seed scripts. A script's stdout *is* its
  interface. Now exempt by path, and all 20 disappeared while every real finding stayed.
- **False positive #2, found dogfooding it on this repo:** `commented-out-code` fired on six
  `SKILL.md` files at once, because `#` is a heading in Markdown. Comment-shaped rules now run on
  source files only — while a credential or a conflict marker is still checked in *every* file type,
  since a leaked key in a README is still a leaked key.

The one finding left on this repo's own recent history is `unreviewable-size` at 2,000 added lines
across 31 files, which is a fair judgement of the commits that built this release.

## 2.8.0

**Benchmarked every checker that had never met code outside this repo.** Five of them hadn't.
Four defects found, all of them the kind only real data exposes.

### The estimator's accuracy claim was false, and is now measured

`measure-read-cost.ts` claimed "±15% on ordinary source" — reasoned, never verified. Installed
`tiktoken` in a throwaway venv and ran **400 real files** (TypeScript, TSX, SQL, YAML, JSON,
Markdown, shell, from four working repositories) through `cl100k_base` as ground truth.

The claim was wrong: **33% of files fell outside ±15%**, and every ratio was biased low —
systematically over-estimating by ~9%. Recalibrated from the measurement (code 3.6→3.95, prose
4.0→4.25, data 3.1→3.55, plus a `.sql` override at 4.15, since SQL tokenises far looser than
general code and was over-estimated by 16%):

| kind | median error | p90 \|error\| | within ±15% |
|---|---|---|---|
| code | −1.1% | 13.5% | 93% |
| prose | +0.1% | 5.4% | 100% |
| data (yaml/json) | 0.0% | 39.7% | 68% |
| **overall** | **−0.5%** (was +8.8%) | 17.5% | **85%** (was 67%) |

The documented claim is now the measured table, and per-file JSON/YAML carries its own caveat —
the errors cancel across a set, so use it on a directory, not on one config file.

**The ground-truth test taught something better than it was written to check.** Four hand-written
fixtures were meant to pin the calibration; they all failed, because **synthetic text tokenises
nothing like real files** — clean repeated prose measured 5.2 chars/token (23% looser than real
markdown, which carries links, code spans and punctuation) and a dense hand-written SQL snippet
measured 3.3 (20% tighter than real `.sql`). So the test now pins the *constants* against their
recorded calibration and pins the lesson itself: **never re-derive these from fixtures you wrote,
because fixtures agree with their author.**

### `check-study.ts`: 38 → 21 findings on real research documents

Run against two genuinely human-written research documents, it produced 38 findings — and **28 of
the 34 `unsourced-figure` hits were in a section headed "Benchmarked on our own repo"**, where every
number *was* the author's own measurement. Demanding an external citation for your own result is
wrong. It now recognises a self-measurement section and attributes figures there to the document
itself. The remaining findings look real, including a genuine missing "what would change my mind".

Then it caught two unsourced research figures in **this repo's own** reference file, which are now
cited.

### `check-test-drift.ts`: a rename is not a regression

On a real 30-commit range it correctly read the suite as grown (116 → 140 tests, 173 → 265
assertions) but reported 4 P2 findings for 4 renamed files. A move is now **P3 when the suite grew
on both counts** — "the net is bigger than it was" is strong evidence nothing was lost — and stays
P2 when it didn't.

### Clean on first contact

- **`check-release.ts`** on four real repos: every finding true (real dirty trees, real missing CI),
  no noise, no crashes.
- **`check-api-surface.ts`** on real history: correct additive-MINOR verdicts with real symbols.
- **`check-skills.ts`** on *other people's* skill libraries — three shipped official plugins — where
  it found real shadowing pairs and a **duplicate trigger phrase between two skills of the same
  published plugin**. Strong evidence it isn't merely tuned to the library it was written for.

## 2.7.0

Four new skills — two for the client KPI-dashboard work, two closing named gaps — and one new
checker. 30 skills, 10 checkers, 18 gates, 259 contract cases. Researched 2026-07-29; sources and
their strength labelled in `skills/kpi/references/methods.md`, including an explicit note that most
of the metric-design evidence is **practitioner consensus, not measurement**.

### `/viby-toolkit:kpi` — define the number before anyone builds it

The client dashboard failure sequence is predictable: definitions never written down → two
dashboards disagree → nobody trusts either → the dashboard becomes a screenshot in a deck. All of it
is decided before any code.

- **Every KPI names the decision it serves.** If nobody would act differently on the answer, it is
  decoration — a footnote, not a tile. The question that saves the engagement: *"when this number is
  bad, what happens next?"*
- **One north star, dozens of KPIs, a guardrail on each.** KPIs answer "is the engine running", the
  north star answers "are we going the right way". And the rule that earns its place: *"every KPI
  should have at least one counter-metric — if a target can improve while harm increases, it's
  incomplete."* Because when a measure becomes a target it ceases to be a good measure — "reduce
  average handling time" works until it rewards hanging up on customers. Write down the laziest way
  to game it; that sentence usually writes the guardrail.
- **The metric contract**: name and the client's aliases, question, formula with exclusions, grain,
  time basis *and timezone*, window and comparison, filters, owner, guardrail, target. In git, not
  in a BI tool's description field — because definition drift is what actually destroys trust:
  "each definition is technically correct in isolation; collectively, they erode trust."
- **Dashboard as questions**: the question in the chart title, every figure carrying a comparison
  (a number with no baseline is not information), refresh time and timezone on the page, no dual
  axes, and a drill-down path for every tile.

### `/viby-toolkit:analytics` — a number is not done until it reconciles

- Implement **once**, in the transformation layer, layered so the grain is explicit at each step —
  most "the totals don't add up" bugs are an undeclared grain change.
- Prove in three layers: **unit tests on hand-built fixtures** (boundary timestamp, NULL, refund,
  duplicate, one-to-many), **rules the data must obey** (PK uniqueness, not-null on the grain key,
  ranges — a conversion rate above 1 is a bug), and **reconciliation against an independent source**
  with the residual delta explained, because an unexplained delta is where trust dies.
- Then guard against silence: **freshness** checks catch the failure where "a source stops updating
  but the pipeline keeps running without errors", **volume** checks catch abnormal shrink or growth.
- The traps a linter can't see are named too: identity stitching, late/out-of-order data and the
  restatement policy, event vs ingest time, soft-deleted rows, mixed currency, and whether history
  shows what a dimension *was* or what it *is*.

### New — `check-analytics-sql.ts` (25 contract cases)

Eleven patterns that return a plausible **wrong** number instead of an error: `BETWEEN` on time
(closed at both ends, so monthly figures stop summing to the annual one), `COUNT(*)` after a
fan-out join, division with no `NULLIF`, `DATE_TRUNC` without a timezone, `= NULL`, float money,
`NOW()` in a definition, plain `UNION` de-duplicating, `SELECT *`, `DISTINCT` papering over a
fan-out, unbounded fact-table scans. Every rule names the safe alternative.

**Precision measured on real client code, and it mattered.** First run on a real 62-file data repo:
**126 findings, 117 of them one rule** — `now-in-definition` at 93% of the total, every one
legitimate, because `NOW()` in a migration default or a dated backfill is correct SQL. `= NULL` was
also firing on `SET col = NULL`, which is assignment, not comparison. Both narrowed; the same repo
now yields **8**. The fixtures had agreed with their author, which is exactly why the doctrine says
validate against a large real corpus.

Also found by its own tests: `\b` placed *after* `NOW()` can never match, because `)` is not a word
character — the rule had silently never fired.

### Gaps closed

- **`/viby-toolkit:docs`** — write for one reader with one next action. Seven genres with their
  reader and next action, and a refusal to mix them; run every command from a clean state; document
  the failure mode with the verbatim error. An ADR's value is entirely the alternatives rejected.
- **`/viby-toolkit:deps`** — one dependency per change, breaking changes of every major crossed read
  before code is touched, queue sorted by *why* not by staleness. Thirty bot PRs are not thirty
  tasks. A dependency you cannot upgrade is a recorded decision with an unsticking condition.
- **Data correctness** is now `analytics`; the **build-phase instrumentation** gap remains open and
  is the one I did not close.

### Also

Five probes added (37 total, all ranking first). `deps` and `migrate` landed in the shadowing-watch
band on arrival — fixed the documented way, by making their cross-reference mutual.

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
