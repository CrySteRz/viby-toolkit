# Study: what goes into viby-toolkit 3.2.0

## Protocol (written 2026-11-08, before the first search)

**Decision.** Which 5–10 changes go into the next release, and which ideas are rejected with the
bar each failed. Every answer below changes what gets built, so the study is worth running.

**Question (PICOC).**
- **Population:** viby-toolkit 3.1.0's skills, agents, checkers and hooks, as run by Claude Code
  2.1.319: an Opus main thread with Sonnet agents, used by one owner on VibyFlow, VibyShot and the
  home cluster.
- **Intervention:** changes suggested by evidence that appeared after the 3.1.0 round. That covers
  papers from August to November 2026, jcode upstream since the port, Claude Code 2.1.310–2.1.319,
  rival skill collections, and the owner's own recorded lessons.
- **Comparison:** 3.1.0 as it is (doing nothing).
- **Outcome:** first, fewer confident-wrong outputs and fewer agents that die without an answer.
  Second, cost in tokens and turns. Third, the interface people use.
- **Context:** one owner on a Mac, cost-sensitive because 5-hour usage windows have run out, and
  the repos moving to Gitea.

**Sub-questions, load-bearing first.**
1. Which failures has the owner already recorded while using the toolkit that it still does not
   encode? This is measured on our own use, so it is the strongest evidence.
2. Which Claude Code changes since 2.1.309 break a toolkit assumption, or offer a cheaper built-in
   mechanism than a hand-built one?
3. Which upstream fixes in jcode, firefox-agent-bridge and agentgrep apply to code we ported
   (`jcode@a3107fb`, `firefox-agent-bridge@b8b0c14`, `agentgrep v0.1.7`)?
4. Which new papers correct doctrine the toolkit states, in the way the v0.13 and v0.14 rounds did?
5. Which ideas have rival collections and harnesses added since September?

**Competing answers.**
- **H1, dogfood debt:** the largest gains are lessons already in memory but not in the toolkit.
- **H2, platform catch-up:** the largest gains are new Claude Code features that replace or break
  hand-built mechanisms.
- **H3, doctrine correction:** new research overturns or sharpens a stated rule.
- **H4, nothing material:** only maintenance is warranted.

**Crucial observation.** Count the candidates that survive verification, by source and severity.
- H4 is excluded if any verified candidate fixes a measured failure.
- Whichever source yields the verified high-severity items decides between H1, H2 and H3.

**Search plan.** Six angles run in parallel, one agent each:
1. Papers on the harness: skills, context, memory, multi-agent work and routing.
2. Papers on quality: review precision, self-verification, tests, reward hacking and supply chain.
3. jcode upstream: commit logs after the baselines.
4. Rival collections and harnesses: superpowers, gstack, compound-engineering, anthropics/skills,
   claude-plugins-official, Codex CLI, opencode and Amp.
5. The Claude Code changelog from 2.1.310 to 2.1.319, mapped onto the plugin's files.
6. Memory lessons checked against the toolkit text, plus a tally of workflow agent failures.

The opposing query looks for evidence that scaffolding, skills or multi-agent setups hurt
frontier models, or that skeptic filters remove true positives.

**Inclusion criteria.**
- Papers dated 2026-08-01 to 2026-11-08 whose primary page was opened.
- Changelog entries for 2.1.310 to 2.1.319, with older ones searched only by keyword.
- Upstream commits after the baselines.
- Memory lessons, each checked by grep against the toolkit.

**Exclusion criteria.**
- Vendor multipliers.
- Numbers that come only from a search summary.
- Ideas already recorded in `docs/provenance.md` or `CHANGELOG.md`.

**Stopping rule.** Effort-bounded: each agent has at most about 28 rounds of tool calls and returns
at most 10 findings. Then there is one synthesis and one verification pass over the claims that
synthesis says are load-bearing.

**What would change my mind.**
- If a Claude Code feature makes a toolkit mechanism redundant, the change is a deletion, not an
  addition.
- If papers show that skill scaffolding hurts strong models, the change is fewer skills, not more.

