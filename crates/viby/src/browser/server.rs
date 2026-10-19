//! `viby browser-server`: the MCP server (stdio) exposing one `browser` tool. Claude Code
//! starts one per session.
//!
//! It reaches the browser through the native host's unix socket (see host.rs). The tab this
//! session works in lives here, so two Claude sessions never share one by accident, and an
//! action with no tab is refused rather than run on whatever tab the person is looking at.

use super::actions::{
    bridge_request, explain_error, failed_result, format_result, input_schema, is_action, js_int,
    js_string, needs_tab, InputError, LOCAL_ACTIONS, TOOL_DESCRIPTION,
};
use super::install::{
    browser_app, chromium_extension_state, default_browser, detect_browsers, ensure_installed,
    extension_version, install, installed_app_here, plugin_version, process_env, safari_installed,
    ExtensionState, Family,
};
use super::protocol::{
    browser_home, home_dir, socket_path, LineDecoder, BROWSER_NAMES, PLATFORM, REQUEST_TIMEOUT_MS,
};
use crate::mcp::{serve, Server, Tool, ToolResult};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A host's answer to one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub ok: bool,
    pub result: Option<Value>,
    pub error: Option<String>,
}

impl Reply {
    fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }

    fn from_message(message: &Value) -> Self {
        Self {
            ok: super::actions::truthy(message.get("ok")),
            result: message.get("result").cloned(),
            error: super::actions::present(message.get("error")).map(|e| js_string(Some(e))),
        }
    }

    fn extension_version(&self) -> Option<&str> {
        self.result.as_ref()?.get("extensionVersion")?.as_str()
    }
}

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A connection error named as Node names it (`ENOENT`, `ECONNREFUSED`, …).
fn error_code(err: &std::io::Error) -> String {
    use std::io::ErrorKind as K;
    match err.kind() {
        K::NotFound => "ENOENT".into(),
        K::ConnectionRefused => "ECONNREFUSED".into(),
        K::PermissionDenied => "EACCES".into(),
        K::ConnectionReset => "ECONNRESET".into(),
        K::BrokenPipe => "EPIPE".into(),
        _ => err.to_string(),
    }
}

trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

#[cfg(unix)]
fn connect(socket: &str, timeout: Duration) -> std::io::Result<Box<dyn Stream>> {
    let stream = std::os::unix::net::UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(timeout))?;
    Ok(Box::new(stream))
}

#[cfg(windows)]
fn connect(socket: &str, timeout: Duration) -> std::io::Result<Box<dyn Stream>> {
    const ERROR_PIPE_BUSY: i32 = 231;
    let end = Instant::now() + timeout;
    loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(socket)
        {
            // Every instance is mid-handoff for a moment; the host makes the next one at once.
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < end => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
            Ok(file) => return Ok(Box::new(file)),
        }
    }
}

fn exchange(socket: &str, id: u64, line: &str, timeout: Duration) -> Option<Reply> {
    let mut stream = match connect(socket, timeout) {
        Ok(stream) => stream,
        Err(e) => {
            return Some(Reply::failed(format!(
                "browser not connected ({})",
                error_code(&e)
            )))
        }
    };
    if let Err(e) = stream.write_all(line.as_bytes()) {
        return Some(Reply::failed(format!(
            "browser not connected ({})",
            error_code(&e)
        )));
    }
    let mut decoder = LineDecoder::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => return None,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return None
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Some(Reply::failed(format!(
                    "browser not connected ({})",
                    error_code(&e)
                )))
            }
            Ok(n) => {
                for message in decoder.push(&buf[..n]) {
                    if message.get("id") == Some(&json!(id)) {
                        return Some(Reply::from_message(&message));
                    }
                }
            }
        }
    }
}

