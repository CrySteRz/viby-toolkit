/**
 * Contract tests for the browser bridge (plugins/viby-toolkit/browser).
 *
 * Run: node --experimental-strip-types --test tests/browser.test.ts
 *
 * No browser is involved: the native host runs as a real child process with this test
 * playing the extension on its stdin/stdout, and the MCP server's tool calls go to a fake
 * host on a real unix socket. A live run against Brave is a separate, manual step.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { setTimeout as pause } from "node:timers/promises";

const BROWSER = path.join(path.dirname(import.meta.dirname), "plugins", "viby-toolkit", "browser");
const HOME = fs.mkdtempSync(path.join(os.tmpdir(), "viby-browser-"));
process.env.VIBY_BROWSER_HOME = HOME;
// Setup registers native hosts under the home folder; never let a test touch the real one.
const FAKE_HOME = fs.mkdtempSync(path.join(os.tmpdir(), "viby-user-"));
process.env.HOME = FAKE_HOME;
// No real browser program counts either: tests add the ones they need here.
const APPS = fs.mkdtempSync(path.join(os.tmpdir(), "viby-apps-"));
process.env.VIBY_BROWSER_APP_ROOTS = APPS;
process.env.VIBY_BROWSER_NO_OPEN = "1"; // never open pages on the machine running the tests
process.env.VIBY_BROWSER_DEFAULT = ""; // nor depend on its default browser

const { CHROMIUM_EXTENSION_ID, CHROMIUM_PUBLIC_KEY, FIREFOX_EXTENSION_ID, NATIVE_HOST_NAME, NativeDecoder, LineDecoder, encodeNative, socketPath, CHUNK_SIZE } =
  await import("../plugins/viby-toolkit/browser/protocol.ts");
const { bridgeRequest, explainError, formatResult, InputError } = await import("../plugins/viby-toolkit/browser/actions.ts");
const { browserForHandler, buildExtension, chromiumExtensionState, chromiumManifest, detectBrowsers, hostLauncher, install, installedApp, nativeHostManifest, registryArgs, stableNodePath } = await import("../plugins/viby-toolkit/browser/install.ts");
const { callTool, chromiumTabScript } = await import("../plugins/viby-toolkit/browser/server.ts");

const children: ChildProcess[] = [];
test.after(() => {
  for (const child of children) child.kill();
  fs.rmSync(HOME, { recursive: true, force: true });
  fs.rmSync(FAKE_HOME, { recursive: true, force: true });
  fs.rmSync(APPS, { recursive: true, force: true });
});

// ── native host ────────────────────────────────────────────────────────────────────────

type Host = { child: ChildProcess; frames: any[]; next: () => Promise<any> };

function startHost(): Host {
  const child = spawn(process.execPath, ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", path.join(BROWSER, "host.ts")], {
    env: { ...process.env, VIBY_BROWSER_HOME: HOME },
    stdio: ["pipe", "pipe", "ignore"],
  });
  children.push(child);
  const decoder = new NativeDecoder();
  const frames: any[] = [];
  const waiters: ((f: any) => void)[] = [];
  child.stdout!.on("data", (chunk: Buffer) => {
    for (const frame of decoder.push(chunk)) {
      const waiter = waiters.shift();
      if (waiter) waiter(frame);
      else frames.push(frame);
    }
  });
  const next = () => (frames.length ? Promise.resolve(frames.shift()) : new Promise((resolve) => waiters.push(resolve)));
  return { child, frames, next };
}

async function waitFor(check: () => boolean, ms = 5000): Promise<void> {
  const end = Date.now() + ms;
  while (!check()) {
    if (Date.now() > end) throw new Error("condition not met in time");
    await pause(20);
  }
}

function client(socket: string) {
  const conn = net.createConnection(socket);
  const decoder = new LineDecoder();
  const replies: any[] = [];
  conn.on("data", (chunk) => replies.push(...decoder.push(chunk)));
  conn.on("error", () => conn.destroy());
  return {
    send: (m: unknown) => conn.write(`${JSON.stringify(m)}\n`),
    reply: async () => {
      await waitFor(() => replies.length > 0 || conn.destroyed, 5000);
      if (!replies.length) throw new Error("connection closed");
      return replies.shift();
    },
    close: () => conn.destroy(),
  };
}

/** A host that has said hello, with a client connected to it (not to a previous test's host). */
async function readyHost() {
  const host = startHost();
  host.child.stdin!.write(encodeNative({ type: "hello", browser: "brave" }));
  let c: ReturnType<typeof client>;
  for (const end = Date.now() + 5000; ; ) {
    await waitFor(() => fs.existsSync(socketPath("brave")));
    c = client(socketPath("brave"));
    c.send({ id: "ready", action: "hostInfo" });
    const info = await c.reply().catch(() => null);
    if (info?.result?.pid === host.child.pid) break;
    c.close();
    if (Date.now() > end) throw new Error("host never became ready");
    await pause(50);
  }
  const stop = async () => {
    c!.close();
    if (host.child.exitCode !== null) return;
    const exited = new Promise((r) => host.child.once("exit", r));
    host.child.stdin!.end();
    await exited;
  };
  return { host, c: c!, stop };
}

