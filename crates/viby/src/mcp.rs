//! MCP over stdio for a server with one tool: newline-delimited JSON-RPC 2.0.
//!
//! Shared by the browser and agentgrep servers, which differ only in the tool they serve. Each
//! tool call runs on its own thread, as the TypeScript ran them as concurrent promises, so a
//! long browser wait never holds up a `ping`. When stdin closes, calls already asked are still
//! answered before the process exits.

use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

/// A tool call's result: MCP content blocks, and whether it is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": text.into() })],
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            is_error: true,
            ..Self::text(text)
        }
    }

    fn to_json(&self) -> Value {
        let mut v = json!({ "content": self.content });
        if self.is_error {
            v["isError"] = Value::Bool(true);
        }
        v
    }
}

pub struct Tool {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Value,
}

pub struct Server {
    pub name: &'static str,
    pub version: String,
}

type Call = dyn Fn(Value) -> ToolResult + Send + Sync;

/// The reply to one message, or None for a notification. Tool calls are not answered here.
fn answer(message: &Value, server: &Server, tool: &Tool) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params");
    Some(match method {
        "initialize" => json!({
            "id": id,
            "result": {
                "protocolVersion": params.and_then(|p| p.get("protocolVersion")).cloned().unwrap_or(json!("2025-06-18")),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": server.name, "version": server.version },
            }
        }),
        "ping" => json!({ "id": id, "result": {} }),
        "tools/list" => {
            json!({ "id": id, "result": { "tools": [{ "name": tool.name, "description": tool.description, "inputSchema": tool.input_schema }] } })
        }
        "tools/call" => {
            let name = params
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            json!({ "id": id, "error": { "code": -32602, "message": format!("Unknown tool: {name}") } })
        }
        _ => {
            json!({ "id": id, "error": { "code": -32601, "message": format!("Method not found: {method}") } })
        }
    })
}

fn send(out: &Mutex<Box<dyn Write + Send>>, mut message: Value) {
    message
        .as_object_mut()
        .map(|m| m.insert("jsonrpc".into(), json!("2.0")));
    let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

/// Serves `tool` over `input`/`output` until `input` ends, then waits for calls in flight.
pub fn serve_on(
    input: impl BufRead,
    output: Box<dyn Write + Send>,
    server: Server,
    tool: Tool,
    call: Arc<Call>,
) {
    let out = Arc::new(Mutex::new(output));
    let mut in_flight = Vec::new();
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            send(
                &out,
                json!({ "id": null, "error": { "code": -32700, "message": "Parse error" } }),
            );
            continue;
        };
        let is_our_call = message.get("method").and_then(Value::as_str) == Some("tools/call")
            && message.get("id").is_some()
            && message.pointer("/params/name").and_then(Value::as_str) == Some(tool.name);
        if is_our_call {
            let (out, call) = (Arc::clone(&out), Arc::clone(&call));
            let id = message["id"].clone();
            let arguments = message
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            in_flight.push(std::thread::spawn(move || {
                send(
                    &out,
                    json!({ "id": id, "result": call(arguments).to_json() }),
                )
            }));
            in_flight.retain(|h| !h.is_finished());
        } else if let Some(reply) = answer(&message, &server, &tool) {
            send(&out, reply);
        }
    }
    // Answer what was already asked before exiting; a closed stdin must not drop a reply.
    for handle in in_flight {
        let _ = handle.join();
    }
}

/// Serves `tool` on stdin/stdout. Returns when stdin closes and every call has been answered.
pub fn serve(
    server: Server,
    tool: Tool,
    call: impl Fn(Value) -> ToolResult + Send + Sync + 'static,
) {
    serve_on(
        std::io::stdin().lock(),
        Box::new(std::io::stdout()),
        server,
        tool,
        Arc::new(call),
    );
}
