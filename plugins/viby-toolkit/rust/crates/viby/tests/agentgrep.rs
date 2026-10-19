//! Tests for the agentgrep MCP server: how calls become agentgrep arguments, what the session
//! remembers from results, and the context it hands agentgrep. agentgrep is linked in, so these
//! run the real thing over small temp repos, and the live checks over a copy of the browser
//! plugin (the 14 steps run against the TypeScript server when it shipped in 2.27.0).

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use viby::agentgrep::{
    build_search, clip, input_schema, shown_in, Search, Session, Shown, TOOL_DESCRIPTION,
};

/// A folder of two TypeScript files and one Rust file, under a visible name.
fn repo() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let repo = dir.path().join("repo");
    fs::create_dir(&repo).expect("repo dir");
    // Several callers of about ten lines: full regions that agentgrep can shorten on a repeat.
    let callers = |file: &str| -> String {
        (0..6)
            .map(|n| {
                format!(
                    "export function {file}Caller{n}(step: number) {{\n  if (step === {n}) {{\n    log(\"{file} {n}\");\n  }}\n  const reply = bridgeRequest(String(step));\n  if (!reply) {{\n    log(\"empty\");\n  }}\n  return reply;\n}}\n\n"
                )
            })
            .collect()
    };
    fs::write(
        repo.join("actions.ts"),
        format!(
            "export function bridgeRequest(action: string): string {{\n  return `bridge:${{action}}`;\n}}\n\n{}",
            callers("actions")
        ),
    )
    .expect("actions.ts");
    fs::write(
        repo.join("server.ts"),
        format!(
            "import {{ bridgeRequest }} from \"./actions\";\n\n{}export const flag = \"--experimental\";\n",
            callers("server")
        ),
    )
    .expect("server.ts");
    fs::write(
        repo.join("bridge.rs"),
        "pub fn bridge_request(action: &str) -> String {\n    format!(\"bridge:{action}\")\n}\n",
    )
    .expect("bridge.rs");
    (dir, repo)
}

/// agentgrep 0.1.7 trace output, trimmed from a real run (`trace subject:bridgeRequest relation:called_from`).
const TRACE: &str = "query parameters:
  subject: bridgeRequest
  relation: called_from

top results: 2 files, 2 regions
best answer likely in actions.ts

