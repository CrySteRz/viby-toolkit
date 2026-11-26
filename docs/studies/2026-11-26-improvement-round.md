# Study: what goes into viby-toolkit 3.4.0

## Protocol (written 2026-11-26, before the first search)

**Decision.** Which changes go into the next release, which dependency moves ship with it, and
which ideas are rejected, with the bar each failed. Each answer changes what gets built, so the
study is worth running.

**Question (PICOC).**
- **Population:** viby-toolkit 3.3.0 (plus the unreleased `0b93d08`): its skills, agents, checkers,
  hooks, launcher and browser bridge. It runs under Claude Code 2.1.332, with an Opus main thread
  and Sonnet agents, for one owner across personal and work repos.
- **Intervention:** changes suggested by evidence that appeared after the 3.2.0/3.3.0 round of
  2026-11-08. That covers papers after arXiv 2611.04367, Claude Code 2.1.320 to 2.1.332, jcode,
  agentgrep and firefox-agent-bridge upstream, rival collections and harnesses, practitioner
  sources this round adds (Hacker News, Anthropic's docs and engineering posts, other agent CLIs),
  the owner's lessons recorded since 2026-11-08, and dependency drift.
- **Comparison:** 3.3.0 as it is (doing nothing).
- **Outcome:** first, fewer confident-wrong outputs and fewer agents that die without an answer.
  Second, cost in tokens, turns and disk. Third, the interface people use.
- **Context:** one owner on a Mac, cost-sensitive (5-hour windows, the 200-search session cap). The
  toolkit repo is public, so lessons from work repos must enter it generalised, with no names.

**Sub-questions, load-bearing first.**
1. Which failures has the owner recorded since 2026-11-08, in personal or work memory, that the
   toolkit's skills, agents or hooks still do not encode?
2. What do the toolkit's own runs since 2026-11-08 show (workflow journals: failures, cut-offs)?
3. Which Claude Code changes from 2.1.320 to 2.1.332 break a toolkit assumption, or offer a
   cheaper built-in mechanism than a hand-built one?
4. Which upstream fixes in jcode, agentgrep and firefox-agent-bridge apply to code we ported?
5. Which dependencies (crates, pinned actions, agentgrep) have moved, and are any moves needed for
   security or correctness, not only freshness?
6. Which new papers correct doctrine the toolkit states?
7. Which ideas have rival collections, other CLIs and practitioners added since 2026-11-08?

**Competing answers.**
- **H1, dogfood debt:** the largest gains are lessons already in memory but not in the toolkit.
- **H2, platform and upstream catch-up:** the largest gains are Claude Code, upstream or
  dependency changes.
- **H3, outside ideas:** papers, rivals or practitioners supply the largest gains.
- **H4, nothing material:** only maintenance (dependency bumps) is warranted.

**Crucial observation.** Count the candidates that survive the skeptics, by source and severity.
- H4 is excluded if any verified candidate fixes a measured failure.
- Whichever source yields the verified high-severity items decides between H1, H2 and H3.

**Search plan.** Nine angles run in parallel, one agent each:
1. Papers on the harness and on quality: skills, context, memory, multi-agent work, routing, cost,
   review precision, self-verification, tests, reward hacking and supply chain.
2. Upstream: jcode, agentgrep and firefox-agent-bridge, measured from the baselines below.
3. Rival collections and harnesses, read through `gh api`.
4. Other sources, new this round: Hacker News, Anthropic's engineering posts and Claude Code docs,
   other agent CLIs' changelogs, practitioner blogs.
5. The Claude Code changelog, 2.1.320 to 2.1.332, mapped onto the plugin's files.
6. Memory lessons: personal memories written since 2026-11-08 and the work memory set, each
   grepped against skills, agents, hooks and the SessionStart contract.
7. Usage, measured: the owner's transcripts and workflow journals since 2026-11-08. It covers
   which skills, agents, hooks and MCP tools ran, which failed or were cut off, and where the
   tokens went.
8. Usage, in the owner's words: prompts, interruptions, rejected tool calls and corrections since
   2026-11-08, clustered into recurring friction.
9. Dependencies: crates, agentgrep, pinned actions and toolchains, with advisories.

The opposing query looks for evidence that scaffolding, skills, hooks or multi-agent setups hurt
frontier models, or that verification gates waste more than they save.

**Baselines.** Claude Code 2.1.319, agentgrep v0.2.0 (`a2c52bb`), jcode up to 2026-11-08,
firefox-agent-bridge `b8b0c14`, arXiv up to 2611.04367, superpowers v7.0.1, gstack 1.98.0.0.

**Inclusion criteria.**
- Papers dated 2026-11-01 to 2026-11-26 whose primary page was opened, plus older ones found by
  snowballing.
- Changelog entries for 2.1.320 to 2.1.332.
- Upstream commits after the baselines.
- Memory lessons, each checked by grep against the toolkit.

**Exclusion criteria.**
- Vendor multipliers.
- Numbers that come only from a search summary.
- Ideas already recorded in `CHANGELOG.md`, or deferred or rejected in the 2026-11-08 study
  without a new reason.
- Decision models (removed in 3.2.0 at the owner's request).

**Stopping rule.** Effort-bounded. Each web researcher gets at most about 35 searches and 30
rounds of tool calls, and returns at most 10 findings. Then come one synthesis and one skeptic
pass over every candidate the synthesis keeps.

**What would change my mind.**
- If a Claude Code feature makes a toolkit mechanism redundant, the change is a deletion.
- If papers show that scaffolding hurts strong models, the change is fewer skills, not more.
- If no candidate survives, the release is a dependency-only patch.

**Deviation, recorded up front.** The protocol was not shown to the owner before the search.
Their request ("start improving viby toolkit as usual, researchers, jcode, etc, search other
sources as well, improvements, dependencies bumps") is taken as the go-ahead.

**Changed before the first search.** While the protocol was being written, the owner added
"improve it based on my usage as well". Two usage angles (7 and 8) were added. To keep the run
near twelve agents, the two paper angles of the last round were merged into one; the window is
only three weeks.

## Status

Run on 2026-11-26 in three steps, all agents read-only, with every write on the main thread.
1. One workflow of 12 agents: 9 angles on Sonnet, 1 synthesis on Opus and 2 skeptics on Sonnet.
2. Mid-round, the owner asked for the browser agent too, then for "mozilla stuff directly". Four
   more agents covered jcode's browser code, browser usage, browser lessons and Mozilla's and
   Chromium's primary sources.
3. A review workflow over the code diff.

| Item | Count |
|---|---|
| Findings returned by the 13 research agents | about 130 |
| Candidates from the synthesis | 15 |
| Confirmed by the skeptics | 3 (C2, C4, C11) |
| Weakened: real, but overstated or larger than needed | 12, each cut back before it shipped |
| Refuted | 0 |
| Browser changes added from the four browser agents | 7 |
| Deferred | 3 |

Five priors were refuted (each measured here):
- **"The Brave install breaks with Chrome's Manifest V2 removal"** (the platform agent).
  `install.rs:87` writes a Manifest V3 build for Chromium; only the Firefox build is MV2.
- **"A schema plus a restricted agentType loses the stage"** (a memory note). Since 2026-11-08,
  35 of 35 typed reviewer and skeptic workflow agents returned their structured result.
- **"Upstream changed the browser code we ported"** (the owner's expectation).
  firefox-agent-bridge has 0 commits since `b8b0c14`. jcode's 9 browser commits since the port
  serve its removed handoff model or shrink its tool text; none fixes code we carry.
- **Three usage counts were off** when the skeptics re-ran them: 157 sleep calls, not 145; 6
  re-review prompts, not about 15; 9 "use the browser agent" prompts, not 17.
- **One of this round's own tests was weak.** A planted break (the transient retry removed)
  still passed, because the fake host answered the server's `ping` with the planted failure.
  The test now fails only the snapshot's own request, and the same plant now fails it.

## The answer

**H1, dogfood debt, wins again, and this round its strongest source is the owner's own usage.
H4 is excluded.**
- Both high-severity candidates came from recorded lessons:
  - Mutation runs that measure the wrong thing (C1).
  - Agents that leave build output and hung processes behind (C2): 88 GB, 149 GB and an orphan
    at 490% CPU (measured here, in the owner's recorded runs).
- The usage angles measured failures nobody had recorded:
  - `eval` rejected `await` in 53 of 132 eval errors (measured here).
  - Agents ended turns on "waiting" with nothing armed: 157 sleep calls and 37 status pings
    (measured here).
  - 323 browser tabs were opened and almost none closed (measured here).
  - The page attached to `open` was 48% of all browser output (measured here).
- H2, platform, upstream and dependency catch-up, gave real but small items:
  - A stale model lineup with a Fable 5.2 security trap.
  - One `unsound` advisory, on a dev-dependency.
  - Three crate jumps, each verified on its own.
  - Firefox 153's `file://` grant.
- H3, outside ideas, gave one paper-backed rule (no special-casing a test,
  [arXiv 2611.23335](https://arxiv.org/abs/2611.23335) (fetched 2026-11-26)) and a rival's null figure for after-edit
  stop hooks.
- Confidence is high for what shipped: each item is a measured local failure or a checked code
  path, and each code change has a test that a planted break makes fail. Confidence is low that
  the contract lines will change behaviour. The sleep-poll line already existed and did not hold.

## What would change it

- A re-count over the two weeks after release. If sleep calls, status pings and stray tabs do
  not fall, the contract lines and the agent brief failed, and the next fix must be a mechanism
  (a hook or a tool behaviour), not prose.
- A live run in each browser after the extension reloads to 0.10.3 (not done here, see below).
  If `pressKey` does not reach a framework's key handler from the content script, revert `press`
  to page-world eval on pages without a strict policy.

## Shipped

Each item names its candidate and the check that proves it.
- **Browser `eval` is async, evaluated once (C9, cut back).** It covers all three evaluators.
  Three Node-run tests failed before the change (`await` rejected, a page-world script run twice)
  and pass after it (measured here).
- **Browser round (from the four browser agents).**
  - `close_tab`, limited to tabs this session opened.
  - `press` dispatched natively, so a page's policy no longer blocks it.
  - An 8,000-character page preview on `open`.
  - Errors that name the next step.
  - One silent retry of a read cut off by navigation; a click is never retried.
  - A window opened when none exists.
  - Extension 0.10.3, plus a test that fails when its code changes without a version bump.
  - Five new tests, three of them proven by planted breaks.
- **Agent rules (C1, C2, C11).**
  - reviewer, skeptic, debugger: read the mutant back, bound the run with `timeout`, mutate in a
    private plain copy.
  - implementer, worker: no special-casing a test; leave nothing behind.
- **Contract and doctrine.**
  - C4: arm a trigger before ending on "waiting".
  - C6: typed agents for read-only fan-out.
  - C5: act vs ask; CI state read from the run, not a watcher's exit code.
  - C3: the resume caveat.
  - C2: disk and processes are cost too.
  - C12: model lineup, and security stops at opus.
  - C7: a re-review covers only what moved.
  - C8: a search budget per researcher, and `browser-needed:` gaps.
  - C13, the README part only: the stop gate's second figure.
- **Dependencies**, one commit each, the full gate green after each:
  - C14: `cargo update`, 17 crates, which clears RUSTSEC-2026-0486.
  - C15: fancy-regex 0.14 to 0.20.2. Five checkers over four real JS/TS repositories and this
    one print byte-identical output, 20 of 20 (measured here).
  - windows-sys 0.59 to 0.61.2, checked and linted for the Windows target.
  - Rust 1.98.1 to 1.100.0.

## Deferred

- **C10, installing the viby binary at session start on every OS.** Whether SessionStart
  finishes before the MCP servers start is unknown, so the hook might only race them. A probe with
  `claude --debug` decides between the hook and a wait inside the launcher.
- **C13's lint subcommand.** The owner's own hook already blocks Workflow scripts with no effort
  set, and nobody else has asked for it.
- **A size budget for the browser tool definition**, deferred again. jcode caps its own at 1,400
  bytes ([jcode a5a17469f](https://github.com/1jehuang/jcode/commit/a5a17469f) (fetched 2026-11-26)), with no accuracy
  data, and its eval-first design hides actions our agents use.
- **Grouping agent tabs** with `tabs.group`
  ([Firefox 151 notes](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/151) (fetched 2026-11-26)).
  `close_tab` answers the measured problem; grouping waits for a failure it would fix.

## Rejected, with the bar each failed

- **Already encoded:** the skill-body size lint, test fixtures inside skill folders, the
  `/ship` name collision, the CSP page-world retry, and the context target.
- **Too little data:** cutting the 19 skills that never ran. The window is 18 days and 71
  sessions; re-check over a longer one.
- **Off-scope:** missed local context (it belongs in each folder's CLAUDE.md), safety-classifier
  stops, and Mozilla's `firefox-devtools-mcp`, which drives a separately started Firefox, not the
  person's session.
- **Cost greater than benefit:**
  - the `dtolnay/rust-toolchain` pin: its only change since our SHA is its own CI file;
  - base64 0.23: it adds SIMD `unsafe` by default and fixes nothing here;
  - the stop gate's observe mode;
  - `engines` in plugin.json.
- **Papers with no Claude result, or abstract only:** 2611.22238, 2611.30185, 2611.06545,
  2611.30932, 2611.27112, 2611.33943, 2611.33098 and 2611.09802 (all arXiv).

## Disagreements between sources

- **Eval-first or snapshot-first?** jcode now steers its model to `eval` and hides the DOM
  helpers. Our usage shows `eval` is already 26% of calls (measured here) and the least reliable action on strict
  pages. We keep snapshot-first, with errors that point to the helpers.
- **Is an after-edit stop hook worth it?** gstack's own measurement says no: "between -0.5 and +2
  points, inside the noise"
  ([gstack 1.100.0.0](https://github.com/garrytan/gstack/blob/main/CHANGELOG.md) (fetched 2026-11-26), fetched
  2026-11-26). Ours stays opt-in and unmeasured.

## Exploratory, not predicted by the protocol

- **The extension's version is its deploy switch.** The server reloads a browser's extension only
  when the version differs (`server.rs:635`). A code change without a bump would never have
  reached the person's browser. A test now pins the code's hash to the version.
- **Typed agents are mostly bypassed** for read-only work (59 of 85 untyped general-purpose
  agents, measured here), and 19 of 32 skills never ran in 18 days (measured here). Both are
  weak-usage signals, not proof.

## What I could not find out

- **Nothing was driven live in a browser.** The extension changes are tested in Node on the real
  functions, and the server in Rust against a fake host. A live check needs the release, which
  the owner gates. Until then, other sessions still run 0.10.2.
- **Brave's handling of unpacked Manifest V2 extensions** is unknown, but the Brave build is V3.
- **Whether the contract lines change behaviour** is unmeasured until the next window.
- **Firefox 162 and 163 notes**, the Add-ons blog's 2026 enforcement date for
  `data_collection_permissions`, and Chrome 150 to 153 beyond the "what's new" list were not
  read.
- **No cut-off was ever recorded:** 0 stop-gate firings and 0 Stop-hook records in the window
  (measured here), so the gate is unexercised.

## Sources

**Verified on the main thread, by fetching or running:**
- [Anthropic, Fable and Mythos 5.2](https://www.anthropic.com/claude-fable-and-mythos-5-2) (fetched 2026-11-26), both
  quotes found in the page text.
- [gstack CHANGELOG](https://github.com/garrytan/gstack/blob/main/CHANGELOG.md) (fetched 2026-11-26), 1.100.0.0,
  line 696.
- [fancy-regex CHANGELOG](https://github.com/fancy-regex/fancy-regex/blob/main/CHANGELOG.md) (fetched 2026-11-26),
  0.15 to 0.20.2.
- The windows-rs releases.
- `cargo audit` 0.22.2 output.
- The owner's transcripts and memory files, re-counted by the skeptics.

**Re-opened by a skeptic after the researcher:**
- [arXiv 2611.23335](https://arxiv.org/abs/2611.23335) (fetched 2026-11-26).
- The Claude Code changelog, 2.1.321 to 2.1.332.
- Work and personal memory files: parallel-mutation, hard-linked copies, mutation applied, call
  site, builds fill the disk, hung processes, resume re-runs, search cap.

**Fetched once, by a researcher only:**
- [MDN Firefox 151](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/151) (fetched 2026-11-26),
  [152](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/152) (fetched 2026-11-26) and
  [153](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/153) (fetched 2026-11-26).
- [Chrome MV2 timeline](https://developer.chrome.com/docs/extensions/develop/migrate/mv2-deprecation-timeline) (fetched 2026-11-26).
- [Mozilla Add-ons blog, Firefox 153](https://blog.mozilla.org/addons/2026/07/23/firefox-153-webextensions-api-updates/) (fetched 2026-11-26).
- jcode commits 915b960, fa9b486, da1ea17, f380ab8, b7c71bd, a5a1746, e00c07c and 553037b.
- The remaining arXiv abstracts listed under "Rejected".
