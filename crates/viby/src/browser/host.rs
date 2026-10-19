//! `viby browser-host`: the native-messaging host for the Viby Browser extension.
//!
//! The browser starts this process when the extension connects, and talks to it over
//! stdin/stdout. Once the extension says which browser it is, the host listens on a unix
//! socket that only this user can open; every MCP server (one per Claude session) connects
//! there. Requests get a host-unique id on the way in and are routed back by it.
//!
//! Port of rust-cli/src/bin/host.rs from 1jehuang/firefox-agent-bridge (MIT, see NOTICE.md),
//! minus the WebSocket, the Safari relay and every credential feature.

use super::protocol::{
    encode_native, node_error, socket_path, LineDecoder, NativeDecoder, CHUNK_SIZE,
    REQUEST_TIMEOUT_MS,
};
use base64::Engine;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Actions the extension may be asked to run. Anything else is refused here, not in the page.
const ALLOWED_ACTIONS: &[&str] = &[
    "ping",
    "reload",
    "listFrames",
    "listTabs",
    "switchTab",
    "newSession",
    "newWindow",
    "setActiveTab",
    "getActiveTab",
    "navigate",
    "reloadTab",
    "click",
    "type",
    "fillForm",
    "waitFor",
    "tryUntil",
    "getContent",
    "getInteractables",
    "screenshot",
    "evaluate",
    "scroll",
    "uploadFile",
    "dropFile",
    "batch",
    "fork",
    "killFork",
    "listForks",
    "parallel",
];

/// What batch, fork and parallel may run inside one tab. The extension dispatches their commands
/// itself, past this allow-list, so they are checked here: no nesting, no tab management, and no
/// file actions (the host reads those files, and only for a top-level request).
const NESTED_ACTIONS: &[&str] = &[
    "navigate",
    "reloadTab",
    "click",
    "type",
    "fillForm",
    "waitFor",
    "getContent",
    "getInteractables",
    "evaluate",
    "scroll",
];

fn nested_commands(action: &str, params: &Value) -> Vec<Value> {
    let list = |value: Option<&Value>| value.and_then(Value::as_array).cloned().unwrap_or_default();
    match action {
        "batch" => list(params.get("commands")),
        "fork" => list(params.get("paths"))
            .iter()
            .flat_map(|p| list(p.get("commands")))
            .collect(),
        "parallel" => list(params.get("branches"))
            .iter()
            .flat_map(|b| list(b.get("commands")))
            .collect(),
        _ => Vec::new(),
    }
}

const MIME: &[(&str, &str)] = &[
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".svg", "image/svg+xml"),
    (".pdf", "application/pdf"),
    (".txt", "text/plain"),
    (".csv", "text/csv"),
    (".json", "application/json"),
    (".html", "text/html"),
    (".zip", "application/zip"),
    (".mp4", "video/mp4"),
];

const WATCH_MS: u64 = 2000;

fn log(text: &str) {
    eprintln!("[viby-browser-host] {text}");
}

/// Where replies to one MCP server go: its connection's write half.
type Client = Arc<Mutex<Box<dyn Write + Send>>>;

struct Pending {
    client: Client,
    client_id: Option<Value>,
    deadline: Instant,
}

#[derive(Default)]
struct Listening {
    active: bool,
    /// The socket file this host made, and its inode, to tell it from a newer host's.
    own: Option<(String, u64)>,
}

struct Host {
    out: Mutex<std::io::Stdout>,
    counter: AtomicU64,
    pending: Mutex<HashMap<String, Pending>>,
    browser: Mutex<String>,
    listening: Mutex<Listening>,
    greeted: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn host() -> &'static Host {
    static HOST: OnceLock<Host> = OnceLock::new();
    HOST.get_or_init(|| Host {
        out: Mutex::new(std::io::stdout()),
        counter: AtomicU64::new(0),
        pending: Mutex::new(HashMap::new()),
        browser: Mutex::new("unknown".into()),
        listening: Mutex::new(Listening::default()),
        greeted: AtomicBool::new(false),
    })
}