test("host: a host the extension never greets opens no socket and exits with its browser", async () => {
  const host = startHost();
  const exited = new Promise((r) => host.child.once("exit", r));
  host.child.stdin!.end();
  assert.equal(await exited, 0);
  assert.equal(fs.existsSync(socketPath("brave")), false);
});

test("host: listens after hello, on a socket only this user can open", async () => {
  const host = startHost();
  const sock = socketPath("brave");
  host.child.stdin!.write(encodeNative({ type: "hello", browser: "brave" }));
  await waitFor(() => fs.existsSync(sock));
  await waitFor(() => (fs.statSync(sock).mode & 0o777) === 0o600);
  assert.equal(fs.statSync(path.dirname(sock)).mode & 0o777, 0o700);
  const exited = new Promise((r) => host.child.once("exit", r));
  host.child.stdin!.end();
  await exited;
  assert.equal(fs.existsSync(sock), false, "socket removed on exit");
});

test("host: routes a request to the extension and the reply back by id", async () => {
  const { host, c, stop } = await readyHost();
  c.send({ id: 7, action: "ping", params: {} });
  const forwarded = await host.next();
  assert.equal(forwarded.action, "ping");
  assert.match(forwarded.id, /^h\d+_\d+$/, "host assigns its own id, so two clients cannot collide");
  host.child.stdin!.write(encodeNative({ id: forwarded.id, ok: true, result: { pong: true } }));
  assert.deepEqual(await c.reply(), { id: 7, ok: true, result: { pong: true } });
  await stop();
});

test("host: when a second host for the same browser exits, the first takes the socket back", async () => {
  const first = await readyHost();
  const second = await readyHost(); // took the socket over
  const ask = async () => {
    const c = client(socketPath("brave"));
    c.send({ id: "who", action: "hostInfo" });
    const reply = await c.reply().catch(() => null);
    c.close();
    return reply?.result?.pid;
  };
  assert.equal(await ask(), second.host.child.pid);
  await second.stop();
  await waitFor(() => fs.existsSync(socketPath("brave")), 6000);
  let pid: number | undefined;
  for (const end = Date.now() + 6000; Date.now() < end && pid !== first.host.child.pid; await pause(100)) pid = await ask();
  assert.equal(pid, first.host.child.pid, "the browser that is still open stays reachable");
  await first.stop();
});

test("host: refuses credential actions and anything off the allow-list", async () => {
  const { host, c, stop } = await readyHost();
  for (const action of ["secureAutoFill", "autoLogin", "getAuthContext", "dropFile"]) {
    c.send({ id: action, action, params: {} });
    const reply = await c.reply();
    assert.equal(reply.ok, false);
    assert.match(reply.error, /not allowed/);
  }
  c.send({ id: "x", params: {} });
  assert.match((await c.reply()).error, /Missing action/);
  assert.equal(host.frames.length, 0, "nothing reached the extension");
  await stop();
});

