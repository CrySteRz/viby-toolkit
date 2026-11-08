//! `viby stop-gate`: blocks a stop once when code was edited after the last check ran, and
//! otherwise prints nothing. Each case writes a synthetic transcript, shaped like Claude Code's
//! JSONL, into a temp dir.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Run the gate on `input`: (exit status, stdout).
fn run(input: &[u8]) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("stop-gate")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn viby stop-gate");
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

fn prompt(text: &str) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": text } })
}

fn tool(name: &str, input: Value) -> Value {
    json!({ "type": "assistant", "message": { "role": "assistant", "content": [
        { "type": "tool_use", "id": "toolu_1", "name": name, "input": input }
    ] } })
}

fn edit(path: &str) -> Value {
    tool(
        "Edit",
        json!({ "file_path": path, "old_string": "a", "new_string": "b" }),
    )
}

fn bash(command: &str) -> Value {
    tool("Bash", json!({ "command": command }))
}

fn result() -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": [
        { "type": "tool_result", "tool_use_id": "toolu_1", "content": "ok" }
    ] } })
}

fn write_transcript(dir: &Path, lines: &[Value]) -> String {
    let path = dir.join("session.jsonl");
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, text).unwrap();
    path.to_str().unwrap().to_string()
}

/// The gate's stdout for a Stop on `lines`.
fn stop(lines: &[Value]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), lines);
    let input = json!({
        "hook_event_name": "Stop",
        "stop_hook_active": false,
        "transcript_path": transcript,
        "cwd": "/repo",
    });
    let (status, out) = run(input.to_string().as_bytes());
    assert_eq!(status, Some(0));
    out
}

/// The parsed block, failing the test unless the output is one with decision "block".
fn block_reason(out: &str) -> String {
    let v: Value = serde_json::from_str(out.trim()).expect("block output is JSON");
    assert_eq!(v["decision"], "block", "{out}");
    v["reason"].as_str().expect("reason").to_string()
}

#[test]
fn blocks_an_edit_after_the_last_check() {
    let out = stop(&[
        prompt("fix it"),
        edit("/repo/src/a.rs"),
        result(),
        bash("cargo test"),
        result(),
        edit("/repo/src/b.rs"),
        result(),
    ]);
    let reason = block_reason(&out);
    assert!(
        reason.starts_with("You changed src/b.rs after the last check ran."),
        "{reason}"
    );
    assert!(!reason.contains("a.rs"), "{reason}");
}

#[test]
fn blocks_edits_with_no_check_at_all_and_names_three_files() {
    let out = stop(&[
        prompt("fix it"),
        edit("/repo/a.rs"),
        edit("/repo/b.rs"),
        bash("git status"),
        edit("/repo/a.rs"),
        tool("Write", json!({ "file_path": "/repo/c.ts", "content": "" })),
        tool(
            "NotebookEdit",
            json!({ "notebook_path": "/repo/d.ipynb", "new_source": "" }),
        ),
    ]);
    let reason = block_reason(&out);
    assert!(
        reason.starts_with("You changed a.rs, b.rs, c.ts and 1 more after the last check ran."),
        "{reason}"
    );
    assert!(
        reason.contains("say plainly why no check applies"),
        "{reason}"
    );
}

#[test]
fn a_check_after_the_last_edit_lets_it_stop() {
    let out = stop(&[
        prompt("fix it"),
        edit("/repo/src/a.rs"),
        result(),
        bash("cd /repo/rust && cargo xtask check 2>&1 | tail -5"),
        result(),
    ]);
    assert_eq!(out, "");
}

#[test]
fn no_edits_lets_it_stop() {
    assert_eq!(stop(&[prompt("what is this?"), bash("ls"), result()]), "");
    assert_eq!(stop(&[]), "");
}

#[test]
fn stop_hook_active_never_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let transcript = write_transcript(dir.path(), &[prompt("fix"), edit("/repo/a.rs")]);
    let input = json!({
        "hook_event_name": "Stop",
        "stop_hook_active": true,
        "transcript_path": transcript,
    });
    assert_eq!(run(input.to_string().as_bytes()), (Some(0), String::new()));
}

#[test]
fn doc_only_edits_let_it_stop() {
    let out = stop(&[
        prompt("update the docs"),
        edit("/repo/README.md"),
        edit("/repo/notes.TXT"),
        edit("/repo/guide.mdx"),
        edit("/repo/index.rst"),
        edit("/repo/docs/plan.yaml"),
    ]);
    assert_eq!(out, "");
}