fn to_extension(frames: &[Value]) {
    // One lock for all of a request's frames, so two requests' chunks never interleave.
    let mut out = lock(&host().out);
    for frame in frames {
        let _ = out.write_all(&encode_native(frame));
    }
    let _ = out.flush();
}

fn reply(client: &Client, client_id: &Option<Value>, rest: Value) {
    let mut message = Map::new();
    if let Some(id) = client_id {
        message.insert("id".into(), id.clone());
    }
    if let Value::Object(rest) = rest {
        message.extend(rest);
    }
    let mut writer = lock(client);
    let _ = writeln!(writer, "{}", Value::Object(message));
    let _ = writer.flush();
}

/// Turns `uploadFile {path, selector}` into the extension's fillForm file field, and
/// `dropFile {path, selector}` into its dropFile file, chunked if large.
fn file_messages(
    id: &str,
    action: &str,
    params: &Map<String, Value>,
) -> Result<Vec<Value>, String> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| format!("{action} needs `path`"))?;
    let bytes = std::fs::read(path).map_err(|e| node_error(&e, "open", Path::new(path)))?;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    let path_ref = Path::new(path);
    let ext = path_ref
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let mime = MIME
        .iter()
        .find(|(e, _)| *e == ext)
        .map_or("application/octet-stream", |(_, m)| m);
    let name = path_ref
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut rest = params.clone();
    rest.shift_remove("path");
    let request = |extra: Value| {
        let mut file = Map::new();
        file.insert("name".into(), json!(name));
        file.insert("type".into(), json!(mime));
        if let Value::Object(extra) = extra {
            file.extend(extra);
        }
        let mut rest = rest.clone();
        if action == "dropFile" {
            rest.insert("file".into(), Value::Object(file));
            return json!({ "id": id, "action": action, "params": rest });
        }
        let selector = params
            .get("selector")
            .and_then(Value::as_str)
            .unwrap_or(r#"input[type="file"]"#);
        rest.shift_remove("selector");
        rest.insert(
            "fields".into(),
            json!([{ "selector": selector, "file": file }]),
        );
        json!({ "id": id, "action": "fillForm", "params": rest })
    };
    if data.len() <= CHUNK_SIZE {
        return Ok(vec![request(json!({ "data": data }))]);
    }

    let transfer_id = format!("chunk_{id}");
    let total_chunks = data.len().div_ceil(CHUNK_SIZE);
    let mut frames = vec![json!({
        "type": "chunk_start", "transferId": transfer_id, "fileName": name, "mimeType": mime,
        "totalSize": data.len(), "totalChunks": total_chunks,
    })];
    // Base64 is ASCII, so byte offsets are character offsets.
    for (i, chunk) in data.as_bytes().chunks(CHUNK_SIZE).enumerate() {
        frames.push(json!({
            "type": "chunk_data", "transferId": transfer_id, "chunkIndex": i,
            "data": String::from_utf8_lossy(chunk),
        }));
    }
    frames.push(request(json!({ "chunkedTransfer": transfer_id })));
    Ok(frames)
}

