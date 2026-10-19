//! Contract tests for the browser bridge (crates/viby/src/browser).
//!
//! No browser is involved: the native host runs as a real child process (`viby browser-host`)
//! with this test playing the extension on its stdin/stdout, and the MCP server's tool calls go
//! to a fake host on a real unix socket. A live run against Brave is a separate, manual step.
//!
//! Every test sets HOME, VIBY_BROWSER_HOME and VIBY_BROWSER_APP_ROOTS to fresh temporary
//! folders, so nothing outside them is touched; the tests run one at a time because the
//! environment and the socket paths are the process's.
#![cfg(unix)]

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use viby::browser::actions::{bridge_request, explain_error, failed_result, format_result};
use viby::browser::install::{
    browser_for_handler, build_extension, chromium_extension_state, chromium_manifest,
    detect_browsers, extension_version, host_binary, host_launcher, install, installed_app_here,
    native_host_manifest, process_env, registry_args, Env, ExtensionState, Family, EXTENSION_FILES,
};
use viby::browser::protocol::{
    encode_native, socket_path, socket_path_for, LineDecoder, NativeDecoder, CHROMIUM_EXTENSION_ID,
    CHROMIUM_PUBLIC_KEY, CHUNK_SIZE, FIREFOX_EXTENSION_ID, NATIVE_HOST_NAME,
};
use viby::browser::server::{chromium_tab_script, BrowserServer};
use viby::mcp::ToolResult;

const VIBY: &str = env!("CARGO_BIN_EXE_viby");

fn extension_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../browser/extension")
}

// ── per-test environment ────────────────────────────────────────────────────────────────

static LOCK: Mutex<()> = Mutex::new(());

/// The temporary folders a test runs in. Dropped last, after the hosts it started.
struct TestEnv {
    home: TempDir,
    fake_home: TempDir,
    apps: TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    fn home(&self) -> &Path {
        self.home.path()
    }
    fn fake_home(&self) -> &Path {
        self.fake_home.path()
    }
    fn apps(&self) -> &Path {
        self.apps.path()
    }
}

fn setup() -> TestEnv {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp = |prefix: &str| {
        tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("temp dir")
    };
    let env = TestEnv {
        home: temp("viby-browser-"),
        fake_home: temp("viby-user-"),
        apps: temp("viby-apps-"),
        _guard: guard,
    };
    std::env::set_var("VIBY_BROWSER_HOME", env.home());
    // Setup registers native hosts under the home folder; never let a test touch the real one.
    std::env::set_var("HOME", env.fake_home());
    // No real browser program counts either: tests add the ones they need here.
    std::env::set_var("VIBY_BROWSER_APP_ROOTS", env.apps());
    std::env::set_var("VIBY_BROWSER_NO_OPEN", "1"); // never open pages on the machine running the tests
    std::env::set_var("VIBY_BROWSER_DEFAULT", ""); // nor depend on its default browser
    env
}

fn server() -> BrowserServer {
    BrowserServer::new(PathBuf::from(VIBY))
}