#[test]
fn a_malformed_transcript_line_fails_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, format!("{}\n{{not json\n", edit("/repo/a.rs"))).unwrap();
    let input = json!({ "stop_hook_active": false, "transcript_path": path });
    assert_eq!(run(input.to_string().as_bytes()), (Some(0), String::new()));
}

#[test]
fn a_missing_transcript_or_bad_input_fails_open() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("gone.jsonl");
    let inputs = [
        json!({ "stop_hook_active": false, "transcript_path": missing }).to_string(),
        json!({ "stop_hook_active": false }).to_string(),
        json!({ "transcript_path": 7 }).to_string(),
        json!({ "transcript_path": dir.path() }).to_string(),
        "not json".to_string(),
        String::new(),
    ];
    for input in inputs {
        assert_eq!(run(input.as_bytes()), (Some(0), String::new()), "{input}");
    }
}

#[test]
fn edits_from_an_earlier_turn_do_not_count() {
    let out = stop(&[
        prompt("fix it"),
        edit("/repo/a.rs"),
        result(),
        prompt("thanks, what does a.rs do?"),
        bash("cat /repo/a.rs"),
        result(),
    ]);
    assert_eq!(out, "");
}

#[test]
fn subagent_edits_in_the_main_transcript_do_not_count_for_stop() {
    let mut sidechain = edit("/repo/a.rs");
    sidechain["isSidechain"] = json!(true);
    assert_eq!(stop(&[prompt("look"), sidechain]), "");
}

#[test]
fn subagent_stop_reads_the_agent_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.jsonl");
    std::fs::write(&main, format!("{}\n", prompt("go"))).unwrap();
    let agent = dir.path().join("agent.jsonl");
    let mut line = edit("/repo/a.rs");
    line["isSidechain"] = json!(true);
    std::fs::write(&agent, format!("{}\n{line}\n", prompt("task"))).unwrap();
    let input = json!({
        "hook_event_name": "SubagentStop",
        "stop_hook_active": false,
        "transcript_path": main,
        "agent_transcript_path": agent,
        "cwd": "/repo",
    });
    let (status, out) = run(input.to_string().as_bytes());
    assert_eq!(status, Some(0));
    assert!(block_reason(&out).starts_with("You changed a.rs "));
    // Without its own transcript, a subagent is not judged on the parent's.
    let input = json!({
        "hook_event_name": "SubagentStop",
        "stop_hook_active": false,
        "transcript_path": agent,
    });
    assert_eq!(run(input.to_string().as_bytes()), (Some(0), String::new()));
}

#[test]
fn recognises_the_usual_checks() {
    let checks = [
        "cargo test -p viby",
        "cargo +nightly clippy --all-targets",
        "cargo build --release",
        "go vet ./...",
        "npm test",
        "pnpm run typecheck",
        "yarn test:unit",
        "bun run lint",
        "pnpm vitest run",
        "npx tsc --noEmit",
        "uv run pytest -q tests/test_a.py",
        "python3 -m pytest",
        "python -m unittest discover",
        "CI=1 ./node_modules/.bin/jest src",
        "ruff check .",
        "mypy src",
        "eslint .",
        "make test",
        "./gradlew test",
        "mvn -q verify",
        "dotnet test",
        "swift build",
        "xcodebuild -scheme App test",
        "ctest --output-on-failure",
        "vendor/bin/phpunit",
        "bundle exec rspec",
        "mix test",
        "timeout 600 cargo test",
        "cd /repo && git status && go test ./... | tail",
    ];
    for command in checks {
        let out = stop(&[prompt("fix"), edit("/repo/a.rs"), bash(command)]);
        assert_eq!(out, "", "not taken as a check: {command}");
    }
}

#[test]
fn other_commands_are_not_checks() {
    let others = [
        "git status",
        "echo cargo test",
        "cat Makefile",
        "ls tests",
        "grep -rn pytest .",
        "npm install",
        "cargo fmt",
        "go mod tidy",
        "python3 script.py",
    ];
    for command in others {
        let out = stop(&[prompt("fix"), edit("/repo/a.rs"), bash(command)]);
        block_reason(&out);
    }
}

#[test]
fn a_check_before_any_edit_does_not_cover_it() {
    let out = stop(&[
        prompt("fix it"),
        bash("cargo test"),
        result(),
        tool(
            "MultiEdit",
            json!({ "file_path": "/elsewhere/x.go", "edits": [] }),
        ),
    ]);
    // Outside cwd the path is shown whole.
    assert!(block_reason(&out).starts_with("You changed /elsewhere/x.go after"));
}