1. actions.ts
   role: generic
   why:
     - exact subject match or symbol hit
   structure:
     - function bridgeRequest @ 169-320 (152 lines)
     - function stepCommands @ 142-155 (14 lines)
   regions:
     - bridgeRequest @ 169-320 (152 lines)
       kind: callsite
       snippet:
         export function bridgeRequest(action: Action, input: Json, tabId: number | undefined): [string, Json] {
       why:
         - exact subject match
     - stepCommands @ 142-155 (14 lines)
       kind: callsite
       full region:
         function stepCommands(steps: unknown) {
         2. not a header: indented body text
         }
       why:
         - exact subject match

2. server.ts
   role: generic
   structure:
     - function callTool @ 348-392 (45 lines)
   regions:
     - callTool @ 360-361 (2 lines)
       kind: callsite
       full region:
         const [bridgeAction, params] = bridgeRequest(action, input, tabId);
";

/// One tool call: whether it is an error, and its text.
fn call(session: &Session, input: Value) -> (bool, String) {
    let result = session.call_tool(input);
    let text = result
        .content
        .iter()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    (result.is_error, text)
}

fn trace_input(root: &Path) -> Value {
    json!({ "mode": "trace", "terms": ["subject:bridgeRequest", "relation:called_from"], "path": root })
}

fn search_args(input: Value, context: Option<&str>) -> Result<Search, String> {
    build_search(&input, Path::new("/r"), context.map(Path::new))
}

#[test]
fn cli_options_first_then_dashes_so_a_query_that_starts_with_dash_stays_a_query() {
    let Ok(Search::Grep(grep)) = search_args(
        json!({ "mode": "grep", "query": "--force", "regex": true, "glob": "*.ts" }),
        None,
    ) else {
        panic!("a grep");
    };
    assert_eq!(grep.query, "--force");
    assert!(grep.regex);
    assert_eq!(grep.glob.as_deref(), Some("*.ts"));
    assert_eq!(grep.path.as_deref(), Some("/r"));

    let Ok(Search::Find(find)) = search_args(
        json!({ "mode": "find", "query": "native  host", "max_files": 3 }),
        None,
    ) else {
        panic!("a find");
    };
    assert_eq!(find.query_parts, ["native", "host"]);
    assert_eq!(find.max_files, 3);

    let Ok(Search::Outline(outline)) = search_args(
        json!({ "mode": "outline", "file": "a.ts", "glob": "*.ts", "paths_only": true }),
        Some("/c.json"),
    ) else {
        panic!("an outline");
    };
    assert_eq!(outline.file, "a.ts");
    assert_eq!(
        outline.context_json, None,
        "outline takes none of the search filters, and no context"
    );

    let Ok(Search::Trace(trace)) = search_args(
        json!({ "mode": "trace", "terms": ["subject:x", "relation:defined"], "max_regions": 2 }),
        Some("/c.json"),
    ) else {
        panic!("a trace");
    };
    assert_eq!(trace.terms, ["subject:x", "relation:defined"]);
    assert_eq!((trace.max_files, trace.max_regions), (5, 2));
    assert_eq!(trace.context_json.as_deref(), Some("/c.json"));

    for (input, message) in [
        (json!({ "mode": "grep" }), "query is required"),
        (json!({ "mode": "outline" }), "file is required"),
        (
            json!({ "mode": "trace", "terms": [] }),
            "terms are required",
        ),
    ] {
        match search_args(input, None) {
            Err(e) => assert!(e.contains(message), "{e}"),
            Ok(_) => panic!("{message}"),
        }
    }
}

#[test]
fn memory_reads_which_structures_and_full_regions_a_trace_showed_not_snippets() {
    assert_eq!(
        shown_in("trace", TRACE),
        [
            Shown {
                file: "actions.ts".into(),
                structure: true,
                regions: vec![(142, 155)]
            },
            Shown {
                file: "server.ts".into(),
                structure: true,
                regions: vec![(360, 361)]
            },
        ]
    );
    assert_eq!(
        shown_in("outline", "file: src/a.ts\nlanguage: typescript\n"),
        [Shown {
            file: "src/a.ts".into(),
            structure: true,
            regions: vec![]
        }]
    );
    assert!(
        shown_in("grep", TRACE).is_empty(),
        "grep results are not remembered"
    );
    assert!(
        shown_in("trace", "something else entirely").is_empty(),
        "unknown output yields nothing, never a guess"
    );
}

#[test]
fn memory_the_context_covers_what_was_shown_under_this_root_and_forgets_a_file_once_it_changes() {
    let (dir, repo) = repo();
    let session = Session::new();
    session.remember(&repo, &shown_in("trace", TRACE));
    session.remember(
        &repo,
        &[Shown {
            file: "gone.ts".into(),
            structure: true,
            regions: vec![],
        }],
    );
    let context = session.harness_context(&repo).expect("a context");
    let files = |context: &Value| -> Vec<Value> {
        context["known_files"]
            .as_array()
            .expect("array")
            .iter()
            .map(|e| e["path"].clone())
            .collect()
    };
    assert_eq!(
        files(&context),
        ["actions.ts", "server.ts"],
        "a file that does not exist is not remembered"
    );
    let regions: Vec<(Value, Value, Value)> = context["known_regions"]
        .as_array()
        .expect("array")
        .iter()
        .map(|r| {
            (
                r["path"].clone(),
                r["start_line"].clone(),
                r["end_line"].clone(),
            )
        })
        .collect();
    assert_eq!(
        regions,
        [
            (json!("actions.ts"), json!(142), json!(155)),
            (json!("server.ts"), json!(360), json!(361))
        ]
    );
    let region = &context["known_regions"][0];
    for key in [
        "body_confidence",
        "current_version_confidence",
        "prune_confidence",
    ] {
        assert!(
            region[key].as_f64().is_some_and(|c| c >= 0.7),
            "{key} clears agentgrep's prune threshold"
        );
    }
    assert!(
        session
            .harness_context(&dir.path().join("elsewhere"))
            .is_none(),
        "nothing seen under another root"
    );

    touch(&repo.join("server.ts"));
    let context = session.harness_context(&repo).expect("a context");
    assert_eq!(
        files(&context),
        ["actions.ts"],
        "an edited file comes back in full"
    );
}

#[test]
fn server_a_repeated_trace_sends_agentgrep_what_this_session_already_saw_force_and_outline_never_do(
) {
    let (_dir, repo) = repo();
    let session = Session::new();
    let (error, first) = call(&session, trace_input(&repo));
    assert!(!error, "{first}");
    assert!(first.contains("full region:"), "{first}");
    assert!(!first.contains("harness context"), "nothing seen yet");

    let (_, again) = call(&session, trace_input(&repo));
    assert!(
        again.contains("harness context"),
        "a repeat is shortened: {again}"
    );
    assert!(
        again.len() < first.len(),
        "{} vs {}\n{first}\n----\n{again}",
        again.len(),
        first.len()
    );

    let mut forced = trace_input(&repo);
    forced["force"] = json!(true);
    let (_, forced) = call(&session, forced);
    assert_eq!(forced, first, "force asks for everything in full");

    let (_, outline) = call(
        &session,
        json!({ "mode": "outline", "file": "actions.ts", "path": &repo }),
    );
    assert!(
        outline.contains("structure:") && !outline.contains("harness context"),
        "an outline is an explicit ask: {outline}"
    );
}

#[test]
fn server_agentgreps_own_errors_come_back_as_tool_errors_a_missing_folder_never_runs_it() {
    let (_dir, repo) = repo();
    let session = Session::new();
    let (error, bad) = call(
        &session,
        json!({ "mode": "trace", "terms": ["x"], "path": &repo }),
    );
    assert!(error);
    assert!(bad.contains("expected key:value"), "{bad}");
    assert!(
        bad.starts_with("agentgrep trace failed: error: invalid smart DSL term: x"),
        "{bad}"
    );

    let (error, missing_file) = call(
        &session,
        json!({ "mode": "outline", "file": "nope.ts", "path": &repo }),
    );
    assert!(
        error && missing_file.contains("file not found"),
        "{missing_file}"
    );

    let gone = repo.join("nope");
    let (error, missing) = call(
        &session,
        json!({ "mode": "grep", "query": "x", "path": &gone }),
    );
    assert!(error);
    assert_eq!(missing, format!("No such folder: {}", gone.display()));

    let (error, unknown) = call(&session, json!({ "mode": "read" }));
    assert!(error);
    assert_eq!(
        unknown,
        "Unknown mode: read. Use one of grep, find, outline, trace."
    );
}

#[test]
fn built_in_agentgrep_needs_no_binary_on_the_path() {
    let (_dir, repo) = repo();
    let mut child = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("agentgrep-server")
        .env_clear()
        .env("PATH", "")
        .env("VIBY_AGENTGREP_BIN", "/nonexistent/agentgrep")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn viby");
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "agentgrep",
            "arguments": { "mode": "grep", "query": "--experimental", "path": &repo } } }),
    ];
    let mut stdin = child.stdin.take().expect("stdin");
    for request in requests {
        writeln!(stdin, "{request}").expect("write");
    }
    drop(stdin);
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success());
    let replies: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("json line"))
        .collect();
    let reply = |id: i64| replies.iter().find(|r| r["id"] == id).expect("a reply");
    assert_eq!(reply(1)["result"]["serverInfo"]["name"], "viby-agentgrep");
    let tool = &reply(2)["result"]["tools"][0];
    assert_eq!(tool["name"], "agentgrep");
    assert_eq!(tool["description"], TOOL_DESCRIPTION);
    assert_eq!(tool["inputSchema"], input_schema());
    let text = reply(3)["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.starts_with("query: --experimental\nmatches: 1 in 1 files"),
        "{text}"
    );
    assert!(reply(3)["result"].get("isError").is_none());
}

