# Where the `study` rules come from

Reference for `/viby-toolkit:study`. Loaded on demand — the skill body carries the doctrine, this
carries the evidence for it. Researched 2026-07-29.

Read this the way the skill tells you to read any source list: **labelled**. "Fetched" means the
page was retrieved and the figure read off it. "Search-summary" means it came from a search
result summarising the source, and the primary was not opened — weaker, and marked as such.

## The rule that changes how you verify

| Finding | Figure | Status |
|---|---|---|
| Deep-research citations: links resolve and are on-topic far more often than they support the claim | link validity **>94%**, relevance **>80%**, factual accuracy only **39–77%** across 14 frontier models | fetched 2026-07-29 — [arXiv 2605.06635](https://arxiv.org/html/2605.06635) |
| Citation hallucination across commercially deployed models | **11–57%** | second-hand — reported inside [arXiv 2605.06635](https://arxiv.org/html/2605.06635) citing Yuan et al. 2026, whose primary was not opened; treat as indicative |

This is the single most useful number in this file. It kills the intuition that a working link
is verification: the failure is not usually a fabricated URL, it is a **real, relevant page that
does not say what it was cited for**. Hence "quote the sentence" rather than "check the link".

## Process, not just outcome

*Why Your Deep Research Agent Fails? On Hallucination Evaluation in Full Research Trajectory* —
[arXiv 2601.22984](https://arxiv.org/abs/2601.22984) (fetched 2026-07-29).

- Outcome-based evaluation only checks the final answer; **process-aware** evaluation decomposes
  the trajectory into atomic actions, claims and sub-queries and verifies each.
- Their **PING** taxonomy: **P**ropagation (errors compounding across steps), **I**ntent
  (drift from the original goal), **N**oise-induced (failure from noisy intermediate data),
  **G**rounding (disconnection from fact).
- Built *DeepHalluBench*, 100 hallucination-prone tasks, six agents. Per-stage failure rates
  were **not** stated in what was retrieved — so the taxonomy is cited here, not a ranking of
  which stage is worst.

Why it matters here: a bad sub-question or a noisy intermediate summary arrives at the
conclusion looking clean, so §7 audits the plan and the notes, not only the answer.

## Protocol before evidence

Preregistration (search-summary; standard, well-documented practice — e.g. [PLOS Biology on
preregistration quality](https://journals.plos.org/plosbiology/article?id=10.1371/journal.pbio.3000937), 2026-07-29):

- Time-stamping question, method and analysis plan **before** collecting data is what lets a
  reader tell a tested prediction from one invented afterwards.
- **HARKing** — hypothesising after the results are known, then presenting it as the motivation —
  is described as normative in some fields and generates a high false-positive rate.
- Crucially: **a preregistration is not a promise to run it as written; it is a promise to be
  transparent about whether you did**, with an explicit *Exploratory* section for what emerged.
  That framing is why §3 encourages deviation-with-a-record rather than rigidity.

## Competing answers, and the observation that excludes one

Platt's **strong inference** (1964), building on **Chamberlin's method of multiple hypotheses**
(1890) — search-summary via [J. Exp. Biol. retrospective](https://journals.biologists.com/jeb/article/217/8/1202/13095/Fifty-years-of-J-R-Platt-s-strong-inference), 2026-07-29:

1. devise alternative hypotheses; 2. devise a crucial experiment that will **exclude** one or
more; 3. perform it and get a clean result; then recycle on what survives.

Chamberlin's stated motive is the reason this is in the skill: researchers "fall in love" with a
favourite hypothesis and fit all evidence into it instead of seeking genuine explanations.

## Framing the question

**PICOC** — Population, Intervention, Comparison, Outcome, Context — from Kitchenham & Charters'
software-engineering SLR guidelines (search-summary). Its documented purpose is exactly the one
§2 uses it for: break the objective into searchable keywords, set scope, force multiple
viewpoints, reduce bias. Comparison and Context are the components that make an answer
transferable to a specific situation.

## Searching and stopping

Snowballing / citation chasing (search-summary, software-engineering guidelines and library
guides): seed set → backward through references → forward through citers → repeat until nothing
new. Search **combined with forward citation tracking identifies more eligible sources than
search alone**.

Three legitimate stopping criteria, and the skill requires naming which one you used:
**theoretical saturation** (no new concepts), **effort-bounded** (top N hits — then N is part of
the result), **evidence exhaustion**.

## Grey literature is evidence, and needs its own appraisal

**Multivocal literature review** (Garousi et al., *Information and Software Technology*, 2019;
[arXiv 1707.02553](https://arxiv.org/abs/1707.02553), fetch attempted 2026-07-29) — an SLR that deliberately includes grey
literature, on the grounds that practitioners mostly *write and read* grey literature, so the
state of practice lives there. Its search process and source-quality assessment differ from a
standard SLR, which is why the skill appraises sources separately from grading evidence.

> **Fetch failure, recorded rather than hidden:** both the PDF and the abstract page were
> retrieved and neither yielded the paper's concrete inclusion checklist or quality criteria —
> the PDF returned unparseable binary. So only the definition and rationale are cited above; the
> criteria in the skill body come from AACODS below, not from this paper. This repo has hit the
> same PDF-extraction failure before, and the rule stands: cite what you actually read.

**AACODS** (Tyndall, Flinders University) — the grey-literature appraisal checklist used in §6:
**A**uthority, **A**ccuracy, **C**overage, **O**bjectivity, **D**ate, **S**ignificance
(search-summary, consistent across multiple university library guides and the
[NHS Evidence Toolkit](https://nhsevidencetoolkit.net/resources/aacods-checklist/), 2026-07-29).

## Grading the evidence

**GRADE** downgrade domains (search-summary, [CDC ACIP GRADE
handbook](https://www.cdc.gov/acip-grade-handbook/hcp/chapter-8-domains-decreasing-certainty-in-the-evidence/index.html), 2026-07-29):
risk of bias, inconsistency, **indirectness**, **imprecision**, publication bias. GRADE is a
medical framework and is not imported wholesale — the skill keeps the four domains that
translate cleanly to engineering evidence and drops the machinery that doesn't. Indirectness is
promoted to "the big one" because a number measured on someone else's stack is the default
situation for an engineering question, not an edge case.

## Vendor numbers

Search-summary, multiple independent write-ups:

- Vendors report from **their own harness on their own curated data**, not independent
  evaluation. The analogy that lands: pharmaceutical companies don't run their own trials.
- Concrete gap: on a legal RAG task, the best systems scored **~14% Precision@1** on an
  independent benchmark while vendors claimed **98%+ accuracy** on nominally the same task — i.e.
  the two are measuring different things
  ([anablock write-up](https://blog.anablock.com/blog/rag-benchmarking-problem-ai-accuracy-claims), 2026-07-29 —
  search-summary; the underlying LegalBench-RAG paper was not opened).
- Self-published agent benchmarks with no public test set or open harness have **no independent
  reproduction**, which makes every number a self-reported claim
  ([DeepSource](https://deepsource.com/blog/ai-code-review-benchmarks), search-summary, 2026-07-29).

Hence: a vendor figure is a hypothesis to reproduce, never a result to cite.

## Sources rot, so quote at fetch time

Pew Research Center, *When Online Content Disappears* (May 2024) — search-summary, figures
consistent across several write-ups of the same study:

All three from [*When Online Content Disappears*](https://www.pewresearch.org/data-labs/2024/05/17/when-online-content-disappears/)
(Pew, May 2024; figures read from search summaries of that report on 2026-07-29, the report page
itself not fetched):

- **38%** of webpages that existed in 2013 were no longer available.
- **25%** of all pages that existed at some point 2013–2023 were gone as of October 2023.
- **54%** of Wikipedia pages contain at least one dead link in References.

## Don't self-grade the report

*DeepResearch Bench* evaluates reports on comprehensiveness, insight/depth,
instruction-following and readability, plus citation accuracy and effective citation count
(search-summary). The follow-up work's critique is the part that matters here: rubrics defined
by LLMs **misalign with human expert judgement, are coarse and weakly interpretable, and push
the judge onto unverifiable internal knowledge**. So `check-study.ts` checks structural
properties that are mechanically decidable, and leaves the quality judgement to a person.

## Query framing

Search-summary, several 2025–2026 papers: a leading or loaded prompt skews the model's prior and
it "disproportionately lean[s] towards responses aligned with the stance embedded in the prompt,
regardless of counter-evidence"; sycophancy means agreeing with a stated belief over the truthful
answer. Practical consequence for a study: your query set *is* part of your method, and a query
set that assumes the conclusion will confirm it. Hence the required opposing query.