/// One request over a fresh socket connection; the host routes replies by id. A connection
/// the host closes without answering ends the wait at once (the TypeScript waited out the timer).
pub fn call_host(socket: &str, action: &str, params: Value, timeout: Duration) -> Reply {
    let id = REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    let line = format!(
        "{}\n",
        json!({ "id": id, "action": action, "params": params })
    );
    let (tx, rx) = mpsc::channel();
    let socket = socket.to_string();
    std::thread::spawn(move || {
        if let Some(reply) = exchange(&socket, id, &line, timeout) {
            let _ = tx.send(reply);
        }
    });
    rx.recv_timeout(timeout)
        .unwrap_or_else(|_| Reply::failed("Request timed out"))
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Browsers with a live host. A stale socket, or no pipe at all, fails to connect and is skipped.
fn live_browsers() -> Vec<String> {
    let checks: Vec<_> = BROWSER_NAMES
        .iter()
        .map(|name| {
            std::thread::spawn(move || {
                call_host(&socket_path(name), "hostInfo", json!({}), ms(1500)).ok
            })
        })
        .collect();
    BROWSER_NAMES
        .iter()
        .zip(checks)
        .filter_map(|(name, check)| matches!(check.join(), Ok(true)).then(|| name.to_string()))
        .collect()
}

enum CallError {
    /// Shown to the model as it is.
    Input(String),
    /// Shown as `browser <action> failed: …`.
    Failed(String),
}

impl From<InputError> for CallError {
    fn from(e: InputError) -> Self {
        CallError::Input(e.0)
    }
}

impl From<String> for CallError {
    fn from(e: String) -> Self {
        CallError::Failed(e)
    }
}

#[derive(Default)]
struct Session {
    tab_id: Option<i64>,
    browser: Option<String>,
    /// Fork names to tabs, in the order they were made.
    forks: Vec<(String, Option<i64>)>,
}

impl Session {
    fn fork(&self, name: &str) -> Option<&Option<i64>> {
        self.forks.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }

    fn set_fork(&mut self, name: String, tab: Option<i64>) {
        match self.forks.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = tab,
            None => self.forks.push((name, tab)),
        }
    }
}

const SANDBOX_HINT: &str = "\nPrivate windows need Viby Browser allowed in them: on the browser's extensions page, open Details on Viby Browser and turn on Allow in Private (Brave) or Allow in Incognito (Chrome, Edge). Ask the person to do that, or drop sandbox.";
const RELOAD_WAIT_MS: u64 = 10_000;
const HELP_EVERY_MS: u64 = 60_000;
const SETUP_WAIT_MS: u64 = 120_000;
const MAC: bool = cfg!(target_os = "macos");

/// AppleScript that opens `page` in a new front tab of a Chromium browser, launching it if needed.
pub fn chromium_tab_script(app_path: &str, page: &str) -> String {
    [
        format!("tell application {}", json!(app_path)),
        "  activate".into(),
        "  if (count of windows) = 0 then make new window".into(),
        format!(
            "  tell front window to make new tab with properties {{URL:{}}}",
            json!(page)
        ),
        "end tell".into(),
    ]
    .join("\n")
}

fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Opens the extensions page itself, not just the browser.
fn open_page(browser: &str, app: &str, page: &str) -> bool {
    if MAC {
        // `open -a Brave brave://extensions` starts Brave on a new tab and drops the address:
        // browsers refuse their internal pages from outside. Their AppleScript is allowed to.
        // Without the Automation permission (or for Firefox) the browser still opens and the
        // guide names the page.
        if browser != "firefox" && succeeds("osascript", &["-e", &chromium_tab_script(app, page)]) {
            return true;
        }
        return succeeds("open", &["-a", app, page]);
    }
    // Windows .exe and Linux binaries both take the page as an argument.
    Command::new(app)
        .arg(page)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Puts the path the person must pick on their clipboard. False when no clipboard tool exists.
fn copy_to_clipboard(text: &str) -> bool {
    let tools: &[(&str, &[&str])] = if MAC {
        &[("pbcopy", &[])]
    } else if cfg!(windows) {
        &[("clip", &[])]
    } else {
        &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])]
    };
    tools.iter().any(|(cmd, args)| {
        let Ok(mut child) = Command::new(cmd)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        child.wait().is_ok_and(|s| s.success())
    })
}

/// How to get the picked item into the system file picker, then confirm it. On macOS the item is
/// shown selected in a Finder window and dragged onto the picker: ⌘⇧G (Go to Folder) did nothing
/// in Brave's picker on macOS 26, measured. Windows and Linux pickers take a pasted path.
///
/// Dragging the folder onto the extensions page was the first design; a drop that misses the
/// list (or lands before Developer mode is on) makes the browser just browse the folder as
/// file:///, so the picker it is.
fn picker_steps(item: &str, confirm: &str) -> Vec<String> {
    if MAC {
        return vec![
            format!("Drag the highlighted {item} from the Finder window onto the file picker."),
            format!("Click {confirm}."),
        ];
    }
    if cfg!(windows) {
        let confirm = if confirm == "Select" {
            "Select Folder"
        } else {
            confirm
        };
        return vec![
            "Paste with Ctrl+V into the box at the bottom, press Enter.".into(),
            format!("Click {confirm}."),
        ];
    }
    vec![
        "Press Ctrl+L, paste with Ctrl+V, press Enter.".into(),
        format!("Click {confirm}."),
    ]
}