#[test]
fn clip_cuts_at_sixty_thousand_characters_and_says_how_to_narrow() {
    assert_eq!(clip("short"), "short");
    let long = "a".repeat(60_010);
    let clipped = clip(&long);
    assert!(clipped.starts_with(&"a".repeat(60_000)));
    assert!(clipped
        .ends_with("\n… [truncated 10 characters; narrow with glob, type, path or max_files]"));
}

/// Copies `from` into `to`, recursively.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dir");
    for entry in fs::read_dir(from).expect("read dir").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// Moves a file's mtime five seconds ahead, as an edit would.
fn touch(path: &Path) {
    let later = SystemTime::now() + Duration::from_secs(5);
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(later))
        .expect("touch");
}

/// The 14 live steps of the TypeScript server's 2.27.0 run, in order: a repeated trace is shorter,
/// force is full, touched files are full again, an outline is never shortened, and the errors read
/// clearly. The corpus was the TypeScript browser plugin; since 3.0.0 it is its Rust port.
#[test]
fn live_steps_over_a_copy_of_the_browser_sources() {
    let dir = tempfile::tempdir().expect("temp dir");
    let repo = dir.path().join("repo");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/browser");
    copy_dir(&source, &repo);
    let session = Session::new();
    let trace = json!({ "mode": "trace", "terms": ["subject:call_host", "relation:called_from"], "path": &repo });
    let ok = |name: &str, input: Value| {
        let (error, text) = call(&session, input);
        assert!(!error, "{name}: {text}");
        text
    };
    let fails = |name: &str, input: Value, expect: &str| {
        let (error, text) = call(&session, input);
        assert!(error, "{name}: {text}");
        assert!(text.contains(expect), "{name}: {text}");
    };

    let text = ok(
        "grep",
        json!({ "mode": "grep", "query": "call_host", "path": &repo }),
    );
    assert!(
        regex::Regex::new(r"matches: \d+ in \d+ files")
            .expect("regex")
            .is_match(&text),
        "{text}"
    );
    let text = ok(
        "grep: a query starting with - stays a query",
        json!({ "mode": "grep", "query": "--experimental", "path": &repo }),
    );
    assert!(text.contains("query: --experimental"), "{text}");
    let text = ok(
        "find",
        json!({ "mode": "find", "query": "native messaging host", "path": &repo }),
    );
    assert!(text.contains("top files"), "{text}");

    let first = ok("trace first", trace.clone());
    assert!(first.contains("full region:") && !first.contains("harness context"));
    let again = ok("trace again: shortened", trace.clone());
    assert!(again.contains("compressed repeated region from harness context"));
    assert!(again.len() < first.len());
    let mut forced = trace.clone();
    forced["force"] = json!(true);
    let forced = ok("trace again with force: full", forced);
    assert!(!forced.contains("harness context"));
    assert_eq!(forced.len(), first.len());

    touch(&repo.join("server.rs"));
    touch(&repo.join("host.rs"));
    let after = ok("trace after the files changed: full again", trace);
    assert!(!after.contains("harness context"));
    assert_eq!(after.len(), first.len());

    let outline = json!({ "mode": "outline", "file": "host.rs", "path": &repo });
    let outline_first = ok("outline is never shortened", outline.clone());
    assert!(outline_first.contains("structure:") && !outline_first.contains("harness context"));
    let outline_again = ok("outline again: still full", outline);
    assert_eq!(outline_again.len(), outline_first.len());

    let text = ok(
        "trace in a file whose outline was seen: structure reduced",
        json!({ "mode": "trace", "terms": ["subject:handle_client", "relation:defined"], "path": &repo }),
    );
    assert!(
        text.contains("reduced repeated structure from harness context"),
        "{text}"
    );

    fails(
        "bad trace DSL is a clear error",
        json!({ "mode": "trace", "terms": ["handle_client"], "path": &repo }),
        "expected key:value",
    );
    fails(
        "missing file",
        json!({ "mode": "outline", "file": "nope.rs", "path": &repo }),
        "file not found",
    );
    fails(
        "missing folder",
        json!({ "mode": "grep", "query": "x", "path": repo.join("nope") }),
        "No such folder",
    );
    fails("unknown mode", json!({ "mode": "read" }), "Unknown mode");
}
