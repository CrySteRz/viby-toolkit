//! The MCP stdio loop: the handshake, one tool, concurrent calls, and replies kept after stdin closes.

use serde_json::{json, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use viby::mcp::{serve_on, Server, Tool, ToolResult};

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run(lines: &[Value], raw_extra: &str) -> Vec<Value> {
    let mut input: String = lines.iter().map(|l| format!("{l}\n")).collect();
    input.push_str(raw_extra);
    let sink = Sink::default();
    let tool = Tool {
        name: "echo",
        description: "echoes".into(),
        input_schema: json!({ "type": "object" }),
    };
    serve_on(
        input.as_bytes(),
        Box::new(sink.clone()),
        Server {
            name: "test",
            version: "1.0.0".into(),
        },
        tool,
        Arc::new(|args: Value| {
            if let Some(ms) = args.get("sleep").and_then(Value::as_u64) {
                std::thread::sleep(Duration::from_millis(ms));
            }
            if args.get("fail").is_some() {
                ToolResult::error("no")
            } else {
                ToolResult::text(args["say"].as_str().unwrap_or("").to_string())
            }
        }),
    );
    let bytes = sink.0.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn by_id(replies: &[Value], id: i64) -> &Value {
    replies
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap_or_else(|| panic!("no reply for id {id}: {replies:?}"))
}

#[test]
fn handshake_list_and_unknowns() {
    let replies = run(
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "other" } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" }),
            json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" }),
        ],
        "{not json\n",
    );
    assert_eq!(
        replies.len(),
        6,
        "a notification gets no reply; bad JSON gets a parse error"
    );
    let init = by_id(&replies, 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(
        init["result"]["serverInfo"],
        json!({ "name": "test", "version": "1.0.0" })
    );
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(by_id(&replies, 2)["result"]["tools"][0]["name"], "echo");
    assert_eq!(by_id(&replies, 3)["error"]["code"], -32602);
    assert_eq!(by_id(&replies, 4)["error"]["code"], -32601);
    assert_eq!(by_id(&replies, 5)["result"], json!({}));
    assert!(replies
        .iter()
        .any(|r| r["error"]["code"] == -32700 && r["id"].is_null()));
}

#[test]
fn calls_run_concurrently_and_are_answered_after_stdin_closes() {
    let replies = run(
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "echo", "arguments": { "say": "slow", "sleep": 300 } } }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "echo", "arguments": { "say": "fast" } } }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "echo", "arguments": { "fail": true } } }),
        ],
        "",
    );
    assert_eq!(
        by_id(&replies, 1)["result"]["content"][0]["text"],
        "slow",
        "the slow call still answered after stdin ended"
    );
    let order: Vec<i64> = replies.iter().map(|r| r["id"].as_i64().unwrap()).collect();
    assert!(
        order.iter().position(|&i| i == 2) < order.iter().position(|&i| i == 1),
        "the fast call did not wait for the slow one: {order:?}"
    );
    assert_eq!(by_id(&replies, 3)["result"]["isError"], true);
    assert!(by_id(&replies, 2)["result"].get("isError").is_none());
}
