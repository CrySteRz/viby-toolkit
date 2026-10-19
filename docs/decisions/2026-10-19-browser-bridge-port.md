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