fn wait_for(check: impl Fn() -> bool, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while !check() {
        assert!(Instant::now() < end, "condition not met in time");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn mode(path: impl AsRef<Path>) -> u32 {
    std::fs::metadata(path).expect("stat").permissions().mode() & 0o777
}

fn re(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("regex")
}

fn text_of(result: &ToolResult) -> String {
    result.content[0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// ── native host ────────────────────────────────────────────────────────────────────────

struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<Value>,
}

impl Host {
    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin
            .write_all(&encode_native(message))
            .expect("write to host");
        stdin.flush().expect("flush");
    }

    fn next(&self) -> Value {
        self.frames
            .recv_timeout(Duration::from_secs(10))
            .expect("a frame from the host")
    }

    /// Frames the host sent that nobody has read.
    fn pending_frames(&self) -> usize {
        std::thread::sleep(Duration::from_millis(100));
        self.frames.try_iter().count()
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Closes stdin, as the browser does, and returns the exit code.
    fn end(&mut self) -> Option<i32> {
        self.stdin = None;
        self.child.wait().ok().and_then(|s| s.code())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_host() -> Host {
    let mut child = Command::new(VIBY)
        .arg("browser-host")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn viby browser-host");
    let mut stdout = child.stdout.take().expect("stdout");
    let (tx, frames) = mpsc::channel();
    std::thread::spawn(move || {
        let mut decoder = NativeDecoder::default();
        let mut buf = vec![0u8; 256 * 1024];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            for frame in decoder.push(&buf[..n]) {
                let _ = tx.send(frame);
            }
        }
    });
    let stdin = child.stdin.take();
    Host {
        child,
        stdin,
        frames,
    }
}

struct Client {
    stream: UnixStream,
    decoder: LineDecoder,
    replies: Vec<Value>,
}

impl Client {
    fn connect(socket: &str) -> Option<Client> {
        let stream = UnixStream::connect(socket).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        Some(Client {
            stream,
            decoder: LineDecoder::default(),
            replies: Vec::new(),
        })
    }

    fn send(&mut self, message: &Value) {
        let _ = self.stream.write_all(format!("{message}\n").as_bytes());
    }

    fn reply(&mut self) -> Result<Value, String> {
        let mut buf = vec![0u8; 64 * 1024];
        while self.replies.is_empty() {
            match self.stream.read(&mut buf) {
                Ok(0) | Err(_) => return Err("connection closed".into()),
                Ok(n) => self.replies.extend(self.decoder.push(&buf[..n])),
            }
        }
        Ok(self.replies.remove(0))
    }
}

/// A host that has said hello, with a client connected to it (not to a previous test's host).
struct Ready {
    client: Client,
    host: Host,
}

impl Ready {
    fn stop(mut self) {
        let _ = self.client.stream.shutdown(std::net::Shutdown::Both);
        self.host.end();
    }
}

fn ready_host() -> Ready {
    let mut host = start_host();
    host.send(&json!({ "type": "hello", "browser": "brave" }));
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        wait_for(|| Path::new(&socket_path("brave")).exists(), 5000);
        if let Some(mut client) = Client::connect(&socket_path("brave")) {
            client.send(&json!({ "id": "ready", "action": "hostInfo" }));
            if let Ok(info) = client.reply() {
                if info["result"]["pid"] == json!(host.pid()) {
                    return Ready { client, host };
                }
            }
        }
        assert!(Instant::now() < end, "host never became ready");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn host_a_host_the_extension_never_greets_opens_no_socket_and_exits_with_its_browser() {
    let _env = setup();
    let mut host = start_host();
    assert_eq!(host.end(), Some(0));
    assert!(!Path::new(&socket_path("brave")).exists());
}

#[test]
fn host_listens_after_hello_on_a_socket_only_this_user_can_open() {
    let _env = setup();
    let mut host = start_host();
    let sock = socket_path("brave");
    host.send(&json!({ "type": "hello", "browser": "brave" }));
    wait_for(|| Path::new(&sock).exists(), 5000);
    wait_for(|| mode(&sock) == 0o600, 5000);
    assert_eq!(mode(Path::new(&sock).parent().expect("run dir")), 0o700);
    host.end();
    assert!(!Path::new(&sock).exists(), "socket removed on exit");
}

#[test]
fn host_routes_a_request_to_the_extension_and_the_reply_back_by_id() {
    let _env = setup();
    let mut ready = ready_host();
    ready
        .client
        .send(&json!({ "id": 7, "action": "ping", "params": {} }));
    let forwarded = ready.host.next();
    assert_eq!(forwarded["action"], "ping");
    let id = forwarded["id"].as_str().expect("string id").to_string();
    assert!(
        re(r"^h\d+_\d+$").is_match(&id),
        "host assigns its own id, so two clients cannot collide"
    );
    ready
        .host
        .send(&json!({ "id": id, "ok": true, "result": { "pong": true } }));
    assert_eq!(
        ready.client.reply().expect("reply"),
        json!({ "id": 7, "ok": true, "result": { "pong": true } })
    );
    ready.stop();
}

#[test]
fn host_when_a_second_host_for_the_same_browser_exits_the_first_takes_the_socket_back() {
    let _env = setup();
    let first = ready_host();
    let second = ready_host(); // took the socket over
    let ask = || {
        let mut c = Client::connect(&socket_path("brave"))?;
        c.send(&json!({ "id": "who", "action": "hostInfo" }));
        c.reply().ok().and_then(|r| r["result"]["pid"].as_u64())
    };
    assert_eq!(ask(), Some(u64::from(second.host.pid())));
    second.stop();
    wait_for(|| Path::new(&socket_path("brave")).exists(), 6000);
    let first_pid = u64::from(first.host.pid());
    let mut pid = None;
    let end = Instant::now() + Duration::from_secs(6);
    while Instant::now() < end && pid != Some(first_pid) {
        pid = ask();
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        pid,
        Some(first_pid),
        "the browser that is still open stays reachable"
    );
    first.stop();
}

#[test]
fn host_refuses_credential_actions_and_anything_off_the_allow_list() {
    let _env = setup();
    let mut ready = ready_host();
    for action in [
        "secureAutoFill",
        "autoLogin",
        "getAuthContext",
        "scout",
        "listDownloads",
    ] {
        ready
            .client
            .send(&json!({ "id": action, "action": action, "params": {} }));
        let reply = ready.client.reply().expect("reply");
        assert_eq!(reply["ok"], false);
        assert!(reply["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not allowed"));
    }
    ready.client.send(&json!({ "id": "x", "params": {} }));
    assert!(ready.client.reply().expect("reply")["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Missing action"));
    assert_eq!(
        ready.host.pending_frames(),
        0,
        "nothing reached the extension"
    );
    ready.stop();
}

#[test]
fn host_uploads_a_small_file_inline_as_a_fill_form_file_field() {
    let env = setup();
    let mut ready = ready_host();
    let file = env.home().join("note.txt");
    std::fs::write(&file, "hello").expect("write");
    ready
        .client
        .send(&json!({ "id": 1, "action": "uploadFile", "params": { "path": file, "tabId": 3 } }));
    let frame = ready.host.next();
    assert_eq!(frame["action"], "fillForm");
    assert_eq!(frame["params"]["tabId"], 3);
    assert!(
        frame["params"].get("path").is_none(),
        "the local path never reaches the page"
    );
    assert_eq!(
        frame["params"]["fields"],
        json!([{ "selector": "input[type=\"file\"]", "file": { "name": "note.txt", "type": "text/plain", "data": "aGVsbG8=" } }])
    );
    ready.stop();
}

#[test]
fn host_chunks_a_file_over_chromes_1_mb_message_cap() {
    let env = setup();
    let mut ready = ready_host();
    let file = env.home().join("big.png");
    std::fs::write(&file, vec![7u8; 1_200_000]).expect("write");
    ready.client.send(
        &json!({ "id": 1, "action": "uploadFile", "params": { "path": file, "selector": "#f" } }),
    );
    let start = ready.host.next();
    assert_eq!(start["type"], "chunk_start");
    use base64::Engine;
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(&file).expect("read"));
    let total = start["totalChunks"].as_u64().expect("totalChunks") as usize;
    let mut parts = vec![String::new(); total];
    for _ in 0..total {
        let chunk = ready.host.next();
        assert_eq!(chunk["type"], "chunk_data");
        let data = chunk["data"].as_str().expect("data");
        assert!(data.len() <= CHUNK_SIZE);
        parts[chunk["chunkIndex"].as_u64().expect("index") as usize] = data.to_string();
    }
    assert_eq!(parts.join(""), encoded);
    let request = ready.host.next();
    assert_eq!(request["action"], "fillForm");
    assert_eq!(
        request["params"]["fields"][0],
        json!({ "selector": "#f", "file": { "name": "big.png", "type": "image/png", "chunkedTransfer": start["transferId"] } })
    );
    ready.stop();
}

#[test]
fn host_a_missing_upload_file_is_an_error_to_the_caller_not_a_crash() {
    let env = setup();
    let mut ready = ready_host();
    ready.client.send(&json!({ "id": 1, "action": "uploadFile", "params": { "path": env.home().join("nope.bin") } }));
    let reply = ready.client.reply().expect("reply");
    assert_eq!(reply["ok"], false);
    assert!(reply["error"]
        .as_str()
        .unwrap_or_default()
        .contains("ENOENT"));
    ready.client.send(&json!({ "id": 2, "action": "hostInfo" }));
    assert_eq!(
        ready.client.reply().expect("reply")["result"]["browser"],
        "brave",
        "host still serving"
    );
    ready.stop();
}

#[test]
fn host_drops_a_file_as_the_extensions_drop_file_file_chunked_the_same_way_as_uploads() {
    let env = setup();
    let mut ready = ready_host();
    let small = env.home().join("pic.png");
    std::fs::write(&small, "png").expect("write");
    ready.client.send(&json!({ "id": 1, "action": "dropFile", "params": { "path": small, "selector": "#editor", "tabId": 3 } }));
    let frame = ready.host.next();
    assert_eq!(
        frame["params"],
        json!({ "selector": "#editor", "tabId": 3, "file": { "name": "pic.png", "type": "image/png", "data": "cG5n" } })
    );
    let big = env.home().join("big.pdf");
    std::fs::write(&big, vec![1u8; 1_200_000]).expect("write");
    ready
        .client
        .send(&json!({ "id": 2, "action": "dropFile", "params": { "path": big } }));
    let start = ready.host.next();
    for _ in 0..start["totalChunks"].as_u64().expect("totalChunks") {
        ready.host.next();
    }
    let request = ready.host.next();
    assert_eq!(request["action"], "dropFile");
    assert_eq!(
        request["params"]["file"],
        json!({ "name": "big.pdf", "type": "application/pdf", "chunkedTransfer": start["transferId"] })
    );
    ready.stop();
}

#[test]
fn host_batch_fork_and_parallel_may_only_nest_in_tab_commands() {
    let _env = setup();
    let mut ready = ready_host();
    let refused = [
        (
            "batch",
            json!({ "commands": [{ "action": "click" }, { "action": "uploadFile", "params": { "path": "/etc/hosts" } }] }),
        ),
        (
            "batch",
            json!({ "commands": [{ "action": "batch", "params": { "commands": [] } }] }),
        ),
        ("batch", json!({ "commands": [{ "action": "reload" }] })),
        ("batch", json!({ "commands": [null] })),
        (
            "fork",
            json!({ "paths": [{ "name": "a", "commands": [{ "action": "newSession" }] }] }),
        ),
        (
            "parallel",
            json!({ "branches": [{ "url": "https://a.test", "commands": [{ "action": "scout" }] }] }),
        ),
    ];
    for (action, params) in &refused {
        ready
            .client
            .send(&json!({ "id": action, "action": action, "params": params }));
        let reply = ready.client.reply().expect("reply");
        assert_eq!(reply["ok"], false, "{params}");
        assert!(reply["error"]
            .as_str()
            .unwrap_or_default()
            .contains("not allowed inside"));
    }
    assert_eq!(
        ready.host.pending_frames(),
        0,
        "nothing reached the extension"
    );
    ready.client.send(&json!({ "id": "ok", "action": "batch", "params": { "commands": [{ "action": "click", "params": { "tabId": 1 } }, { "action": "getContent", "params": { "tabId": 1 } }] } }));
    assert_eq!(ready.host.next()["action"], "batch");
    ready.stop();
}

// ── action mapping ─────────────────────────────────────────────────────────────────────

fn bridge(action: &str, input: Value, tab: Option<i64>) -> (String, Value) {
    bridge_request(action, &input, tab).expect("mapped")
}

fn bridge_err(action: &str, input: Value, tab: Option<i64>) -> String {
    bridge_request(action, &input, tab).expect_err("refused").0
}

#[test]
fn actions_open_with_no_tab_opens_a_background_tab_instead_of_taking_over_the_current_one() {
    assert_eq!(
        bridge("open", json!({ "url": "https://a.test" }), None),
        (
            "newSession".into(),
            json!({ "url": "https://a.test", "focus": false, "wait": true })
        )
    );
    assert_eq!(
        bridge("open", json!({ "url": "https://a.test" }), Some(4)),
        (
            "navigate".into(),
            json!({ "tabId": 4, "url": "https://a.test", "wait": true })
        )
    );
    assert_eq!(
        bridge(
            "open",
            json!({ "url": "https://a.test", "new_tab": true }),
            Some(4)
        )
        .0,
        "newSession"
    );
}

#[test]
fn actions_each_action_maps_to_the_extensions_verb_and_params() {
    assert_eq!(
        bridge("snapshot", json!({}), Some(1)),
        (
            "getContent".into(),
            json!({ "tabId": 1, "format": "annotated" })
        )
    );
    assert_eq!(
        bridge("get_content", json!({}), Some(1)),
        ("getContent".into(), json!({ "tabId": 1, "format": "text" }))
    );
    assert_eq!(
        bridge("select", json!({ "selector": "#s", "text": "b" }), Some(1)),
        (
            "fillForm".into(),
            json!({ "tabId": 1, "fields": [{ "selector": "#s", "value": "b" }] })
        )
    );
    assert_eq!(
        bridge(
            "wait",
            json!({ "contains": "Done", "timeout_ms": 900 }),
            Some(1)
        ),
        (
            "waitFor".into(),
            json!({ "tabId": 1, "contains": "Done", "timeout": 900 })
        )
    );
    assert_eq!(
        bridge("upload", json!({ "path": "/tmp/x" }), Some(1)),
        ("uploadFile".into(), json!({ "tabId": 1, "path": "/tmp/x" }))
    );
    assert_eq!(
        bridge(
            "fill_form",
            json!({ "fields": [{ "selector": "#a", "value": "1" }, { "selector": "#c", "checked": true }] }),
            Some(1)
        ),
        (
            "fillForm".into(),
            json!({ "tabId": 1, "fields": [{ "selector": "#a", "value": "1" }, { "selector": "#c", "checked": true }] })
        )
    );
    let (scroll_action, scroll_params) =
        bridge("scroll", json!({ "selector": "#list", "y": 300 }), Some(1));
    assert_eq!(
        scroll_action, "evaluate",
        "selector + delta scrolls that container, not the page"
    );
    assert!(scroll_params["script"]
        .as_str()
        .unwrap_or_default()
        .contains(r##"document.querySelector("#list")"##));
    let (press_action, press_params) = bridge("press", json!({ "key": "Enter" }), Some(1));
    assert_eq!(press_action, "evaluate");
    assert_eq!(press_params["pageWorld"], true);
    assert!(press_params["script"]
        .as_str()
        .unwrap_or_default()
        .contains(r#"const key = "Enter""#));
}

#[test]
fn actions_missing_required_inputs_are_refused_before_reaching_the_browser() {
    assert!(bridge_request("click", &json!({}), Some(1)).is_err());
    assert!(bridge_err("type", json!({}), Some(1)).contains("text is required"));
    assert!(bridge_err("open", json!({}), Some(1)).contains("url is required"));
    assert!(bridge_err("wait", json!({}), Some(1)).contains("selector, text, or contains"));
    assert!(bridge_err("scroll", json!({}), Some(1)).contains("scroll requires"));
    assert!(
        bridge_err("fill_form", json!({ "fields": [] }), Some(1)).contains("fields are required")
    );
    assert!(bridge_err("press", json!({}), Some(1)).contains("key is required"));
}

#[test]
fn actions_screenshots_come_back_as_an_image_block_errors_name_the_next_step() {
    let blocks = format_result(
        "screenshot",
        &json!({ "tabId": 2, "dataUrl": "data:image/png;base64,QUJD" }),
        &json!({}),
    );
    assert_eq!(
        blocks[0],
        json!({ "type": "image", "data": "QUJD", "mimeType": "image/png" })
    );
    assert!(blocks[1]["text"]
        .as_str()
        .unwrap_or_default()
        .contains("tab 2"));
    assert!(explain_error("open", "Timed out waiting for page load").contains("snapshot"));
    assert!(explain_error("snapshot", "Cannot access a chrome:// URL").contains("internal pages"));
    assert_eq!(
        explain_error("click", "Element not found: #x"),
        "Element not found: #x"
    );
}

#[test]
fn actions_windows_private_windows_and_reload() {
    assert_eq!(
        bridge("new_window", json!({ "url": "https://a.test" }), None),
        (
            "newWindow".into(),
            json!({ "url": "https://a.test", "focus": false, "wait": true })
        )
    );
    assert_eq!(
        bridge(
            "open",
            json!({ "url": "https://a.test", "sandbox": true }),
            Some(4)
        ),
        (
            "newSession".into(),
            json!({ "url": "https://a.test", "focus": false, "wait": true, "sandbox": true })
        ),
        "a private page never reuses the session's tab"
    );
    assert_eq!(
        bridge("reload", json!({ "bypass_cache": true }), Some(4)),
        (
            "reloadTab".into(),
            json!({ "tabId": 4, "wait": true, "bypassCache": true })
        )
    );
    assert_eq!(
        bridge(
            "drop_file",
            json!({ "path": "/tmp/a.png", "selector": "#ed" }),
            Some(4)
        ),
        (
            "dropFile".into(),
            json!({ "tabId": 4, "selector": "#ed", "path": "/tmp/a.png" })
        )
    );
}

#[test]
fn actions_batch_steps_map_like_top_level_calls_in_the_sessions_tab() {
    let (action, params) = bridge(
        "batch",
        json!({ "steps": [{ "action": "click", "selector": "#a" }, { "action": "wait", "contains": "Done" }, { "action": "open", "url": "https://b.test", "new_tab": true }] }),
        Some(7),
    );
    assert_eq!(action, "batch");
    assert_eq!(
        params,
        json!({
            "stopOnError": true,
            "commands": [
                { "action": "click", "params": { "tabId": 7, "selector": "#a" } },
                { "action": "waitFor", "params": { "tabId": 7, "contains": "Done" } },
                { "action": "navigate", "params": { "tabId": 7, "url": "https://b.test", "wait": true } },
            ],
        }),
        "an open step navigates this tab, never a new one"
    );
    for step in [
        json!({ "action": "screenshot" }),
        json!({ "action": "upload", "path": "/x" }),
        json!({ "action": "batch", "steps": [] }),
        json!({ "action": "new_tab" }),
        json!({ "action": "list_tabs" }),
    ] {
        assert!(
            bridge_err("batch", json!({ "steps": [step.clone()] }), Some(7))
                .contains("cannot run there"),
            "{step}"
        );
    }
    assert!(bridge_err("batch", json!({ "steps": [] }), Some(7)).contains("non-empty steps"));
    assert!(
        bridge_err(
            "batch",
            json!({ "steps": [{ "action": "click" }] }),
            Some(7)
        )
        .contains("click requires"),
        "each step is checked like a call"
    );
}

#[test]
fn actions_try_until_fork_and_parallel() {
    assert_eq!(
        bridge(
            "try_until",
            json!({ "alternatives": [{ "action": "click", "selector": "#a" }, { "action": "wait", "text": "Hi" }], "timeout_ms": 3000 }),
            Some(2)
        ),
        (
            "tryUntil".into(),
            json!({ "tabId": 2, "alternatives": [{ "action": "click", "params": { "selector": "#a" } }, { "action": "waitFor", "params": { "text": "Hi" } }], "timeout": 3000 })
        )
    );
    assert!(bridge_err(
        "try_until",
        json!({ "alternatives": [{ "action": "eval", "script": "1" }] }),
        Some(2)
    )
    .contains("can try click, type, wait"));
    assert_eq!(
        bridge(
            "fork",
            json!({ "forks": [{ "name": "a" }, { "name": "b", "steps": [{ "action": "click", "text": "Next" }] }] }),
            Some(2)
        ),
        (
            "fork".into(),
            json!({ "tabId": 2, "paths": [{ "name": "a", "commands": [] }, { "name": "b", "commands": [{ "action": "click", "params": { "text": "Next" } }] }] })
        ),
        "a fork's steps get the fork's tab from the extension"
    );
    assert!(
        bridge_err("fork", json!({ "forks": [{ "steps": [] }] }), Some(2)).contains("needs a name")
    );
    assert_eq!(
        bridge(
            "parallel",
            json!({ "branches": [{ "url": "https://a.test", "steps": [{ "action": "get_content", "format": "title" }] }] }),
            None
        ),
        (
            "parallel".into(),
            json!({ "branches": [{ "url": "https://a.test", "commands": [{ "action": "getContent", "params": { "format": "title" } }] }] })
        )
    );
    assert!(
        bridge_err(
            "parallel",
            json!({ "branches": [{ "url": "https://a.test" }] }),
            None
        )
        .contains("non-empty steps"),
        "a branch closes its tab, so it needs steps"
    );
    assert_eq!(
        bridge("kill_fork", json!({ "fork": "a" }), None),
        ("killFork".into(), json!({ "fork": "a" }))
    );
}

#[test]
fn actions_step_results_read_like_their_own_actions_a_failed_step_or_try_is_an_error() {
    let batch = json!({
        "results": [
            { "index": 0, "action": "getContent", "ok": true, "result": { "title": "T", "url": "u", "content": "body" } },
            { "index": 1, "action": "click", "ok": false, "error": "Element not found: #x" },
        ],
        "completed": 2,
        "total": 3,
    });
    let blocks = format_result(
        "batch",
        &batch,
        &json!({ "steps": [{ "action": "get_content" }, { "action": "click" }, { "action": "snapshot" }] }),
    );
    let text = blocks[0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("step 1 get_content: T\nu\n\nbody"), "{text}");
    assert!(text.contains("step 2 click FAILED: Element not found: #x"));
    assert!(text.contains("last 1 of 3 steps did not run"));
    assert!(failed_result("batch", &batch));
    assert!(failed_result(
        "try_until",
        &json!({ "success": false, "errors": [] })
    ));
    assert!(!failed_result("try_until", &json!({ "success": true })));
    let slow = format_result(
        "open",
        &json!({ "tabId": 1, "windowId": 2, "loadWarning": "Timed out waiting for page load", "content": { "title": "Dash", "url": "u", "content": "rows" } }),
        &json!({}),
    );
    assert!(
        re(r"never reported loaded[\s\S]*rows")
            .is_match(slow[0]["text"].as_str().unwrap_or_default()),
        "a slow page still comes back with its content"
    );
}

// ── MCP server tool calls against a fake host ──────────────────────────────────────────

type Answer = dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync;

struct FakeHost {
    seen: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    path: String,
}

impl FakeHost {
    fn seen(&self) -> Vec<Value> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn last(&self) -> Value {
        self.seen()
            .last()
            .cloned()
            .expect("a request reached the host")
    }

    /// Stops listening and removes the socket file, as Node's `server.close()` did.
    fn close(&self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        self.close();
    }
}

fn fake_host(
    browser: &'static str,
    answer: impl Fn(&str, &Value) -> Result<Value, String> + Send + Sync + 'static,
) -> FakeHost {
    let current = Some(env!("CARGO_PKG_VERSION").to_string());
    fake_host_of_version(browser, Arc::new(Mutex::new(current)), answer)
}

/// A fake host whose `hostInfo` reports `version` (none, as hosts before 3.0.0-rc.2 do).
fn fake_host_of_version(
    browser: &'static str,
    version: Arc<Mutex<Option<String>>>,
    answer: impl Fn(&str, &Value) -> Result<Value, String> + Send + Sync + 'static,
) -> FakeHost {
    let path = socket_path(browser);
    std::fs::create_dir_all(Path::new(&path).parent().expect("run dir")).expect("mkdir");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let answer: Arc<Answer> = Arc::new(answer);
    let (seen_in, stop_in) = (Arc::clone(&seen), Arc::clone(&stop));
    std::thread::spawn(move || {
        while !stop_in.load(Ordering::SeqCst) {
            let Ok((mut conn, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let _ = conn.set_nonblocking(false);
            let (seen, answer, version) = (
                Arc::clone(&seen_in),
                Arc::clone(&answer),
                Arc::clone(&version),
            );
            std::thread::spawn(move || {
                let mut decoder = LineDecoder::default();
                let mut buf = vec![0u8; 64 * 1024];
                while let Ok(n) = conn.read(&mut buf) {
                    if n == 0 {
                        return;
                    }
                    for m in decoder.push(&buf[..n]) {
                        let action = m["action"].as_str().unwrap_or_default().to_string();
                        // hostInfo and ping are the server checking in, not work it was asked to do.
                        if action != "hostInfo" && action != "ping" {
                            seen.lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .push(m.clone());
                        }
                        let result = if action == "hostInfo" {
                            let version = version.lock().unwrap_or_else(|e| e.into_inner());
                            Ok(match version.as_deref() {
                                Some(v) => json!({ "browser": browser, "version": v }),
                                None => json!({ "browser": browser }),
                            })
                        } else {
                            answer(&action, &m["params"])
                        };
                        let reply = match result {
                            Ok(result) => json!({ "id": m["id"], "ok": true, "result": result }),
                            Err(error) => json!({ "id": m["id"], "ok": false, "error": error }),
                        };
                        let _ = conn.write_all(format!("{reply}\n").as_bytes());
                    }
                }
            });
        }
    });
    FakeHost { seen, stop, path }
}

#[test]
fn server_refuses_tab_actions_until_the_session_has_a_tab_then_keeps_using_that_tab() {
    let _env = setup();
    let server = server();
    let host = fake_host("brave", |action, params| {
        Ok(if action == "newSession" {
            json!({ "tabId": 42, "windowId": 1, "url": params["url"], "content": { "title": "A", "url": params["url"], "content": "[button: \"Go\" | selector: #go]" } })
        } else {
            json!({ "clicked": true })
        })
    });
    let refused = server.call_tool(&json!({ "action": "click", "selector": "#go" }));
    assert!(refused.is_error);
    assert!(text_of(&refused).contains("No tab yet"));
    assert_eq!(host.seen().len(), 0);

    let opened = server.call_tool(&json!({ "action": "open", "url": "https://a.test" }));
    assert!(!opened.is_error);
    assert!(re(r"tab 42[\s\S]*selector: #go").is_match(&text_of(&opened)));
    server.call_tool(&json!({ "action": "click", "selector": "#go" }));
    let last = host.last();
    assert_eq!(
        last,
        json!({ "id": last["id"], "action": "click", "params": { "tabId": 42, "selector": "#go" } })
    );
    host.close();
}

#[test]
fn server_forks_belong_to_this_session_namespaced_in_the_extension_targeted_by_name() {
    let _env = setup();
    let server = server();
    let forks: Arc<Mutex<Vec<(String, i64)>>> = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::clone(&forks);
    let host = fake_host("brave", move |action, params| {
        let mut forks = state.lock().unwrap_or_else(|e| e.into_inner());
        Ok(match action {
            "newSession" | "navigate" => json!({ "tabId": 5, "windowId": 1, "url": params["url"] }),
            "fork" => {
                let made: Vec<Value> = params["paths"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(i, p)| {
                        let name = p["name"].as_str().unwrap_or_default().to_string();
                        forks.push((name.clone(), 60 + i as i64));
                        json!({ "name": name, "tabId": 60 + i as i64, "url": "u", "commandResults": [] })
                    })
                    .collect();
                json!({ "forks": made })
            }
            "listForks" => {
                let mut list: Vec<Value> = forks
                    .iter()
                    .map(|(name, tab)| json!({ "name": name, "tabId": tab, "alive": true }))
                    .collect();
                list.push(json!({ "name": "s1-theirs", "tabId": 99, "alive": true }));
                json!({ "forks": list })
            }
            "killFork" => {
                let name = params["fork"].as_str().unwrap_or_default();
                forks.retain(|(n, _)| n != name);
                json!({ "killed": true })
            }
            _ => json!({ "clicked": true }),
        })
    });
    let pid = std::process::id();
    server.call_tool(&json!({ "action": "open", "url": "https://a.test" }));
    let made = server.call_tool(
        &json!({ "action": "fork", "forks": [{ "name": "left" }, { "name": "right" }] }),
    );
    assert!(re(r"fork left: tab 60[\s\S]*fork right: tab 61").is_match(&text_of(&made)));
    let sent = host.last();
    let names: Vec<&str> = sent["params"]["paths"]
        .as_array()
        .expect("paths")
        .iter()
        .filter_map(|p| p["name"].as_str())
        .collect();
    assert_eq!(names, [format!("s{pid}-left"), format!("s{pid}-right")]);
    assert!(text_of(
        &server.call_tool(&json!({ "action": "fork", "forks": [{ "name": "left" }] }))
    )
    .contains("already exists"));

    server.call_tool(&json!({ "action": "click", "selector": "#go", "fork": "right" }));
    assert_eq!(host.last()["params"]["tabId"], 61);
    server.call_tool(&json!({ "action": "click", "selector": "#go" }));
    assert_eq!(
        host.last()["params"]["tabId"],
        5,
        "the session's own tab is unchanged"
    );

    let listed = text_of(&server.call_tool(&json!({ "action": "list_forks" })));
    assert!(re(r"(?m)^left: tab 60").is_match(&listed));
    assert!(
        !listed.contains("theirs"),
        "another session's forks are not listed"
    );
    assert!(
        text_of(&server.call_tool(&json!({ "action": "kill_fork", "fork": "theirs" })))
            .contains("No fork named theirs")
    );
    server.call_tool(&json!({ "action": "kill_fork", "fork": "left" }));
    assert_eq!(host.last()["params"]["fork"], format!("s{pid}-left"));
    assert!(text_of(
        &server.call_tool(&json!({ "action": "click", "selector": "#a", "fork": "left" }))
    )
    .contains("No fork named left"));
    host.close();
}

#[test]
fn server_a_closed_session_tab_is_forgotten_open_retries_once_in_a_new_background_tab() {
    let _env = setup();
    let server = server();
    let opened = Arc::new(Mutex::new(0));
    let host = fake_host("brave", move |action, params| {
        if action == "newSession" {
            let mut n = opened.lock().unwrap_or_else(|e| e.into_inner());
            *n += 1;
            return Ok(json!({ "tabId": 10 + *n, "windowId": 1, "url": params["url"] }));
        }
        Err(format!("No tab with id: {}.", params["tabId"]))
    });
    server.call_tool(&json!({ "action": "open", "url": "https://a.test" }));
    let reopened = server.call_tool(&json!({ "action": "open", "url": "https://b.test" }));
    assert!(!reopened.is_error);
    assert!(re(r"had been closed[\s\S]*tab 12").is_match(&text_of(&reopened)));
    let seen = host.seen();
    let last_two: Vec<&str> = seen[seen.len() - 2..]
        .iter()
        .filter_map(|m| m["action"].as_str())
        .collect();
    assert_eq!(last_two, ["navigate", "newSession"]);
    let clicked = server.call_tool(&json!({ "action": "click", "selector": "#go" }));
    assert!(clicked.is_error, "a click is never repeated on its own");
    assert_eq!(
        host.seen()
            .iter()
            .filter(|m| m["action"] == "click")
            .count(),
        1
    );
    assert!(
        text_of(&server.call_tool(&json!({ "action": "snapshot" }))).contains("No tab yet"),
        "the dead tab is not tried again"
    );
    host.close();
}

#[test]
fn server_a_failed_private_window_says_how_to_allow_the_extension_there() {
    let _env = setup();
    let server = server();
    let host = fake_host("brave", |_, _| {
        Err("Cannot read properties of undefined (reading '0')".into())
    });
    let result = server
        .call_tool(&json!({ "action": "new_window", "url": "https://a.test", "sandbox": true }));
    assert!(result.is_error);
    assert!(text_of(&result).contains("Allow in Private"));
    host.close();
}

#[test]
fn server_a_browser_running_an_older_extension_is_updated_and_reloaded_once() {
    let env = setup();
    let server = server();
    let _ = std::fs::remove_file(env.home().join("VERSION"));
    let version = Arc::new(Mutex::new("0.0.1".to_string()));
    let host = fake_host("chrome", move |action, _| {
        Ok(match action {
            "ping" => {
                json!({ "extensionVersion": *version.lock().unwrap_or_else(|e| e.into_inner()) })
            }
            "reload" => {
                let version = Arc::clone(&version);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    *version.lock().unwrap_or_else(|e| e.into_inner()) = extension_version();
                });
                json!({ "reloading": true })
            }
            _ => json!({ "tabs": [] }),
        })
    });
    let result = server.call_tool(&json!({ "action": "list_tabs", "browser": "chrome" }));
    assert!(!result.is_error, "{}", text_of(&result));
    let installed = std::fs::read_to_string(env.home().join("VERSION")).unwrap_or_default();
    assert!(
        !installed.trim().is_empty(),
        "the new files were installed first"
    );
    let actions: Vec<Value> = host.seen().iter().map(|m| m["action"].clone()).collect();
    assert_eq!(
        actions,
        [json!("reload"), json!("listTabs")],
        "reload, wait for the new version, then the call"
    );
    server.call_tool(&json!({ "action": "list_tabs", "browser": "chrome" }));
    assert_eq!(
        host.seen()
            .iter()
            .filter(|m| m["action"] == "reload")
            .count(),
        1,
        "once per session"
    );
    host.close();
}

#[test]
fn server_an_older_host_behind_a_current_extension_is_reloaded_once() {
    // A plugin update that leaves the extension's version alone: the browser keeps the old host
    // process running until the extension reloads, so the new binary would otherwise never start.
    let _env = setup();
    let server = server();
    let host_version: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let reloaded = Arc::clone(&host_version);
    let host = fake_host_of_version("chrome", Arc::clone(&host_version), move |action, _| {
        Ok(match action {
            "ping" => json!({ "extensionVersion": extension_version() }),
            "reload" => {
                let reloaded = Arc::clone(&reloaded);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    *reloaded.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(env!("CARGO_PKG_VERSION").to_string());
                });
                json!({ "reloading": true })
            }
            _ => json!({ "tabs": [] }),
        })
    });
    let result = server.call_tool(&json!({ "action": "list_tabs", "browser": "chrome" }));
    assert!(!result.is_error, "{}", text_of(&result));
    let actions: Vec<Value> = host.seen().iter().map(|m| m["action"].clone()).collect();
    assert_eq!(
        actions,
        [json!("reload"), json!("listTabs")],
        "reload, wait for the new host, then the call"
    );
    server.call_tool(&json!({ "action": "list_tabs", "browser": "chrome" }));
    assert_eq!(
        host.seen()
            .iter()
            .filter(|m| m["action"] == "reload")
            .count(),
        1,
        "once per session"
    );
    host.close();
}

#[test]
fn server_extension_errors_come_back_as_tool_errors_with_guidance() {
    let _env = setup();
    let server = server();
    let host = fake_host("brave", |_, _| Err("No tab with id: 42".into()));
    let result = server.call_tool(&json!({ "action": "snapshot" }));
    assert!(result.is_error);
    assert!(text_of(&result).contains("list_tabs"));
    host.close();
}

#[test]
fn server_several_connected_browsers_need_an_explicit_choice() {
    let _env = setup();
    let server = server();
    let a = fake_host("brave", |_, _| Ok(json!({})));
    // The TypeScript suite shared one session across tests; here this session first works in brave.
    server.call_tool(&json!({ "action": "list_tabs", "browser": "brave" }));
    let b = fake_host("firefox", |_, _| Ok(json!({ "tabs": [] })));
    let ok = server.call_tool(&json!({ "action": "list_tabs" }));
    assert!(
        !ok.is_error,
        "the session's browser is still connected, so it is reused"
    );
    a.close();
    let _ = std::fs::remove_file(socket_path("brave"));
    let switched = server.call_tool(&json!({ "action": "list_tabs" }));
    assert!(
        !switched.is_error,
        "only firefox left: chosen without asking"
    );
    let c = fake_host("chrome", |_, _| Ok(json!({})));
    server.call_tool(&json!({ "action": "list_tabs", "browser": "chrome" }));
    assert_eq!(c.seen().len(), 1);
    b.close();
    c.close();
}

#[test]
fn server_with_nothing_connected_any_action_installs_silently_and_returns_a_guide_for_the_person() {
    let env = setup();
    let server = server();
    let _ = std::fs::remove_file(env.home().join("VERSION"));
    let profiles = if cfg!(target_os = "macos") {
        env.fake_home().join("Library/Application Support/Firefox")
    } else {
        env.fake_home().join(".mozilla").join("firefox")
    };
    std::fs::create_dir_all(profiles).expect("mkdir");
    let run = Path::new(&socket_path("x"))
        .parent()
        .expect("run dir")
        .to_path_buf();
    for entry in std::fs::read_dir(&run).into_iter().flatten().flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    let firefox_app = if cfg!(target_os = "macos") {
        env.apps().join("Firefox.app")
    } else {
        env.apps().join("firefox")
    };
    std::fs::create_dir(&firefox_app).expect("mkdir");
    let result = server.call_tool(&json!({ "action": "open", "url": "https://a.test" }));
    std::fs::remove_dir_all(&firefox_app).expect("rm");
    assert!(result.is_error);
    let text = text_of(&result);
    assert!(
        text.contains("Show the person these steps exactly as written"),
        "{text}"
    );
    assert!(
        text.contains("2. Click Load Temporary Add-on…"),
        "only Firefox is present in this home, so its steps are given"
    );
    assert!(text.contains("repeats after each restart"));
    assert!(
        text.contains("end your reply there"),
        "a waiting tool call would hide the steps, so Claude shows them and stops"
    );
    assert!(
        !text.contains("is opening"),
        "with opening switched off, the guide does not claim it opened anything"
    );
    assert!(
        env.home().join("VERSION").exists(),
        "installed without being asked"
    );
    let status = server.call_tool(&json!({ "action": "status" }));
    assert!(text_of(&status).contains("connected: none"));
}

#[test]
fn install_reads_whether_the_extension_is_loaded_off_or_loaded_from_another_folder() {
    let app_dir = tempfile::Builder::new()
        .prefix("viby-chromium-")
        .tempdir()
        .expect("temp dir");
    let expected = Path::new("/x/extension/chromium");
    let write = |record: Option<Value>, profile: &str| {
        std::fs::create_dir_all(app_dir.path().join(profile)).expect("mkdir");
        let settings = match record {
            Some(r) => json!({ CHROMIUM_EXTENSION_ID: r }),
            None => json!({}),
        };
        std::fs::write(
            app_dir.path().join(profile).join("Secure Preferences"),
            json!({ "extensions": { "settings": settings } }).to_string(),
        )
        .expect("write");
    };
    write(None, "Default");
    assert_eq!(
        chromium_extension_state(app_dir.path(), expected),
        ExtensionState::Missing
    );
    write(
        Some(json!({ "path": expected, "disable_reasons": [1] })),
        "Default",
    );
    assert_eq!(
        chromium_extension_state(app_dir.path(), expected),
        ExtensionState::Disabled {
            profile: "Default".into()
        }
    );
    write(Some(json!({ "path": "/old/copy" })), "Default");
    assert_eq!(
        chromium_extension_state(app_dir.path(), expected),
        ExtensionState::OtherFolder {
            profile: "Default".into(),
            path: "/old/copy".into()
        }
    );
    write(Some(json!({ "path": expected })), "Profile 2");
    assert_eq!(
        chromium_extension_state(app_dir.path(), expected),
        ExtensionState::Installed {
            profile: "Profile 2".into()
        }
    );
}

#[test]
fn server_with_no_usable_browser_the_person_is_asked_to_install_one() {
    let env = setup();
    let server = server();
    let _ = std::fs::remove_dir_all(env.fake_home().join("Library/Application Support/Firefox"));
    let _ = std::fs::remove_dir_all(env.fake_home().join(".mozilla"));
    std::fs::create_dir(env.apps().join("Safari.app")).expect("mkdir");
    let text = text_of(&server.call_tool(&json!({ "action": "snapshot" })));
    std::fs::remove_dir_all(env.apps().join("Safari.app")).expect("rm");
    assert!(text.contains("Install Brave (or one of these)"), "{text}");
    assert!(
        !text.contains("opened Brave's download page"),
        "opening is switched off here, so it is not claimed"
    );
    for url in [
        "brave.com",
        "google.com/chrome",
        "microsoft.com/edge",
        "mozilla.org/firefox",
    ] {
        assert!(text.contains(url), "{url}");
    }
    if cfg!(target_os = "macos") {
        assert!(text.contains("Safari is installed, but"));
    }
    assert!(
        text.contains("Do not call setup"),
        "nothing to set up until a browser exists"
    );
    assert!(!text.contains("20 seconds"));
}

#[test]
fn server_a_browser_installed_but_never_opened_is_found_and_gets_the_one_time_step() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let env = setup();
    let server = server();
    std::fs::create_dir(env.apps().join("Brave Browser.app")).expect("mkdir");
    let text = text_of(&server.call_tool(&json!({ "action": "status" })));
    std::fs::remove_dir_all(env.apps().join("Brave Browser.app")).expect("rm");
    assert!(
        text.contains("Let me use Brave: one time, about 20 seconds."),
        "{text}"
    );
    let numbered = re(r"^\s+\d\. ");
    let steps: Vec<&str> = text
        .split('\n')
        .filter(|l| numbered.is_match(l))
        .map(str::trim)
        .collect();
    assert_eq!(
        steps[..3],
        [
            "1. Open brave://extensions.",
            "2. Turn on Developer mode (the switch at the top right).",
            "3. Click Load unpacked (top left)."
        ]
    );
    assert_eq!(
        steps.len(),
        5,
        "one short list: page, switch, button, pick, confirm"
    );
    assert_eq!(
        steps[3],
        "4. Drag the highlighted \"chromium\" folder from the Finder window onto the file picker."
    );
    assert!(
        !text.contains("⌘⇧G"),
        "Go to Folder did nothing in Brave's picker on macOS 26"
    );
    assert!(text.contains("Page didn't open? Go to brave://extensions"));
    assert_eq!(installed_app_here("chrome"), None);
}

#[test]
fn setup_lands_on_the_extensions_page_itself_through_the_browsers_applescript() {
    let script = chromium_tab_script("/Applications/Brave Browser.app", "brave://extensions");
    assert!(
        re(r#"(?m)^tell application "/Applications/Brave Browser\.app"$"#).is_match(&script),
        "the exact app found, not whatever LaunchServices picks"
    );
    assert!(script.contains(r#"make new tab with properties {URL:"brave://extensions"}"#));
    assert!(script.contains("if (count of windows) = 0 then make new window"));
}

#[test]
fn default_browser_handler_ids_from_macos_windows_and_linux_map_to_our_browsers() {
    assert_eq!(
        browser_for_handler("com.brave.browser"),
        Some("brave"),
        "macOS stores it lower-cased"
    );
    assert_eq!(browser_for_handler("com.microsoft.edgemac"), Some("edge"));
    assert_eq!(browser_for_handler("ChromeHTML"), Some("chrome"));
    assert_eq!(browser_for_handler("MSEdgeHTM"), Some("edge"));
    assert_eq!(
        browser_for_handler("FirefoxURL-308046B0AF4A39CB"),
        Some("firefox")
    );
    assert_eq!(
        browser_for_handler("brave-browser.desktop\n"),
        Some("brave")
    );
    assert_eq!(browser_for_handler("com.apple.safari"), None);
}

#[test]
fn server_the_persons_default_browser_is_the_one_guided_in_that_browsers_own_words() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let env = setup();
    let server = server();
    for app in ["Brave Browser.app", "Microsoft Edge.app"] {
        std::fs::create_dir(env.apps().join(app)).expect("mkdir");
    }
    std::env::set_var("VIBY_BROWSER_DEFAULT", "edge");
    let edge = text_of(&server.call_tool(&json!({ "action": "status" })));
    std::env::set_var("VIBY_BROWSER_DEFAULT", "firefox");
    std::fs::create_dir(env.apps().join("Firefox.app")).expect("mkdir");
    let firefox_default = text_of(&server.call_tool(&json!({ "action": "status" })));
    std::env::set_var("VIBY_BROWSER_DEFAULT", "");
    assert!(edge.contains("Let me use Edge"), "{edge}");
    assert!(edge.contains("Open edge://extensions."));
    assert!(
        edge.contains("Developer mode (the switch in the menu on the left"),
        "Edge keeps the switch in its side menu, not top right"
    );
    assert!(edge.contains("Load unpacked (above the list of extensions)"));
    assert!(edge.contains("(Brave works the same way.)"));
    assert!(
        firefox_default.contains("Let me use Brave"),
        "Firefox forgets the add-on, so an installed Chromium browser is used"
    );
    assert!(firefox_default.contains("Firefox is your default, but"));
}

#[test]
fn server_unknown_actions_are_refused() {
    let _env = setup();
    let result = server().call_tool(&json!({ "action": "secureAutoFill" }));
    assert!(result.is_error);
}

// ── extension build and install ────────────────────────────────────────────────────────

/// SHA-256 (FIPS 180-4), enough to check the extension id without a crypto dependency.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            (hh, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[test]
fn install_the_chromium_id_is_the_one_the_committed_public_key_yields() {
    use base64::Engine;
    assert_eq!(
        sha256(b"abc")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "the hash itself is right"
    );
    let der = base64::engine::general_purpose::STANDARD
        .decode(CHROMIUM_PUBLIC_KEY)
        .expect("base64");
    let hex: String = sha256(&der).iter().map(|b| format!("{b:02x}")).collect();
    let id: String = hex[..32]
        .chars()
        .map(|h| char::from(b'a' + h.to_digit(16).expect("hex") as u8))
        .collect();
    assert_eq!(id, CHROMIUM_EXTENSION_ID);
}

#[test]
fn install_builds_an_mv3_chromium_and_an_mv2_firefox_extension_without_debugger_or_notification_rights(
) {
    let env = setup();
    let out = build_extension(&env.home().join("build")).expect("built");
    let read = |dir: &Path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).expect("read"))
            .expect("json")
    };
    let chromium = read(&out.chromium);
    let firefox = read(&out.firefox);
    assert_eq!(chromium["manifest_version"], 3);
    assert_eq!(chromium["key"], CHROMIUM_PUBLIC_KEY);
    assert_eq!(chromium["background"]["service_worker"], "background.js");
    assert_eq!(firefox["manifest_version"], 2);
    assert_eq!(
        firefox["browser_specific_settings"]["gecko"]["id"],
        FIREFOX_EXTENSION_ID
    );
    for m in [&chromium, &firefox] {
        let permissions = m["permissions"].as_array().expect("permissions");
        assert!(!permissions.contains(&json!("debugger")));
        assert!(!permissions.contains(&json!("notifications")));
    }
    for dir in [&out.chromium, &out.firefox] {
        for f in [
            "background.js",
            "content.js",
            "popup.html",
            "popup.js",
            "LICENSE",
            "icons/icon-48.png",
        ] {
            assert!(dir.join(f).exists(), "{f}");
        }
    }
    assert_eq!(
        chromium_manifest(&firefox)["action"],
        firefox["browser_action"]
    );
}

#[test]
fn install_the_embedded_extension_matches_the_vendored_folder() {
    // The binary carries the extension; a file added to or changed in the folder must reach it.
    let root = extension_dir();
    let mut on_disk = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let rel = path
                    .strip_prefix(&root)
                    .expect("inside")
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel != "manifest.json" && !rel.ends_with(".DS_Store") {
                    on_disk.push(rel);
                }
            }
        }
    }
    on_disk.sort();
    let mut embedded: Vec<String> = EXTENSION_FILES
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    embedded.sort();
    assert_eq!(embedded, on_disk);
    for (name, bytes) in EXTENSION_FILES {
        assert_eq!(
            &std::fs::read(root.join(name)).expect("read"),
            bytes,
            "{name}"
        );
    }
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).expect("read"))
            .expect("json");
    assert_eq!(manifest["version"], extension_version());
}