/// One browser to set up: the steps the person follows, already numbered in order.
struct Plan {
    browser: String,
    page: String,
    app: String,
    pick: PathBuf,
    intro: String,
    steps: Vec<String>,
    note: String,
}

/// A path as the person would type it: ~ for their home folder (macOS and Linux).
fn shown(path: &std::path::Path) -> String {
    let path = path.to_string_lossy().into_owned();
    let home = home_dir().to_string_lossy().into_owned();
    match path.strip_prefix(&format!("{home}/")) {
        Some(rest) if !cfg!(windows) => format!("~/{rest}"),
        _ => path,
    }
}

/// The one browser to set up, with steps for the state it is in. One browser, not every one
/// found: a person needs a single short list. Brave, Chrome, Edge, Chromium in that order;
/// Firefox only when none of those is installed, since it forgets the add-on on every restart.
fn onboarding_plan(live: &[String]) -> Option<Plan> {
    let folder = browser_home().join("extension").join("chromium");
    let home = home_dir();
    let found = detect_browsers(&home, PLATFORM, &process_env());
    let preferred = default_browser(PLATFORM, &home);
    // The person's default browser first, when it is a Chromium one; then Brave, Chrome, Edge, Chromium.
    let mut chromium: Vec<_> = found
        .into_iter()
        .filter(|t| t.family == Family::Chromium && installed_app_here(&t.browser).is_some())
        .collect();
    chromium.sort_by_key(|t| Some(&t.browser) != preferred.as_ref());
    let finder = if MAC {
        "No Finder window?"
    } else {
        "Clipboard empty?"
    };
    if let Some(target) = chromium.iter().find(|t| !live.contains(&t.browser)) {
        let app = browser_app(&target.browser)?;
        let others: Vec<&str> = chromium
            .iter()
            .filter(|t| t.browser != target.browser)
            .filter_map(|t| browser_app(&t.browser).map(|a| a.name))
            .collect();
        let mut load = vec![
            format!("Turn on {}.", app.dev_mode),
            format!("Click {}.", app.load_unpacked),
        ];
        load.extend(picker_steps("\"chromium\" folder", "Select"));
        let steps = match chromium_extension_state(&target.app_dir, &folder) {
            ExtensionState::Disabled { .. } => vec!["Switch on \"Viby Browser\".".into()],
            ExtensionState::OtherFolder { .. } => {
                let mut steps = vec!["Click Remove on the old \"Viby Browser\".".to_string()];
                steps.extend(load.into_iter().skip(1));
                steps
            }
            ExtensionState::Installed { .. } => {
                vec!["Click the reload arrow on \"Viby Browser\".".into()]
            }
            ExtensionState::Missing => load,
        };
        let name = app.name;
        let also = match others.len() {
            0 => String::new(),
            1 => format!(" ({} works the same way.)", others[0]),
            _ => format!(" ({} work the same way.)", others.join(" and ")),
        };
        let firefox_default = if preferred.as_deref() == Some("firefox") {
            format!(" Firefox is your default, but it forgets add-ons it did not get from Mozilla on every restart, so this uses {name}.")
        } else {
            String::new()
        };
        let opening = if MAC {
            ", next to a Finder window with the folder to pick"
        } else {
            ", and the folder to pick is on your clipboard"
        };
        return Some(Plan {
            browser: target.browser.clone(),
            page: app.page.into(),
            app: installed_app_here(&target.browser)?.to_string_lossy().into_owned(),
            intro: format!("Let me use {name}: one time, about 20 seconds. {name} is opening its extensions page{opening}."),
            steps,
            note: format!(
                "That's it: it connects by itself and stays connected after restarts.{also}{firefox_default}\n  Page didn't open? Go to {}. {finder} The folder is {}.",
                app.page,
                shown(&folder)
            ),
            pick: folder,
        });
    }
    let firefox_app = installed_app_here("firefox");
    if let (true, Some(firefox_app), false) = (
        chromium.is_empty(),
        firefox_app,
        live.iter().any(|b| b == "firefox"),
    ) {
        let manifest = browser_home()
            .join("extension")
            .join("firefox")
            .join("manifest.json");
        let app = browser_app("firefox")?;
        let opening = if MAC {
            ", next to a Finder window with the file to pick"
        } else {
            ", and the file to pick is on your clipboard"
        };
        let mut steps = vec![format!("Click {}", app.load_unpacked)];
        steps.extend(picker_steps("\"manifest.json\" file", "Open"));
        return Some(Plan {
            browser: "firefox".into(),
            page: app.page.into(),
            app: firefox_app.to_string_lossy().into_owned(),
            intro: format!("Let me use Firefox: about 20 seconds. Firefox is opening its debugging page{opening}."),
            steps,
            note: format!(
                "Firefox forgets add-ons it did not get from Mozilla when it quits, so this repeats after each restart; Brave, Chrome or Edge keep it.\n  Page didn't open? Go to {}. {finder} The file is {}.",
                app.page,
                shown(&manifest)
            ),
            pick: manifest,
        });
    }
    None
}

