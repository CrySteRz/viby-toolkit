//! Smoke tests for the viby-toolkit statusline (ported from tests/statusline.test.ts).
//!
//! Payload shapes follow the documented statusLine stdin contract, including the
//! documented null cases: `current_usage` is null before the first API call and again
//! after /compact, `used_percentage` may be null early, and `rate_limits` appears only
//! for Pro/Max subscribers after the first API response.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn full() -> Value {
    json!({
        "model": { "display_name": "Opus 5", "id": "claude-opus-5" },
        "cost": { "total_cost_usd": 1.2345 },
        "context_window": {
            "context_window_size": 1000000,
            "used_percentage": 43,
            "current_usage": {
                "input_tokens": 1200,
                "cache_creation_input_tokens": 800,
                "cache_read_input_tokens": 18000,
                "output_tokens": 400,
            },
        },
        "rate_limits": {
            "five_hour": { "used_percentage": 23.5 },
            "seven_day": { "used_percentage": 81.2 },
        },
        "exceeds_200k_tokens": false,
    })
}

/// The object that holds the last key of `path`.
fn parent<'a>(d: &'a mut Value, path: &[&str]) -> &'a mut serde_json::Map<String, Value> {
    let mut node = d;
    for k in &path[..path.len() - 1] {
        node = &mut node[*k];
    }
    node.as_object_mut().expect("path leads through objects")
}

/// `d` with the key at `path` removed.
fn drop(mut d: Value, path: &[&str]) -> Value {
    parent(&mut d, path).remove(path[path.len() - 1]);
    d
}

/// `d` with the key at `path` set to null.
fn nullify(mut d: Value, path: &[&str]) -> Value {
    parent(&mut d, path).insert(path[path.len() - 1].into(), Value::Null);
    d
}

/// `d` with the key at `path` set to `value`.
fn set(mut d: Value, path: &[&str], value: Value) -> Value {
    parent(&mut d, path).insert(path[path.len() - 1].into(), value);
    d
}

/// Spawn `viby statusline` with `input` on stdin: (exit status, stdout).
fn run_raw(input: &[u8]) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("statusline")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn viby statusline");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn run_json(payload: &Value) -> String {
    run_raw(payload.to_string().as_bytes()).1
}

fn check(payload: &Value, must: &[&str], must_not: &[&str]) {
    let (status, stdout) = run_raw(payload.to_string().as_bytes());
    assert_eq!(status, Some(0), "expected exit 0");
    let lines = stdout.matches('\n').count();
    assert_eq!(lines, 1, "expected exactly one line, got {lines}");
    assert_ne!(stdout.trim(), "", "empty output");
    for m in must {
        assert!(stdout.contains(m), "missing {m:?} in {stdout:?}");
    }
    for m in must_not {
        assert!(
            !stdout.contains(m),
            "should not contain {m:?} in {stdout:?}"
        );
    }
}

#[test]
fn full_payload() {
    check(
        &full(),
        &[
            "Opus 5",
            "ctx 43%",
            "cache 90%",
            "5h 24%",
            "7d 81%",
            "$1.23",
        ],
        &[],
    );
}

#[test]
fn no_rate_limits_non_subscriber() {
    check(
        &drop(full(), &["rate_limits"]),
        &["ctx 43%"],
        &["5h ", "7d "],
    );
}

#[test]
fn five_hour_only() {
    check(
        &drop(full(), &["rate_limits", "seven_day"]),
        &["5h 24%"],
        &["7d "],
    );
}

#[test]
fn current_usage_null_pre_first_call() {
    check(
        &nullify(full(), &["context_window", "current_usage"]),
        &["ctx 43%"],
        &["cache "],
    );
}

#[test]
fn used_percentage_null_usage_present() {
    check(
        &nullify(full(), &["context_window", "used_percentage"]),
        &["ctx 2%"],
        &[],
    );
}

#[test]
fn used_percentage_null_and_no_size() {
    let d = nullify(full(), &["context_window", "used_percentage"]);
    check(
        &drop(d, &["context_window", "context_window_size"]),
        &[],
        &["ctx "],
    );
}

