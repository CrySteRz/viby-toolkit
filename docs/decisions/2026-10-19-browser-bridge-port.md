# Browser control: port JCode's bridge instead of Playwright

> Status: **decided by the owner** (2026-10-19) · build not started · sources read at
> `1jehuang/jcode@a3107fb` and `1jehuang/firefox-agent-bridge` (HEAD of 2026-10-06), both MIT.

## What JCode actually ships

JCode's `browser` tool (`crates/jcode-app-core/src/tool/browser.rs`) is a thin dispatcher. It
shells out to `browser <action> '<json>'`, the CLI of a separate project,
`1jehuang/firefox-agent-bridge`: a WebExtension in the user's real browser, a native-messaging
host, and a loopback WebSocket (`127.0.0.1:8766`) between them. JCode's own additions are the
tool schema and description, setup and update plumbing, and `handoff`, a loop in which a hosted
classifier model ("Jev", paid key) picks the next action from a menu.

## Decisions

| Question | Chosen | Not chosen |
|---|---|---|
| How to get it | **Port the bridge**: vendor the MIT extension, rewrite the Rust CLI and host as zero-dependency Node inside the plugin, expose one `browser` MCP tool shaped like JCode's | Wrapping their prebuilt binaries (unchecksummed downloads, upstream dependency); a CDP driver (cannot use the signed-in default profile) |
| Handoff | **A `browser` subagent** that runs observe → act → check on a cheaper model, with JCode's rules, and returns the action trace and final page | Direct tools only; Haiku through the API (separate key and bill) |
| Browsers, first release | **Chromium family (Brave, Chrome, Edge) and Firefox** | Safari (Xcode conversion and local signing) |

## Constraints carried into the build

- The plugin's zero-runtime-dependency rule holds: the WebSocket and native-messaging framing
  are written against `node:` modules.
- Both MIT notices are kept beside the vendored code.
- No credential handling is ported (no autologin, no password typing); a sensitive page hands
  back to the person, as JCode's handoff does.

## Follow-up decisions (2026-10-19, later the same day)

- **Feature parity with upstream**, all four groups chosen by the owner: `new_window` (a window
  per session), `sandbox` (private window), `reload`; `batch` and `try_until`; `fork`/`kill_fork`/
  `list_forks` and `parallel`; `drop_file`, slow-page and closed-tab recovery, and the extension
  reloading itself after an update. Still out: auto-login, vault and auth prompts (they fill
  passwords); `--record` screen recording (not asked for).
- **agentgrep**: wrap the real tool (`1jehuang/agentgrep`, MIT per its Cargo.toml, v0.1.7) as an
  `agentgrep` MCP server in viby-toolkit, plus our own per-session "already seen" trimming, the
  part jcode adds in its harness. Not a TypeScript port, not guidance-only.
- The owner's "JCode Browser Bridge" means 1jehuang/jcode's Firefox Agent Bridge. cnjack/jcode's
  Chrome Web Store "jcode Browser Bridge" is an unrelated project with the same name.
- **How "already seen" works (built 2026-10-19).** agentgrep 0.1.7 already shortens repeated
  structure and regions when given a harness context (`--context-json`), which is what jcode's
  harness builds from its session. So the server does the same with its own results: it
  remembers the file structures and full regions it returned (with each file's mtime) and passes
  them to `trace`. It does not rewrite agentgrep's output into `[seen: …]` lines as the plan
  first sketched, because agentgrep's own note already says the same. A file changed since, an
  `outline` (always an explicit ask), and `force: true` are never shortened. `grep` and `find`
  take no context in 0.1.7.
- **agentgrep is not routed to yet.** In a measured comparison it did not save tokens against
  grep/Read: `docs/research/2026-10-19-agentgrep-measurement.md`.
- **`open` keeps a background tab by default.** The plan raised making `new_window` the default
  for a session (JCode binds a window per session). Left as is: a new window per Claude session
  is more intrusive than a background tab, and `new_window` is one call away. The owner can
  reverse this.
