# viby-toolkit

A Claude Code plugin with my engineering workflows: planning, building, reviewing, testing,
debugging, and the checks that keep each one honest. Install it once at user scope and it
applies to every project on the machine.

It is built around how I work, so read it as a working reference rather than a product. It sends
nothing anywhere unless you switch on the skill hint below, and no hook blocks or intercepts a
command.

## Install

You need Claude Code. Nothing else: the repo is public, and the plugin downloads its own binary.

```sh
claude plugin marketplace add CrySteRz/viby-toolkit
claude plugin install viby-toolkit@viby-toolkit
```

Then turn on auto-update. Claude Code leaves it off for every marketplace except Anthropic's:
open `/plugin`, pick the `viby-toolkit` marketplace and choose **Enable auto-update**. Or set it
in `~/.claude/settings.json`, which also works as a complete install on a new machine:

```json
{
  "extraKnownMarketplaces": {
    "viby-toolkit": {
      "source": { "source": "github", "repo": "CrySteRz/viby-toolkit" },
      "autoUpdate": true
    }
  },
  "enabledPlugins": { "viby-toolkit@viby-toolkit": true }
}
```

Restart Claude Code and type `/viby-toolkit:`; the skills should autocomplete.

**Updates.** With auto-update on, Claude Code checks the marketplace during an interactive
session, after your first message and a random delay of up to ten minutes, and the new version
loads in your next session. `claude -p` never triggers it. To update right away:
`/plugin update viby-toolkit`.