/// Opens a web page in the default browser (Safari counts: it only has to show a download page).
fn open_url(url: &str) {
    if MAC {
        succeeds("open", &[url]);
    } else if cfg!(windows) {
        succeeds("cmd", &["/c", "start", "", url]);
    } else {
        succeeds("xdg-open", &[url]);
    }
}

fn person_steps(plan: &Plan, opened: bool) -> Vec<String> {
    let mut steps = Vec::new();
    if !opened {
        steps.push(format!("Open {}.", plan.page));
    }
    steps.extend(plan.steps.iter().cloned());
    let intro = if opened {
        plan.intro.clone()
    } else {
        match regex::Regex::new(r" \S+ is opening .*$") {
            Ok(re) => re.replace(&plan.intro, "").into_owned(),
            Err(_) => plan.intro.clone(),
        }
    };
    let mut lines = vec![format!("  {intro}")];
    lines.extend(
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("  {}. {s}", i + 1)),
    );
    lines.push(format!("  {}", plan.note));
    lines
}

fn guide(plan: Option<&Plan>, opened: bool, after_setup: bool) -> String {
    let Some(plan) = plan else {
        let mut lines = vec![
            "Browser control is not connected and no usable browser is installed. Show the person this, then stop:".to_string(),
            String::new(),
            format!(
                "  I can't find a browser I can work with{}. Install Brave (or one of these), open it once, then ask me again:",
                if opened { ", so I've opened Brave's download page" } else { "" }
            ),
        ];
        for key in ["brave", "chrome", "edge", "firefox"] {
            if let Some(app) = browser_app(key) {
                lines.push(format!("  • {}: {}", app.name, app.download));
            }
        }
        if safari_installed(PLATFORM, &process_env(), &home_dir()) {
            lines.push(
                "  Safari is installed, but it does not allow this kind of extension.".into(),
            );
        }
        lines.push(String::new());
        lines.push("Do not call setup: there is nothing to set up until one is installed.".into());
        return lines.join("\n");
    };
    let steps = person_steps(plan, opened);
    let head = if after_setup {
        "Not connected yet after two minutes. If the person is still on it, wait for them to say done, then call action='status'. If they are stuck, show these steps again:"
    } else {
        "Browser control is not connected yet. Show the person these steps exactly as written, as a numbered list, and end your reply there. When they say it is done, call the tool again: it connects as soon as the extension is loaded."
    };
    let mut lines = vec![head.to_string(), String::new()];
    lines.extend(steps);
    lines.join("\n")
}

/// The browser tool's state for one Claude session: its browser, its tab and its forks.
pub struct BrowserServer {
    /// The viby binary install copies for the browser to run: the running one, outside tests.
    exe: PathBuf,
    session: Mutex<Session>,
    synced: Mutex<HashSet<String>>,
    last_help: Mutex<Option<Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The extension keeps one fork list for every session; this prefix keeps each session to its own.
fn fork_prefix() -> String {
    format!("s{}-", std::process::id())
}

impl BrowserServer {
    pub fn new(exe: PathBuf) -> Self {
        Self {
            exe,
            session: Mutex::new(Session::default()),
            synced: Mutex::new(HashSet::new()),
            last_help: Mutex::new(None),
        }
    }

