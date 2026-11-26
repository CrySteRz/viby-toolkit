# Study: papers and GitHub projects worth building into viby-toolkit

## Protocol (written 2026-11-26, before the first search)

**Decision.** Which mechanisms from research papers and open-source projects go into the next
release (3.5.0), and which are rejected, with the bar each failed. If nothing survives, the next
release has no outside-idea items, and that is a valid answer.

**Question (PICOC).**
- **Population:** viby-toolkit 3.4.0: its 32 skills, typed agents, checkers (`viby check-*`,
  `scan-test-quality`), hooks, the browser tool and agentgrep. It runs on Claude Code 2.1.332 for
  one owner.
- **Intervention:** mechanisms (a checker, a hook, a protocol step, an agent rule, a tool
  behaviour) taken from papers dated 2026-06 to 2026-11, and from GitHub projects active since
  2026-08.
- **Comparison:** 3.4.0 as it is.
- **Outcome:** first, fewer confident-wrong outputs: bugs missed in review, tests that do not
  test, claims that are not verified. Second, cost. Third, the interface.
- **Context:** one owner; the repo is public; the session's web-search budget is mostly spent, so
  discovery runs through `gh api` search and the arXiv and Semantic Scholar APIs.

**Sub-questions, load-bearing first.**
1. Which verification, review and testing mechanisms have measured gains that transfer to a
   Claude Code toolkit (executed checks, mutation feedback, differential testing, review
   precision)?
2. Which memory, context and planning mechanisms have measured gains (lesson capture, procedural
   memory, compaction, decomposition)?
3. Which Claude Code ecosystem projects (plugins, skills, hooks, MCP servers) do something the
   toolkit lacks, with evidence it works?
4. Which general coding-agent tools (review bots, test generators, code search, evaluation
   harnesses, sandboxes) carry a mechanism worth porting?

**Competing answers.**
- **P1, papers lead:** the verified high-value items come mostly from papers.
- **P2, practice leads:** they come mostly from working open-source projects.
- **P3, nothing new:** the morning round's coverage left nothing material; every candidate is
  already encoded, deferred or rejected.

**Crucial observation.** Count the candidates that survive the skeptics, by source. P3 is
excluded if any verified candidate is new to the toolkit and rests on a measurement.

**Search plan.** Four researchers, one angle each, discovery through APIs:
1. Papers on verification, review, testing and reward hacking.
2. Papers on memory, context, planning and multi-agent coordination.
3. The Claude Code ecosystem on GitHub: plugins, skill collections, hooks, statuslines and MCP
   servers, beyond superpowers, gstack, compound-engineering and anthropics/skills, which were
   read this morning.
4. General coding-agent tools on GitHub: review bots, test generation, mutation testing, code
   search, evaluation harnesses and sandboxes.

Each runs an opposing query: evidence that the mechanism fails, or fails to transfer to strong
models.

**Inclusion criteria.** A paper whose abstract or HTML was opened and that reports a measured
effect. A repository with real use (stars, releases or commits since 2026-08) whose code or docs
were opened.

**Exclusion criteria.**
- Everything shipped, deferred or rejected in `docs/studies/2026-11-26-improvement-round.md` and
  `docs/studies/2026-11-08-improvement-round.md`, or recorded in `CHANGELOG.md` and
  `docs/provenance.md`, unless there is a new reason.
