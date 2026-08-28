# Should viby-toolkit default to deterministic multi-agent Workflow orchestration?

## 0. Status

- **Date:** 2026-08-27. **Decision served:** what to change in viby-toolkit 2.21.3 so its skills start
  agent workflows by default.
- **Angles searched:** 9 (6 in round 1, 3 more to close gaps the round-1 process audit named).
- **Stopping rule:** **effort-bounded** — top sources per angle, two rounds, plus a mandated
  gap-closing pass. Not saturation: §9 lists what a third round would still go after.
- **Agents:** 16 (6 researchers + 6 verifiers + 1 process auditor + 3 gap-closers). 0 errors.
  ~534k subagent tokens.
- **Sources read vs cited:** round 1 verified only 9 of ~50 findings, and 4 of its 6 angles produced
  zero verifier-confirmed findings. Round 2 exists because of that.
- **Claims that could not be verified:** the Co-Coder figures (2.10x wall-clock, +14.0pp, "beats Claude
  Code Agent Teams", <https://arxiv.org/abs/2606.00953>) remain `search-summary` — the primary was *(2026-08-27)*
  never rendered. Treat as unconfirmed.
- **Priors refuted: two, both mine.**
  1. That "Workflow everywhere" was unconditionally better than selective fan-out. It is not; the
     evidence sets a hard shape on where fan-out may go.
  2. That this toolkit's own MAST citation was correct. It is not — see §6.

## 1. The answer

**Deterministic workflow-as-code is the right orchestration *mechanism*, and read-only parallelism is
the only fan-out shape the evidence supports.** Those are two separate findings and both survived.

- *Mechanism:* centralized coordination contains error amplification to **4.4x**, versus **17.2x** for
  independent uncoordinated agents (Kim et al., <https://arxiv.org/html/2512.08296v1>, fetched). A Workflow script *(2026-08-27)*
  is centralized coordination written down, so making it the default engine is supported.
- *Shape:* every measured win from parallelism in a **coding** setting is read-only. Every attempt to
  find a measured win from parallel *writers* failed — see §7.

Confidence: **high** on the shape, **medium-high** on the mechanism (the mechanism evidence is one
paper plus the absence of a counter-example).

## 1b. The decision taken

**Hybrid, adopted 2026-08-27.** Fanning out is a default everywhere — 3–4 read-only agents in one
message, never a permission. A `Workflow` script is reserved for work that genuinely has stages:
`orchestrate` (scout → escalate weak areas), `review` (review → validate), `study`
(research → verify → audit). Single sweeps — `explore`, `debug`, `plan`, `migrate`, `secure`,
`perf`, `adopt` — dispatch directly, because a script buys nothing when there is nothing to
sequence. Writes stay single-threaded in every case.

The initial intent was to route *every* skill through a Workflow. §5 and §7 are why that changed:
the mechanism evidence supports declaring stages, and none of it supports adding stages that
aren't there. Enforced by `skills/principles/scripts/check-orchestration.ts`.

## 2. What would change this conclusion

- A published ablation where multiple agents concurrently edit the same codebase and beat a
  single-agent baseline with a number. None exists as of this search (§7).
- A coding-domain result showing a real (non-oracle) verifier closes the best-of-N selection gap.
- Kim et al.'s 45% threshold failing to replicate on a current-generation base model — the whole
  "don't fan out when your baseline is strong" rule rests on it.

## 3. The competing answers, and what happened to them

| # | Answer | Verdict |
|---|---|---|
| 1 | **Topology is the lever** — more parallel agents + verification stages win | **Excluded in its strong form.** Agent count has a hard ceiling at 3–4 and negative returns past a 45% baseline (<https://arxiv.org/html/2512.08296v1>). | *(2026-08-27)*
| 2 | **Seams are the lever** — determinism and structured hand-off beat agent count | **Survives, with a split** (§8). |
| 3 | **A wash or worse** — multi-agent costs more and regresses correctness | **Survives for writes, excluded for reads.** |

The observation that did the excluding on (1): **ChromaFlow** — adding more aggressive orchestration
to a tool-augmented agent took GAIA Level-1 accuracy *down*, 54.72% → 50.94%, with more tracebacks and
timeouts (<https://arxiv.org/abs/2605.14102>). A controlled negative ablation, verifier-confirmed. *(2026-08-27)*

The observation that did the excluding on (3), for reads: **FastContext** — bolting a parallel
read-only exploration subagent onto a coding agent raised SWE-bench Multilingual resolution 71.7 → 73.3
and SWE-bench Pro 46.0 → 51.5 (GPT-5.4), 17.5 → 22.5 (GLM-5.1), *while cutting main-model token
consumption by up to 60.3%* (<https://arxiv.org/html/2606.14066v3>). This is the toolkit's own scout pattern, measured on coding benchmarks. *(2026-08-27)*

## 4. The numbers that set the design

All four verified verbatim in the paper's own HTML render (arXiv 2512.08296, Kim et al., *Towards a
Science of Scaling Agent Systems*, <https://arxiv.org/html/2512.08296v1>) — round 1 had them only via a *(2026-08-27)*
blog, and the direct read confirmed the blog did not fabricate them, it only rounded.

| Finding | Quote | Design consequence |
|---|---|---|
| **45% rule** | "coordination yields diminishing or negative returns (beta=-0.408, p<0.001) once single-agent baselines exceed ~45%" | Don't fan out work the main thread already does well. This is the most actionable finding in the corpus. |
| **3–4 agent ceiling** | "per-agent reasoning capacity becomes prohibitively thin beyond 3-4 agents, creating a hard resource ceiling" | Cap a fan-out stage at 3–4 agents. Not 10. |
| **Error amplification** | "independent agents amplify errors 17.2x through unchecked propagation, while centralized coordination contains this to 4.4x" | Always reconcile through an orchestrator. Never let agents chain to each other. |
| **Task-shape asymmetry** | "+80.9% on parallelizable tasks ... for sequential reasoning tasks, all multi-agent variants degraded performance by 39-70%" | Fan out breadth. Never fan out a chain. |

## 5. What the winning coding scaffolds actually do

The domain gap round 1 skipped. Every top-of-leaderboard scaffold checked is a **single-agent
sequential loop**, with subagents used only for reading:

- **Live-SWE-agent** — 79.2% SWE-bench Verified with Claude Opus 4.5, leading open-source: a
  single-agent self-evolving loop, no subagents for search or writing (<https://live-swe-agent.github.io/>). *(2026-08-27)*
- **Confucius Code Agent** (Meta) — 74.6%, beats OpenHands on an identical backbone: one orchestrator;
  auxiliary agents only for note-taking and context summarization (<https://arxiv.org/html/2512.10398v1>). *(2026-08-27)*
- **OpenHands / SWE-agent** — single-agent by design.
- **Verdent** — the one genuine multi-agent counter-example (76.1% pass@1). Its *own* ablation puts the
  review subagent's contribution at **~0.5% pass@3**
  (<https://www.verdent.ai/blog/swe-bench-verified-technical-report>). *(2026-08-27)*

And Anthropic's own Claude Code guidance, on the platform this toolkit ships for:

> "Two subagents editing the same file in parallel is a recipe for conflict."
> "a single session handling the chain is usually cleaner than a relay of subagents passing state through files"

## 6. A correction to this toolkit's own principles

`principles/SKILL.md:70` and `references/the-fan-out-law.md:62` state that MAST attributes **~36.9% of
multi-agent failures to inter-agent misalignment**, and conclude the seams are "the majority category."
Source under test: <https://arxiv.org/pdf/2503.13657v3> (fetched 2026-08-27).

**Both the number and the conclusion are wrong.** Extracted directly from the MAST v3 PDF text
(`pdftotext`, not a fetch-tool summary), Figure 1's caption ties its percentages to 1642 traces —
*"The percentages shown represent the prevalence of each failure mode and category as observed in our
analysis of 1642 MAS execution traces."*

- **System Design Issues — 44.2%** ← the actual majority category. <https://arxiv.org/pdf/2503.13657v3> (2026-08-27)
- **Inter-Agent Misalignment — 32.3%**, not 36.9%. <https://arxiv.org/pdf/2503.13657v3> (2026-08-27)
- **Task Verification — 23.5%**. <https://arxiv.org/pdf/2503.13657v3> (2026-08-27)

The 36.9% figure traces to a **WebFetch summarization artifact** against an ar5iv render
(<https://ar5iv.labs.arxiv.org/html/2503.13657>) — a repeat query against the same HTML could not *(2026-08-27)*
locate any such consolidated percentage in the paper at all. This
is the exact failure `/viby-toolkit:study` warns about (a citation that is live and on-topic but does not
support the claim), caught in the toolkit's own always-loaded contract.

The correction *strengthens* the workflow-as-code direction rather than weakening it: if **system
design** is the largest failure category, then writing the orchestration down as an explicit script —
instead of leaving it to prose the model re-improvises each run — is attacking the biggest bucket.

## 7. Best-of-N: real, and bounded by the verifier

The strongest pro-fan-out mechanism, which round 1 never searched.

- **Coverage genuinely scales.** "coverage — the fraction of problems that are solved by any generated
  sample — scales with the number of samples over four orders of magnitude"; on SWE-bench Lite,
  15.9% at K=1 → **56% at K=250** (<https://arxiv.org/abs/2407.21787>, fetched). *(2026-08-27)*
- **But that is an oracle bound.** On SWE-bench Verified at K=16, oracle Pass@16 is 51.4–65.6% while
  the best real verifier reaches 40.6–54.2% Best@16 — a persistent **11–25 point** gap that better
  rerankers close only a few points at a time (<https://arxiv.org/html/2601.04171v1>). *(2026-08-27)*
- **And it can go negative.** "even with zero computational cost, the optimal number of samples is
  finite and very low (e.g., K≤5 ...)... Resampling cannot decrease this probability [of false
  positives], so it imposes an upper bound to the accuracy of resampling-based inference scaling,
  regardless of compute budget" (<https://arxiv.org/html/2411.17501>, fetched). *(2026-08-27)*

**Consequence for the toolkit:** best-of-N is worth it only where a *strong verifier* exists. In this
codebase that means an executed check — a test run, a type error, a reproduced crash — not another
model's opinion. That is already the review's "execute, don't argue" rule; this is its
justification.

## 8. Unresolved disagreements, left visible

1. **"Tighten the seams" is underdetermined, and its halves point opposite ways.** EMNLP 2024 (fetched,
   verifier-supported): schema-forcing output **measurably degrades reasoning**, worse under stricter
   constraints. FlashEvolve (fetched, supported): removing **synchronization barriers** recovers
   3.5–4.9x. So: synchronization discipline helps, format rigidity hurts. Design consequence — prefer
   `pipeline()` over barrier `parallel()`, and keep return schemas *loose enough to reason in*
   (a prose field beside the structured ones), rather than maximally strict.
2. **Does agent count buy verification accuracy?** One paper shows monotone gains through 4 agents;
   another argues marginal verification value is exactly zero outside near-tied decisions. No shared
   benchmark. This is precisely the call review makes, and it is open.
3. **Capability confound.** Nearly every cited number was measured on an older base model. Whether the
   multi-agent advantage shrinks as base models improve is decisive for a 2026 decision and unanswered.

## 9. What I could not find out

- Any ablation isolating **parallel code-writing** vs single-agent with a measured coding-benchmark
  delta. Nine angles, not found. Treat its absence as the finding.
- Terminal-Bench / SWE-Lancer top-entry architectures (not fetched).
- Co-Coder's primary text — four attempts, never rendered. Its figures stay unconfirmed.
- **First-party measurement.** The highest-grade evidence available here is the one nobody gathered:
  run the same task on this toolkit with fan-out on and off. That belongs to
  `/viby-toolkit:evaluate`, and it is the recommended next step.

## 10. Sources

**Fetched and load-bearing** — page opened, figure read off it. All fetched **2026-08-27**:

- Kim et al., *Towards a Science of Scaling Agent Systems* — <https://arxiv.org/html/2512.08296v1> *(2026-08-27)*
- MAST, *Why Do Multi-Agent LLM Systems Fail?* (PDF text extracted directly) — <https://arxiv.org/pdf/2503.13657v3> *(2026-08-27)*
- *Large Language Monkeys* / repeated sampling — <https://arxiv.org/abs/2407.21787> *(2026-08-27)*
- Resampling ceiling from verifier false positives — <https://arxiv.org/html/2411.17501> *(2026-08-27)*
- Agentic rubrics / Best@16 vs oracle Pass@16 — <https://arxiv.org/html/2601.04171v1> *(2026-08-27)*
- FastContext (parallel read-only exploration subagent) — <https://arxiv.org/html/2606.14066v3> *(2026-08-27)*
- Confucius Code Agent — <https://arxiv.org/html/2512.10398v1> *(2026-08-27)*
- ChromaFlow (negative orchestration ablation) — <https://arxiv.org/abs/2605.14102> *(2026-08-27)*
- Live-SWE-agent — <https://live-swe-agent.github.io/> *(2026-08-27)*
- Subagents in Claude Code — <https://claude.com/blog/subagents-in-claude-code> *(2026-08-27)*
- Anthropic multi-agent research system — <https://www.anthropic.com/engineering/multi-agent-research-system> *(2026-08-27)*
- Verdent SWE-bench Verified report — <https://www.verdent.ai/blog/swe-bench-verified-technical-report> *(2026-08-27)*

**Unverified / search-summary — do not cite for numbers.** All attempted **2026-08-27**:

- Co-Coder — <https://arxiv.org/abs/2606.00953> (primary never rendered, four attempts) *(2026-08-27)*
- k-shot vs agentic reasoning — <https://arxiv.org/abs/2605.08478> (PDF extraction failed twice) *(2026-08-27)*
- 2–6x tool-count penalty — <https://venturebeat.com/orchestration/research-shows-more-agents-isnt-a-reliable-path-to-better-enterprise-ai> *(2026-08-27)*
- The blog that carried Kim et al.'s numbers second-hand — <https://towardsdatascience.com/why-your-multi-agent-system-is-failing-escaping-the-17x-error-trap-of-the-bag-of-agents/> *(2026-08-27)*