#[test]
fn install_host_sh_runs_the_copy_in_the_browser_home_not_the_versioned_binary_that_installed_it() {
    // Replaces "host.sh gets the PATH node …": the launcher used to name node, and a node path
    // that moved on upgrade broke it. Now it names viby's own copy, which no update moves.
    let root = Path::new("/home/a/.viby/browser");
    let (file, text) = host_launcher(root, "darwin");
    assert_eq!(file, root.join("host.sh"));
    assert_eq!(
        text,
        "#!/bin/sh\nexec \"/home/a/.viby/browser/viby\" browser-host \"$@\"\n"
    );
    assert!(
        !text.contains(VIBY),
        "never the binary in the plugin's versioned cache"
    );
}

#[test]
fn install_registers_the_native_host_only_for_browsers_that_are_installed() {
    let env = setup();
    let home = tempfile::Builder::new()
        .prefix("viby-home-")
        .tempdir()
        .expect("temp dir");
    std::fs::create_dir_all(
        home.path()
            .join("Library/Application Support/BraveSoftware/Brave-Browser"),
    )
    .expect("mkdir");
    std::fs::create_dir_all(home.path().join("Library/Application Support/Firefox"))
        .expect("mkdir");
    let found: Vec<String> = detect_browsers(home.path(), "darwin", &process_env())
        .into_iter()
        .map(|b| b.browser)
        .collect();
    assert_eq!(found, ["brave", "firefox"]);
    let report = install(Path::new(VIBY), home.path(), "darwin").expect("installed");
    drop(home);

    let registered: Vec<&str> = report
        .registered
        .iter()
        .map(|r| r.browser.as_str())
        .collect();
    assert_eq!(registered, ["brave", "firefox"]);
    let host_sh = std::fs::read_to_string(env.home().join("host.sh")).expect("host.sh");
    let installed = env.home().join("viby");
    assert_eq!(
        host_sh,
        format!(
            "#!/bin/sh\nexec \"{}\" browser-host \"$@\"\n",
            installed.display()
        ),
        "absolute path: browsers launch hosts with a bare PATH"
    );
    assert!(mode(env.home().join("host.sh")) & 0o100 != 0);
    assert!(mode(&installed) & 0o100 != 0);
    assert_eq!(
        std::fs::read(&installed).expect("copy"),
        std::fs::read(VIBY).expect("viby"),
        "the running binary, copied"
    );
    for f in ["viby", "VERSION", "extension/chromium/manifest.json"] {
        assert!(env.home().join(f).exists(), "{f}");
    }
    assert_eq!(
        native_host_manifest(Family::Chromium, Path::new("/h"))["allowed_origins"],
        json!([format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
    );
    assert_eq!(
        native_host_manifest(Family::Firefox, Path::new("/h"))["allowed_extensions"],
        json!([FIREFOX_EXTENSION_ID])
    );
    assert_eq!(
        native_host_manifest(Family::Firefox, Path::new("/h"))["name"],
        NATIVE_HOST_NAME
    );
}

// ── vendored extension adaptations ─────────────────────────────────────────────────────

fn background() -> String {
    std::fs::read_to_string(extension_dir().join("background.js")).expect("background.js")
}

fn content() -> String {
    std::fs::read_to_string(extension_dir().join("content.js")).expect("content.js")
}

/// Runs a script in Node and returns its stdout, or None when Node is not installed.
fn node(script: &str) -> Option<String> {
    let out = Command::new("node").arg("-e").arg(script).output().ok()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn extension_connects_to_our_native_host_name() {
    assert!(background().contains(&format!("const NATIVE_APP_NAME = \"{NATIVE_HOST_NAME}\";")));
}

#[test]
fn extension_credential_and_relay_code_is_gone() {
    let (background, content) = (background(), content());
    for name in [
        "secureAutoFill",
        "getAuthContext",
        "requestAuth",
        "detectAuth",
        "authConfig",
        "RELAY_URL",
        "connectRelayPort",
    ] {
        assert!(
            !background.contains(name) && !content.contains(name),
            "{name}"
        );
    }
}

#[test]
fn extension_firefox_injection_falls_back_to_tabs_execute_script_not_to_itself() {
    let background = background();
    let body = re(r"async function injectContentScript\(tabId, frameId\) \{([\s\S]*?)\n\}")
        .captures(&background)
        .and_then(|c| c.get(1))
        .expect("injectContentScript")
        .as_str();
    assert!(!body.contains("injectContentScript("), "no self-recursion");
    assert!(re(r"browser\.tabs\.executeScript\(tabId").is_match(body));
}

#[test]
fn extension_one_click_runs_the_pages_click_handler_once() {
    let content = content();
    let block = re(r"// Standard click handling for other elements[\s\S]*?return \{ clicked: true")
        .find(&content)
        .expect("click block")
        .as_str();
    assert_eq!(
        block.matches("new MouseEvent(\"click\"").count(),
        1,
        "a synthetic click only as the fallback"
    );
    assert!(re(r#"if \(typeof target\.click === "function"\) target\.click\(\);\s*else target\.dispatchEvent\(new MouseEvent\("click""#).is_match(block));
}

#[test]
fn extension_snapshots_never_carry_a_password_fields_value() {
    let content = content();
    let source = re(r"function visibleValue\(el\) \{[\s\S]*?\n\}")
        .find(&content)
        .expect("visibleValue")
        .as_str();
    let script = format!(
        "const visibleValue = new Function({}); const f = visibleValue(); console.log(JSON.stringify([f({{ type: 'password', value: 'hunter2' }}), f({{ type: 'text', value: 'hello' }}), f({{}})]));",
        json!(format!("{source}; return visibleValue;"))
    );
    if let Some(out) = node(&script) {
        assert_eq!(
            serde_json::from_str::<Value>(&out).expect("json"),
            json!(["•••••••", "hello", ""])
        );
    }
    let rest = content.replacen(source, "", 1).replace("el.value = ", "");
    let raw_read = fancy_regex::Regex::new(r"\bel\.value\b(?!\.length)").expect("regex");
    assert!(
        !raw_read.is_match(&rest).expect("match"),
        "every other read of el.value goes through visibleValue"
    );
}

#[test]
fn extension_a_chunked_drop_reassembles_into_the_file_the_content_script_reads() {
    let background = background();
    let source = re(r"function reassembleChunkedData\(message\) \{[\s\S]*?\n\}")
        .find(&background)
        .expect("reassembleChunkedData")
        .as_str();
    let script = format!(
        "const pendingChunks = new Map([['t1', {{ chunks: ['QU', 'JD'] }}]]); const reassemble = new Function('pendingChunks', {})(pendingChunks); const message = {{ action: 'dropFile', params: {{ selector: '#ed', file: {{ name: 'a.png', type: 'image/png', chunkedTransfer: 't1' }} }} }}; reassemble(message); console.log(JSON.stringify([message.params.file, pendingChunks.size]));",
        json!(format!("{source}; return reassembleChunkedData;"))
    );
    let Some(out) = node(&script) else {
        return; // Node is not installed: the extension's JavaScript cannot run here.
    };
    let parsed: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(
        parsed[0],
        json!({ "name": "a.png", "type": "image/png", "data": "QUJD" })
    );
    assert_eq!(parsed[1], 0);
}

#[test]
fn extension_a_page_that_never_finishes_loading_is_a_warning_not_a_failure() {
    let background = background();
    for f in ["openSessionTarget", "navigateTo", "reloadTab"] {
        let pattern = format!(r"async function {f}\(params[^)]*\) \{{([\s\S]*?)\n\}}");
        let body = re(&pattern)
            .captures(&background)
            .and_then(|c| c.get(1))
            .expect(f)
            .as_str();
        assert!(body.contains("waitForLoadOrWarn("), "{f}");
        assert!(!body.contains("await waitForTabComplete("), "{f}");
    }
}

// ── Windows (pure parts; no Windows machine runs this suite yet) ───────────────────────

#[test]
fn windows_hosts_listen_on_a_per_user_named_pipe() {
    assert!(re(r"^\\\\\.\\pipe\\viby-browser-[A-Za-z0-9-]+-brave$")
        .is_match(&socket_path_for("brave", "win32")));
    assert_ne!(
        socket_path_for("brave", "win32"),
        socket_path_for("edge", "win32")
    );
}

#[test]
fn windows_browsers_are_found_under_localappdata_appdata_and_registered_through_hkcu() {
    let root = tempfile::Builder::new()
        .prefix("viby-win-")
        .tempdir()
        .expect("temp dir");
    let mut env = Env::new();
    env.insert(
        "LOCALAPPDATA".into(),
        root.path().join("Local").to_string_lossy().into_owned(),
    );
    env.insert(
        "APPDATA".into(),
        root.path().join("Roaming").to_string_lossy().into_owned(),
    );
    std::fs::create_dir_all(
        Path::new(&env["LOCALAPPDATA"]).join("BraveSoftware\\Brave-Browser\\User Data"),
    )
    .expect("mkdir");
    std::fs::create_dir_all(Path::new(&env["LOCALAPPDATA"]).join("Microsoft\\Edge\\User Data"))
        .expect("mkdir");
    std::fs::create_dir_all(Path::new(&env["APPDATA"]).join("Mozilla\\Firefox")).expect("mkdir");
    let found = detect_browsers(root.path(), "win32", &env);
    drop(root);
    let names: Vec<&str> = found.iter().map(|b| b.browser.as_str()).collect();
    assert_eq!(names, ["brave", "edge", "firefox"]);
    assert_eq!(
        found[0].registry_key.as_deref(),
        Some("HKCU\\Software\\BraveSoftware\\Brave-Browser\\NativeMessagingHosts\\viby_browser")
    );
    assert_eq!(
        found[1].registry_key.as_deref(),
        Some("HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\viby_browser")
    );
    assert_eq!(
        found[2].registry_key.as_deref(),
        Some("HKCU\\Software\\Mozilla\\NativeMessagingHosts\\viby_browser")
    );
    let key = found[0].registry_key.clone().unwrap_or_default();
    assert_eq!(
        registry_args(&key, "C:\\m.json"),
        [
            "add",
            key.as_str(),
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            "C:\\m.json",
            "/f"
        ]
    );
}

#[test]
fn windows_the_host_launcher_is_a_quiet_cmd_that_passes_arguments_through() {
    let (file, text) = host_launcher(Path::new("C:\\Users\\a\\.viby\\browser"), "win32");
    assert!(file.to_string_lossy().ends_with("host.cmd"));
    assert!(
        text.starts_with("@echo off\r\n"),
        "nothing may be echoed onto the native-messaging stdout"
    );
    assert!(
        re(r#"(?m)^"C:\\Users\\a\\\.viby\\browser[\\/]viby\.exe" browser-host %\*\r\n$"#)
            .is_match(&text),
        "{text:?}"
    );
    assert!(
        re(r#"^#!/bin/sh\nexec "/h/viby""#).is_match(&host_launcher(Path::new("/h"), "darwin").1)
    );
}

#[test]
fn windows_the_installed_host_is_viby_exe() {
    // Replaces "the PATH node is node.exe": the launcher runs viby's own copy now.
    assert!(host_binary(Path::new("C:\\b"), "win32")
        .to_string_lossy()
        .ends_with("viby.exe"));
    assert_eq!(host_binary(Path::new("/b"), "darwin"), Path::new("/b/viby"));
}