**No network** (an air-gapped machine): on a connected machine, download
`viby-toolkit-<version>-offline.tar.gz` from the
[latest release](https://github.com/CrySteRz/viby-toolkit/releases/latest). It is the source plus
the binaries for every platform, about 11 MB. Copy it over, then:

```sh
tar -xzf viby-toolkit-*-offline.tar.gz
bash viby-toolkit/install.sh
```

Keep the extracted folder, since the plugin is installed from it. To update, extract a newer
bundle over it and run `install.sh` again. If the machine already has the GitHub install, the
script refuses to replace it; `VIBY_INSTALL_FORCE_LOCAL=1 bash viby-toolkit/install.sh`
overrides that.

## What you get

**Skills.** Claude picks them up from context, or you call them as `/viby-toolkit:<name>`.
Several ship a checker that Claude runs as part of the skill (in brackets).

| Skill | For |
|---|---|
| `brainstorm` | Deciding *what* to build before any code. Holds implementation until you approve a design. |
| `plan` | Turning an agreed idea into an ordered task list, each task owning its files and its check. [`check-plan`] |
| `orchestrate` | Running a change end to end: research, plan, build, verify, self-review. [`check-diff-hygiene`] |
| `review` | Parallel reviewers per dimension, then a fresh validator per finding to drop false positives. |
| `verify` | Running the project's real checks before anything is called done, and catching passes that tested nothing. [`detect-stack`] |
| `test` | Test design and auditing: no-assertion tests, tautologies, over-mocking, skipped tests, sleeps. [`scan-test-quality`] |
| `debug` | Root cause by hypothesis and evidence: reproduce, localize, confirm, fix. |
| `incident` | Production is down: mitigate first (rollback, flag, shed load), then hand over to `debug`. |
| `explore` | Getting oriented in an unfamiliar codebase, and writing down the map. |
| `refactor` | Restructuring without changing behaviour, proven by the same tests before and after. |
| `migrate` | Wide mechanical changes: find every site, change in batches, sweep until none are left. |
| `schema` | Database changes that deploy while old code runs and can be rolled back. [`check-migration`] |
| `api` | Designing or changing an interface others depend on, and telling whether a change breaks them. [`check-api-surface`] |
| `release` | Choosing the version from what changed, and the pre-release checks. [`check-release`] |
| `deps` | Upgrading dependencies one at a time, reading the changelog of every major crossed. |
| `secure` | Security pass: secrets first, then supply chain and CI, then code. Also audits skills and plugins. [`check-skill-safety`] |
| `perf` | Speed and memory: baseline, profile, one change, measure again. |
| `observe` | Logging, metrics and tracing someone can use at 3am. [`check-logging`] |
| `ui` | Checking a UI by seeing or driving it, not by reading the markup. |
| `kpi` | Defining a metric (formula, owner, guardrail) before anyone builds it. |
| `analytics` | Implementing a metric and reconciling it against an independent source. [`check-analytics-sql`] |
| `docs` | READMEs, ADRs, PR descriptions and release notes for one reader. [`check-docs`] |
| `study` | Researching a question outside the codebase, protocol first, every claim quoted. [`check-study`] |
| `evaluate` | Deciding whether to adopt a tool by measuring it on a case with a known answer. [`measure-read-cost`] |
| `adopt` | Taking over code you didn't write: capture its behaviour first, then conform it. [`check-test-drift`] |
| `learn` | Saving a lesson to Claude's memory so the next session knows it. |
| `brain` | Auditing that memory for entries that are stale or unsourced. [`check-memory`] |
| `handoff` | Writing down where a task stands so a fresh session can pick it up. |
| `worktrees` | Isolating parallel or risky work in git worktrees. |
| `extend` | Adding to this toolkit without making routing worse. [`check-skills`, `check-hooks`, `check-agents`] |
| `principles` | The rules the others follow: evidence before "done", when to fan out, which model does what. |

**Commands.** `/viby-toolkit:ship <task>` runs the whole pipeline until the result is verified.
`/viby-toolkit:browser` connects the browser tool, step by step.

**Browser tool** (MCP, `browser`). Drives your own Brave, Chrome, Edge or Firefox, with your
logins and tabs, through a small extension and a native host. Each Claude session works in its
own background tab. It can click, type, fill forms, upload, drag and drop, take screenshots and
run several steps or pages in one call. Passwords are masked in everything Claude reads. The
first call walks you through loading the extension (Developer mode, Load unpacked); browsers
don't let any program install an extension silently. Ported from JCode's bridge; details in
`plugins/viby-toolkit/browser/NOTICE.md` and `skills/ui/references/browser.md`.

**Code search** (MCP, `agentgrep`). Structure-aware grep, find, outline and trace over Rust,
TypeScript/JavaScript and Python, from [1jehuang/agentgrep](https://github.com/1jehuang/agentgrep).
It is available, but nothing tells Claude to prefer it: on a real repo it saved no tokens over
grep and Read (`docs/research/2026-10-19-agentgrep-measurement.md`).

**Agents** the skills dispatch: `scout`, `researcher`, `implementer`, `reviewer`, `skeptic`,
`debugger` and `browser` run on Sonnet; `worker` (the general Workflow stage agent) takes the
model its stage asks for. Cheap models find; the main session decides and writes.

**Two hooks.** At session start one adds a short working-style contract (evidence before claiming
done, fan out reads but keep writes single-threaded, the smallest change that fully works). The
other, the skill hint, is off unless you switch it on (below).

## How it runs

Skills, agents and commands are markdown. Everything executable is one Rust binary, `viby`: the
checkers, the two MCP servers, the statusline and the format hook.
`plugins/viby-toolkit/launcher/viby` starts it, and on the first call of each plugin version it
gets the binary:

1. It takes the build for this machine from the offline bundle if there is one, otherwise
   downloads it from the GitHub release, and checks its SHA-256
   against `plugins/viby-toolkit/launcher/checksums.txt`, which ships inside the plugin. A
   mismatch is refused.
2. If there is no build or no network, it builds from the plugin's own source with cargo, once
   (about 1–4 minutes).
3. If cargo is missing too, it stops with the command that installs Rust.

Prebuilt for macOS (Apple silicon, Intel), Linux (x86-64, arm64; static) and Windows (x86-64,
through Git Bash). Each release is tested on a fresh machine of each of these.

Binaries are cached per version, in the plugin's data folder or `~/.viby/bin`. An update brings
a new binary on its next call and keeps the previous one for sessions still running it. The
browser's native host follows: the first browser call after an update restarts it.

### When something goes wrong

| You see | Meaning | Fix |
|---|---|---|
| `viby: building viby 3.0.0 from source (once, 1-2 minutes)` | No download for this machine, or no network. | Wait; it happens once per version. |
| `viby: refused viby-<target>.tar.gz: its SHA-256 is …, the plugin expects …` | The download doesn't match the plugin's checksums. | Don't override it. Update the plugin; if it persists, report it. |
| `viby … is not installed and could not be built: Rust is missing. Install it with: curl … sh.rustup.rs …` | No download and no cargo. | Run the command it prints, then retry. |
| `viby … could not be downloaded, and building it with cargo failed` | The cargo build failed; its errors are printed above. | Fix what cargo reports (often no network for crates.io). |
| Skills report nothing found for `viby` | The plugin isn't where the locator looks. | Check `/plugin`. If you set `CLAUDE_CONFIG_DIR`, the plugin lives under that folder, which the locator also searches. |
| The marketplace never updates | The clone uses SSH and no key is loaded. | `git -C ~/.claude/plugins/marketplaces/viby-toolkit remote -v` should show `https://`. |

## Optional extras

**Statusline.** Shows the model, context use, cache reuse, rate-limit use and cost:

```
Opus 5.5 · ctx 42% · 5h 31% · 7d 12% · $1.23
```

`ctx` is green under 60%, yellow under 80% and red above. Fields Claude Code doesn't send are left
out. Add to `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command",
    "command": "sh \"$(ls -t \"${CLAUDE_CONFIG_DIR:-$HOME/.claude}\"/plugins/cache/viby-toolkit/viby-toolkit/*/launcher/viby | head -n 1)\" statusline" } }
```

A 2.x statusline line that runs `hooks/run.sh … statusline.ts` keeps working through 3.x.

**Format hook.** `viby format-hook` formats edited files, only with a formatter the project
already uses, and never blocks. Add it as a `PostToolUse` hook if you want it.

**Skill hint (opt-in).** With both `AI_GATEWAY_API_KEY` (a Vercel AI Gateway key) and
`VIBY_DECIDE=on` set, each prompt you type is sent to a decision model on Vercel AI Gateway. If
the model is at least 0.9 sure which skill fits, Claude gets one line of context, e.g.
`Skill hint: viby-toolkit:test (confidence 0.97, …)`. It never runs a skill itself. Slash
commands, prompts that name a skill and replies under four words are never sent. On the
labelled routing prompts the default models picked the intended skill 97–100 % of the time
(`docs/studies/2026-10-23-decision-model-mammoth.md`).

Without both variables nothing runs, and the prompt never leaves the machine. With them, the
prompt text goes to Vercel and the model's provider; the gateway's free tier offers no
zero-retention guarantee, so leave it off for work you can't share. Every failure (no network,
rate limit, timeout, an unusable answer) is silent and costs at most about 3 s.
The same switch adds one line to agentgrep `find` results: the file a decision model would open
first, chosen from `find`'s top 25, shown only at 0.9 confidence or above. On 40 labelled "where
is X" questions over two repos, `find`'s own first file was right 7 times; at that threshold the
model named a file 19 times and was right all 19. Below the threshold the result is exactly
agentgrep's. `VIBY_DECIDE_MODELS` sets the models tried in order (default
`baseten/mammoth-1,baseten/elephant-1,liquid/d1-ultra`). `viby decide` asks one question
directly: JSON `{state, questions}` on stdin, `{model, answers}` on stdout.

