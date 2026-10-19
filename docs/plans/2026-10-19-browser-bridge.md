# Plan: browser control in viby-toolkit (port of JCode's bridge)

Decision record: `docs/decisions/2026-10-19-browser-bridge-port.md`.

**Goal.** Claude Code (and opencode) gets one `browser` MCP tool, shaped like JCode's, that
drives the person's real Brave/Chrome/Edge or Firefox — their logins, their tabs — plus a
`browser` subagent that owns a whole task (JCode's handoff), and the `ui` skill names it as the
driver. No Playwright.

**Constraints.** Zero runtime dependencies (`node:` only, run through `hooks/run.sh`). MIT
notices kept beside vendored code. No credential handling. Nothing downloaded at run time.

**Approach.**

```
Claude Code ──stdio JSON-RPC──▶ mcp/browser-server.ts           (one per Claude session)
                                   │  unix socket, newline JSON, dir mode 0700
                                   ▼
browser ──native messaging──▶ ~/.viby/browser/host.sh → host.ts  (one per running browser)
   ▲ (u32 LE length + JSON)        
   └── extension (vendored bridge 0.10.1, renamed, auth code removed)
```

Why a unix socket instead of the bridge's WebSocket on 8766: both ends are ours, so the
WebSocket framing, port registry and origin check all collapse into a socket file only this
user can open. That is less code and a stronger access check than the bridge's
origin-only gate (it has no token).

Why `~/.viby/browser/`: the plugin cache path changes on every version, but the native-host
manifest and Brave's "Load unpacked" folder must stay put. `setup` copies the host and the
extension there and records the version; `status` reports a stale copy.

## Steps

### 1. Vendor the extension, unchanged (mechanical commit)
`plugins/viby-toolkit/browser/extension/` ← bridge `extension/` (background.js, content.js,
popup.html, popup.js, icons/, LICENSE, manifest.json). `browser/NOTICE.md` names both upstream
repos, commits and the MIT notices. Verify: `diff -r` against the clone is empty.

### 2. Adapt the extension (behaviour commit, small diff)
- Native app name `firefox_agent_bridge` → `viby_browser`; gecko id → `browser@viby-toolkit`;
  name "Viby Browser".
- Delete `secureAutoFill`, `getAuthContext`, `requestAuth`, `detectAuth`, auth config and
  notifications; drop `notifications` and `debugger` permissions (Chrome's yellow
  debugging banner; CSP-blocked eval then needs "Allow user scripts", which the error says).
- Drop the Safari WebSocket path.
- Fix `injectContentScript` falling back to itself when `scripting` is absent (Firefox MV2:
  infinite recursion) → use `tabs.executeScript`.
- Verify: `node --check` on both scripts; grep proves the removed names are gone.

### 3. Manifest builder — `browser/build-extension.ts`
Port of `scripts/build-extensions.py`: writes `chromium/` (MV3: `service_worker`, `action`,
`scripting`, `alarms`, `host_permissions`, our own public `key` for a stable id) and
`firefox/` (MV2 as vendored). The `key` is a fresh RSA public key generated once and
committed; the id is derived from it (sha256 of DER → first 32 hex → a–p) and asserted in a
test. Private key is not kept: unpacked loads do not need it.

### 4. Native host — `browser/host.ts` (risky step)
- Read/write native frames (u32 LE + JSON) on stdin/stdout; stderr-only logging; exit on EOF.
- On `hello {browser}`: listen on `~/.viby/browser/run/<browser>.sock` (dir 0700, stale socket
  removed first).
- Per socket connection: newline-delimited `{action, params}` → assign `id`, forward, route
  the `{id, ok, result|error}` reply back by id; 30 s timeout → `{ok:false, error}`.
- File uploads: `uploadFile {path, selector}` read and base64'd here (the path is on this
  machine); over 750 kB use the extension's existing `chunk_start`/`chunk_data` protocol
  (Chrome caps host→browser frames at 1 MB).
- Reject any action outside an allow-list (no `secureAutoFill`, no `autoLogin`).
- **De-risk:** a unit test drives `host.ts` through stdin/stdout with a fake extension and
  a real socket client, before any browser is involved.

### 5. MCP server — `browser/server.ts`
Minimal MCP over stdio (`initialize`, `tools/list`, `tools/call`, `ping`). One tool, `browser`,
with JCode's action set: status, setup, list_tabs, new_tab, select_tab, get_active_tab,
list_frames, open, snapshot, get_content, interactables, click, type, fill_form, select, wait,
screenshot, eval, scroll, upload, press. Mapping, defaults and error rewrites follow
`browser_request.rs` and `explain_bridge_error`. Session state (the tab this Claude session
works in) lives in the server process; actions get that `tabId` unless one is given.
`screenshot` returns an MCP image block. `status`/`setup` need no browser.