test("host: uploads a small file inline as a fillForm file field", async () => {
  const { host, c, stop } = await readyHost();
  const file = path.join(HOME, "note.txt");
  fs.writeFileSync(file, "hello");
  c.send({ id: 1, action: "uploadFile", params: { path: file, tabId: 3 } });
  const frame = await host.next();
  assert.equal(frame.action, "fillForm");
  assert.equal(frame.params.tabId, 3);
  assert.equal(frame.params.path, undefined, "the local path never reaches the page");
  assert.deepEqual(frame.params.fields, [{ selector: 'input[type="file"]', file: { name: "note.txt", type: "text/plain", data: Buffer.from("hello").toString("base64") } }]);
  await stop();
});

test("host: chunks a file over Chrome's 1 MB message cap", async () => {
  const { host, c, stop } = await readyHost();
  const file = path.join(HOME, "big.png");
  fs.writeFileSync(file, Buffer.alloc(1_200_000, 7));
  c.send({ id: 1, action: "uploadFile", params: { path: file, selector: "#f" } });
  const start = await host.next();
  assert.equal(start.type, "chunk_start");
  const encoded = fs.readFileSync(file).toString("base64");
  const parts: string[] = [];
  for (let i = 0; i < start.totalChunks; i++) {
    const chunk = await host.next();
    assert.equal(chunk.type, "chunk_data");
    assert.ok(chunk.data.length <= CHUNK_SIZE);
    parts[chunk.chunkIndex] = chunk.data;
  }
  assert.equal(parts.join(""), encoded);
  const request = await host.next();
  assert.equal(request.action, "fillForm");
  assert.deepEqual(request.params.fields[0], { selector: "#f", file: { name: "big.png", type: "image/png", chunkedTransfer: start.transferId } });
  await stop();
});

test("host: a missing upload file is an error to the caller, not a crash", async () => {
  const { host, c, stop } = await readyHost();
  c.send({ id: 1, action: "uploadFile", params: { path: path.join(HOME, "nope.bin") } });
  const reply = await c.reply();
  assert.equal(reply.ok, false);
  assert.match(reply.error, /ENOENT/);
  c.send({ id: 2, action: "hostInfo" });
  assert.equal((await c.reply()).result.browser, "brave", "host still serving");
  await stop();
});

// ── action mapping ─────────────────────────────────────────────────────────────────────

test("actions: open with no tab opens a background tab instead of taking over the current one", () => {
  assert.deepEqual(bridgeRequest("open", { url: "https://a.test" }, undefined), ["newSession", { url: "https://a.test", focus: false, wait: true }]);
  assert.deepEqual(bridgeRequest("open", { url: "https://a.test" }, 4), ["navigate", { tabId: 4, url: "https://a.test", wait: true }]);
  assert.equal(bridgeRequest("open", { url: "https://a.test", new_tab: true }, 4)[0], "newSession");
});

test("actions: each action maps to the extension's verb and params", () => {
  assert.deepEqual(bridgeRequest("snapshot", {}, 1), ["getContent", { tabId: 1, format: "annotated" }]);
  assert.deepEqual(bridgeRequest("get_content", {}, 1), ["getContent", { tabId: 1, format: "text" }]);
  assert.deepEqual(bridgeRequest("select", { selector: "#s", text: "b" }, 1), ["fillForm", { tabId: 1, fields: [{ selector: "#s", value: "b" }] }]);
  assert.deepEqual(bridgeRequest("wait", { contains: "Done", timeout_ms: 900 }, 1), ["waitFor", { tabId: 1, contains: "Done", timeout: 900 }]);
  assert.deepEqual(bridgeRequest("upload", { path: "/tmp/x" }, 1), ["uploadFile", { tabId: 1, path: "/tmp/x" }]);
  assert.deepEqual(bridgeRequest("fill_form", { fields: [{ selector: "#a", value: "1" }, { selector: "#c", checked: true }] }, 1), [
    "fillForm",
    { tabId: 1, fields: [{ selector: "#a", value: "1" }, { selector: "#c", checked: true }] },
  ]);
  const [scrollAction, scrollParams] = bridgeRequest("scroll", { selector: "#list", y: 300 }, 1);
  assert.equal(scrollAction, "evaluate", "selector + delta scrolls that container, not the page");
  assert.match(scrollParams.script, /document\.querySelector\("#list"\)/);
  const [pressAction, pressParams] = bridgeRequest("press", { key: "Enter" }, 1);
  assert.equal(pressAction, "evaluate");
  assert.equal(pressParams.pageWorld, true);
  assert.match(pressParams.script, /const key = "Enter"/);
});

