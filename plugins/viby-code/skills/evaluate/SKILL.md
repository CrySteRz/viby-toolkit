---
name: evaluate
description: >
  Use when choosing whether to adopt a tool, library, dependency, service or MCP server —
  "should we use X or Y", "is this library worth adding", "evaluate these options", "run a
  spike on this", "benchmark these two approaches", "is this MCP worth installing", "does
  this actually save us anything". Produces a decision record measured against a case whose
  answer you already know, with the rejections and the back-out path recorded. Distinct from
  brainstorm, which decides WHAT to build, and plan, which decides HOW to build it — this
  decides what to ADOPT, including "nothing".
---

# Evaluate (measure it on a case you already know the answer to)

```
IRON LAW: Rank a candidate only on a case whose correct answer you established FIRST.
          A cost number with no correctness verdict beside it is not a result.
          A fast, cheap, WRONG answer is worse than the slow baseline it replaces.
```

Adoption decisions are unusually easy to get wrong because the evidence on offer is
vendor-shaped: token-savings multipliers, star counts, a demo on a toy repo. All of it
measures the thing that is easy to measure and none of it measures the thing that matters —
whether the answer is *right on your code*. The single most valuable result a spike can
produce is a cheap tool confidently returning the wrong answer, and you only ever see that
result if you knew the right one before you ran it. Follow `/viby-code:principles`.

## 1. Write the oracle down before you install anything

Pick the query or task that represents the payoff — the actual reason you would adopt this —
and establish its correct answer *independently*, by hand, before a candidate is in the
picture. Write down: the question, the ground-truth answer, and how you established it.

> Example: "which app modules import the shared `AuthGuard`?" → ground truth: two, `reports`
> and `web`, established by reading both module files.

Do this first because it is the only step a candidate cannot influence. An oracle picked
after the first run is picked to be one the front-runner passes. If you cannot state a
ground truth for anything the tool is supposed to do, stop — you have no way to tell
adoption from theatre.

Also record the **baseline**: what it costs to do this the way you do it today. Measure it,
don't estimate by feel:

```bash
METER=$(ls "$HOME"/.claude/plugins/cache/*/viby-code/*/skills/evaluate/scripts/measure-read-cost.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-code/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$METER" src/ --top 10                  # what grep-and-read actually costs
sh "$RUN" "$METER" src/ --repeat 6 --budget 60000 # a tool that re-sends state every step
```

## 2. Name the operational criteria before you look at the candidates

List the constraints that would make you refuse a tool no matter how good it is — machine
dependencies, language fit, integration effort, who maintains it, whether it stays fresh
without a manual step, where data goes. Write them down *now*, so the scorecard cannot be
reverse-engineered from the option you already like.

## 3. Trial in isolation, pinned, with the back-out written first

- **Throwaway worktree** (`/viby-code:worktrees`) or a scratch dir, an isolated env.
- **Pin exact versions** in everything you record. "It worked" about an unnamed version is
  not a reproducible result.
- **Verify the package identity** — the plausible-looking name is sometimes the unregistered
  one. Check the publisher matches the repository you actually read.
- **Add nothing to the project's manifests.** A dev/CI aid must not become a runtime
  dependency by accident.
- **Write the uninstall command down before you run the install command.** If you cannot
  state how to back it out, you are not running a trial, you are migrating.

## 4. Measure cost AND correctness in one table

One row per candidate, one column per task, and **every cost carries a verdict**:

| Task | tool A | tool B | baseline (grep+read) |
|---|---|---|---|
| Cross-file impact | 289 ✅ | 244 ❌ *(0 results)* | 6,019 ✅ |
| Find callers | 50 ✅ | 126 ✅ | 8,237 ✅ |
| Exhaustive rename | 485 ❌ *(20 of 61 files)* | 392 ❌ | 3,587 ✅ |

Read that table the way it has to be read: tool B is *cheaper* on impact and *useless* on
it. A savings column on its own would have recommended it. Two rules follow:

