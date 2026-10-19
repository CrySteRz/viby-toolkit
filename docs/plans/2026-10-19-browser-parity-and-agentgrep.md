# Plan: browser parity with upstream, then agentgrep

Decisions: `docs/decisions/2026-10-19-browser-bridge-port.md` (follow-up section).
State at start: `main` at 2.26.0 (browser tool merged, 5 commits ahead of origin, not pushed).
Owner's instruction: build, **test again live**, and only if it passes, merge to main (and push).

Sources (re-clone if /tmp was wiped):
- `git clone --depth 1 https://github.com/1jehuang/firefox-agent-bridge` (upstream extension, b8b0c14)
- `git clone --depth 1 https://github.com/1jehuang/jcode` (tool + handoff, a3107fb)
- `git clone --depth 1 --branch v0.1.7 https://github.com/1jehuang/agentgrep`

## Part A — browser parity (plugins/viby-toolkit/browser)

The extension already implements every one of these; the work is exposing them safely.

| New action | Extension call | Notes |
|---|---|---|
| `new_window` | `newWindow {url, focus:false}` | Session tab lives in its own window. Consider making it the default for `open` with no tab (JCode binds a window per session); decide with the owner. |
| `sandbox: true` on `open`/`new_tab`/`new_window` | `newSession {sandbox:true}` | Private window. Chromium needs "Allow in Incognito" on the extension: detect the failure and guide. |
| `reload` | `navigate` to the current URL, or `browser.tabs.reload` (add to the extension if missing) | Not the extension's `reload` action, which reloads the extension. |
| `batch` | `batch {commands:[{action, params}]}` | Map each sub-command through `bridgeRequest`; refuse disallowed actions inside. |
| `try_until` | `tryUntil {alternatives, timeout}` | Same mapping per alternative. |
| `fork` / `kill_fork` / `list_forks` | `fork {paths}`, `killFork`, `listForks` | Forks are tabs: keep the server's session map of fork name → tabId (the Rust host did this, `host.rs:896-943`). |
| `parallel` | `parallel {branches:[{url, commands}]}` | Background tabs; return per-branch results. |
| `drop_file` | `dropFile` with `{file:{name,type,data}}` | Host reads the path like `uploadFile`; chunk above 750 kB (extension reassembles `dropFile` by `chunkedTransfer`). |

Self-healing:
- Slow page: on "Timed out waiting for page load", read `getContent annotated` from the same tab
  and return it with a load warning (jcode `recover_slow_navigation`, browser.rs:745).
- Closed tab: on "No tab with id", drop the session tab and retry once in a new background tab
  when the action is `open`; otherwise return the guidance (never repeat a click).
- Extension update: after `ensureInstalled()` reinstalls a new version and a browser is live, send
  the extension's own `reload` action so it picks up the new files (verified to work live).

Add each to: `ALLOWED_ACTIONS` (host.ts), `INPUT_SCHEMA` + `bridgeRequest` + `formatResult`
(actions.ts), the agent and `skills/ui/references/browser.md`, and tests.

## Part B — agentgrep

1. Scout first: how jcode uses the agentgrep crate and where its "seen" trimming lives
   (`grep -rn agentgrep crates/jcode-app-core` and the tool that calls it). Record the trimming
   rule before designing ours.
2. `plugins/viby-toolkit/agentgrep/server.ts`: a stdio MCP server like `browser/server.ts`, tool
   `agentgrep` with modes `grep`, `find`, `outline`, `trace`, mapping to the CLI's subcommands.
3. Per-session seen-set: file path → line ranges already returned. Later results replace ranges
   already shown with a one-line `[seen: L10–40 of foo.ts]`. Never trim a result the agent asks
   for explicitly (`outline` of one file, `force: true`).
4. Binary: locate `agentgrep` on PATH or `~/.cargo/bin`; if missing, guide install
   (`cargo install --git https://github.com/1jehuang/agentgrep --tag v0.1.7`), as the browser
   tool guides setup. Pin the tag. Windows: same via cargo.
5. Measure before claiming savings: run the same 3 code-exploration questions with Grep/Read vs
   agentgrep on a real repo and record tokens (`/viby-toolkit:evaluate`), not just a demo.

## Verification (both parts)
- `npm run check` green; new suites added to `tests/run-all.ts` with `minPassing`.
- Browser: rerun the live driver (scratchpad `live/drive.mjs`; rebuild it if wiped: one server
  process, JSON steps, PASS/FAIL per step) against the bridge's `test-challenges.html` on
  127.0.0.1:3456, extended with one step per new action. All must pass before merging.
- Version 2.27.0, changelog, README; commits split (browser parity, then agentgrep).