    fn install(&self) -> Result<(), String> {
        install(&self.exe, &home_dir(), PLATFORM).map(|_| ())
    }

    /// Brings ~/.viby/browser up to this plugin's version. Silent: nothing the person must do.
    fn ensure_installed(&self) -> Result<(), String> {
        ensure_installed(&self.exe, &home_dir(), PLATFORM)
    }

    fn choose_browser(&self, requested: Option<&Value>) -> Result<String, CallError> {
        let live = live_browsers();
        if let Some(requested) = requested.and_then(Value::as_str) {
            if !live.iter().any(|b| b == requested) {
                let connected = if live.is_empty() {
                    "none".to_string()
                } else {
                    live.join(", ")
                };
                return Err(CallError::Input(format!(
                    "{requested} is not connected. Connected: {connected}. Check action='status'."
                )));
            }
            return Ok(requested.to_string());
        }
        if let Some(browser) = lock(&self.session)
            .browser
            .clone()
            .filter(|b| live.contains(b))
        {
            return Ok(browser);
        }
        match live.len() {
            1 => Ok(live[0].clone()),
            0 => {
                self.ensure_installed()?;
                Err(CallError::Input(self.help_now(&live)))
            }
            _ => Err(CallError::Input(format!(
                "Several browsers are connected ({}); pass browser=… once.",
                live.join(", ")
            ))),
        }
    }

    /// A browser still running an older extension gets the new files and is told to reload itself,
    /// once per browser per session. The reload restarts the native host too, so this waits for the
    /// new one to answer. A browser that loads the extension from somewhere else keeps its version;
    /// the call then goes ahead with it.
    fn sync_extension(&self, browser: &str) -> Result<(), String> {
        if !lock(&self.synced).insert(browser.to_string()) {
            return Ok(());
        }
        let socket = socket_path(browser);
        let wanted = extension_version();
        let ping = call_host(&socket, "ping", json!({}), ms(3000));
        match ping.extension_version() {
            Some(running) if running != wanted => {}
            _ => return Ok(()),
        }
        self.install()?;
        call_host(&socket, "reload", json!({}), ms(3000));
        let end = Instant::now() + ms(RELOAD_WAIT_MS);
        while Instant::now() < end {
            std::thread::sleep(ms(500));
            if call_host(&socket, "ping", json!({}), ms(1500)).extension_version()
                == Some(wanted.as_str())
            {
                return Ok(());
            }
        }
        Ok(())
    }

