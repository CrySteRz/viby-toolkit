/**
 * Builds the extension for each browser family and installs the native host.
 *
 * Everything lands in browserHome() (~/.viby/browser), not the plugin cache: the cache path
 * changes with every plugin version, but the native-host manifest and the folder the person
 * loaded the extension from must stay put.
 *
 * Port of scripts/build-extensions.py from 1jehuang/firefox-agent-bridge (MIT, see NOTICE.md).
 */
import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { spawnSync } from "node:child_process";
import { delimiter, dirname, isAbsolute, join, resolve } from "node:path";
import {
  CHROMIUM_EXTENSION_ID,
  CHROMIUM_PUBLIC_KEY,
  FIREFOX_EXTENSION_ID,
  NATIVE_HOST_NAME,
  browserHome,
} from "./protocol.ts";

const HERE = import.meta.dirname;
const EXTENSION_SOURCE = join(HERE, "extension");
const EXTENSION_FILES = ["background.js", "content.js", "popup.html", "popup.js", "LICENSE", "icons"];
const HOST_FILES = ["host.ts", "protocol.ts"];

type Json = Record<string, any>;
export type Family = "chromium" | "firefox";
/** `hostDir` holds the native-host manifest; on Windows the browser finds it through `registryKey`. */
export type BrowserTarget = { browser: string; family: Family; appDir: string; hostDir: string; registryKey?: string };

export function chromiumManifest(mv2: Json): Json {
  return {
    manifest_version: 3,
    name: mv2.name,
    version: mv2.version,
    description: mv2.description,
    icons: mv2.icons,
    key: CHROMIUM_PUBLIC_KEY,
    minimum_chrome_version: "116",
    permissions: ["tabs", "activeTab", "nativeMessaging", "storage", "webNavigation", "downloads", "scripting", "alarms"],
    // With "Allow user scripts" on, evaluate also works on pages whose CSP forbids eval.
    optional_permissions: ["userScripts"],
    host_permissions: ["<all_urls>"],
    background: { service_worker: "background.js" },
    content_scripts: structuredClone(mv2.content_scripts),
    action: structuredClone(mv2.browser_action),
  };
}

