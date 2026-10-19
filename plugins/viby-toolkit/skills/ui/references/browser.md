# The `browser` tool

viby-toolkit's port of JCode's browser control (its bridge, `1jehuang/firefox-agent-bridge`,
MIT). One MCP tool, `browser`, with an `action` field. It drives the person's own browser
window, so every page is signed in as them.

## First use

1. Any call with nothing connected does the first move itself: it opens the person's default
   browser (when supported; else Brave, Chrome, Edge, Chromium) on its extensions page, shows
   `~/.viby/browser/extension/chromium` selected in Finder (macOS) or puts it on the clipboard
   (Windows, Linux), and returns steps worded for that browser. Show them as a numbered list and
   stop; the next call connects: Developer mode on → Load unpacked → drag the folder from Finder
   onto the picker (Windows: paste into the box at the bottom; Linux: Ctrl+L, paste) → Select.
   - Firefox (only when no Chromium browser is installed): Load Temporary Add-on with
     `~/.viby/browser/extension/firefox/manifest.json`. Unsigned, so it repeats after each restart.
   - No supported browser: it opens Brave's download page and lists Chrome, Edge and Firefox.
2. `action='status'` re-reads the state (not loaded, switched off, loaded from another folder) and
   gives the matching steps. `action='setup'` does the same and then waits up to two minutes for
   the connection; use it only after the person has the steps in front of them.
3. After a plugin update the bridge updates itself on the next call: when the connected
   extension is older, the tool installs the new files and tells the extension to reload
   itself, then goes on with the call.

## Working loop

| Step | Action | Notes |
|---|---|---|
| Get a tab | `open` with `url` | The first `open` creates this session's own background tab; later ones navigate it. `new_tab: true` for another, `new_window` for a window of its own, `sandbox: true` for a private window (no cookies or logins). To use a tab the person already has open: `list_tabs`, then `select_tab`. |
| Read | `snapshot` | Annotated text: `[button: "Save" \| selector: #save]`, `[input:email: "Email" \| value: "" \| selector: #email]`, form state included. Cheaper than a screenshot and carries selectors. |
| Act | `click`, `type`, `fill_form`, `select`, `press`, `upload`, `drop_file`, `scroll`, `reload` | Use selectors from the latest snapshot. `fill_form` handles inputs, selects, checkboxes, radios and rich editors (ProseMirror, Lexical, Draft.js, TinyMCE). `drop_file` drags a local file onto a drop zone or editor (`selector`, `path`). |
| Confirm | `wait` with `selector`, `text` or `contains` | Never sleep. Then snapshot again. |
| Look | `screenshot` | Returns the image. Chromium can only capture the visible tab, so it brings that tab to the front. |

## Fewer round trips

| Action | Input | Use it for |
|---|---|---|
| `batch` | `steps: [{action, …}]`, `stop_on_error` | Several known steps in this tab in one call: fill, click, read back. Each step takes the same fields as a call. Stops at the first failure by default and says which steps did not run. |
| `try_until` | `alternatives: [{action: click\|type\|wait, …}]` | One goal, several ways: two selectors for the same button. Returns which one worked. |
| `fork` | `forks: [{name, steps?}]` | Copies of this tab to try paths side by side. Act in one with `fork: '<name>'` on any action; `list_forks`; `kill_fork` with `fork` closes it. Forks belong to this session. |
| `parallel` | `branches: [{url, steps}]` | Read several pages at once in background tabs, which close afterwards. |

Steps may be `open` (navigates that tab), `reload`, `snapshot`, `get_content`, `interactables`,
`click`, `type`, `fill_form`, `select`, `wait`, `eval`, `scroll`, `press`. Screenshots, file
actions and tab management stay top-level calls. The whole call shares the 30 s limit.

Other actions: `get_content` (`format`: text, textFast, html, title, annotated), `interactables`
(numbered list of controls), `eval` (`script` with `return`; `page_world: true` to reach page
globals), `list_frames` + `frame_id` / `all_frames` for iframes, `get_active_tab`.

## Pitfalls

- **Selectors can go stale.** After navigation or a re-render, snapshot again before acting.
- **Text matching prefers visible elements** but can still pick the wrong one; prefer a selector.
- **Pages that never finish loading** (dashboards): `open` and `reload` give up waiting after
  `timeout_ms` (default 15 s) and return the page as it is, with a note. Snapshot again before
  relying on a part that may still be loading.
- **Internal pages** (`chrome://`, `about:`, extension stores) cannot be scripted.
- **`eval` on a strict-CSP page** fails on Chromium unless "Allow user scripts" is enabled for
  Viby Browser on the extensions page.
- **Password fields** show as dots in snapshots by design. Never type passwords, codes or
  payment details: ask the person to do that step in the tab.
- **Several browsers connected?** Pass `browser` once; the session remembers it.
- **Private windows** need the extension allowed in them (Brave: Allow in Private; Chrome,
  Edge: Allow in Incognito, under Details on the extensions page). Without it `sandbox` fails
  with those steps; ask the person.

## Recovery

| Symptom | Do |
|---|---|
| `No browser is connected` | Open the browser; check the extension is loaded and enabled; `status`. |
| `No tab with id` | The tab was closed and the session forgot it. `open` with a url opens a new one by itself; for anything else, `open` or `list_tabs`. |
| `Receiving end does not exist` | The tab predates the extension or is still loading: `open` it again. |
| `Request timed out` after a click or submit | Snapshot before repeating — it may have worked. |
| Extension loaded but never connects | Run `setup` again (it reinstalls the host), then reload the extension. |