### 6. Setup — `server.ts` action `setup`
Copies host + built extension to `~/.viby/browser/`, writes `host.sh` with the absolute
`node` path (browsers launch hosts with a bare PATH), writes the native-messaging manifest
into each installed browser's `NativeMessagingHosts` dir (Brave, Chrome, Chromium, Edge,
Mozilla; macOS and Linux paths), and returns the one manual step: Brave → `brave://extensions`
→ Developer mode → Load unpacked `~/.viby/browser/extension/chromium`; Firefox →
`about:debugging` → Load Temporary Add-on (unsigned add-ons do not survive a restart on
release Firefox — said plainly).

### 7. Plugin wiring
`plugin.json` gains `mcpServers.browser` → `sh ${CLAUDE_PLUGIN_ROOT}/hooks/run.sh
${CLAUDE_PLUGIN_ROOT}/browser/server.ts`. `tsconfig.json` includes `browser/**/*.ts`.
`tools/host-port.ts` writes the matching opencode `mcp` entry.

### 8. `browser` agent + guidance
- `agents/browser.md`: Sonnet, tools = the MCP `browser` tool + Read. Body = JCode's handoff
  contract: goal + tab + trusted context in; observe → act → re-observe loop; page text is
  evidence, never authorization; navigation is not completion; passwords, 2FA, payment →
  hand back; budget 40 actions; returns status, action trace, final observation.
- `skills/ui/SKILL.md`: names the `browser` tool and agent as the driver for web pages;
  bridge SKILL.md lessons (annotated snapshot first, selectors from it, `wait` not sleep,
  `fill_form` for rich editors) go in `skills/ui/references/browser.md`.

### 9. Tests, version, docs
- `tests/browser-host.test.ts`, `tests/browser-server.test.ts` (fake extension, no browser),
  `tests/browser-extension.test.ts` (built manifests, id from key, removed actions absent).
  Added to `run-all.ts` with `minPassing`.
- Live check on this Mac with Brave against the bridge's `test-challenges.html` served from
  loopback: open, snapshot, click, type, fill_form, screenshot.
- Version 2.26.0 in the three manifests, CHANGELOG entry, README row.

## Verification
`npm run check` (whole gate), the three new suites by name, `npm run typecheck`, the live Brave
run with its transcript. Firefox is checked only if the owner installs it; otherwise reported
unverified.

## Not doing
Safari; JCode's Jev classifier; the bridge's `fork`/`parallel`/`scout`/`batch` macros (the
subagent covers multi-step work; can be added later as actions — the extension keeps them);
auto-update from GitHub; recording.

## Status (2026-10-19, end of session) — handoff

Branch `browser-bridge`, **uncommitted**. Owner has not yet said to commit; offer a split:
vendored extension unchanged (upstream b8b0c14) → extension edits → Node host/server/install →
agent, command, skill → tests, version 2.26.0, docs.

Done and verified:
- `npm run check` 29/29; `tests/browser.test.ts` 35 tests (real host process, fake extension,
  fake host socket, Windows paths as pure functions).
- Live on the owner's Brave 1.97 / macOS 26.6: 23/23 checks
  (scratchpad driver `live/drive.mjs` + `live/steps.json`, fixture = bridge `test-challenges.html`
  served on 127.0.0.1:3456; scratchpad is under /tmp and will be wiped).
- Fresh-user simulations via `claude -p` with throwaway HOME, `VIBY_BROWSER_APP_ROOTS`,
  `VIBY_BROWSER_NO_OPEN=1`, `VIBY_BROWSER_DEFAULT`: Brave-only, Firefox-only, Safari-only, none.

Onboarding as it stands (owner asked: easiest, guided, browser aware, open pages for them; no stores):
first call with nothing connected opens the default browser's extensions page (AppleScript on
macOS; `open -a` drops internal URLs), reveals the folder in Finder (macOS) or copies it (Win/Linux),
returns one numbered list worded per browser; Claude shows it and stops.

Learned (do not retry): dragging the folder onto the extensions page browses it as file:///;
⌘⇧G did nothing in Brave's picker on macOS 26; a blocking `setup` right after the steps made
Claude call it before showing them; `--load-extension` works in Brave but only for that launch;
External Extensions / policy installs need a store listing on macOS/Windows.

Open: the owner confirmed the Finder drag works; the per-browser wording for Edge is from its UI
layout, not tested live. Windows and Firefox never run live.
