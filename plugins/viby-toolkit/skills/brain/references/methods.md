# Where the `brain` rules come from

Reference for `/viby-toolkit:brain`. Researched 2026-07-29. Labelled as `/viby-toolkit:study`
requires: **fetched** = retrieved and read; **search-summary** = from a search result summarising the
source, primary not opened.

## The taxonomy is settled, and it is not this repo's invention

Search-summary, 2026-07-29 (agentic-memory design write-ups and the *Memory in the Age of AI Agents*
paper list): "over 2025 and into 2026, the agent ecosystem converged on a remarkably consistent
three-tier taxonomy — **episodic, semantic, and procedural** memory — that mirrors decades of
cognitive science research." Episodic = "experience from earlier decision cycles", the log-like record
of what happened in what sequence. Semantic = "what is generally true". Procedural = "how to perform
tasks: reusable skills, execution strategies".

Mapping it onto this toolkit is what made §1 worth writing: `handoff` is episodic, `learn` is
semantic, and **the skills are procedural memory** — which reframes editing a skill as a memory
operation rather than a docs change.

## Retrieval failure is where the errors are — the most actionable finding here

Search-summary, 2026-07-29 (memory-benchmark analyses): existing memory benchmarks are "largely
driven by retrieval success, with answer errors in LongMemEval, LoCoMo, STALE and PersonaMem
concentrated in cases where retrieval failed, while **retrieval successes with wrong answers account
for only 5.8%–13.7%**".

Consequence for §4: the lever is findability — naming, indexing, entry size — not richer content. A
perfectly written memory that is never retrieved has no effect at all.

## Staleness: the hard, under-measured failure

*STALE: Can LLM Agents Know When Their Memories Are No Longer Valid?*
([arXiv 2605.06527](https://arxiv.org/pdf/2605.06527), search-summary, 2026-07-29). The gap it names
is precisely §5's hardest case: benchmarks "rarely isolate whether a model can determine that a
previously valid memory has been rendered obsolete by a structurally related yet linguistically
distinct new observation". Also noted across this literature: real memory systems "have to forget and
drop facts when they go stale, merge duplicates, and reconcile contradictions".

In a coding project a useful slice of this is mechanical — an obsolete memory usually cites a path
that no longer exists — which is what `check-memory.ts` exploits.

## Poisoning is defended with provenance

*When Does Belief-Based Agent Memory Help? Reliability-Conditional Updating and Provenance-Capped
Poisoning Defense* ([arXiv 2606.22030](https://arxiv.org/html/2606.22030), search-summary,
2026-07-29). The title is the finding: updates conditioned on reliability, and an entry's influence
**capped by its provenance**. You cannot cap what is not recorded, so §3 makes provenance mandatory.

Related: *MemSyco-Bench: Benchmarking Sycophancy in Agent Memory*
([arXiv 2607.01071](https://arxiv.org/html/2607.01071v1), search-summary, 2026-07-29) — storing what
the user said as though it were established is a measured failure mode with its own benchmark, hence
§3's "store what was established, not what was said". And *GateMem*
([arXiv 2606.18829](https://arxiv.org/pdf/2606.18829), search-summary, 2026-07-29) benchmarks
governance in shared memory across multiple principals, which is the situation of any store synced
across work and personal machines.

## Retrieval signals worth knowing

Search-summary, 2026-07-29: retrieval increasingly fuses several passes — semantic similarity,
keyword and entity matching — and weights results by "a quality reward reflecting how successful the
past trajectory was, and a frequency reward checking how recently a similar strategy was used". This
repo has no vector store and does not need one, but the *principle* transfers: **record the outcome**,
so a failed approach is not retrieved as precedent (§3).

## Measured here (2026-07-29)

`check-memory.ts` was run against **five real memory stores, 53 entries**, and the run corrected the
checker twice:

1. First pass reported **23 stale references**. Almost all were false: a store's relative paths belong
   to *its* project, and every store had been resolved against one repo root. Fixed with
   self-calibration — if nothing in a file resolves, report `root-unknown` rather than inventing
   findings — which cut it to 6.
2. Those 6 included two in this repo's own memory that were *also* root artifacts: a memory about a
   project legitimately cites paths from the repo root, a package subdirectory and home. Fixed by
   trying the root and two ancestors. Remaining true findings on this store: one undated entry and
   one entry that had grown past 900 words.

Across the five stores the real signals were: **24 undated**, **34 without provenance**, 4 entries
missing from their index, and 1 duplicate-topic pair — i.e. the two fields §3 makes mandatory are
exactly the two the existing store mostly lacks.