test("actions: missing required inputs are refused before reaching the browser", () => {
  assert.throws(() => bridgeRequest("click", {}, 1), InputError);
  assert.throws(() => bridgeRequest("type", {}, 1), /text is required/);
  assert.throws(() => bridgeRequest("open", {}, 1), /url is required/);
  assert.throws(() => bridgeRequest("wait", {}, 1), /selector, text, or contains/);
  assert.throws(() => bridgeRequest("scroll", {}, 1), /scroll requires/);
  assert.throws(() => bridgeRequest("fill_form", { fields: [] }, 1), /fields are required/);
  assert.throws(() => bridgeRequest("press", {}, 1), /key is required/);
});

test("actions: screenshots come back as an image block; errors name the next step", () => {
  const [image, caption] = formatResult("screenshot", { tabId: 2, dataUrl: "data:image/png;base64,QUJD" });
  assert.deepEqual(image, { type: "image", data: "QUJD", mimeType: "image/png" });
  assert.match(caption!.text, /tab 2/);
  assert.match(explainError("open", "Timed out waiting for page load"), /snapshot/);
  assert.match(explainError("snapshot", "Cannot access a chrome:// URL"), /internal pages/);
  assert.equal(explainError("click", "Element not found: #x"), "Element not found: #x");
});

// ── MCP server tool calls against a fake host ──────────────────────────────────────────

function fakeHost(browser: string, answer: (action: string, params: any) => any): Promise<{ seen: any[]; close: () => void }> {
  fs.mkdirSync(path.dirname(socketPath(browser)), { recursive: true });
  fs.rmSync(socketPath(browser), { force: true });
  const seen: any[] = [];
  const server = net.createServer((conn) => {
    const decoder = new LineDecoder();
    conn.on("data", (chunk) => {
      for (const m of decoder.push(chunk) as any[]) {
        if (m.action !== "hostInfo") seen.push(m);
        const result = m.action === "hostInfo" ? { browser } : answer(m.action, m.params);
        conn.write(`${JSON.stringify(result instanceof Error ? { id: m.id, ok: false, error: result.message } : { id: m.id, ok: true, result })}\n`);
      }
    });
  });
  return new Promise((resolve) => server.listen(socketPath(browser), () => resolve({ seen, close: () => server.close() })));
}

