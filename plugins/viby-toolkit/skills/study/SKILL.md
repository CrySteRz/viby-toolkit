---
name: study
description: >
  Use when the user brings an idea or open question whose answer lives OUTSIDE the codebase and
  wants it researched properly — "research this", "do a deep dive on", "what's the state of the
  art for", "how do other people solve this", "is this claim true", "write me a study on", "look
  into whether we should". Produces a protocol first (question, competing answers, search plan,
  stopping rule) for approval, then the study document. Distinct from /viby-toolkit:evaluate,
  which measures 2–5 named candidates against your own code; hand over to it once the question
  narrows to a shortlist. Distinct from /viby-toolkit:explore, which maps a codebase you have.
---

# Study (protocol first, then evidence, then the answer)

```
IRON LAW: Write the protocol BEFORE the first search — question, competing answers, the
          observation that would exclude one, and the stopping rule.
          Every claim carries the quoted sentence that supports it. A working link is not
          support: models keep links valid >94% and on-topic >80% while only 39–77% of
          citations actually support the claim.
```

A study is not a pile of links with a conclusion on top. It is an argument that survived a
deliberate attempt to break it, and it is worth writing only when its answer would change what
you do. Follow `/viby-toolkit:principles`. Sources and measured figures behind every rule here:
`references/methods.md`.

## 1. Name the decision, or don't run the study

Before anything: **what will be done differently under each possible answer?** Write the
options and the action each implies. If every answer leads to the same action, the study is
worth nothing — say so and stop. This is value-of-information reasoning, and its ceiling rule
is blunt: perfect information is worth only what it changes.

Size the depth to the **cost of being wrong**, not to how interesting the question is. A
reversible choice with a cheap fix gets an afternoon; something in the path of data loss,
security, money, or a public API gets the full protocol.

## 2. Turn the idea into an answerable question

An idea becomes researchable when it has a **comparison** and a **context**. Use PICOC:
**P**opulation (who/what is affected), **I**ntervention (the thing under consideration),
**C**omparison (against what — including "against doing nothing"), **O**utcome (the measure
that decides it), **C**ontext (your stack, scale, constraints).

The two people usually skip are Comparison and Context, and they are exactly the two that make
an answer transferable to *your* situation rather than true in general.

Then split it into **sub-questions that can each be answered independently**, and mark which
are load-bearing. A wrong sub-question poisons everything downstream and is invisible in the
final answer — which is why the audit in §7 checks the plan, not only the conclusion.

## 3. Write the protocol, and get it approved before searching

The protocol is the first deliverable — the thing to show the user before spending an hour:

1. **The decision** and what each answer changes (§1).
2. **The question**, PICOC'd, with sub-questions ranked load-bearing first.
3. **Competing answers** — 2–4 of them (§4).
4. **The crucial observation** that would exclude at least one.
5. **Search plan**: the angles, the sources you expect to be authoritative, and the
   deliberately opposing query.
6. **Inclusion/exclusion criteria** — what counts as evidence here, decided now.
7. **The stopping rule** (§5).
8. **What would change my mind**, written *before* you know the answer.

Pre-committing is what separates a study from a rationalisation: a hypothesis specified after
seeing the evidence, but presented as the motivation, is the standard way false conclusions get
published. **The protocol is not a promise to follow it — it is a promise to be transparent
about where you deviated.** Deviate freely; record it, and put anything that emerged in an
**Exploratory** section rather than dressing it up as something you predicted.

## 4. Devise competing answers, then try to exclude one

Do not research *whether X is good*. List the 2–4 answers that could be true, then design the
observation that **kills at least one of them**. Strong inference is the loop: alternative
hypotheses → an observation that excludes some → a clean result → repeat with what survives.

The named failure mode this defends against is old and specific: you fall in love with one
hypothesis and start fitting all evidence to it, which feels exactly like doing research. If no
possible finding could exclude any of your candidates, you do not yet have a study design — you
have a shopping list.

## 5. Search on multiple angles, snowball, and stop by a stated rule

- **Several angles, not one query rephrased.** Search by problem, by mechanism, by the name
  people who solved it would use, by the failure mode, and by the tool/vendor name. Each angle
  is blind to what the others surface.
- **Fan out: dispatch one `researcher` per angle, in parallel.** This is pure read work, so the
  fan-out law permits it, and it is the whole reason to fan out here — a search pass generates
  enormous raw output, and the researcher returns quoted, dated, labelled findings while the
  snippets and fetched pages die with it. Give each one its angle, the oracle, and the opposing
  query. Reconcile their findings yourself; they gather, you decide.
- **Then snowball**: from a good source, work backwards through its references and forwards
  through what cites it. Search plus forward-citation chasing finds more than search alone.
- **Write neutral queries, plus one that argues the opposite.** Leading or loaded phrasing
  skews what comes back and pulls a model toward agreeing with the premise it was handed. If
  every query assumes the conclusion, the search will confirm it. Include at least one query
  actively hunting for the strongest case against.