fn handle_client(client: &Client, message: &Value) {
    let client_id = message.get("id").cloned();
    if message.get("__invalid").is_some() {
        return reply(
            client,
            &client_id,
            json!({ "ok": false, "error": "Invalid JSON" }),
        );
    }
    let action = message.get("action");
    if action.and_then(Value::as_str) == Some("hostInfo") {
        let browser = lock(&host().browser).clone();
        return reply(
            client,
            &client_id,
            json!({ "ok": true, "result": { "browser": browser, "pid": std::process::id() } }),
        );
    }
    let Some(action) = action.and_then(Value::as_str) else {
        return reply(
            client,
            &client_id,
            json!({ "ok": false, "error": "Missing action" }),
        );
    };
    if !ALLOWED_ACTIONS.contains(&action) {
        return reply(
            client,
            &client_id,
            json!({ "ok": false, "error": format!("Action not allowed: {action}") }),
        );
    }
    let params = match message.get("params") {
        Some(p @ (Value::Object(_) | Value::Array(_))) => p.clone(),
        _ => json!({}),
    };
    let refused = nested_commands(action, &params).into_iter().find(|c| {
        !c.get("action")
            .and_then(Value::as_str)
            .is_some_and(|a| NESTED_ACTIONS.contains(&a))
    });
    if let Some(refused) = refused {
        let named = super::actions::present(refused.get("action")).map_or_else(
            || "malformed command".to_string(),
            |a| super::actions::js_string(Some(a)),
        );
        return reply(
            client,
            &client_id,
            json!({ "ok": false, "error": format!("Action not allowed inside {action}: {named}") }),
        );
    }
    let n = host().counter.fetch_add(1, Ordering::SeqCst) + 1;
    let id = format!("h{}_{n}", std::process::id());
    let frames = if action == "uploadFile" || action == "dropFile" {
        match file_messages(&id, action, params.as_object().unwrap_or(&Map::new())) {
            Ok(frames) => frames,
            Err(error) => return reply(client, &client_id, json!({ "ok": false, "error": error })),
        }
    } else {
        vec![json!({ "id": id, "action": action, "params": params })]
    };
    let deadline = Instant::now() + Duration::from_millis(REQUEST_TIMEOUT_MS);
    lock(&host().pending).insert(
        id,
        Pending {
            client: Arc::clone(client),
            client_id,
            deadline,
        },
    );
    to_extension(&frames);
}

fn handle_extension(message: &Value) {
    if message.get("type").and_then(Value::as_str) == Some("hello") {
        let browser = message
            .get("browser")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        log(&format!("hello from {browser}"));
        *lock(&host().browser) = browser;
        listen();
        host().greeted.store(true, Ordering::SeqCst);
        return;
    }
    let id = message.get("id").and_then(Value::as_str).unwrap_or("");
    let Some(entry) = lock(&host().pending).remove(id) else {
        return;
    };
    let mut rest = message.clone();
    if let Some(o) = rest.as_object_mut() {
        // Keep the reply's own key order, with the caller's id where the host's was.
        match &entry.client_id {
            Some(client_id) => {
                o.insert("id".into(), client_id.clone());
            }
            None => {
                o.shift_remove("id");
            }
        }
    }
    let mut writer = lock(&entry.client);
    let _ = writeln!(writer, "{rest}");
    let _ = writer.flush();
}

/// Reads one MCP server's requests until it disconnects.
fn serve_connection(mut reader: impl Read, client: Client) {
    let mut decoder = LineDecoder::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                for message in decoder.push(&buf[..n]) {
                    handle_client(&client, &message);
                }
            }
        }
    }
}

/// Answers requests the extension never did, every tick.
fn expire_pending() {
    let now = Instant::now();
    let expired: Vec<Pending> = {
        let mut pending = lock(&host().pending);
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, p)| p.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect();
        ids.iter().filter_map(|id| pending.remove(id)).collect()
    };
    for p in expired {
        let error = format!("Request timed out after {}s", REQUEST_TIMEOUT_MS / 1000);
        reply(
            &p.client,
            &p.client_id,
            json!({ "ok": false, "error": error }),
        );
    }
}

/// Two hosts can serve one browser name at once: two profiles, or a second instance. The newest
/// takes the socket; when it goes away, watch() lets the older one take it back, so a browser
/// that is still open never ends up unreachable. On Windows a pipe name cannot be taken over,
/// so the newer host waits for it instead.
fn listen() {
    let mut listening = lock(&host().listening);
    if listening.active {
        return;
    }
    let path = socket_path(&lock(&host().browser));
    match platform::bind(&path) {
        Ok(own) => {
            listening.active = true;
            listening.own = own;
            log(&format!("listening on {path}"));
        }
        // Another host holds the pipe (Windows); watch() tries again.
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {}
        Err(e) => log(&format!("cannot listen on {path} {e}")),
    }
}