- **Cost is payload × cadence, not payload.** A tool that re-sends its whole state after
  every step pays its cost once per step; a 15k payload across a six-step flow is 90k, and
  it will lose to a 40k one-shot read. Measure the multi-step flow you will actually run,
  not one call (`--repeat`).
- **Exclude what nobody reads** from a baseline — `node_modules`, `dist`, lockfiles. A
  ratio computed against an inflated denominator is a flattering fiction.

## 5. Label every cell: measured / inferred / not tested

Say which of the three each claim is, per row, in the table itself. "Engine-equivalent to
the one we did test" is a legitimate finding *and* an inference — write it as one. The
label is what lets a reader weight the row, and it is what stops your own summary from
promoting an inference to a result three paragraphs later.

## 6. Rank twice when the weightings disagree

Rank once by correctness and supply chain; rank again by the operational criteria from step
2. If the leader changes, **show both rankings** and say which weighting you are
recommending on. A single ranking that silently blends the two is where an unstated
preference hides. The winner is usually not "best at everything" but "the only one that
clears every hard constraint" — say it that way.

## 7. Name the winner's failure, and route around it

Every recommendation states the case where its own choice is wrong or incomplete, with the
number attached ("20 of 61 files"), and what to fall back to there. A recommendation with no
failure case has not been tested hard enough to trust.

This is also why the answer is usually a **routing rule rather than a tool**: use the graph
for structure, plain grep for exhaustive text; use the cheap browser tool in the dev loop,
the heavy one for committed E2E. Write the routing rule where it will be read by default —
the project's `CLAUDE.md` or a skill — not only in the spike document nobody opens again.

## 8. Generalisation check

Re-run the decision rule on a **second, unrelated case** — a different repo, a different
ticket shape, a different feature. If the rule holds in both, it generalises; if the winner
flips, you have found the boundary, which is the real finding. One case makes a champion, two
make a rule.

## 9. Record the rejections, and the claims your measurement corrected

- **Every rejected candidate gets a line naming the bar it failed** — "sends page content to
  a third-party service → fails the privacy bar", "needs a runtime we don't have → fails the
  dependency bar". Rejections are the most reusable part of the document: they stop the same
  option being re-proposed every quarter.
- **List what the measurement proved wrong about your own research.** "We asserted X in the
  survey; the run refutes it" is the highest-credibility paragraph in the document, and
  writing it is what stops a survey-stage guess from being cited later as a result.
- If no independent benchmark exists for this comparison, say so plainly — a labeled local
  test is then your evidence, and that is worth more than an adjacent citation.

## 10. Supply chain and egress are gates, not tie-breakers

- **Where does the data go?** Name every egress and how to turn it off. A candidate that
  ships your source or your users' data to a third party fails on that alone, whatever it
  scores. State the egress that exists anyway so the delta is honest.
- **Who maintains it?** One-maintainer projects, no commits in months, and a star count that
  outruns the commit history are risk signals — record them as accepted cost, not as
  disqualifiers, and never as adoption evidence.
- **What is the blast radius if it is wrong?** A planning aid that misleads costs a wasted
  hour. Something in the path of auth, payments or migrations costs more, so the correctness
  bar rises with it. Say which one this is.

## Output — the decision record

Save it (`docs/decisions/<date>-<topic>.md` or the project's convention):

1. **Decision** in one paragraph, up front, with the accepted trade-off named.
2. **The oracle** — question, ground truth, how established.
3. **The table** — cost × correctness, labeled measured/inferred/not tested, versions pinned.
4. **Both rankings**, if they disagree, and which one you recommend on.
5. **The winner's failure case** and the fallback for it — the routing rule.
6. **Rejections**, each with the bar it failed.
7. **Corrections** — what the measurement proved wrong about the research.
8. **Back-out** — the exact commands, and what is left behind after them.

Then record the durable part with `/viby-code:learn` — a rejected-option-and-why is exactly
the lesson a future session would otherwise pay to re-derive.