- **Stop by a rule you stated in the protocol**, and say which: *saturation* (two rounds
  surfaced nothing new), *effort-bounded* (top N per angle — then N is part of the finding), or
  *exhaustion* (the population is small enough to read completely). "I got tired" is
  effort-bounded; label it that way.
- **Grey literature counts here.** For engineering questions most of the real knowledge is in
  docs, changelogs, issue threads, and practitioner posts, not papers — and the state of
  *practice* is often what the question is about. Include it deliberately and appraise it (§6).

## 6. Appraise the source, then grade the evidence

Two different judgements, both needed. **The source** (AACODS, built for grey literature):
**A**uthority — who is responsible, and are they in a position to know; **A**ccuracy — is it
supported by something checkable; **C**overage — does it state its own limits and parameters;
**O**bjectivity — whose interest does it serve, and is the bias visible; **D**ate — is it
dated at all, and is that date still relevant; **S**ignificance — does it add anything the
others didn't.

**The evidence** then gets downgraded for the reasons that actually apply to engineering:

- **Indirectness — the big one.** It was measured, but on a different stack, scale, workload or
  population than yours. Most published numbers are indirect evidence for your case, and this
  is the single most common reason a well-sourced study reaches a wrong local conclusion.
- **Imprecision.** One run, no variance, no confidence interval, n=1.
- **Inconsistency.** Independent sources disagree and nobody explains why. That disagreement
  is a finding — report it rather than picking the convenient side.
- **Risk of bias**, of which the sharpest case is below.

**Vendor and self-published numbers are a claim about a different question.** Self-evaluation
is biased even in good faith: the harness, the dataset and the metric are all chosen by the
party being measured, and independent reproductions frequently land nowhere near — a documented
case has vendors claiming 98%+ accuracy on a task where the best systems score ~14% on an
independent benchmark, because the two are not measuring the same thing. So treat a vendor
figure as **a hypothesis to reproduce, never a result to cite**, and say which it is.

## 7. Verify every claim, and audit the process not just the answer

- **Quote the sentence.** Every non-obvious claim carries the verbatim line from the source that
  supports it, with the URL and the date you fetched it. This is the same grounding gate
  `/viby-toolkit:review-cluster` uses on code findings, and it exists because a link that
  resolves and is on-topic still fails to support its claim a large fraction of the time.
- **Date-stamp and quote at fetch time, because sources rot.** A quarter of pages that existed
  over a recent decade are already gone. A URL you cannot quote from today may be
  unverifiable next quarter — and *fetched-and-quoted-on-DATE* survives that.
- **Say when a fetch failed.** A PDF that returned no text, a paywall, a 404: name it and
  either re-fetch another way or drop the claim. Answering from a title and metadata produces
  exactly the "appears to / likely / the title suggests" prose that reads like evidence and
  isn't.
- **Audit the trajectory, not the conclusion.** Errors compound: a mis-framed sub-question, a
  noisy intermediate summary, or drift from the original intent all reach the final answer
  looking clean. Re-read your own plan and intermediate notes against the question you started
  with before writing the conclusion.
- **Don't let a model grade its own study on a vague rubric.** Self-scored quality dimensions
  correlate poorly with expert judgement and lean on knowledge nobody can check. Use the
  mechanical audit instead, and a human bar:

```bash
CHECK=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/study/scripts/check-study.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$CHECK" docs/studies/my-study.md
```

It flags unsourced numbers, hedged language inside claims presented as measured, a missing
status header, a missing "what would change my mind", single-domain sourcing, and undated
citations. It cannot tell you whether a quote is real — that part is yours.

## 8. Land it, or hand it over

- If the answer narrowed to **2–5 named candidates and the decisive evidence is a measurement
  on your own code**, stop here and hand to `/viby-toolkit:evaluate` — a labeled local test
  outranks every citation in the document.
- If the answer is **"the evidence doesn't decide it"**, that is a real result. Say what would
  decide it and what it would cost.
- Record the durable part with `/viby-toolkit:learn` — especially a rejected direction and the
  bar it failed, which is the part a future session would otherwise pay to re-derive.

## Output — the study document

Save it (`docs/studies/<date>-<topic>.md` or the project's convention).

0. **Status line** — what was actually done: date, how many angles searched, how many sources
   read versus cited, how many claims you could not verify, and **how many of your own priors
   the evidence refuted**. A study that refuted nothing usually didn't test anything.
1. **The answer**, in a paragraph, with its confidence and the decision it serves.
2. **What would change it** — stated as observations, not feelings.
3. **The competing answers** and which the evidence excluded, with the observation that did it.
4. **The evidence table** — claim → source → quote → date → grade (direct/indirect, precise/n=1),
   labelled measured / inferred / not tested.
5. **Disagreements between sources**, unresolved and visible.
6. **Deviations from the protocol**, and an **Exploratory** section for what emerged.
7. **What I could not find out**, and where the answer probably lives.
8. **Sources**, marked verified vs unverified — never a bare list implying all were read.