- Decision models and skill hints (removed in 3.2.0 at the owner's request).
- Vendor multipliers, and numbers from a search summary.

**Stopping rule.** Effort-bounded. Each researcher gets at most 30 rounds of tool calls and 6 web
searches, and returns at most 10 findings. Then come one synthesis and two skeptics.

**What would change my mind.** If the skeptics find that most candidates restate rules the
toolkit already has, the sweep was redundant with the morning round, and the next one waits for
new material.

**Deviation, recorded up front.** The protocol was not shown to the owner. Their request ("search
research papers, githubs, and find useful stuff to implement in my toolkit") is taken as the
go-ahead.

## Status

Run on 2026-11-26 as one workflow of 7 agents: 4 researchers on Sonnet, 1 synthesis on Opus and 2
skeptics on Sonnet.

| Item | Count |
|---|---|
| Findings returned by the 4 angles | about 40 |
| Ideas the synthesis dropped, each with its bar | 26 |
| Candidates kept by the synthesis | 6 |
| Confirmed by the skeptics | 0 |
| Weakened: real, but overstated or larger than needed | 6 |
| Refuted | 0 |

One prior was refuted: the browser tool-definition size budget, deferred twice, is now rejected.
A measurement on Opus 5.5 found accuracy barely moved with tool count (94.6% at 30 tools,
[gstack PR #4814](https://github.com/garrytan/gstack/pull/4814), fetched 2026-11-26).

## The answer

**P3, little new, nearly holds. P1, papers lead, holds for the little there is.**
- **Nothing passed cleanly.** No paper or repository yielded a mechanism (a checker or a hook)
  that survived the skeptics unchanged.
- **What passed, cut back:** four one-line protocol additions, each from a paper measured on
  frontier models, and one evaluation. Every one was weakened:
  - The measured setting differs from the toolkit's: benchmark audits, planted leaks, or test
    writers on older models.
  - The toolkit already says part of it.
- **The rest:**
  - The memory and planning papers argue against adding machinery.
  - The GitHub ecosystem gave mostly token-only numbers.
- **Confidence:** high that the sweep was thorough for its window, since most dropped items were
  already encoded. Low that the four lines change behaviour, because no 5-vs-5 run was made.

## Candidates, as the skeptics cut them back

- **C1, verify.** In the report, list each requirement from the request beside the check that
  exercised it; an unmapped requirement is unverified. Source:
  [TestJack, arXiv 2610.10619](https://arxiv.org/html/2610.10619) (fetched 2026-11-26).
  Tests written from the task text refuted 1,545 of 4,487 "resolved" trials (34.4%).
- **C2, worker.** If the task conflicts with a test or checker, stop and report rather than
  proceed. Source: [arXiv 2608.29460](https://arxiv.org/html/2608.29460) (fetched 2026-11-26).
  Hacking was 23.6% with no rule, 9.7% with the policy alone and 5.3% with escalation plus
  policy. Its escalation was a tool, not prose.
- **C3, skeptic.** When a repro is cheap, also run it on the base commit before calling a defect
  "introduced". Source: [arXiv 2606.16062](https://arxiv.org/abs/2606.16062) (fetched
  2026-11-26): "61.9% of LLM-written repair tests fail on the gold patch". The test writer there
  was Sonnet 4.
- **C4, skeptic.** When rejecting a finding whose spot still looks wrong, name the spot in the
  reason. Source: [SecPRBench, arXiv 2610.33509](https://arxiv.org/html/2610.33509) (fetched
  2026-11-26). In 68% of the analysed misses, the agent reported the right file with the wrong
  mechanism.
- **C6, evaluation (large).** An outcome arm comparing /secure with a plain audit request.
  [gstack PR #4833](https://github.com/garrytan/gstack/pull/4833) (fetched 2026-11-26) cut its own
  security skill after a comparison on Opus 5.5 (13 vs 7 and 12 vs 8 of 51 tasks).
  `tests/outcome` grades a hidden test, and /secure writes a report, not a patch. So this needs
  report scoring first, then a 2-3 task pilot.
- **Dropped:** C5, departures listed before coding
  ([arXiv 2611.16289](https://arxiv.org/html/2611.16289), fetched 2026-11-26). There was no
  Claude result, and it harmed one model.

## What would change it

- A 5-vs-5 run of each line on the outcome fixtures, as each candidate's own check describes. If
  a line moves nothing, it does not ship.
- The /secure arm. If a plain audit request matches /secure, the change is a cut, not an
  addition.

## What I could not find out

- **Most papers were read from the abstract and HTML only.**
- **Web search was capped:** the session's web-search allowance was nearly spent, so discovery
  ran through APIs and may have missed projects that only search engines surface.
- **The general coding-agent tooling angle returned confirmations only.** It found no new
  mechanism.
- **No candidate was run.**

## Sources

The 26 dropped ideas and every source the agents read are in the workflow journal
`wf_cbb379b2-3cc`. The sources above were re-opened by a skeptic after the researcher.
