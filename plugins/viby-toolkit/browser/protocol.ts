/**
 * Shared constants and framing for the browser bridge.
 *
 *   MCP server ⇄ host:       newline-delimited JSON over a unix socket in runDir() (mode 0700),
 *                            or a per-user named pipe on Windows.
 *   host ⇄ extension:        native messaging — u32 little-endian length, then UTF-8 JSON.
 */
import { homedir, userInfo } from "node:os";
import { join } from "node:path";

export const NATIVE_HOST_NAME = "viby_browser";
export const FIREFOX_EXTENSION_ID = "browser@viby-toolkit";
/** Public half of the key that pins the Chromium extension id; the private half was never kept. */
export const CHROMIUM_PUBLIC_KEY =
  "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAyFP02O7ihyoVtbK3tVNmAczpGUoqac65DNzJIzUVNjJDUcFwHcreRhlgXh642qmM8APDbZ7LCf/TCeUQhtYKqyvUp2R4CGvnQcyDFiIrJ+EPUOFrgAuJvBBZor96AtnheJsLnWQFKYkY2aDdkfplLgmgPXqkRnY6tWTekBoqOQBWd1WjBCsiQJY1NSvnKtRDu32ns87sxMRC8/qMUtrIPp5C7KJ1El+dqvHXqO8WcseOWkcR0I2rUiyA4y26xdgE2h9bzjlgylsjgW6DY+c8rSmaBk24l+M0LLKx0mxu593+/3A1W7pdgv10pm3NaUqwaXa4PISmESGo8hPYSqIJJQIDAQAB";
export const CHROMIUM_EXTENSION_ID = "jndceenifmkbbhlpfamnkokbdjildmme";

/** Chrome rejects host→extension messages over 1 MB, so file payloads above this are chunked. */
export const CHUNK_SIZE = 750_000;
export const REQUEST_TIMEOUT_MS = 30_000;

export function browserHome(): string {
  return process.env.VIBY_BROWSER_HOME || join(homedir(), ".viby", "browser");
}

export function runDir(): string {
  return join(browserHome(), "run");
}

/** Names a host can register under: what the extension's detectBrowserName() reports. */
export const BROWSER_NAMES = ["brave", "chrome", "chromium", "edge", "firefox", "unknown"] as const;

export function socketPath(browser: string, platform = process.platform): string {
  const name = browser.replace(/[^a-z0-9-]/gi, "");
  if (platform === "win32") {
    // Pipes share one machine-wide namespace, so the user name keeps two accounts apart.
    const user = userInfo().username.replace(/[^a-z0-9-]/gi, "");
    return `\\\\.\\pipe\\viby-browser-${user}-${name}`;
  }
  return join(runDir(), `${name}.sock`);
}

export function encodeNative(message: unknown): Buffer {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  return Buffer.concat([header, body]);
}

/** Incremental decoder for native-messaging frames. */
export class NativeDecoder {
  private buffer = Buffer.alloc(0);

  push(chunk: Buffer): unknown[] {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    const out: unknown[] = [];
    while (this.buffer.length >= 4) {
      const length = this.buffer.readUInt32LE(0);
      if (this.buffer.length < 4 + length) break;
      const body = this.buffer.subarray(4, 4 + length).toString("utf8");
      this.buffer = this.buffer.subarray(4 + length);
      try {
        out.push(JSON.parse(body));
      } catch {
        // A frame we cannot parse is dropped; the request it carried times out.
      }
    }
    return out;
  }
}

/** Incremental decoder for newline-delimited JSON. */
export class LineDecoder {
  private pending = "";

  push(chunk: Buffer | string): unknown[] {
    this.pending += chunk.toString();
    const lines = this.pending.split("\n");
    this.pending = lines.pop() ?? "";
    const out: unknown[] = [];
    for (const line of lines) {
      if (!line.trim()) continue;
      try {
        out.push(JSON.parse(line));
      } catch {
        out.push({ __invalid: line });
      }
    }
    return out;
  }
}
