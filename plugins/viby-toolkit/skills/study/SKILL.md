---
name: study
description: >
  Always load when the answer lives OUTSIDE the codebase and needs researching properly —
  "research this", "do a deep dive on", "what's the state of the art", "how do others solve this",
  "write me a study on", "is this claim true". Not /viby-toolkit:evaluate.

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
- **Fan out: author a `Workflow` that runs one `researcher` per angle.** This is pure read work, so
  the fan-out law permits it, and it is the whole reason to fan out here — a search pass generates
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

Rank what you found by what it can actually support: a primary source you opened beats a summary of
it, a controlled measurement beats a vendor claim, and an independent replication beats either. Label
every claim **fetched** (page retrieved, figure read off it) or **search-summary** (came from a search
result; primary not opened) — and never let a search-summary carry a load-bearing number.

## 7. Verify every claim, and audit the process not just the answer

Re-check each claim against the source it cites before it enters the document, and ask separately
whether the *search* was sound — which angles were never run, which competing answer was never given
a fair chance to win. An answer that no one tried to refute is a hypothesis.

The full appraisal ladder, the grading rubric, and the process-audit checklist are in
`references/appraisal-and-verification.md`.

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
