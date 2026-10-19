/**
 * MCP server (stdio) exposing one `browser` tool. Claude Code starts one per session.
 *
 * It reaches the browser through the native host's unix socket (see host.ts). The tab this
 * session works in lives here, so two Claude sessions never share one by accident, and an
 * action with no tab is refused rather than run on whatever tab the person is looking at.
 */
import { realpathSync } from "node:fs";
import { homedir } from "node:os";
import { spawn, spawnSync } from "node:child_process";
import { createConnection } from "node:net";
import { join } from "node:path";
import {
  INPUT_SCHEMA,
  InputError,
  LOCAL_ACTIONS,
  TOOL_DESCRIPTION,
  bridgeRequest,
  explainError,
  formatResult,
  isAction,
  needsTab,
  type Action,
  type Json,
} from "./actions.ts";
import { BROWSER_APPS, chromiumExtensionState, defaultBrowser, detectBrowsers, install, installedApp, installedVersion, pluginVersion, safariInstalled } from "./install.ts";
import { BROWSER_NAMES, LineDecoder, REQUEST_TIMEOUT_MS, browserHome, socketPath } from "./protocol.ts";

type Reply = { ok: boolean; result?: any; error?: string };

const session: { tabId?: number; browser?: string } = {};
let requestCounter = 0;

/** One request over a fresh socket connection; the host routes replies by id. */
export function callHost(socket: string, action: string, params: Json, timeoutMs = REQUEST_TIMEOUT_MS + 2000): Promise<Reply> {
  return new Promise((resolve) => {
    const id = ++requestCounter;
    const conn = createConnection(socket);
    const decoder = new LineDecoder();
    const done = (reply: Reply) => {
      clearTimeout(timer);
      conn.destroy();
      resolve(reply);
    };
    const timer = setTimeout(() => done({ ok: false, error: "Request timed out" }), timeoutMs);
    conn.on("connect", () => conn.write(`${JSON.stringify({ id, action, params })}\n`));
    conn.on("data", (chunk) => {
      for (const message of decoder.push(chunk) as Json[]) if (message.id === id) done(message as Reply);
    });
    conn.on("error", (err) => done({ ok: false, error: `browser not connected (${(err as NodeJS.ErrnoException).code ?? err.message})` }));
  });
}

/** Browsers with a live host. A stale socket, or no pipe at all, fails to connect and is skipped. */
async function liveBrowsers(): Promise<string[]> {
  const checks = await Promise.all(BROWSER_NAMES.map(async (name): Promise<string | null> => ((await callHost(socketPath(name), "hostInfo", {}, 1500)).ok ? name : null)));
  return checks.filter((n): n is string => n !== null);
}

async function chooseBrowser(requested: unknown): Promise<string> {
  const live = await liveBrowsers();
  if (typeof requested === "string") {
    if (!live.includes(requested)) throw new InputError(`${requested} is not connected. Connected: ${live.join(", ") || "none"}. Check action='status'.`);
    return requested;
  }
  if (session.browser && live.includes(session.browser)) return session.browser;
  if (live.length === 1) return live[0]!;
  if (live.length === 0) {
    ensureInstalled();
    throw new InputError(helpNow(live));
  }
  throw new InputError(`Several browsers are connected (${live.join(", ")}); pass browser=… once.`);
}

/** Brings ~/.viby/browser up to this plugin's version. Silent: nothing the person must do. */
function ensureInstalled(): void {
  if (installedVersion() !== pluginVersion()) install();
}

/** AppleScript that opens `page` in a new front tab of a Chromium browser, launching it if needed. */
export function chromiumTabScript(appPath: string, page: string): string {
  const app = JSON.stringify(appPath);
  return [
    `tell application ${app}`,
    "  activate",
    "  if (count of windows) = 0 then make new window",
    `  tell front window to make new tab with properties {URL:${JSON.stringify(page)}}`,
    "end tell",
  ].join("\n");
}

/** Opens the extensions page itself, not just the browser. */
function openPage(browser: string, app: string, page: string): boolean {
  if (process.platform === "darwin") {
    // `open -a Brave brave://extensions` starts Brave on a new tab and drops the address:
    // browsers refuse their internal pages from outside. Their AppleScript is allowed to.
    // Without the Automation permission (or for Firefox) the browser still opens and the
    // guide names the page.
    if (browser !== "firefox" && spawnSync("osascript", ["-e", chromiumTabScript(app, page)]).status === 0) return true;
    return spawnSync("open", ["-a", app, page]).status === 0;
  }
  // Windows .exe and Linux binaries both take the page as an argument.
  try {
    spawn(app, [page], { detached: true, stdio: "ignore" }).unref();
    return true;
  } catch {
    return false;
  }
}

