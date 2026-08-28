# The two hard gates, and the shape of the decision record

Read at two moments: when you are about to rank candidates (the gates below disqualify, they do not
trade off), and when you sit down to write the document.

## 10. Supply chain and egress are gates, not tie-breakers

- **Where does the data go?** Name every egress and how to turn it off. A candidate that
  ships your source or your users' data to a third party fails on that alone, whatever it
  scores. State the egress that exists anyway so the delta is honest.
- **Who maintains it?** Dispatch a `researcher` for this rather than reading repos inline — it is
  read-only fan-out and the raw pages are worthless to you afterwards. One-maintainer projects, no
  commits in months, and a star count that outruns the commit history are risk signals — record them as accepted cost, not as
  disqualifiers, and never as adoption evidence.
- **What is the blast radius if it is wrong?** A planning aid that misleads costs a wasted
  hour. Something in the path of auth, payments or migrations costs more, so the correctness
  bar rises with it. Say which one this is.

## Output — the decision record

Save it (`docs/decisions/<date>-<topic>.md` or the project's convention):

0. **A status line, first thing** — what was actually done, so a reader can weigh the
   document before reading it. The same audit trail `review` ends with, at the top:

   > Status: **benchmarked on this repo** (2026-07-08, commit `d1a0b95`) · 5 candidates, 3
   > stood up and measured, 2 rejected on stated bars · 4 tasks × ground truth · **2 claims
   > from the survey section refuted by the run**.

   A document that cannot fill that line honestly is a survey, not an evaluation — label it
   as one rather than dressing it up.
1. **Decision** in one paragraph, up front, with the accepted trade-off named.
2. **The oracle** — question, ground truth, how established.
3. **The table** — cost × correctness, labeled measured/inferred/not tested, versions pinned.
4. **Both rankings**, if they disagree, and which one you recommend on.
5. **The winner's failure case** and the fallback for it — the routing rule.
6. **Rejections**, each with the bar it failed.
7. **Corrections** — what the measurement proved wrong about the research.
8. **Back-out** — the exact commands, and what is left behind after them.

Then record the durable part with `/viby-toolkit:learn` — a rejected-option-and-why is exactly
the lesson a future session would otherwise pay to re-derive.