## Working on the toolkit

```
.claude-plugin/marketplace.json     marketplace manifest
plugins/viby-toolkit/
  .claude-plugin/plugin.json        plugin manifest and MCP servers
  skills/  agents/  commands/       the markdown
  hooks/                            session-start.sh, and run.sh for 2.x paths
  launcher/viby, checksums.txt      gets and runs the binary
  browser/extension/                the browser extension (JavaScript)
  rust/                             Cargo workspace: crates/viby (the binary), xtask (the gate)
tests/routing/                      checks that prompts route to the right skill (Python)
docs/                               plans, decisions, research, provenance
```

The workspace sits inside the plugin because an install copies only `plugins/viby-toolkit`, and
the cargo fallback needs the source.

**Before pushing**, run the gate. CI runs it too.

```sh
cd plugins/viby-toolkit/rust && cargo xtask check
```

It runs every test suite against a minimum pass count, clippy, rustfmt and
`claude plugin validate`. It also checks that versions agree, that every skill, agent and
`viby` command the docs mention exists, and that the SessionStart output is valid JSON. When you
add a checker rule, pin both halves: what it must flag and what it must leave alone.

**Releasing.** Bump `version` in `plugin.json`, `marketplace.json` and
`plugins/viby-toolkit/rust/Cargo.toml`, then push to a branch, not to `main`. The push is the release:

1. `release.yml` builds the five binaries, commits their checksums, tags the commit and
   publishes.
2. `launcher.yml` tests the result on a fresh machine of each OS.
3. A final version (no `-rc`) then fast-forwards `main` by itself.

`main` therefore never names a version whose binaries aren't out yet; a bump pushed straight to
`main` fails the release.

**Don't commit secrets.** This repo is public and syncs across work and personal machines. Keep
skills and agents project-agnostic; anything project-specific belongs in that project's
`.claude/`.

## Background

- `/viby-toolkit:principles` is the full operating contract.
- `docs/provenance.md` lists the research and spikes behind each rule.
- `CHANGELOG.md` records what changed in each release. 3.0.0 replaced the TypeScript with the
  Rust binary.