/// The listener died; the next watch() makes a new one.
#[cfg_attr(unix, allow(dead_code))]
fn stopped_listening() {
    *lock(&host().listening) = Listening::default();
}

fn watch() {
    if cfg!(windows) {
        return listen();
    }
    let own = lock(&host().listening).own.clone();
    let Some((path, ino)) = own else {
        return;
    };
    match platform::inode(&path) {
        Some(current) if current == ino => {}
        // A newer host holds the path; keep checking until it leaves.
        Some(_) => {}
        // The socket file is gone: the host that took it over has exited. The old listener is
        // left behind: no path reaches it any more, and std cannot interrupt its accept.
        None => {
            *lock(&host().listening) = Listening::default();
            listen();
        }
    }
}

fn shutdown() -> ! {
    // Remove the socket only if it is still ours, not a newer host's at the same path.
    if let Some((path, ino)) = lock(&host().listening).own.clone() {
        if platform::inode(&path) == Some(ino) {
            let _ = std::fs::remove_file(&path);
        }
    }
    std::process::exit(0)
}

pub fn main(_args: &[String]) -> i32 {
    // Browsers pass the extension's origin (and on Windows a window handle); none is needed.
    platform::on_terminate();
    std::thread::spawn(|| {
        let mut last_watch = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if platform::terminated() {
                shutdown();
            }
            expire_pending();
            if host().greeted.load(Ordering::SeqCst)
                && last_watch.elapsed() >= Duration::from_millis(WATCH_MS)
            {
                last_watch = Instant::now();
                watch();
            }
        }
    });
    let mut stdin = std::io::stdin().lock();
    let mut decoder = NativeDecoder::default();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) => shutdown(),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => shutdown(),
            Ok(n) => {
                for message in decoder.push(&buf[..n]) {
                    if message.is_object() {
                        handle_extension(&message);
                    }
                }
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::{serve_connection, Client};
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// Listens on `path` (a socket file in a 0700 folder, itself 0600) and serves each
    /// connection on its own thread. Returns the path and inode it made.
    pub fn bind(path: &str) -> io::Result<Option<(String, u64)>> {
        let dir = Path::new(path).parent().unwrap_or(Path::new("."));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let ino = std::fs::metadata(path)?.ino();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else {
                    // Out of file descriptors, say: let some close before trying again.
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    continue;
                };
                let Ok(writer) = stream.try_clone() else {
                    continue;
                };
                let client: Client = Arc::new(Mutex::new(Box::new(writer)));
                std::thread::spawn(move || serve_connection(stream, client));
            }
        });
        Ok(Some((path.to_string(), ino)))
    }

    pub fn inode(path: &str) -> Option<u64> {
        std::fs::metadata(path).ok().map(|m| m.ino())
    }

    static TERMINATED: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(_: i32) {
        TERMINATED.store(true, Ordering::SeqCst);
    }

    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
    }

    /// SIGTERM and SIGINT end the host as a closed stdin does, removing its socket.
    pub fn on_terminate() {
        const SIGINT: i32 = 2;
        const SIGTERM: i32 = 15;
        // SAFETY: `signal` is the C library's; the handler only stores to an atomic.
        unsafe {
            signal(SIGTERM, on_signal);
            signal(SIGINT, on_signal);
        }
    }

    pub fn terminated() -> bool {
        TERMINATED.load(Ordering::SeqCst)
    }
}

#[cfg(windows)]
mod platform {
    use super::{log, serve_connection, Client};
    use std::io;
    use std::sync::{Arc, Mutex};
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_IO_PENDING,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
        PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::CreateEventW;
    use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

    /// One end of a named pipe, opened for overlapped I/O: a reply can be written while the
    /// connection's reader thread waits in a read. Synchronous pipe handles serialise the two,
    /// so a reply would wait for the next request.
    struct Pipe(HANDLE);
    // SAFETY: a pipe handle may be used from any thread; overlapped I/O allows concurrent calls.
    unsafe impl Send for Pipe {}
    unsafe impl Sync for Pipe {}