**Deviation, recorded up front.** The protocol was not shown to the owner for approval before the
search. Their request ("start improving again viby toolkit, with research, with findings, new
research papers, with tools like jcode") is taken as the go-ahead for this scan.

## Status

Run on 2026-11-08 as one workflow of 9 agents: 6 angles, 1 synthesis on Opus and 2 skeptics on
Sonnet.

| Item | Count |
|---|---|
| Findings returned by the six angles | 72 |
| Of those, from search summaries only | 3 (none carries a number here) |
| Candidate changes from the synthesis | 12 |
| Candidates re-checked against their sources by the skeptics | 12 |
| Fully confirmed (C1) | 1 |
| Weakened, real but overstated | 11 |
| Refuted | 0 |

The weakened claims were corrected before anything shipped. Three priors were refuted:
- **"The 3.1.0 routing A/B was contaminated by the opt-in skill hint."** The hint fired in 28
  ordinary sessions and in 0 of the 155 A/B transcripts.
- **"Skill bodies are cut at 20,000 characters after compaction."** A third party said so
  ([Superpowers v7.0.1](https://github.com/obra/superpowers), fetched 2026-11-08). The 2.1.319 binary, measured here
  with `strings`, showed no such cut, and the changelog line is about the skill list, not the bodies.
- **"A schema is proven to be why researchers fail."** The 2026-10-29 failures could also be turn
  caps; only the schema-free format is proven to work.

## The answer

**H1, dogfood debt, wins, and H2, platform and upstream catch-up, is a close second. H4 is
excluded.**
- The biggest verified gains were lessons the owner had already paid for and recorded in memory:
  - research schemas;
  - budgets in briefs and the builder cap;
  - launch and wait rules;
  - scratch fences;
  - release traps.

  None was in the plugin, and one contradicted it.
- Next came a real bug in ported code: an agentgrep search that times out keeps running. Upstream
  had fixed it in v0.2.0.
- Last came Claude Code changes that made doctrine out of date.
- H3, doctrine correction from papers, is real but narrow. No paper in the window gave a reason to
  change a rule by itself.
- Confidence is high for what shipped, because each item is either a measured local failure or a
  checked code path. Confidence is low for the deferred items below, which all need a measurement
  we have not run.

## What would change it

- A task-outcome run showing that `debug` or `plan` gives no lift over a bare session would turn
  the next round into deletions.
- A retest on 2.1.319 where a side question no longer derails a builder would retire the launch
  rule.

## Shipped in 3.2.0

See CHANGELOG 3.2.0. Each item, with the candidate it came from:
- **agentgrep v0.2.0 with cancellation (C1).** It was verified by a fresh agent with four planted
  breaks.
- **`exec-bit-lost` in `check-release` (C8).** It needs a `#!` line, after the first version flagged
  `LICENSE` files in viby-flow and viby-shot.
- **Doctrine (C3, C4, C5, C7, C10).** In each case the wording was cut back to what the skeptics
  confirmed.
- **Agent scratch fences and backgrounded-command waits (C2, C6).**
- **Test, orchestrate and README lines.**

## Deferred: these need a measurement or a decision from the owner

- **C9, routing-harness isolation.** `drive.py` loads the owner's user settings, including the
  vibyshot skill, in both arms. Using `--setting-sources project,local` would also drop the toolkit
  itself, so the fix needs `--plugin-dir`. The paired delta from 3.1.0 holds; only the absolute
  level is suspect.
- **C11, a task-outcome arm per skill.** [gstack 1.98.0.0](https://github.com/garrytan/gstack)
  (fetched 2026-11-08) cut `/investigate` at +2.8 points (CI −5.8 to +11.4), which is
  underpowered rather than a null. Reaching a 10-point detectable lift took about 360 trials per arm, so this is the most expensive item and the most informative.
- **C12, an opt-in Stop or SubagentStop hook for the evidence gate.** The only evidence is
  [gstack](https://github.com/garrytan/gstack)'s (fetched 2026-11-08) "26.2% vs 3.6%", which
  counts the hook and their skill as one unit.
- **A size budget for the browser tool definition.**
  [jcode a5a17469f](https://github.com/1jehuang/jcode/commit/a5a17469f) (fetched 2026-11-08) cut
  its own to under 1,400 characters and 14 actions, eval-first, with no accuracy data. Plugin MCP tools are deferred behind
  ToolSearch, so the cost is paid only when the tool loads.

## Rejected, with the bar each failed

- **Already done:**
  - a skill-overlap lint (`check_skills.rs` shadowing);
  - skeptics that execute rather than argue;
  - skipping `/` prompts in the hint;
  - a poisoning gate for memory;
  - per-task state across a task switch (`/clear` between unrelated tasks);
  - the 55-minute wake and no sleep-polling, which are already in the SessionStart contract (the
    memory angle grepped skills only and missed it).
- **Off-scope, because Claude Code owns it:** extractive-compaction rules, and a guard against
  refused handoffs (jcode#2089).
- **Abstract only, no rates:** prescriptive versus conditional wording ([arXiv 2611.04367](https://arxiv.org/abs/2611.04367)), and
  clashing worked examples ([arXiv 2610.25064](https://arxiv.org/abs/2610.25064), no Claude models tested).
- **Cost greater than benefit:**
  - Spine-style inline subtasks ([arXiv 2610.37224](https://arxiv.org/abs/2610.37224)) and SquidAgent's cost test
    before fan-out ([arXiv 2610.08647](https://arxiv.org/abs/2610.08647)). The numbers come from the authors' own harnesses, and writes here already stay
    on one thread.
  - jcode's lane schema, read from commit subjects only.
  - `needs-user` plan markers, with no failure behind them.
- **Kept on purpose:** our browser rule against typing passwords and one-time codes. jcode dropped
  its own on 2026-10-23 for its hosted decision service, which is not evidence that ours is wrong.

## Disagreements between sources

- **Self-preference in review.** [NeurIPS 2026/173082](https://neurips.cc/virtual/2026/173082) measured +11.9 points of self-approval when the
  model really is the author. An older paper (arXiv 2606.20093, search summary only) found none. Either
  way the fresh-context skeptic stands.
- **Is fan-out good?** SquidAgent says parallel setups often lose on wall time. The doctrine keeps
  read-only fan-out, with the 45% baseline rule (`principles` §3).
- **Do skills help?** gstack cut skills that showed no lift. [arXiv 2608.14036](https://arxiv.org/abs/2608.14036) finds skills help as
  "procedural anchors". Both point to measuring each skill (C11).

## Exploratory, not predicted by the protocol

- **Skill routing under a large library.** arXiv 2610.18522 reports loaded-skill recall falling
  from 73.6% to 27.4% when 1,000 irrelevant skills were added, with open-weight executors. This
  supports the listing-budget doctrine. Our 31 skills sit inside a 160+ skill install.
- **Precision of test-weakening detectors.** [arXiv 2608.09290](https://arxiv.org/html/2608.09290) found a regex detector flagged 21.3% of
  trajectories, while a judge confirmed 2.5%. That is a caution for any future "agent weakened the
  test" checker: confirm each candidate, as `secure` already does.
- **No-skill efficiency.** [arXiv 2608.11888](https://arxiv.org/abs/2608.11888) attributes 182 of 307 skill-induced failures to efficiency,
  67 of them to "excessive verification". The evidence gate stands. A stopping rule for `verify` is
  unmeasured.

## What I could not find out

- arXiv rate-limited the harness angle (HTTP 429), so three planned queries did not run.
- Only abstracts were read, not full papers.
- Coverage of October and November papers is thin.
- gstack's `evidence.json` and Amp's docs were not opened.
- Whether a timed-out browser `new_tab` can leave a stale session tab was not traced.
- The workflow journals have no agent-type field, so failure rates per agent are unknown.
  Measured here from those journals: 2,282 started, 2,224 returned and 27 failed since 2026-10-23.

## Sources

**Verified twice:** re-opened by a skeptic after the researcher.
- agentgrep v0.2.0 (`a2c52bb`) README, control.rs and release notes; jcode#1932.
- Claude Code changelog 2.1.301, 2.1.304, 2.1.305, 2.1.311, 2.1.316, 2.1.317 and 2.1.318.
- The gstack 1.98.0.0 changelog.
- arXiv 2608.11888 (abstract).
- Owner memory files: workflow-researcher-schema-failures, workflow-scout-turn-cap,
  workflow-usage-burn, workflow-agents-hear-owner-questions, viby-toolkit-filemode-false and
  test-stdin-epipe-linux-only.

**Fetched once, by the researcher only:**
- arXiv 2608.14036, 2608.24188, 2609.26779, 2610.02163, 2610.08647, 2610.18522, 2610.25064,
  2610.37224, 2611.01181, 2611.02017 and 2611.04367.
- arXiv 2608.09290, 2608.15242, 2608.22103, 2609.00088, 2609.07925, 2609.14079, 2609.17817,
  2609.19101 and 2610.14766.
- ICML 2026/67968 and NeurIPS 2026/173082.
- [jcode](https://github.com/1jehuang/jcode) commits a5a17469f, b7c71bd15, 3587784eb, 67de129ba, 99a791511, 65531f5ec, 92ca1819a,
  eaf55dc3b and fa9b48695.
- Superpowers v7.0.1 and Compound Engineering PR #2011.