    fn no_fork(&self, name: &str) -> String {
        let session = lock(&self.session);
        let forks = if session.forks.is_empty() {
            String::new()
        } else {
            format!(
                " Forks: {}.",
                session
                    .forks
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        format!("No fork named {name} in this session.{forks} Make one with action='fork'.")
    }

    /// Does the person's first move for them: opens the extensions page and copies the path, or,
    /// with no usable browser, opens Brave's download page. At most once a minute, so repeated
    /// calls do not keep throwing windows at them. VIBY_BROWSER_NO_OPEN=1 turns it off (tests).
    fn open_help(&self, plan: Option<&Plan>, force: bool) -> bool {
        if std::env::var("VIBY_BROWSER_NO_OPEN").as_deref() == Ok("1") {
            return false;
        }
        {
            let mut last = lock(&self.last_help);
            if !force && last.is_some_and(|t| t.elapsed() < ms(HELP_EVERY_MS)) {
                return true; // opened a moment ago
            }
            *last = Some(Instant::now());
        }
        let Some(plan) = plan else {
            if let Some(brave) = browser_app("brave") {
                open_url(brave.download);
            }
            return true;
        };
        let opened = open_page(&plan.browser, &plan.app, &plan.page);
        if MAC {
            // Last, so Finder is in front with the item selected.
            succeeds("open", &["-R", &plan.pick.to_string_lossy()]);
        } else {
            copy_to_clipboard(&plan.pick.to_string_lossy());
        }
        opened
    }

    /// The guide for a session with nothing connected, after doing what can be done for the person.
    fn help_now(&self, live: &[String]) -> String {
        let plan = onboarding_plan(live);
        let opened = self.open_help(plan.as_ref(), false);
        guide(plan.as_ref(), opened, false)
    }

    fn status(&self) -> Result<String, CallError> {
        self.ensure_installed()?;
        let live = live_browsers();
        let (browser, tab) = {
            let s = lock(&self.session);
            (
                s.browser.clone().unwrap_or_else(|| "-".into()),
                s.tab_id.map_or_else(|| "-".into(), |t| t.to_string()),
            )
        };
        let connected = if live.is_empty() {
            "none".to_string()
        } else {
            live.join(", ")
        };
        let mut lines = vec![format!(
            "viby-toolkit {}; connected: {connected}; this session: {browser} tab {tab}",
            plugin_version()
        )];
        for name in &live {
            let ping = call_host(&socket_path(name), "ping", json!({}), ms(3000));
            let state = if ping.ok {
                format!("extension {}", ping.extension_version().unwrap_or("?"))
            } else {
                ping.error.unwrap_or_default()
            };
            lines.push(format!("  {name}: {state}"));
        }
        lines.push(if live.is_empty() {
            self.help_now(&live)
        } else {
            "Ready.".into()
        });
        Ok(lines.join("\n"))
    }

    /// Guided setup: install, open each browser's extensions page, put the path to pick on the
    /// clipboard, then wait for the extension to connect, so the call itself ends with "connected".
    fn setup(&self) -> Result<String, CallError> {
        self.install()?;
        let before = live_browsers();
        let plan = onboarding_plan(&before);
        if !before.is_empty() && plan.is_none() {
            return Ok(format!("Connected: {}. Ready.", before.join(", ")));
        }
        let Some(plan) = plan else {
            let opened = self.open_help(None, true);
            return Ok(guide(None, opened, false));
        };
        let opened = self.open_help(Some(&plan), true);
        let end = Instant::now() + ms(SETUP_WAIT_MS);
        while Instant::now() < end {
            let added: Vec<String> = live_browsers()
                .into_iter()
                .filter(|b| !before.contains(b))
                .collect();
            if !added.is_empty() {
                return Ok(format!(
                    "Connected: {}. Ready — tell the person it worked.",
                    added.join(", ")
                ));
            }
            std::thread::sleep(ms(1000));
        }
        Ok(guide(Some(&plan), opened, true))
    }

    pub fn call_tool(&self, input: &Value) -> ToolResult {
        let action = input.get("action");
        if !is_action(action.unwrap_or(&Value::Null)) {
            return ToolResult::error(format!("Unknown action: {}", js_string(action)));
        }
        let action = action.and_then(Value::as_str).unwrap_or_default();
        match self.run(action, input) {
            Ok(result) => result,
            Err(CallError::Input(message)) => ToolResult::error(message),
            Err(CallError::Failed(message)) => {
                ToolResult::error(format!("browser {action} failed: {message}"))
            }
        }
    }

    fn run(&self, action: &str, input: &Value) -> Result<ToolResult, CallError> {
        if LOCAL_ACTIONS.contains(&action) {
            return Ok(ToolResult::text(if action == "setup" {
                self.setup()?
            } else {
                self.status()?
            }));
        }
        let browser = self.choose_browser(input.get("browser"))?;
        {
            let mut s = lock(&self.session);
            if s.browser.as_deref() != Some(browser.as_str()) {
                s.browser = Some(browser.clone());
                s.tab_id = None;
                s.forks.clear();
            }
        }
        self.sync_extension(&browser)?;
        let fork = input.get("fork").and_then(Value::as_str);
        let in_fork = fork.is_some() && action != "kill_fork";
        let (tab_id, session_tab_id) = {
            let s = lock(&self.session);
            let tab = match fork {
                Some(name) if in_fork => s.fork(name).copied().flatten(),
                _ => js_int(input.get("tab_id")).or(s.tab_id),
            };
            (tab, s.tab_id)
        };
        if in_fork && tab_id.is_none() {
            return Err(CallError::Input(self.no_fork(fork.unwrap_or_default())));
        }
        if needs_tab(action) && tab_id.is_none() {
            return Err(CallError::Input(
                "No tab yet. Use action='open' with a url, or list_tabs then select_tab.".into(),
            ));
        }
        if action == "kill_fork" && fork.is_none_or(|name| lock(&self.session).fork(name).is_none())
        {
            return Err(CallError::Input(
                self.no_fork(&js_string(input.get("fork"))),
            ));
        }
        if action == "fork" {
            let taken = input
                .get("forks")
                .and_then(Value::as_array)
                .and_then(|forks| {
                    let s = lock(&self.session);
                    forks.iter().find_map(|f| {
                        f.get("name")
                            .and_then(Value::as_str)
                            .filter(|n| s.fork(n).is_some())
                            .map(str::to_string)
                    })
                });
            if let Some(name) = taken {
                return Err(CallError::Input(format!(
                    "A fork named {name} already exists; kill_fork it first or pick another name."
                )));
            }
        }

        let (bridge_action, mut params) = bridge_request(action, input, tab_id)?;
        let prefix = fork_prefix();
        if action == "fork" {
            if let Some(paths) = params.get_mut("paths").and_then(Value::as_array_mut) {
                for path in paths {
                    let name = format!("{prefix}{}", js_string(path.get("name")));
                    path["name"] = json!(name);
                }
            }
        }
        if action == "kill_fork" {
            params["fork"] = json!(format!("{prefix}{}", js_string(params.get("fork"))));
        }
        let session_tab = tab_id.is_some() && tab_id == session_tab_id && !in_fork;
        let socket = socket_path(&browser);
        let timeout = ms(REQUEST_TIMEOUT_MS + 2000);
        let mut reply = call_host(&socket, &bridge_action, params, timeout);
        let mut note = "";
        let tab_closed = regex::Regex::new(r"(?i)no tab with id")
            .is_ok_and(|re| re.is_match(reply.error.as_deref().unwrap_or("")));
        if !reply.ok && tab_closed && tab_id.is_some() {
            // The tab is gone: forget it, so the session does not keep failing on it.
            {
                let mut s = lock(&self.session);
                if session_tab {
                    s.tab_id = None;
                }
                s.forks.retain(|(_, id)| *id != tab_id);
            }
            if action == "open" && session_tab {
                note = "The session's tab had been closed, so this opened a new background tab.\n";
                let (retry_action, retry_params) = bridge_request(action, input, None)?;
                reply = call_host(&socket, &retry_action, retry_params, timeout);
            }
        }
        if !reply.ok {
            let hint = if input.get("sandbox") == Some(&json!(true)) {
                SANDBOX_HINT
            } else {
                ""
            };
            let error = reply.error.as_deref().unwrap_or("unknown error");
            return Ok(ToolResult::error(format!(
                "{}{hint}",
                explain_error(action, error)
            )));
        }

        let mut result = reply.result.unwrap_or(Value::Null);
        {
            let mut s = lock(&self.session);
            let result_tab = js_int(result.get("tabId"));
            if ["open", "new_tab", "new_window", "select_tab"].contains(&action)
                && result_tab.is_some()
                && !in_fork
            {
                s.tab_id = result_tab;
            } else if action == "select_tab" {
                s.tab_id = js_int(input.get("tab_id"));
            }
            if action == "fork" || action == "list_forks" {
                if let Some(forks) = result.get_mut("forks").and_then(Value::as_array_mut) {
                    let mine: Vec<Value> = forks
                        .iter()
                        .filter_map(|f| {
                            let name = js_string(f.get("name"));
                            let short = name.strip_prefix(&prefix)?.to_string();
                            let mut f = f.clone();
                            if let Some(o) = f.as_object_mut() {
                                o.insert("name".into(), json!(short));
                            }
                            Some(f)
                        })
                        .collect();
                    *forks = mine;
                    if action == "list_forks" {
                        s.forks.clear();
                    }
                    for f in forks.iter() {
                        if f.get("alive") != Some(&json!(false)) {
                            s.set_fork(js_string(f.get("name")), js_int(f.get("tabId")));
                        }
                    }
                }
            }
            if action == "kill_fork" {
                if let Some(name) = fork {
                    s.forks.retain(|(n, _)| n != name);
                }
            }
        }
        let mut content = format_result(action, &result, input);
        if !note.is_empty() {
            if let Some(first) = content.first_mut() {
                first["text"] = json!(format!("{note}{}", js_string(first.get("text"))));
            }
        }
        Ok(ToolResult {
            content,
            is_error: failed_result(action, &result),
        })
    }
}

pub fn main(_args: &[String]) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("viby browser-server: cannot find its own binary: {e}");
            return 1;
        }
    };
    let server = BrowserServer::new(exe);
    serve(
        Server {
            name: "viby-browser",
            version: plugin_version(),
        },
        Tool {
            name: "browser",
            description: TOOL_DESCRIPTION.into(),
            input_schema: input_schema(),
        },
        move |input| server.call_tool(&input),
    );
    0
}