    impl Drop for Pipe {
        fn drop(&mut self) {
            // SAFETY: the handle is owned and closed once.
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Pipe {
        /// Runs one overlapped operation to completion. `start` issues it with the OVERLAPPED.
        fn wait(&self, start: impl FnOnce(*mut OVERLAPPED) -> i32) -> io::Result<u32> {
            // SAFETY: plain Win32 calls on an owned handle; `overlapped` outlives the operation,
            // which GetOverlappedResult waits for before returning.
            unsafe {
                let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
                if event.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let mut overlapped: OVERLAPPED = std::mem::zeroed();
                overlapped.hEvent = event;
                let mut done = 0u32;
                let failed = (start(&mut overlapped) == 0 && GetLastError() != ERROR_IO_PENDING)
                    || GetOverlappedResult(self.0, &overlapped, &mut done, 1) == 0;
                let result = if failed {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(done)
                };
                CloseHandle(event);
                result
            }
        }
    }

    impl io::Read for &Pipe {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let handle = self.0;
            // SAFETY: `buf` is valid for `len` bytes until the wait returns.
            match self.wait(|ov| unsafe {
                ReadFile(handle, buf.as_mut_ptr(), len, std::ptr::null_mut(), ov)
            }) {
                Ok(n) => Ok(n as usize),
                Err(e) if e.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => Ok(0),
                Err(e) => Err(e),
            }
        }
    }

    impl io::Write for &Pipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let handle = self.0;
            // SAFETY: `buf` is valid for `len` bytes until the wait returns.
            self.wait(|ov| unsafe {
                WriteFile(handle, buf.as_ptr(), len, std::ptr::null_mut(), ov)
            })
            .map(|n| n as usize)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct Writer(Arc<Pipe>);
    impl io::Write for Writer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            (&*self.0).write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct Reader(Arc<Pipe>);
    impl io::Read for Reader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            (&*self.0).read(buf)
        }
    }

    fn create(name: &[u16], first: bool) -> io::Result<Pipe> {
        let open = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: `name` is NUL-terminated UTF-16; default security gives the creating user access.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let err = io::Error::last_os_error();
            // Another host owns the name: the same answer a unix bind gives for a held address.
            if matches!(err.raw_os_error(), Some(c) if c == ERROR_ACCESS_DENIED as i32 || c == ERROR_PIPE_BUSY as i32)
            {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, err));
            }
            return Err(err);
        }
        Ok(Pipe(handle))
    }

    /// Takes the pipe name if no other host holds it, and serves each connection on its own
    /// thread. Pipes leave no file behind, so there is no inode to track.
    pub fn bind(path: &str) -> io::Result<Option<(String, u64)>> {
        let name: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut pipe = create(&name, true)?;
        std::thread::spawn(move || loop {
            let handle = pipe.0;
            // SAFETY: the pipe is owned; the OVERLAPPED is waited on before it goes.
            let connected = pipe.wait(|ov| unsafe { ConnectNamedPipe(handle, ov) });
            let connected = match connected {
                Ok(_) => true,
                Err(e) => e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32),
            };
            // The next instance exists before this one is handed off, so the name is never free.
            let next = match create(&name, false) {
                Ok(next) => next,
                Err(e) => {
                    log(&format!(
                        "cannot listen on {} {e}",
                        String::from_utf16_lossy(&name[..name.len() - 1])
                    ));
                    super::stopped_listening(); // watch() tries again
                    return;
                }
            };
            let current = std::mem::replace(&mut pipe, next);
            if connected {
                let current = Arc::new(current);
                let client: Client = Arc::new(Mutex::new(Box::new(Writer(Arc::clone(&current)))));
                std::thread::spawn(move || serve_connection(Reader(current), client));
            }
        });
        Ok(None)
    }

    pub fn inode(_path: &str) -> Option<u64> {
        None
    }

    pub fn on_terminate() {}

    pub fn terminated() -> bool {
        false
    }
}