#[test]
fn no_context_window_at_all() {
    check(
        &drop(full(), &["context_window"]),
        &["Opus 5"],
        &["ctx ", "cache "],
    );
}

#[test]
fn zero_cost_omitted() {
    check(
        &set(full(), &["cost"], json!({ "total_cost_usd": 0 })),
        &["ctx 43%"],
        &["$"],
    );
}

#[test]
fn no_cost_key() {
    check(&drop(full(), &["cost"]), &["ctx 43%"], &["$"]);
}

#[test]
fn model_id_fallback() {
    check(
        &json!({ "model": { "id": "claude-sonnet-5" } }),
        &["claude-sonnet-5"],
        &["ctx "],
    );
}

#[test]
fn empty_object() {
    check(&json!({}), &["claude"], &["ctx ", "$"]);
}

#[test]
fn red_band_at_high_ctx() {
    let d = set(full(), &["context_window", "used_percentage"], json!(91));
    check(&d, &["\x1b[31mctx 91%"], &[]);
}

#[test]
fn green_band_at_low_ctx() {
    let d = set(full(), &["context_window", "used_percentage"], json!(12));
    check(&d, &["\x1b[32mctx 12%"], &[]);
}

// --- beyond the TypeScript table: the exact bytes, and every way the input can be bad

#[test]
fn full_payload_is_byte_exact() {
    let expected = "\x1b[2mOpus 5\x1b[0m · \x1b[32mctx 43%\x1b[0m · \x1b[2mcache 90%\x1b[0m \
                    · \x1b[32m5h 24%\x1b[0m · \x1b[31m7d 81%\x1b[0m · \x1b[2m$1.23\x1b[0m\n";
    assert_eq!(run_json(&full()), expected);
}

#[test]
fn unreadable_input_prints_the_minimal_line_and_exits_0() {
    let inputs: [&[u8]; 6] = [
        b"",
        b"   ",
        b"{",
        b"not json",
        b"{} trailing",
        b"\xef\xbb\xbf{}",
    ];
    for input in inputs {
        let (status, out) = run_raw(input);
        assert_eq!(status, Some(0), "{input:?}");
        assert_eq!(out, "viby-toolkit\n", "{input:?}");
    }
}

#[test]
fn json_that_is_not_an_object_is_an_empty_payload() {
    for input in ["null", "[]", "42", "\"text\"", "true"] {
        let (status, out) = run_raw(input.as_bytes());
        assert_eq!(status, Some(0), "{input}");
        assert_eq!(out, "\x1b[2mclaude\x1b[0m\n", "{input}");
    }
}

#[test]
fn nulls_and_wrong_types_are_skipped() {
    let d = json!({
        "model": null,
        "context_window": { "used_percentage": null, "current_usage": "x", "context_window_size": "9" },
        "rate_limits": { "five_hour": null, "seven_day": [] },
        "cost": { "total_cost_usd": "1.5" },
    });
    assert_eq!(run_json(&d), "\x1b[2mclaude\x1b[0m\n");
}

#[test]
fn percentages_are_clamped_rounded_and_parsed_like_javascript() {
    let render = |pct: Value| run_json(&json!({ "context_window": { "used_percentage": pct } }));
    assert!(render(json!(-50)).contains("\x1b[32mctx 0%"));
    assert!(render(json!(250)).contains("\x1b[31mctx 100%"));
    assert!(render(json!(59.5)).contains("\x1b[33mctx 60%"));
    assert!(render(json!("42")).contains("ctx 42%"));
    assert!(render(json!(" 0x10 ")).contains("ctx 16%"));
    assert!(render(json!("")).contains("ctx 0%"));
    assert!(!render(json!("abc")).contains("ctx "));
    assert!(!render(json!(true)).contains("ctx "));
}

#[test]
fn cost_rounds_like_to_fixed() {
    let render = |cost: f64| run_json(&json!({ "cost": { "total_cost_usd": cost } }));
    assert!(render(0.125).contains("$0.13"));
    assert!(render(0.375).contains("$0.38"));
    assert!(render(1.005).contains("$1.00"));
    assert!(render(0.004).contains("$0.00"));
    assert!(!render(-1.0).contains('$'));
}
