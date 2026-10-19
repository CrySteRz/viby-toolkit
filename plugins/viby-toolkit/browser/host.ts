/**
 * Native-messaging host for the Viby Browser extension.
 *
 * The browser starts this process when the extension connects, and talks to it over
 * stdin/stdout. Once the extension says which browser it is, the host listens on a unix
 * socket that only this user can open; every MCP server (one per Claude session) connects
 * there. Requests get a host-unique id on the way in and are routed back by it.
 *
 * Port of rust-cli/src/bin/host.rs from 1jehuang/firefox-agent-bridge (MIT, see NOTICE.md),
 * minus the WebSocket, the Safari relay and every credential feature.
 */
import { chmodSync, mkdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { createServer, type Server, type Socket } from "node:net";
import { basename, extname } from "node:path";
import {
  CHUNK_SIZE,
  LineDecoder,
  NativeDecoder,
  REQUEST_TIMEOUT_MS,
  encodeNative,
  runDir,
  socketPath,
} from "./protocol.ts";

/** Actions the extension may be asked to run. Anything else is refused here, not in the page. */
const ALLOWED_ACTIONS = new Set([
  "ping", "reload", "listFrames", "listTabs", "switchTab", "newSession", "newWindow",
  "setActiveTab", "getActiveTab", "navigate", "reloadTab", "click", "type", "fillForm", "waitFor",
  "tryUntil", "getContent", "getInteractables", "screenshot", "evaluate", "scroll",
  "uploadFile", "dropFile", "batch", "fork", "killFork", "listForks", "parallel",
]);

/**
 * What batch, fork and parallel may run inside one tab. The extension dispatches their commands
 * itself, past this allow-list, so they are checked here: no nesting, no tab management, and no
 * file actions (the host reads those files, and only for a top-level request).
 */
const NESTED_ACTIONS = new Set([
  "navigate", "reloadTab", "click", "type", "fillForm", "waitFor", "getContent", "getInteractables", "evaluate", "scroll",
]);

function nestedCommands(action: string, params: Json): unknown[] {
  const list = (value: unknown) => (Array.isArray(value) ? value : []);
  if (action === "batch") return list(params.commands);
  if (action === "fork") return list(params.paths).flatMap((p) => list((p as Json)?.commands));
  if (action === "parallel") return list(params.branches).flatMap((b) => list((b as Json)?.commands));
  return [];
}

const MIME: Record<string, string> = {
  ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif",
  ".webp": "image/webp", ".svg": "image/svg+xml", ".pdf": "application/pdf",
  ".txt": "text/plain", ".csv": "text/csv", ".json": "application/json",
  ".html": "text/html", ".zip": "application/zip", ".mp4": "video/mp4",
};

type Json = Record<string, unknown>;
type Pending = { socket: Socket; clientId: unknown; timer: NodeJS.Timeout };

const log = (...parts: unknown[]) => process.stderr.write(`[viby-browser-host] ${parts.join(" ")}\n`);

let counter = 0;
const pending = new Map<string, Pending>();
let server: Server | null = null;
let ownSocket: { path: string; ino: number } | null = null;
let browserName = "unknown";

function toExtension(message: unknown): void {
  process.stdout.write(encodeNative(message));
}

function reply(socket: Socket, message: Json): void {
  if (!socket.destroyed) socket.write(`${JSON.stringify(message)}\n`);
}

/**
 * Turns `uploadFile {path, selector}` into the extension's fillForm file field, and
 * `dropFile {path, selector}` into its dropFile file, chunked if large.
 */
function fileMessages(id: string, action: string, params: Json): unknown[] {
  const path = params.path;
  if (typeof path !== "string" || !path) throw new Error(`${action} needs \`path\``);
  const data = readFileSync(path).toString("base64");
  const file: Json = {
    name: basename(path),
    type: MIME[extname(path).toLowerCase()] ?? "application/octet-stream",
  };
  const rest = { ...params };
  delete rest.path;
  const request = (f: Json) => {
    if (action === "dropFile") return { id, action, params: { ...rest, file: f } };
    const selector = typeof params.selector === "string" ? params.selector : 'input[type="file"]';
    delete rest.selector;
    return { id, action: "fillForm", params: { ...rest, fields: [{ selector, file: f }] } };
  };
  if (data.length <= CHUNK_SIZE) return [request({ ...file, data })];

  const transferId = `chunk_${id}`;
  const totalChunks = Math.ceil(data.length / CHUNK_SIZE);
  const frames: unknown[] = [
    { type: "chunk_start", transferId, fileName: file.name, mimeType: file.type, totalSize: data.length, totalChunks },
  ];
  for (let i = 0; i < totalChunks; i++) {
    frames.push({ type: "chunk_data", transferId, chunkIndex: i, data: data.slice(i * CHUNK_SIZE, (i + 1) * CHUNK_SIZE) });
  }
  frames.push(request({ ...file, chunkedTransfer: transferId }));
  return frames;
}

function handleClient(socket: Socket, message: Json): void {
  const clientId = message.id;
  if ("__invalid" in message) return reply(socket, { id: clientId, ok: false, error: "Invalid JSON" });
  const action = message.action;
  if (action === "hostInfo") {
    return reply(socket, { id: clientId, ok: true, result: { browser: browserName, pid: process.pid } });
  }
  if (typeof action !== "string") return reply(socket, { id: clientId, ok: false, error: "Missing action" });
  if (!ALLOWED_ACTIONS.has(action)) {
    return reply(socket, { id: clientId, ok: false, error: `Action not allowed: ${action}` });
  }
  const params = (message.params && typeof message.params === "object" ? message.params : {}) as Json;
  const refused = nestedCommands(action, params).find((c) => !NESTED_ACTIONS.has((c as Json | null)?.action as string));
  if (refused !== undefined) {
    return reply(socket, { id: clientId, ok: false, error: `Action not allowed inside ${action}: ${String((refused as Json | null)?.action ?? "malformed command")}` });
  }
  const id = `h${process.pid}_${++counter}`;
  let frames: unknown[];
  try {
    frames = action === "uploadFile" || action === "dropFile" ? fileMessages(id, action, params) : [{ id, action, params }];
  } catch (err) {
    return reply(socket, { id: clientId, ok: false, error: (err as Error).message });
  }
  const timer = setTimeout(() => {
    pending.delete(id);
    reply(socket, { id: clientId, ok: false, error: `Request timed out after ${REQUEST_TIMEOUT_MS / 1000}s` });
  }, REQUEST_TIMEOUT_MS);
  pending.set(id, { socket, clientId, timer });
  for (const frame of frames) toExtension(frame);
}

function handleExtension(message: Json): void {
  if (message.type === "hello") {
    browserName = typeof message.browser === "string" ? message.browser : "unknown";
    log("hello from", browserName);
    listen();
    setInterval(watch, WATCH_MS).unref();
    return;
  }
  const id = typeof message.id === "string" ? message.id : "";
  const entry = pending.get(id);
  if (!entry) return;
  pending.delete(id);
  clearTimeout(entry.timer);
  reply(entry.socket, { ...message, id: entry.clientId });
}

const WINDOWS = process.platform === "win32";
const WATCH_MS = 2000;

/**
 * Two hosts can serve one browser name at once: two profiles, or a second instance. The newest
 * takes the socket; when it goes away, watch() lets the older one take it back, so a browser
 * that is still open never ends up unreachable. On Windows a pipe name cannot be taken over,
 * so the newer host waits for it instead.
 */
function listen(): void {
  if (server) return;
  const path = socketPath(browserName);
  if (!WINDOWS) {
    mkdirSync(runDir(), { recursive: true, mode: 0o700 });
    chmodSync(runDir(), 0o700);
    rmSync(path, { force: true });
  }
  const created = createServer((socket) => {
    const decoder = new LineDecoder();
    socket.on("data", (chunk) => {
      for (const message of decoder.push(chunk)) handleClient(socket, message as Json);
    });
    socket.on("error", () => socket.destroy());
  });
  server = created;
  created.on("error", (err: NodeJS.ErrnoException) => {
    if (server === created) server = null; // watch() tries again
    if (err.code !== "EADDRINUSE") log("cannot listen on", path, err.message);
  });
  created.listen(path, () => {
    if (!WINDOWS) {
      chmodSync(path, 0o600);
      ownSocket = { path, ino: statSync(path).ino };
    }
    log("listening on", path);
  });
}

function watch(): void {
  if (WINDOWS) return listen();
  if (!ownSocket) return;
  let ino: number | null = null;
  try {
    ino = statSync(ownSocket.path).ino;
  } catch {
    // The socket file is gone: the host that took it over has exited.
  }
  if (ino === ownSocket.ino) return;
  if (ino === null) {
    server?.close();
    server = null;
    ownSocket = null;
    listen();
  }
  // Otherwise a newer host holds the path; keep checking until it leaves.
}

function shutdown(): void {
  // Remove the socket only if it is still ours, not a newer host's at the same path.
  try {
    if (ownSocket && statSync(ownSocket.path).ino === ownSocket.ino) rmSync(ownSocket.path, { force: true });
  } catch {
    // Already gone.
  }
  process.exit(0);
}

const decoder = new NativeDecoder();
process.stdin.on("data", (chunk: Buffer) => {
  for (const message of decoder.push(chunk)) {
    if (message && typeof message === "object") handleExtension(message as Json);
  }
});
process.stdin.on("end", shutdown);
process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);