/** Writes `<outDir>/chromium` (MV3) and `<outDir>/firefox` (MV2) from the vendored source. */
export function buildExtension(outDir: string): Record<Family, string> {
  const mv2 = JSON.parse(readFileSync(join(EXTENSION_SOURCE, "manifest.json"), "utf8")) as Json;
  const targets: Record<Family, Json> = { chromium: chromiumManifest(mv2), firefox: mv2 };
  const out = {} as Record<Family, string>;
  for (const [family, manifest] of Object.entries(targets) as [Family, Json][]) {
    const dir = join(outDir, family);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    for (const name of EXTENSION_FILES) cpSync(join(EXTENSION_SOURCE, name), join(dir, name), { recursive: true });
    writeFileSync(join(dir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
    out[family] = dir;
  }
  return out;
}

/** Where each supported browser's program lives, and how to send the person to get it. */
export type BrowserApp = {
  name: string;
  page: string;
  /** Where Developer mode and Load unpacked sit on that browser's extensions page. */
  devMode: string;
  loadUnpacked: string;
  download: string;
  mac: string;
  win: string[];
  linux: string[];
};

const CHROME_UI = { devMode: "Developer mode (the switch at the top right)", loadUnpacked: "Load unpacked (top left)" };

export const BROWSER_APPS: Record<string, BrowserApp> = {
  brave: { name: "Brave", page: "brave://extensions", ...CHROME_UI, download: "https://brave.com/download/", mac: "Brave Browser", win: ["BraveSoftware\\Brave-Browser\\Application\\brave.exe"], linux: ["brave-browser", "brave"] },
  chrome: { name: "Chrome", page: "chrome://extensions", ...CHROME_UI, download: "https://www.google.com/chrome/", mac: "Google Chrome", win: ["Google\\Chrome\\Application\\chrome.exe"], linux: ["google-chrome", "google-chrome-stable"] },
  chromium: { name: "Chromium", page: "chrome://extensions", ...CHROME_UI, download: "https://www.chromium.org/getting-involved/download-chromium/", mac: "Chromium", win: ["Chromium\\Application\\chrome.exe"], linux: ["chromium", "chromium-browser"] },
  edge: {
    name: "Edge",
    page: "edge://extensions",
    devMode: "Developer mode (the switch in the menu on the left; click ☰ if the menu is hidden)",
    loadUnpacked: "Load unpacked (above the list of extensions)",
    download: "https://www.microsoft.com/edge",
    mac: "Microsoft Edge",
    win: ["Microsoft\\Edge\\Application\\msedge.exe"],
    linux: ["microsoft-edge", "microsoft-edge-stable"],
  },
  firefox: { name: "Firefox", page: "about:debugging#/runtime/this-firefox", devMode: "", loadUnpacked: "Load Temporary Add-on…", download: "https://www.mozilla.org/firefox/", mac: "Firefox", win: ["Mozilla Firefox\\firefox.exe"], linux: ["firefox"] },
};

/** macOS bundle ids, Windows ProgIds (prefixes) and Linux .desktop names of the supported browsers. */
const HANDLERS: [string, string][] = [
  ["com.brave.browser", "brave"], ["com.google.chrome", "chrome"], ["org.chromium.chromium", "chromium"], ["com.microsoft.edgemac", "edge"], ["org.mozilla.firefox", "firefox"],
  ["bravehtml", "brave"], ["chromehtml", "chrome"], ["chromiumhtm", "chromium"], ["msedgehtm", "edge"], ["firefoxurl", "firefox"],
  ["brave-browser.desktop", "brave"], ["google-chrome.desktop", "chrome"], ["chromium.desktop", "chromium"], ["chromium-browser.desktop", "chromium"], ["microsoft-edge.desktop", "edge"], ["firefox.desktop", "firefox"],
];

export function browserForHandler(id: string): string | undefined {
  const key = id.trim().toLowerCase();
  return HANDLERS.find(([prefix]) => key === prefix || key.startsWith(prefix))?.[1];
}

/**
 * The person's default web browser, when it is one we support. VIBY_BROWSER_DEFAULT overrides it
 * (empty: none), so tests and simulations do not depend on this machine.
 */
export function defaultBrowser(platform = process.platform, home = homedir()): string | undefined {
  const forced = process.env.VIBY_BROWSER_DEFAULT;
  if (forced !== undefined) return forced || undefined;
  try {
    if (platform === "darwin") {
      const plist = join(home, "Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist");
      const out = spawnSync("plutil", ["-convert", "json", "-o", "-", plist], { encoding: "utf8" }).stdout;
      const handler = (JSON.parse(out).LSHandlers as Json[]).find((h) => h.LSHandlerURLScheme === "https");
      return handler ? browserForHandler(String(handler.LSHandlerRoleAll ?? "")) : undefined;
    }
    if (platform === "win32") {
      const key = "HKCU\\Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\https\\UserChoice";
      const out = spawnSync("reg", ["query", key, "/v", "ProgId"], { encoding: "utf8" }).stdout;
      const progId = /ProgId\s+REG_SZ\s+(\S+)/i.exec(out)?.[1];
      return progId ? browserForHandler(progId) : undefined;
    }
    const out = spawnSync("xdg-settings", ["get", "default-web-browser"], { encoding: "utf8" }).stdout;
    return out ? browserForHandler(out) : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Folders searched for browser programs. VIBY_BROWSER_APP_ROOTS (path-list) replaces them, so a
 * fresh machine can be simulated on a real one. On macOS only /Applications and ~/Applications:
 * LaunchServices would also find test-runner copies (Playwright's Chromium in ~/Library/Caches).
 */
function appRoots(platform: NodeJS.Platform, env: NodeJS.ProcessEnv, home: string): string[] {
  if (env.VIBY_BROWSER_APP_ROOTS !== undefined) return env.VIBY_BROWSER_APP_ROOTS.split(delimiter).filter(Boolean);
  if (platform === "darwin") return ["/Applications", join(home, "Applications")];
  if (platform === "win32") return [env.LOCALAPPDATA, env.PROGRAMFILES, env["PROGRAMFILES(X86)"]].filter((d): d is string => !!d);
  return (env.PATH ?? "").split(delimiter).filter(Boolean);
}

/** The installed program for a browser (macOS .app, Windows .exe, Linux binary), or undefined. */
export function installedApp(browser: string, platform = process.platform, env: NodeJS.ProcessEnv = process.env, home = homedir()): string | undefined {
  const target = BROWSER_APPS[browser];
  if (!target) return undefined;
  const names = platform === "darwin" ? [`${target.mac}.app`] : platform === "win32" ? target.win : target.linux;
  return appRoots(platform, env, home)
    .flatMap((root) => names.map((n) => join(root, n)))
    .find((p) => existsSync(p));
}

/** Safari is common on a Mac and cannot run this extension, so the guide names it. */
export function safariInstalled(platform = process.platform, env: NodeJS.ProcessEnv = process.env, home = homedir()): boolean {
  return platform === "darwin" && appRoots(platform, env, home).some((r) => existsSync(join(r, "Safari.app")));
}

/** Browsers installed (program found) or used (profile folder found), with where their native-host manifests go. */
export function detectBrowsers(home = homedir(), platform = process.platform, env: NodeJS.ProcessEnv = process.env): BrowserTarget[] {
  const mac = (p: string) => join(home, "Library", "Application Support", p);
  const linux = (p: string) => join(home, ".config", p);
  const local = (p: string) => join(env.LOCALAPPDATA || join(home, "AppData", "Local"), p);
  const roaming = (p: string) => join(env.APPDATA || join(home, "AppData", "Roaming"), p);
  const hkcu = (p: string) => `HKCU\\Software\\${p}\\NativeMessagingHosts\\${NATIVE_HOST_NAME}`;
  if (platform === "win32") {
    const hostDir = (browser: string) => join(browserHome(), "native-hosts", browser);
    return [
      { browser: "brave", family: "chromium" as Family, appDir: local("BraveSoftware\\Brave-Browser\\User Data"), registryKey: hkcu("BraveSoftware\\Brave-Browser") },
      { browser: "chrome", family: "chromium" as Family, appDir: local("Google\\Chrome\\User Data"), registryKey: hkcu("Google\\Chrome") },
      { browser: "chromium", family: "chromium" as Family, appDir: local("Chromium\\User Data"), registryKey: hkcu("Chromium") },
      { browser: "edge", family: "chromium" as Family, appDir: local("Microsoft\\Edge\\User Data"), registryKey: hkcu("Microsoft\\Edge") },
      { browser: "firefox", family: "firefox" as Family, appDir: roaming("Mozilla\\Firefox"), registryKey: hkcu("Mozilla") },
    ]
      .filter((t) => existsSync(t.appDir) || installedApp(t.browser, platform, env, home))
      .map((t) => ({ ...t, hostDir: hostDir(t.browser) }));
  }
  const table: { browser: string; family: Family; appDir: string; hostDir?: string }[] =
    platform === "darwin"
      ? [
          { browser: "brave", family: "chromium", appDir: mac("BraveSoftware/Brave-Browser") },
          { browser: "chrome", family: "chromium", appDir: mac("Google/Chrome") },
          { browser: "chromium", family: "chromium", appDir: mac("Chromium") },
          { browser: "edge", family: "chromium", appDir: mac("Microsoft Edge") },
          { browser: "firefox", family: "firefox", appDir: mac("Firefox"), hostDir: mac("Mozilla/NativeMessagingHosts") },
        ]
      : [
          { browser: "brave", family: "chromium", appDir: linux("BraveSoftware/Brave-Browser") },
          { browser: "chrome", family: "chromium", appDir: linux("google-chrome") },
          { browser: "chromium", family: "chromium", appDir: linux("chromium") },
          { browser: "edge", family: "chromium", appDir: linux("microsoft-edge") },
          { browser: "firefox", family: "firefox", appDir: join(home, ".mozilla", "firefox"), hostDir: join(home, ".mozilla", "native-messaging-hosts") },
        ];
  return table
    .filter((t) => existsSync(t.appDir) || installedApp(t.browser, platform, env, home))
    .map((t) => ({ browser: t.browser, family: t.family, appDir: t.appDir, hostDir: t.hostDir ?? join(t.appDir, "NativeMessagingHosts") }));
}

export type ExtensionState =
  | { state: "installed"; profile: string }
  | { state: "disabled"; profile: string }
  | { state: "other-folder"; profile: string; path: string }
  | { state: "missing" };

/**
 * Where the extension stands in a Chromium browser's profiles, read from each profile's
 * `Secure Preferences` / `Preferences`. Lets the guide say "turn it on" or "load it from the
 * right folder" instead of a generic "not connected". From jcode's browser_chromium_diag.rs.
 */
export function chromiumExtensionState(appDir: string, expectedDir = join(browserHome(), "extension", "chromium")): ExtensionState {
  let profiles: string[] = [];
  try {
    profiles = readdirSync(appDir).filter((d) => d === "Default" || d.startsWith("Profile "));
  } catch {
    return { state: "missing" };
  }
  let best: ExtensionState = { state: "missing" };
  for (const profile of profiles) {
    for (const file of ["Secure Preferences", "Preferences"]) {
      let record: Json | undefined;
      try {
        record = JSON.parse(readFileSync(join(appDir, profile, file), "utf8"))?.extensions?.settings?.[CHROMIUM_EXTENSION_ID];
      } catch {
        continue;
      }
      if (!record) continue;
      const disabled = (Array.isArray(record.disable_reasons) && record.disable_reasons.length > 0) || (typeof record.disable_reasons === "number" && record.disable_reasons !== 0) || record.state === 0;
      if (typeof record.path === "string" && isAbsolute(record.path) && !samePath(record.path, expectedDir)) {
        best = { state: "other-folder", profile, path: record.path };
      } else if (disabled) {
        if (best.state === "missing") best = { state: "disabled", profile };
      } else {
        return { state: "installed", profile };
      }
    }
  }
  return best;
}

function samePath(a: string, b: string): boolean {
  const norm = (p: string) => (process.platform === "win32" ? resolve(p).toLowerCase() : resolve(p));
  return norm(a) === norm(b);
}

export function nativeHostManifest(family: Family, hostPath: string): Json {
  const base = { name: NATIVE_HOST_NAME, description: "Viby Browser native host (viby-toolkit)", path: hostPath, type: "stdio" };
  return family === "chromium"
    ? { ...base, allowed_origins: [`chrome-extension://${CHROMIUM_EXTENSION_ID}/`] }
    : { ...base, allowed_extensions: [FIREFOX_EXTENSION_ID] };
}

/** The vendored extension's version: what a connected browser should report once it is up to date. */
export function extensionVersion(): string {
  return String(JSON.parse(readFileSync(join(EXTENSION_SOURCE, "manifest.json"), "utf8")).version);
}

export function pluginVersion(): string {
  const manifest = join(HERE, "..", ".claude-plugin", "plugin.json");
  try {
    return String(JSON.parse(readFileSync(manifest, "utf8")).version);
  } catch {
    return "unknown";
  }
}

/**
 * The node to bake into the host launcher. A version-pinned path (Homebrew's Cellar) breaks on
 * the next upgrade, so the PATH entry is preferred when it is the same binary.
 */
export function stableNodePath(execPath = process.execPath, pathEnv = process.env.PATH ?? "", platform = process.platform): string {
  const real = (p: string) => {
    try {
      return realpathSync(p);
    } catch {
      return null;
    }
  };
  const target = real(execPath);
  for (const dir of pathEnv.split(delimiter)) {
    const candidate = join(dir, platform === "win32" ? "node.exe" : "node");
    if (dir && target && real(candidate) === target) return candidate;
  }
  return execPath;
}

export type InstallReport = {
  home: string;
  version: string;
  extension: Record<Family, string>;
  registered: { browser: string; manifest: string }[];
  failed: { browser: string; error: string }[];
};

/** The launcher the browser runs. Browsers start native hosts with a bare PATH, so it names node absolutely. */
export function hostLauncher(root: string, nodePath: string, platform = process.platform): { file: string; text: string } {
  const flags = "--experimental-strip-types --disable-warning=ExperimentalWarning";
  // @echo off matters: anything a .cmd echoes lands on stdout, inside the native-messaging stream.
  return platform === "win32"
    ? { file: join(root, "host.cmd"), text: `@echo off\r\n"${nodePath}" ${flags} "${join(root, "host.ts")}" %*\r\n` }
    : { file: join(root, "host.sh"), text: `#!/bin/sh\nexec "${nodePath}" ${flags} "${join(root, "host.ts")}" "$@"\n` };
}

/** `reg add` arguments that point a browser at its manifest (Windows only). */
export function registryArgs(registryKey: string, manifest: string): string[] {
  return ["add", registryKey, "/ve", "/t", "REG_SZ", "/d", manifest, "/f"];
}

/** Installs into browserHome() and registers the native host with every browser found. */
export function install(nodePath = stableNodePath(), home = homedir(), platform = process.platform): InstallReport {
  const root = browserHome();
  mkdirSync(root, { recursive: true });
  for (const file of HOST_FILES) cpSync(join(HERE, file), join(root, file));
  const launcher = hostLauncher(root, nodePath, platform);
  writeFileSync(launcher.file, launcher.text);
  if (platform !== "win32") chmodSync(launcher.file, 0o755);
  const extension = buildExtension(join(root, "extension"));

  const registered: InstallReport["registered"] = [];
  const failed: InstallReport["failed"] = [];
  for (const target of detectBrowsers(home, platform)) {
    const manifest = join(target.hostDir, `${NATIVE_HOST_NAME}.json`);
    mkdirSync(dirname(manifest), { recursive: true });
    writeFileSync(manifest, `${JSON.stringify(nativeHostManifest(target.family, launcher.file), null, 2)}\n`);
    if (target.registryKey) {
      const reg = spawnSync("reg", registryArgs(target.registryKey, manifest), { encoding: "utf8" });
      if (reg.status !== 0) {
        failed.push({ browser: target.browser, error: (reg.stderr || reg.error?.message || "reg add failed").trim() });
        continue;
      }
    }
    registered.push({ browser: target.browser, manifest });
  }
  const version = pluginVersion();
  writeFileSync(join(root, "VERSION"), `${version}\n`);
  return { home: root, version, extension, registered, failed };
}

export function installedVersion(): string | null {
  try {
    return readFileSync(join(browserHome(), "VERSION"), "utf8").trim();
  } catch {
    return null;
  }
}
