/**
 * MCP over stdio for a server with one tool: newline-delimited JSON-RPC 2.0.
 *
 * Shared by the browser and agentgrep servers, which differ only in the tool they serve.
 */
import { createInterface } from "node:readline";

type Json = Record<string, any>;
export type ToolResult = { content: Json[]; isError?: boolean };
export type Tool = { name: string; description: string; inputSchema: Json };

function send(message: Json): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
}

async function handle(message: Json, server: { name: string; version: string }, tool: Tool, call: (input: Json) => Promise<ToolResult>): Promise<void> {
  const { id, method, params } = message;
  if (id === undefined) return; // notifications (initialized, cancelled) need no reply
  switch (method) {
    case "initialize":
      return send({
        id,
        result: {
          protocolVersion: params?.protocolVersion ?? "2025-06-18",
          capabilities: { tools: {} },
          serverInfo: server,
        },
      });
    case "ping":
      return send({ id, result: {} });
    case "tools/list":
      return send({ id, result: { tools: [tool] } });
    case "tools/call":
      if (params?.name !== tool.name) return send({ id, error: { code: -32602, message: `Unknown tool: ${params?.name}` } });
      return send({ id, result: await call(params.arguments ?? {}) });
    default:
      return send({ id, error: { code: -32601, message: `Method not found: ${method}` } });
  }
}

/** Serves `tool` on stdin/stdout until stdin closes. Calls run concurrently. */
export function serveTool(server: { name: string; version: string }, tool: Tool, call: (input: Json) => Promise<ToolResult>): void {
  const inFlight = new Set<Promise<void>>();
  const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
  lines.on("line", (line) => {
    if (!line.trim()) return;
    let message: Json;
    try {
      message = JSON.parse(line);
    } catch {
      return send({ id: null, error: { code: -32700, message: "Parse error" } });
    }
    const job = handle(message, server, tool, call).finally(() => inFlight.delete(job));
    inFlight.add(job);
  });
  // Answer what was already asked before exiting; a closed stdin must not drop a reply.
  lines.on("close", () => void Promise.allSettled([...inFlight]).then(() => process.exit(0)));
}
