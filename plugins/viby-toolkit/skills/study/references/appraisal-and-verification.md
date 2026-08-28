# §6–§7 in depth — appraising sources and auditing your own process

Read once you have search results in hand and are deciding what each one is worth, and again before
you write the study up. The skill body carries the rules; this carries the procedure.

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
  `/viby-toolkit:review` uses on code findings, and it exists because a link that
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