/** Puts the path the person must pick on their clipboard. False when no clipboard tool exists. */
function copyToClipboard(text: string): boolean {
  const tools: [string, string[]][] =
    process.platform === "darwin" ? [["pbcopy", []]] : process.platform === "win32" ? [["clip", []]] : [["wl-copy", []], ["xclip", ["-selection", "clipboard"]]];
  return tools.some(([cmd, args]) => spawnSync(cmd, args, { input: text }).status === 0);
}

/**
 * How to reach a path in the system file picker by pasting it. Dragging the folder onto the
 * extensions page was the first design; a drop that misses the list (or lands before
 * Developer mode is on) makes the browser just browse the folder as file:///, so the picker it is.
 */
const MAC = process.platform === "darwin";

/**
 * How to get the picked item into the system file picker, then confirm it. On macOS the item is
 * shown selected in a Finder window and dragged onto the picker: ⌘⇧G (Go to Folder) did nothing
 * in Brave's picker on macOS 26, measured. Windows and Linux pickers take a pasted path.
 */
function pickerSteps(item: string, confirm: string): string[] {
  if (MAC) return [`Drag the highlighted ${item} from the Finder window onto the file picker.`, `Click ${confirm}.`];
  if (process.platform === "win32") return ["Paste with Ctrl+V into the box at the bottom, press Enter.", `Click ${confirm === "Select" ? "Select Folder" : confirm}.`];
  return ["Press Ctrl+L, paste with Ctrl+V, press Enter.", `Click ${confirm}.`];
}

/** One browser to set up: the steps the person follows, already numbered in order. */
type Plan = { browser: string; name: string; page: string; app: string; pick: string; intro: string; steps: string[]; note: string };

/** A path as the person would type it: ~ for their home folder (macOS and Linux). */
function shown(path: string): string {
  const home = homedir();
  return process.platform !== "win32" && path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
}

/**
 * The one browser to set up, with steps for the state it is in. One browser, not every one
 * found: a person needs a single short list. Brave, Chrome, Edge, Chromium in that order;
 * Firefox only when none of those is installed, since it forgets the add-on on every restart.
 */
function onboardingPlan(live: string[]): Plan | null {
  const folder = join(browserHome(), "extension", "chromium");
  const found = detectBrowsers();
  const preferred = defaultBrowser();
  // The person's default browser first, when it is a Chromium one; then Brave, Chrome, Edge, Chromium.
  const chromium = found
    .filter((t) => t.family === "chromium" && installedApp(t.browser))
    .sort((a, b) => Number(b.browser === preferred) - Number(a.browser === preferred));
  const target = chromium.find((t) => !live.includes(t.browser));
  if (target) {
    const { name, page, devMode, loadUnpacked } = BROWSER_APPS[target.browser]!;
    const others = chromium.filter((t) => t !== target).map((t) => BROWSER_APPS[t.browser]!.name);
    const load = [`Turn on ${devMode}.`, `Click ${loadUnpacked}.`, ...pickerSteps('"chromium" folder', "Select")];
    const state = chromiumExtensionState(target.appDir);
    const steps =
      state.state === "disabled"
        ? ['Switch on "Viby Browser".']
        : state.state === "other-folder"
          ? ['Click Remove on the old "Viby Browser".', ...load.slice(1)]
          : state.state === "installed"
            ? ['Click the reload arrow on "Viby Browser".']
            : load;
    return {
      browser: target.browser,
      name,
      page,
      app: installedApp(target.browser)!,
      pick: folder,
      intro: `Let me use ${name}: one time, about 20 seconds. ${name} is opening its extensions page${MAC ? ", next to a Finder window with the folder to pick" : ", and the folder to pick is on your clipboard"}.`,
      steps,
      note: `That's it: it connects by itself and stays connected after restarts.${others.length ? ` (${others.join(" and ")} ${others.length === 1 ? "works" : "work"} the same way.)` : ""}${preferred === "firefox" ? ` Firefox is your default, but it forgets add-ons it did not get from Mozilla on every restart, so this uses ${name}.` : ""}\n  Page didn't open? Go to ${page}. ${MAC ? "No Finder window?" : "Clipboard empty?"} The folder is ${shown(folder)}.`,
    };
  }
  const firefoxApp = installedApp("firefox");
  if (!chromium.length && firefoxApp && !live.includes("firefox")) {
    const manifest = join(browserHome(), "extension", "firefox", "manifest.json");
    const { page } = BROWSER_APPS.firefox!;
    return {
      browser: "firefox",
      name: "Firefox",
      page,
      app: firefoxApp,
      pick: manifest,
      intro: `Let me use Firefox: about 20 seconds. Firefox is opening its debugging page${MAC ? ", next to a Finder window with the file to pick" : ", and the file to pick is on your clipboard"}.`,
      steps: [`Click ${BROWSER_APPS.firefox!.loadUnpacked}`, ...pickerSteps('"manifest.json" file', "Open")],
      note: `Firefox forgets add-ons it did not get from Mozilla when it quits, so this repeats after each restart; Brave, Chrome or Edge keep it.\n  Page didn't open? Go to ${page}. ${MAC ? "No Finder window?" : "Clipboard empty?"} The file is ${shown(manifest)}.`,
    };
  }
  return null;
}

