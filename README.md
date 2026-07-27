# viby-toolkit

My personal Claude Code toolkit, distributed as a private plugin marketplace. One repo,
installed once per machine at **user scope**, so it applies automatically to every
project — work and personal — and travels with me to any new computer.

Marketplace: **`viby-toolkit`** · Plugin: **`viby-code`**

---

## What's in it

`viby-code` is an accuracy-first, token-disciplined set of engineering workflows. It's
stack-agnostic (detects the project at runtime; assumes nothing). Everything executable is
TypeScript with zero runtime dependencies and no build step — see
[Why TypeScript](#why-typescript-and-how-it-runs-without-a-build-step).

### Skills (auto-trigger by context, or call with `/viby-code:<name>`)

| Skill | What it does |
|---|---|
| `/viby-code:brainstorm` | **Design-before-code gate.** Decides WHAT to build (and whether it's the right thing) with an Iron-Law hold on any implementation until you approve the design. Runs before plan/orchestrate for anything whose shape isn't settled. |
| `/viby-code:orchestrate` | Drives a task end-to-end: scope → research → plan → implement → verify → self-review. Fans out cheap scouts for discovery, keeps writes single-threaded, keeps main context clean. |
| `/viby-code:review-cluster` | **Review cluster + false-positive filter.** Parallel per-dimension reviewers (incl. an adversarial chaos-engineer dimension) find candidates; a grounding gate drops anything that can't quote its own line; one fresh-context validator per finding confirms real/introduced/not-already-handled; a confidence gate suppresses below-threshold. Reports the full kill count. |
| `/viby-code:verify` | **The evidence gate, executed.** Finds the project's real checks (CI config is authoritative), scopes them to the change, exercises the actual behavior — then screens the output for silent-pass modes, because a zero exit code with zero tests collected is not a pass. Fix the code, never the check. |
| `/viby-code:test` | **QA and test design, with a scanner.** Picks the test level deliberately, insists every new test is *seen failing for the right reason* before it's trusted, and enforces mocking discipline (coding agents over-mock measurably more than humans). Ships an executable auditor — `scan-test-quality.ts` finds no-assertion tests, tautologies, over-mocking, `.only`/`.skip` left in, sleep-waits and swallowed errors, with `file:line`. |
| `/viby-code:debug` | Root-cause debugging by hypothesis and evidence — reproduce (as a failing test, routed to the strong model) → localize → confirm → fix → verify. No speculative patching. |
| `/viby-code:migrate` | Wide mechanical changes (renames, upgrades, pattern sweeps): discover every site → transform in batches → verify each → final zero-remaining sweep. |
| `/viby-code:plan` | Turns an agreed idea into an ordered, file-anchored change-list with the risky step and verification strategy called out. Plan doubles as a durable checkpoint. |
| `/viby-code:learn` | Records a reusable lesson (gotcha, build quirk, rejected finding, known past risk, "never compact X") to Claude's native project memory — the compounding loop, both suppressing false positives and raising recall on known risks. |
| `/viby-code:handoff` | Serializes live task state (goal, decisions, next step) so a fresh session resumes mid-task without re-deriving it. Ephemeral, distinct from `learn`. |
| `/viby-code:worktrees` | Isolates work (parallel implementers, risky experiments) — detect existing isolation first, prefer the native worktree tool, never fight the harness. |
| `/viby-code:principles` | The operating contract everything follows: accuracy rules, the fan-out law, model-routing + escalation ladder, context discipline, the evidence gate. Read-only reference. |

### Command

- `/viby-code:ship <task>` — run the whole pipeline autonomously and don't stop until verified.

### Agents (dispatched by the skills; cheap models by design)

`scout` (haiku, read-only recon) · `implementer` (sonnet) · `reviewer` (sonnet,
one per review dimension) · `skeptic` (sonnet, adversarial false-positive filter) ·
`debugger` (sonnet, evidence gathering). Each is colour-coded in the transcript so a
parallel fan-out is readable at a glance.

### Hooks

One `SessionStart` hook injects the working-style defaults. Nothing intercepts or blocks
commands. See **Hooks** below.

---

## The token / rate-limit strategy (without losing accuracy)

Designed for a Claude Max subscription, where the scarce resources are the **main
thread's context window** and your **rate-limit budget** — not dollars.

1. **The fan-out law.** *Fan out for READ; keep WRITES single-threaded.* Parallel
   read-only subagents (search/explore/review) are a genuine win — they isolate verbose
   output and improve quality. Parallel *writers* are a trap: they make conflicting
   decisions from partial context and produce incoherent results. This is where Anthropic
   and Cognition both landed after a year in production; on Max, every fan-out also burns
   rate-limit budget ~an order of magnitude faster, so it's gated behind "is this genuinely
   parallel *and* read-only?"
2. **Subagents are context firewalls.** Bulk reading (grep 40 files, read 10) happens in
   disposable subagents that return a ~200-token conclusion. The 30k tokens of file dumps
   die with the subagent and never touch main context.
3. **Model routing — the full lineup, by need.** `haiku` (4.5) for mechanical search;
   `sonnet` (5) for read-only reviewing/scouting fan-out; `opus` (4.8) for planning,
   synthesis, judgment, and all writes; `fable` (5), the most capable tier, reserved for
   the hardest, highest-stakes calls (authoring a repro test, resolving conflicting
   verdicts, subtle security/concurrency) and the top of the escalation ladder. Cheap
   models *find*; the strong model *decides*; escalate haiku → sonnet → opus → fable on low
   confidence. Fable is a scalpel (heaviest on rate-limit), not a default.
4. **Frequent intentional compaction.** Target 40–60% context utilization. Research and
   plan become durable markdown artifacts; the plan doubles as a checkpoint so a `/clear`
   loses no state. Context quality priority: Correctness > Completeness > Size.
5. **Evidence-gated completion.** Never claim done without running the check fresh and
   showing its output *and exit code*. The words "should / probably / seems" are the tell
   that you skipped verification — and a zero exit code is not a pass if zero tests ran,
   everything skipped, or the check was neutered by `|| true`. `/viby-code:verify` runs this
   as a procedure rather than leaving it as an aspiration.
6. **Adversarial verification** keeps accuracy high while most tokens are spent cheaply —
   many cheap voices get cross-checked, so a single cheap voice being wrong doesn't sink
   the result.
7. **Compounding.** Each solved problem and each rejected review finding is recorded to
   native memory (`/viby-code:learn`), so the next session is cheaper and the reviewer's taste
   drifts toward yours.

Full contract: `/viby-code:principles`.

### Provenance

The workflows distill what's actually working in production agentic coding as of mid-2026,
keeping the mechanisms and discarding the marketing multipliers: Anthropic's
context-engineering, multi-agent-research, and Claude Code best-practices docs; humanlayer's
Advanced Context Engineering (frequent intentional compaction); obra/superpowers (the
Iron-Law skill format, trigger-only descriptions, TDD/verification/systematic-debugging);
Cognition (the read-vs-write fan-out rule); and Every's compound engineering (the learning
loop, multi-persona review, grounded findings schema).

The v0.3.0 review pipeline and reliability upgrades additionally draw on 2026 research:
the quote-the-line grounding gate and single-fresh-validator-over-panel design
(Refute-or-Promote; "Nine Judges, Two Effective Votes"); reproduction-test-as-the-bottleneck
(TDFlow and cogeneration-of-repro-test papers); the context-ledger idea (VISTA); failure-
driven "never-compact" lessons (ACON); and the escalation-ladder / cheap-model-danger-zones
model-routing guidance. Overstated single-number claims were deliberately dropped after an
adversarial fact-check; only convergent, credible mechanisms were kept.

The v0.5.0 testing module (`/viby-code:test`) is grounded in four verified sources, each read
rather than taken from a summary:

- **Over-mocking is an agent-specific failure mode.** *Are Coding Agents Generating
  Over-Mocked Tests?* ([arXiv 2602.00409](https://arxiv.org/abs/2602.00409)) measured real
  repositories: agent test commits add mocks 36% of the time vs 26% for non-agent commits,
  and agents use the `mock` double in 95% of cases where humans spread across mocks (91%),
  fakes (57%) and spies (51%). The paper's own recommendation is to *"include guidance on
  mocking best practices and anti-patterns in agent configuration files"* — which is exactly
  what the skill's mocking section is.
- **Mutation beats coverage as the quality signal.** *Test vs Mutant: Adversarial LLM Agents
  for Robust Unit Test Generation* ([arXiv 2602.08146](https://arxiv.org/abs/2602.08146))
  runs a test-generator against a mutant-generator in an adversarial loop, reporting +8.56%
  fault detection over LLM baselines and +63.30% over EvoSuite on Defects4J; Meta's ACH
  deployed mutation-guided generation at scale with 73% of generated tests accepted. The
  skill folds in the cheap manual form — deliberately break the code, confirm the test goes
  red — plus the "weakest-test question", since most repos have no mutation tooling.
- **Named smells to hunt.** *Test smells in LLM-Generated Unit Tests*
  ([arXiv 2410.10628](https://arxiv.org/abs/2410.10628)) finds Assertion Roulette and Magic
  Number Test most prevalent across 20,505 generated suites. Assertion roulette became a
  scanner check; magic numbers stayed guidance only, because as a mechanical check it fires
  constantly on legitimate table-driven tests — precision over coverage, as everywhere else
  here.
- **Ship executable checks, not just prose.** *Harness Engineering for Agentic AI Coding
  Tools* ([arXiv 2602.14690](https://arxiv.org/abs/2602.14690)), a study of 2,853
  repositories, finds that where Skills are used at all they "typically rely on static
  instructions rather than executable scripts." That's the gap `scan-test-quality.ts` and
  `tests/` close: guidance an agent can *run*, not only read.

---

## Hooks

- **SessionStart** injects the accuracy/fan-out defaults (~400 tokens) so the working style
  applies even when no skill is invoked. This is the only hook enabled by default.
- **Opt-in** (shipped, not enabled): `hooks/post-tool-use-format.ts` auto-formats edited
  files *only* when the formatter is already installed and the project uses it (never
  installs anything, never blocks). Enable by adding a `PostToolUse` matcher if you want it.

**No command-blocking guard.** An earlier version shipped a `PreToolUse` hook that vetoed
destructive Bash commands. It's gone: this is a single-user toolkit, and a veto that has to
be argued with costs more than it protects. Nothing here now intercepts or blocks a command
— permissions are left entirely to Claude Code itself. (The removed hook and its 110-case
contract are still in git history if it's ever wanted back.)

## Telemetry — prove the token strategy works

The context discipline is measurable, not just asserted:

- **Statusline** (`hooks/statusline.ts`) — shows
  `model · ctx NN% · cache NN% · 5h NN% · 7d NN% · $cost`. `ctx` is
  `context_window.used_percentage` (input tokens: fresh + cache creation + cache read, as a
  share of the window) colour-banded green <60 / yellow <80 / red — watching it is how you
  hold the 40–60% target. `5h`/`7d` are **rate-limit consumption**, which is the resource
  that actually binds on Max, so they rank above the cost figure; they appear only for
  Pro/Max after the first API response. Absent or null fields are skipped rather than shown
  as `0`. Wire it in `~/.claude/settings.json` (or just use `bunx ccusage statusline`) —
  this form survives version bumps instead of hardcoding one:
  ```json
  { "statusLine": { "type": "command",
      "command": "sh \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-code/*/hooks/run.sh | tail -1)\" \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-code/*/hooks/statusline.ts | tail -1)\"" } }
  ```
- **OpenTelemetry** — set `CLAUDE_CODE_ENABLE_TELEMETRY=1` and export
  `claude_code.token.usage`; group by `query_source` (`main` vs `subagent`) and `agent.name`
  to measure fan-out ROI directly (main-thread tokens saved vs subagent tokens spent), and
  by `type=cacheRead` to confirm cache reuse.
- **ccusage** — `npx ccusage@latest blocks --live` for a zero-setup real-time view;
  `/cost` for an in-session check.

## Install on a new machine

Prereqs: `gh` authenticated (`gh auth status`) or SSH access to this private repo.

**Runtime:** the skills, agents and prompts are plain markdown and need nothing. The
executable parts (statusline, test scanner) need **Node ≥22.6**, or bun, or
`tsx` — `hooks/run.sh` takes the first one it finds. With none of them present the hooks
no-op silently and the markdown half still works, so a machine without a JS runtime gets a
degraded-but-functional install rather than errors. Check with `node --version`.

**Option A — via Claude Code (recommended):**

```
/plugin marketplace add ionutblidaruvsp/viby-toolkit
/plugin install viby-code@viby-toolkit
```

Then confirm it's enabled at **user scope** so it applies to every project.

**Option B — via settings.json** (makes it declarative / reproducible). Add to
`~/.claude/settings.json`:

```jsonc
{
  "extraKnownMarketplaces": {
    "viby-toolkit": {
      "source": { "source": "github", "repo": "ionutblidaruvsp/viby-toolkit" }
    }
  },
  "enabledPlugins": {
    "viby-code@viby-toolkit": true
  }
}
```

Restart Claude Code. Verify with `/plugin` (should list `viby-code` as enabled) and by typing
`/viby-code:` (skills should autocomplete).

### Private-repo auto-update note

Background auto-update disables git credential helpers, so HTTPS pulls of a private
marketplace can fail silently. Either:
- use **SSH** (loaded key in `ssh-agent`) for the marketplace, or
- run `gh auth setup-git` once, or
- set `CLAUDE_CODE_PLUGIN_KEEP_MARKETPLACE_ON_FAILURE=1` to keep the working clone if a
  refresh fails.

Force a manual update anytime with `/plugin update viby-code`.

## Install on a machine without GitHub access (portable bundle)

For a PC that can't reach the private repo (no account, no token), install straight from a
copy of this folder — no GitHub, no login, no network:

1. Copy the whole `viby-toolkit` folder to the target machine (USB, `scp`, a cloud drive,
   or Syncthing). A clean copy without git history:
   ```bash
   git archive --format=tar.gz -o viby-toolkit.tar.gz HEAD    # on a machine that has the repo
   # move the .tar.gz over, then on the target:  tar xzf viby-toolkit.tar.gz
   ```
2. On the target machine, from inside the folder, run:
   ```bash
   bash install.sh
   ```
   It registers this folder as a local marketplace and installs `viby-code` at user scope.
   Restart Claude Code and type `/viby-code:`.

**Keep the folder** — the plugin loads from it. **To update later:** copy a newer copy of
the folder over the old one and re-run `bash install.sh` (it's idempotent). This trades
auto-update for total independence from GitHub.

---

## Editing the toolkit

Everything lives here as plain files:

```
.claude-plugin/marketplace.json      # marketplace manifest
package.json + tsconfig.json         # TS config; zero runtime dependencies
plugins/viby-code/
  .claude-plugin/plugin.json         # plugin manifest
  skills/<name>/SKILL.md             # the workflows
  skills/test/scripts/scan-test-quality.ts   # executable test auditor
  agents/<name>.md                   # the subagents (model routing in frontmatter)
  commands/ship.md                   # the autonomous entry command
  hooks/hooks.json + session-start.sh   # SessionStart is the only default hook
  hooks/run.sh                       # picks a TS runtime; no-ops if none exists
  hooks/statusline.ts + post-tool-use-format.ts   # opt-in, wired in settings.json
tests/*.test.ts                      # contract tests (node:test)
tests/run-all.ts                     # the pre-push gate
```

### Why TypeScript, and how it runs without a build step

Everything executable is TypeScript with **zero runtime dependencies** — only `node:`
builtins and the built-in `node:test` runner. There is no compile step and no
`node_modules`: Node ≥22.6 strips the types and runs the file directly.

Hooks are shell commands, so each one goes through `hooks/run.sh`, which picks the first
runtime it finds — node ≥22.6 (`--experimental-strip-types`), then bun, then `tsx` — and
**exits 0 silently when there is none**. A machine with no TypeScript runtime therefore
degrades to "no hook" rather than a broken session, which is the same fail-open rule the
guard itself follows. Because type stripping erases types rather than compiling them,
`tsconfig.json` sets `erasableSyntaxOnly` so the compiler rejects anything the runtime
cannot execute (no enums, no namespaces, no parameter properties).

After editing, bump `version` in `plugin.json` **and** in `marketplace.json` (both carry
it), commit, push. Machines pick up the change on next session (or `/plugin update
viby-code`).

### Verify before pushing

The toolkit holds itself to its own evidence gate — the hooks are executable code, so they
have tests. Run all three checks:

```bash
npm run check                     # the gate: every check, one verdict
```

Which runs:

```bash
claude plugin validate .          # manifests + skill/agent frontmatter
npm test                          # every tests/*.test.ts via the built-in node:test runner
#   statusline.test.ts  — payload shapes incl. the documented null cases
#   scanner.test.ts     — test-quality checks + file classification
npm run typecheck                 # tsc --noEmit (fetched on demand; skipped when offline)
# + scanner self-audit, SessionStart JSON validity, and a runner-shim probe
```

These are specifications, not smoke tests. Both halves of each contract are pinned: every
catastrophic command that must be denied **and** every piece of routine work that must be
allowed; every smell that must be flagged **and** every healthy test that must not be. If
you add a rule, add cases for both — a guard that blocks real work gets switched off, and a
scanner that flags good tests gets ignored. Either way it then protects nothing.

The must-allow half earns its keep: the test scanner flagged 23 false positives on its own
fixtures until it learned to blank string literals before matching, then flagged them again
when it met multi-line template literals. Both times the root cause was the same — matching
raw text instead of parsed code.

**Secrets:** this repo syncs across work and personal machines — never commit tokens,
credentials, client names, or internal hostnames. `.gitignore` blocks the common ones;
keep skills/agents project-agnostic and put anything project-specific in that project's
own `.claude/`.