test("server: refuses tab actions until the session has a tab, then keeps using that tab", async () => {
  const host = await fakeHost("brave", (action, params) =>
    action === "newSession" ? { tabId: 42, windowId: 1, url: params.url, content: { title: "A", url: params.url, content: "[button: \"Go\" | selector: #go]" } } : { clicked: true },
  );
  const refused = await callTool({ action: "click", selector: "#go" });
  assert.equal(refused.isError, true);
  assert.match(refused.content[0]!.text, /No tab yet/);
  assert.equal(host.seen.length, 0);

  const opened = await callTool({ action: "open", url: "https://a.test" });
  assert.equal(opened.isError, undefined);
  assert.match(opened.content[0]!.text, /tab 42[\s\S]*selector: #go/);
  await callTool({ action: "click", selector: "#go" });
  assert.deepEqual(host.seen.at(-1), { id: host.seen.at(-1).id, action: "click", params: { tabId: 42, selector: "#go" } });
  host.close();
});

test("server: extension errors come back as tool errors with guidance", async () => {
  const host = await fakeHost("brave", () => new Error("No tab with id: 42"));
  const result = await callTool({ action: "snapshot" });
  assert.equal(result.isError, true);
  assert.match(result.content[0]!.text, /list_tabs/);
  host.close();
});

test("server: several connected browsers need an explicit choice", async () => {
  const a = await fakeHost("brave", () => ({}));
  const b = await fakeHost("firefox", () => ({ tabs: [] }));
  const ok = await callTool({ action: "list_tabs" });
  assert.equal(ok.isError, undefined, "the session's browser is still connected, so it is reused");
  a.close();
  fs.rmSync(socketPath("brave"), { force: true });
  const switched = await callTool({ action: "list_tabs" });
  assert.equal(switched.isError, undefined, "only firefox left: chosen without asking");
  const c = await fakeHost("chrome", () => ({}));
  await callTool({ action: "list_tabs", browser: "chrome" });
  assert.equal(c.seen.length, 1);
  b.close();
  c.close();
});

test("server: with nothing connected, any action installs silently and returns a guide for the person", async () => {
  fs.rmSync(path.join(HOME, "VERSION"), { force: true });
  fs.mkdirSync(process.platform === "darwin" ? path.join(FAKE_HOME, "Library/Application Support/Firefox") : path.join(FAKE_HOME, ".mozilla", "firefox"), { recursive: true });
  for (const f of fs.readdirSync(path.dirname(socketPath("x")))) fs.rmSync(path.join(path.dirname(socketPath("x")), f), { force: true });
  const firefoxApp = process.platform === "darwin" ? path.join(APPS, "Firefox.app") : path.join(APPS, "firefox");
  fs.mkdirSync(firefoxApp);
  const result = await callTool({ action: "open", url: "https://a.test" });
  fs.rmSync(firefoxApp, { recursive: true });
  assert.equal(result.isError, true);
  const text = result.content[0]!.text;
  assert.match(text, /Show the person these steps exactly as written/);
  assert.match(text, /2\. Click Load Temporary Add-on…/, "only Firefox is present in this home, so its steps are given");
  assert.match(text, /repeats after each restart/);
  assert.match(text, /end your reply there/, "a waiting tool call would hide the steps, so Claude shows them and stops");
  assert.doesNotMatch(text, /is opening/, "with opening switched off, the guide does not claim it opened anything");
  assert.ok(fs.existsSync(path.join(HOME, "VERSION")), "installed without being asked");
  const status = await callTool({ action: "status" });
  assert.match(status.content[0]!.text, /connected: none/);
});

test("install: reads whether the extension is loaded, off, or loaded from another folder", () => {
  const appDir = fs.mkdtempSync(path.join(os.tmpdir(), "viby-chromium-"));
  const expected = "/x/extension/chromium";
  const write = (record: unknown, profile = "Default") => {
    fs.mkdirSync(path.join(appDir, profile), { recursive: true });
    fs.writeFileSync(path.join(appDir, profile, "Secure Preferences"), JSON.stringify({ extensions: { settings: record ? { [CHROMIUM_EXTENSION_ID]: record } : {} } }));
  };
  write(null);
  assert.deepEqual(chromiumExtensionState(appDir, expected), { state: "missing" });
  write({ path: expected, disable_reasons: [1] });
  assert.deepEqual(chromiumExtensionState(appDir, expected), { state: "disabled", profile: "Default" });
  write({ path: "/old/copy" });
  assert.deepEqual(chromiumExtensionState(appDir, expected), { state: "other-folder", profile: "Default", path: "/old/copy" });
  write({ path: expected }, "Profile 2");
  assert.deepEqual(chromiumExtensionState(appDir, expected), { state: "installed", profile: "Profile 2" });
  fs.rmSync(appDir, { recursive: true, force: true });
});

test("server: with no usable browser, the person is asked to install one", async () => {
  const profiles = path.join(FAKE_HOME, "Library/Application Support/Firefox");
  fs.rmSync(profiles, { recursive: true, force: true });
  fs.rmSync(path.join(FAKE_HOME, ".mozilla"), { recursive: true, force: true });
  fs.mkdirSync(path.join(APPS, "Safari.app"));
  const text = (await callTool({ action: "snapshot" })).content[0]!.text;
  fs.rmSync(path.join(APPS, "Safari.app"), { recursive: true });
  assert.match(text, /Install Brave \(or one of these\)/);
  assert.doesNotMatch(text, /opened Brave's download page/, "opening is switched off here, so it is not claimed");
  for (const url of ["brave.com", "google.com/chrome", "microsoft.com/edge", "mozilla.org/firefox"]) assert.ok(text.includes(url), url);
  if (process.platform === "darwin") assert.match(text, /Safari is installed, but/);
  assert.match(text, /Do not call setup/, "nothing to set up until a browser exists");
  assert.doesNotMatch(text, /20 seconds/);
});

test("server: a browser installed but never opened is found and gets the one-time step", async () => {
  if (process.platform !== "darwin") return;
  fs.mkdirSync(path.join(APPS, "Brave Browser.app"));
  const text = (await callTool({ action: "status" })).content[0]!.text;
  fs.rmSync(path.join(APPS, "Brave Browser.app"), { recursive: true });
  assert.match(text, /Let me use Brave: one time, about 20 seconds\./);
  const steps = (text as string).split("\n").filter((l) => /^\s+\d\. /.test(l)).map((l) => l.trim());
  assert.deepEqual(steps.slice(0, 3), ["1. Open brave://extensions.", "2. Turn on Developer mode (the switch at the top right).", "3. Click Load unpacked (top left)."]);
  assert.equal(steps.length, 5, "one short list: page, switch, button, pick, confirm");
  if (process.platform === "darwin") {
    assert.equal(steps[3], '4. Drag the highlighted "chromium" folder from the Finder window onto the file picker.');
    assert.doesNotMatch(text, /⌘⇧G/, "Go to Folder did nothing in Brave's picker on macOS 26");
  }
  assert.match(text, /Page didn't open\? Go to brave:\/\/extensions/);
  assert.equal(installedApp("chrome"), undefined);
});

test("setup: lands on the extensions page itself, through the browser's AppleScript", () => {
  const script = chromiumTabScript("/Applications/Brave Browser.app", "brave://extensions");
  assert.match(script, /^tell application "\/Applications\/Brave Browser\.app"$/m, "the exact app found, not whatever LaunchServices picks");
  assert.match(script, /make new tab with properties \{URL:"brave:\/\/extensions"\}/);
  assert.match(script, /if \(count of windows\) = 0 then make new window/);
});

test("default browser: handler ids from macOS, Windows and Linux map to our browsers", () => {
  assert.equal(browserForHandler("com.brave.browser"), "brave", "macOS stores it lower-cased");
  assert.equal(browserForHandler("com.microsoft.edgemac"), "edge");
  assert.equal(browserForHandler("ChromeHTML"), "chrome");
  assert.equal(browserForHandler("MSEdgeHTM"), "edge");
  assert.equal(browserForHandler("FirefoxURL-308046B0AF4A39CB"), "firefox");
  assert.equal(browserForHandler("brave-browser.desktop\n"), "brave");
  assert.equal(browserForHandler("com.apple.safari"), undefined);
});

test("server: the person's default browser is the one guided, in that browser's own words", async () => {
  if (process.platform !== "darwin") return;
  for (const app of ["Brave Browser.app", "Microsoft Edge.app"]) fs.mkdirSync(path.join(APPS, app));
  process.env.VIBY_BROWSER_DEFAULT = "edge";
  const edge = (await callTool({ action: "status" })).content[0]!.text;
  process.env.VIBY_BROWSER_DEFAULT = "firefox";
  fs.mkdirSync(path.join(APPS, "Firefox.app"));
  const firefoxDefault = (await callTool({ action: "status" })).content[0]!.text;
  process.env.VIBY_BROWSER_DEFAULT = "";
  for (const app of ["Brave Browser.app", "Microsoft Edge.app", "Firefox.app"]) fs.rmSync(path.join(APPS, app), { recursive: true });
  assert.match(edge, /Let me use Edge/);
  assert.match(edge, /Open edge:\/\/extensions\./);
  assert.match(edge, /Developer mode \(the switch in the menu on the left/, "Edge keeps the switch in its side menu, not top right");
  assert.match(edge, /Load unpacked \(above the list of extensions\)/);
  assert.match(edge, /\(Brave works the same way\.\)/);
  assert.match(firefoxDefault, /Let me use Brave/, "Firefox forgets the add-on, so an installed Chromium browser is used");
  assert.match(firefoxDefault, /Firefox is your default, but/);
});

test("server: unknown actions are refused", async () => {
  const result = await callTool({ action: "secureAutoFill" });
  assert.equal(result.isError, true);
});

// ── extension build and install ────────────────────────────────────────────────────────

test("install: the Chromium id is the one the committed public key yields", () => {
  const der = Buffer.from(CHROMIUM_PUBLIC_KEY, "base64");
  const hex = createHash("sha256").update(der).digest("hex").slice(0, 32);
  const id = [...hex].map((h) => String.fromCharCode(97 + parseInt(h, 16))).join("");
  assert.equal(id, CHROMIUM_EXTENSION_ID);
});

test("install: builds an MV3 Chromium and an MV2 Firefox extension without debugger or notification rights", () => {
  const out = buildExtension(path.join(HOME, "build"));
  const chromium = JSON.parse(fs.readFileSync(path.join(out.chromium, "manifest.json"), "utf8"));
  const firefox = JSON.parse(fs.readFileSync(path.join(out.firefox, "manifest.json"), "utf8"));
  assert.equal(chromium.manifest_version, 3);
  assert.equal(chromium.key, CHROMIUM_PUBLIC_KEY);
  assert.equal(chromium.background.service_worker, "background.js");
  assert.equal(firefox.manifest_version, 2);
  assert.equal(firefox.browser_specific_settings.gecko.id, FIREFOX_EXTENSION_ID);
  for (const m of [chromium, firefox]) {
    assert.ok(!m.permissions.includes("debugger"));
    assert.ok(!m.permissions.includes("notifications"));
  }
  for (const dir of [out.chromium, out.firefox]) {
    for (const f of ["background.js", "content.js", "popup.html", "popup.js", "LICENSE", "icons/icon-48.png"]) assert.ok(fs.existsSync(path.join(dir, f)), f);
  }
  assert.deepEqual(chromiumManifest(firefox).action, firefox.browser_action);
});

test("install: host.sh gets the PATH node when it is the running binary, not its versioned real path", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "viby-node-"));
  fs.symlinkSync(process.execPath, path.join(dir, "node"));
  assert.equal(stableNodePath(process.execPath, `/nonexistent${path.delimiter}${dir}`), path.join(dir, "node"));
  assert.equal(stableNodePath(process.execPath, "/nonexistent"), process.execPath);
  fs.rmSync(dir, { recursive: true, force: true });
});

test("install: registers the native host only for browsers that are installed", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "viby-home-"));
  fs.mkdirSync(path.join(home, "Library/Application Support/BraveSoftware/Brave-Browser"), { recursive: true });
  fs.mkdirSync(path.join(home, "Library/Application Support/Firefox"), { recursive: true });
  assert.deepEqual(detectBrowsers(home, "darwin").map((b) => b.browser), ["brave", "firefox"]);
  const report = install("/opt/node/bin/node", home);
  fs.rmSync(home, { recursive: true, force: true });

  assert.deepEqual(report.registered.map((r) => r.browser), ["brave", "firefox"]);
  const hostSh = fs.readFileSync(path.join(HOME, "host.sh"), "utf8");
  assert.match(hostSh, /exec "\/opt\/node\/bin\/node" --experimental-strip-types/, "absolute node: browsers launch hosts with a bare PATH");
  assert.ok(fs.statSync(path.join(HOME, "host.sh")).mode & 0o100);
  for (const f of ["host.ts", "protocol.ts", "VERSION", "extension/chromium/manifest.json"]) assert.ok(fs.existsSync(path.join(HOME, f)), f);
  assert.deepEqual(nativeHostManifest("chromium", "/h").allowed_origins, [`chrome-extension://${CHROMIUM_EXTENSION_ID}/`]);
  assert.deepEqual(nativeHostManifest("firefox", "/h").allowed_extensions, [FIREFOX_EXTENSION_ID]);
  assert.equal(nativeHostManifest("firefox", "/h").name, NATIVE_HOST_NAME);
});