const HELP_EVERY_MS = 60_000;
let lastHelp = 0;

/** Opens a web page in the default browser (Safari counts: it only has to show a download page). */
function openUrl(url: string): void {
  if (process.platform === "darwin") spawnSync("open", [url]);
  else if (process.platform === "win32") spawnSync("cmd", ["/c", "start", "", url]);
  else spawnSync("xdg-open", [url]);
}

/**
 * Does the person's first move for them: opens the extensions page and copies the path, or,
 * with no usable browser, opens Brave's download page. At most once a minute, so repeated
 * calls do not keep throwing windows at them. VIBY_BROWSER_NO_OPEN=1 turns it off (tests).
 */
function openHelp(plan: Plan | null, force = false): boolean {
  if (process.env.VIBY_BROWSER_NO_OPEN === "1") return false;
  if (!force && Date.now() - lastHelp < HELP_EVERY_MS) return true; // opened a moment ago
  lastHelp = Date.now();
  if (!plan) {
    openUrl(BROWSER_APPS.brave!.download);
    return true;
  }
  const opened = openPage(plan.browser, plan.app, plan.page);
  if (MAC) spawnSync("open", ["-R", plan.pick]); // last, so Finder is in front with the item selected
  else copyToClipboard(plan.pick);
  return opened;
}

/** The guide for a session with nothing connected, after doing what can be done for the person. */
function helpNow(live: string[]): string {
  const plan = onboardingPlan(live);
  return guide(plan, { opened: openHelp(plan) });
}

function personSteps(plan: Plan, opened: boolean): string[] {
  const steps = opened ? plan.steps : [`Open ${plan.page}.`, ...plan.steps];
  return [`  ${opened ? plan.intro : plan.intro.replace(/ \S+ is opening .*$/, "")}`, ...steps.map((s, i) => `  ${i + 1}. ${s}`), `  ${plan.note}`];
}

function guide(plan: Plan | null, { opened = false, afterSetup = false } = {}): string {
  if (!plan) {
    const lines = ["Browser control is not connected and no usable browser is installed. Show the person this, then stop:", ""];
    lines.push(`  I can't find a browser I can work with${opened ? ", so I've opened Brave's download page" : ""}. Install Brave (or one of these), open it once, then ask me again:`);
    for (const key of ["brave", "chrome", "edge", "firefox"]) lines.push(`  • ${BROWSER_APPS[key]!.name}: ${BROWSER_APPS[key]!.download}`);
    if (safariInstalled()) lines.push("  Safari is installed, but it does not allow this kind of extension.");
    lines.push("", "Do not call setup: there is nothing to set up until one is installed.");
    return lines.join("\n");
  }
  const steps = personSteps(plan, opened);
  if (afterSetup) {
    return ["Not connected yet after two minutes. If the person is still on it, wait for them to say done, then call action='status'. If they are stuck, show these steps again:", "", ...steps].join("\n");
  }
  return [
    "Browser control is not connected yet. Show the person these steps exactly as written, as a numbered list, and end your reply there. When they say it is done, call the tool again: it connects as soon as the extension is loaded.",
    "",
    ...steps,
  ].join("\n");
}

async function status(): Promise<string> {
  ensureInstalled();
  const live = await liveBrowsers();
  const lines = [`viby-toolkit ${pluginVersion()}; connected: ${live.join(", ") || "none"}; this session: ${session.browser ?? "-"} tab ${session.tabId ?? "-"}`];
  for (const name of live) {
    const ping = await callHost(socketPath(name), "ping", {}, 3000);
    lines.push(`  ${name}: ${ping.ok ? `extension ${ping.result?.extensionVersion ?? "?"}` : ping.error}`);
  }
  lines.push(live.length ? "Ready." : helpNow(live));
  return lines.join("\n");
}

const SETUP_WAIT_MS = 120_000;

/**
 * Guided setup: install, open each browser's extensions page, put the path to pick on the
 * clipboard, then wait for the extension to connect, so the call itself ends with "connected".
 */
async function setup(): Promise<string> {
  install();
  const before = await liveBrowsers();
  const plan = onboardingPlan(before);
  if (before.length && !plan) return `Connected: ${before.join(", ")}. Ready.`;
  if (!plan) return guide(plan, { opened: openHelp(plan, true) });
  const opened = openHelp(plan, true);
  for (const end = Date.now() + SETUP_WAIT_MS; Date.now() < end; ) {
    const live = await liveBrowsers();
    const added = live.filter((b) => !before.includes(b));
    if (added.length) return `Connected: ${added.join(", ")}. Ready — tell the person it worked.`;
    await new Promise((r) => setTimeout(r, 1000));
  }
  return guide(plan, { opened, afterSetup: true });
}

export async function callTool(input: Json): Promise<{ content: Json[]; isError?: boolean }> {
  const action = input.action;
  const fail = (text: string) => ({ content: [{ type: "text", text }], isError: true });
  if (!isAction(action)) return fail(`Unknown action: ${String(action)}`);
  try {
    if (LOCAL_ACTIONS.has(action)) return { content: [{ type: "text", text: action === "setup" ? await setup() : await status() }] };
    const browser = await chooseBrowser(input.browser);
    if (browser !== session.browser) {
      session.browser = browser;
      delete session.tabId;
    }
    const tabId: number | undefined = Number.isInteger(input.tab_id) ? input.tab_id : session.tabId;
    if (needsTab(action) && tabId === undefined) {
      throw new InputError("No tab yet. Use action='open' with a url, or list_tabs then select_tab.");
    }
    const [bridgeAction, params] = bridgeRequest(action as Action, input, tabId);
    const reply = await callHost(socketPath(browser), bridgeAction, params);
    if (!reply.ok) return fail(explainError(action, reply.error ?? "unknown error"));
    if ((action === "open" || action === "new_tab" || action === "select_tab") && Number.isInteger(reply.result?.tabId)) {
      session.tabId = reply.result.tabId;
    } else if (action === "select_tab") {
      session.tabId = input.tab_id;
    }
    return { content: formatResult(action, reply.result) };
  } catch (err) {
    return fail(err instanceof InputError ? err.message : `browser ${action} failed: ${(err as Error).message}`);
  }
}

// ── MCP over stdio: newline-delimited JSON-RPC 2.0 ──────────────────────────────────────

function send(message: Json): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
}

async function handle(message: Json): Promise<void> {
  const { id, method, params } = message;
  if (id === undefined) return; // notifications (initialized, cancelled) need no reply
  switch (method) {
    case "initialize":
      return send({
        id,
        result: {
          protocolVersion: params?.protocolVersion ?? "2025-06-18",
          capabilities: { tools: {} },
          serverInfo: { name: "viby-browser", version: pluginVersion() },
        },
      });
    case "ping":
      return send({ id, result: {} });
    case "tools/list":
      return send({ id, result: { tools: [{ name: "browser", description: TOOL_DESCRIPTION, inputSchema: INPUT_SCHEMA }] } });
    case "tools/call":
      if (params?.name !== "browser") return send({ id, error: { code: -32602, message: `Unknown tool: ${params?.name}` } });
      return send({ id, result: await callTool(params.arguments ?? {}) });
    default:
      return send({ id, error: { code: -32601, message: `Method not found: ${method}` } });
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  const decoder = new LineDecoder();
  const inFlight = new Set<Promise<void>>();
  process.stdin.on("data", (chunk: Buffer) => {
    for (const message of decoder.push(chunk) as Json[]) {
      if ("__invalid" in message) {
        send({ id: null, error: { code: -32700, message: "Parse error" } });
        continue;
      }
      const job = handle(message).finally(() => inFlight.delete(job));
      inFlight.add(job);
    }
  });
  // Answer what was already asked before exiting; a closed stdin must not drop a reply.
  process.stdin.on("end", () => void Promise.allSettled([...inFlight]).then(() => process.exit(0)));
}