// ── vendored extension adaptations ─────────────────────────────────────────────────────

const background = fs.readFileSync(path.join(BROWSER, "extension", "background.js"), "utf8");
const content = fs.readFileSync(path.join(BROWSER, "extension", "content.js"), "utf8");

test("extension: connects to our native host name", () => {
  assert.match(background, new RegExp(`const NATIVE_APP_NAME = "${NATIVE_HOST_NAME}";`));
});

test("extension: credential and relay code is gone", () => {
  for (const name of ["secureAutoFill", "getAuthContext", "requestAuth", "detectAuth", "authConfig", "RELAY_URL", "connectRelayPort"]) {
    assert.ok(!background.includes(name) && !content.includes(name), name);
  }
});

test("extension: Firefox injection falls back to tabs.executeScript, not to itself", () => {
  const body = /async function injectContentScript\(tabId, frameId\) \{([\s\S]*?)\n\}/.exec(background)![1]!;
  assert.ok(!body.includes("injectContentScript("), "no self-recursion");
  assert.match(body, /browser\.tabs\.executeScript\(tabId/);
});

test("extension: one click runs the page's click handler once", () => {
  const block = /\/\/ Standard click handling for other elements[\s\S]*?return \{ clicked: true/.exec(content)![0];
  assert.equal((block.match(/new MouseEvent\("click"/g) ?? []).length, 1, "a synthetic click only as the fallback");
  assert.match(block, /if \(typeof target\.click === "function"\) target\.click\(\);\s*else target\.dispatchEvent\(new MouseEvent\("click"/);
});

test("extension: snapshots never carry a password field's value", () => {
  const source = /function visibleValue\(el\) \{[\s\S]*?\n\}/.exec(content)![0];
  const visibleValue = new Function(`${source}; return visibleValue;`)() as (el: unknown) => string;
  assert.equal(visibleValue({ type: "password", value: "hunter2" }), "•••••••");
  assert.equal(visibleValue({ type: "text", value: "hello" }), "hello");
  assert.equal(visibleValue({}), "");
  assert.ok(!/\bel\.value\b(?!\.length)/.test(content.replace(source, "").replace(/el\.value = /g, "")), "every other read of el.value goes through visibleValue");
});

// ── Windows (pure parts; no Windows machine runs this suite yet) ───────────────────────

test("windows: hosts listen on a per-user named pipe", () => {
  assert.match(socketPath("brave", "win32"), /^\\\\\.\\pipe\\viby-browser-[A-Za-z0-9-]+-brave$/);
  assert.notEqual(socketPath("brave", "win32"), socketPath("edge", "win32"));
});

test("windows: browsers are found under LOCALAPPDATA/APPDATA and registered through HKCU", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "viby-win-"));
  const env = { LOCALAPPDATA: path.join(root, "Local"), APPDATA: path.join(root, "Roaming") };
  fs.mkdirSync(path.join(env.LOCALAPPDATA, "BraveSoftware\\Brave-Browser\\User Data"), { recursive: true });
  fs.mkdirSync(path.join(env.LOCALAPPDATA, "Microsoft\\Edge\\User Data"), { recursive: true });
  fs.mkdirSync(path.join(env.APPDATA, "Mozilla\\Firefox"), { recursive: true });
  const found = detectBrowsers(root, "win32", env);
  fs.rmSync(root, { recursive: true, force: true });
  assert.deepEqual(found.map((b) => b.browser), ["brave", "edge", "firefox"]);
  assert.equal(found[0]!.registryKey, "HKCU\\Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts\\viby_browser");
  assert.equal(found[1]!.registryKey, "HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\viby_browser");
  assert.equal(found[2]!.registryKey, "HKCU\\Software\\Mozilla\\NativeMessagingHosts\\viby_browser");
  assert.deepEqual(registryArgs(found[0]!.registryKey!, "C:\\m.json"), ["add", found[0]!.registryKey, "/ve", "/t", "REG_SZ", "/d", "C:\\m.json", "/f"]);
});

test("windows: the host launcher is a quiet .cmd that passes arguments through", () => {
  const { file, text } = hostLauncher("C:\\Users\\a\\.viby\\browser", "C:\\Program Files\\nodejs\\node.exe", "win32");
  assert.match(file, /host\.cmd$/);
  assert.ok(text.startsWith("@echo off\r\n"), "nothing may be echoed onto the native-messaging stdout");
  assert.match(text, /^"C:\\Program Files\\nodejs\\node\.exe" --experimental-strip-types .*host\.ts" %\*\r\n$/m);
  assert.match(hostLauncher("/h", "/usr/bin/node", "darwin").text, /^#!\/bin\/sh\nexec "\/usr\/bin\/node"/);
});

test("windows: the PATH node is node.exe", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "viby-node-"));
  fs.symlinkSync(process.execPath, path.join(dir, "node.exe"));
  assert.equal(stableNodePath(process.execPath, dir, "win32"), path.join(dir, "node.exe"));
  fs.rmSync(dir, { recursive: true, force: true });
});
