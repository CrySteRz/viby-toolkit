//! Builds the extension for each browser family and installs the native host.
//!
//! Everything lands in browser_home() (~/.viby/browser), not the plugin cache: the cache path
//! changes with every plugin version, but the native-host manifest and the folder the person
//! loaded the extension from must stay put.
//!
//! The host is this binary: install copies the running `viby` into browser_home() and the
//! launcher the browser starts (host.sh, or host.cmd on Windows; the names registered browsers
//! already point at) runs `<copy> browser-host`. A copy, not the original, because the original
//! lives in a per-version cache that the next update deletes. The extension's files and the
//! plugin version are compiled in, so a downloaded binary needs nothing from the plugin folder.
//!
//! Port of scripts/build-extensions.py from 1jehuang/firefox-agent-bridge (MIT, see NOTICE.md).

use super::protocol::{
    browser_home, home_dir, node_error, CHROMIUM_EXTENSION_ID, CHROMIUM_PUBLIC_KEY,
    FIREFOX_EXTENSION_ID, NATIVE_HOST_NAME, PLATFORM,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

macro_rules! extension_file {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                "../../../../plugins/viby-toolkit/browser/extension/",
                $name
            )) as &[u8],
        )
    };
}

/// The vendored extension (plugins/viby-toolkit/browser/extension), minus its manifest, which
/// is written per browser family. A test checks this list against the folder.
pub const EXTENSION_FILES: &[(&str, &[u8])] = &[
    extension_file!("background.js"),
    extension_file!("content.js"),
    extension_file!("popup.html"),
    extension_file!("popup.js"),
    extension_file!("LICENSE"),
    extension_file!("icons/icon-16.png"),
    extension_file!("icons/icon-32.png"),
    extension_file!("icons/icon-48.png"),
    extension_file!("icons/icon-96.png"),
    extension_file!("icons/icon.svg"),
];
const EXTENSION_MANIFEST: &str =
    include_str!("../../../../plugins/viby-toolkit/browser/extension/manifest.json");
const PLUGIN_MANIFEST: &str =
    include_str!("../../../../plugins/viby-toolkit/.claude-plugin/plugin.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Chromium,
    Firefox,
}

/// `host_dir` holds the native-host manifest; on Windows the browser finds it through `registry_key`.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserTarget {
    pub browser: String,
    pub family: Family,
    pub app_dir: PathBuf,
    pub host_dir: PathBuf,
    pub registry_key: Option<String>,
}

/// Environment variables, as Node's `process.env`: a test passes its own.
pub type Env = HashMap<String, String>;

/// This process's environment. Windows names are case-insensitive, so they are kept upper-case.
pub fn process_env() -> Env {
    std::env::vars()
        .map(|(k, v)| (if cfg!(windows) { k.to_uppercase() } else { k }, v))
        .collect()
}

fn env_get<'a>(env: &'a Env, key: &str) -> Option<&'a str> {
    env.get(key).map(String::as_str).filter(|v| !v.is_empty())
}

fn write_err(err: std::io::Error, syscall: &str, path: &Path) -> String {
    node_error(&err, syscall, path)
}

pub fn chromium_manifest(mv2: &Value) -> Value {
    let mut m = Map::new();
    m.insert("manifest_version".into(), json!(3));
    for key in ["name", "version", "description", "icons"] {
        if let Some(v) = mv2.get(key) {
            m.insert(key.into(), v.clone());
        }
    }
    m.insert("key".into(), json!(CHROMIUM_PUBLIC_KEY));
    m.insert("minimum_chrome_version".into(), json!("116"));
    m.insert(
        "permissions".into(),
        json!([
            "tabs",
            "activeTab",
            "nativeMessaging",
            "storage",
            "webNavigation",
            "downloads",
            "scripting",
            "alarms"
        ]),
    );
    // With "Allow user scripts" on, evaluate also works on pages whose CSP forbids eval.
    m.insert("optional_permissions".into(), json!(["userScripts"]));
    m.insert("host_permissions".into(), json!(["<all_urls>"]));
    m.insert(
        "background".into(),
        json!({ "service_worker": "background.js" }),
    );
    if let Some(v) = mv2.get("content_scripts") {
        m.insert("content_scripts".into(), v.clone());
    }
    if let Some(v) = mv2.get("browser_action") {
        m.insert("action".into(), v.clone());
    }
    Value::Object(m)
}

/// Where the built extension for each family went.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionDirs {
    pub chromium: PathBuf,
    pub firefox: PathBuf,
}

fn pretty_line(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}

/// Writes `<out_dir>/chromium` (MV3) and `<out_dir>/firefox` (MV2) from the vendored source.
pub fn build_extension(out_dir: &Path) -> Result<ExtensionDirs, String> {
    let mv2: Value =
        serde_json::from_str(EXTENSION_MANIFEST).map_err(|e| format!("extension manifest: {e}"))?;
    let mut dirs = Vec::new();
    for (family, manifest) in [
        ("chromium", chromium_manifest(&mv2)),
        ("firefox", mv2.clone()),
    ] {
        let dir = out_dir.join(family);
        match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(write_err(e, "rm", &dir))
            }
            _ => {}
        }
        for (name, bytes) in EXTENSION_FILES {
            let file = dir.join(name);
            if let Some(parent) = file.parent() {
                fs::create_dir_all(parent).map_err(|e| write_err(e, "mkdir", parent))?;
            }
            fs::write(&file, bytes).map_err(|e| write_err(e, "open", &file))?;
        }
        let file = dir.join("manifest.json");
        fs::write(&file, pretty_line(&manifest)).map_err(|e| write_err(e, "open", &file))?;
        dirs.push(dir);
    }
    let firefox = dirs.pop().unwrap_or_default();
    let chromium = dirs.pop().unwrap_or_default();
    Ok(ExtensionDirs { chromium, firefox })
}

/// Where each supported browser's program lives, and how to send the person to get it.
pub struct BrowserApp {
    pub key: &'static str,
    pub name: &'static str,
    pub page: &'static str,
    /// Where Developer mode and Load unpacked sit on that browser's extensions page.
    pub dev_mode: &'static str,
    pub load_unpacked: &'static str,
    pub download: &'static str,
    pub mac: &'static str,
    pub win: &'static [&'static str],
    pub linux: &'static [&'static str],
}

const CHROME_DEV_MODE: &str = "Developer mode (the switch at the top right)";
const CHROME_LOAD_UNPACKED: &str = "Load unpacked (top left)";

pub const BROWSER_APPS: &[BrowserApp] = &[
    BrowserApp {
        key: "brave",
        name: "Brave",
        page: "brave://extensions",
        dev_mode: CHROME_DEV_MODE,
        load_unpacked: CHROME_LOAD_UNPACKED,
        download: "https://brave.com/download/",
        mac: "Brave Browser",
        win: &["BraveSoftware\\Brave-Browser\\Application\\brave.exe"],
        linux: &["brave-browser", "brave"],
    },
    BrowserApp {
        key: "chrome",
        name: "Chrome",
        page: "chrome://extensions",
        dev_mode: CHROME_DEV_MODE,
        load_unpacked: CHROME_LOAD_UNPACKED,
        download: "https://www.google.com/chrome/",
        mac: "Google Chrome",
        win: &["Google\\Chrome\\Application\\chrome.exe"],
        linux: &["google-chrome", "google-chrome-stable"],
    },
    BrowserApp {
        key: "chromium",
        name: "Chromium",
        page: "chrome://extensions",
        dev_mode: CHROME_DEV_MODE,
        load_unpacked: CHROME_LOAD_UNPACKED,
        download: "https://www.chromium.org/getting-involved/download-chromium/",
        mac: "Chromium",
        win: &["Chromium\\Application\\chrome.exe"],
        linux: &["chromium", "chromium-browser"],
    },
    BrowserApp {
        key: "edge",
        name: "Edge",
        page: "edge://extensions",
        dev_mode:
            "Developer mode (the switch in the menu on the left; click ☰ if the menu is hidden)",
        load_unpacked: "Load unpacked (above the list of extensions)",
        download: "https://www.microsoft.com/edge",
        mac: "Microsoft Edge",
        win: &["Microsoft\\Edge\\Application\\msedge.exe"],
        linux: &["microsoft-edge", "microsoft-edge-stable"],
    },
    BrowserApp {
        key: "firefox",
        name: "Firefox",
        page: "about:debugging#/runtime/this-firefox",
        dev_mode: "",
        load_unpacked: "Load Temporary Add-on…",
        download: "https://www.mozilla.org/firefox/",
        mac: "Firefox",
        win: &["Mozilla Firefox\\firefox.exe"],
        linux: &["firefox"],
    },
];

pub fn browser_app(key: &str) -> Option<&'static BrowserApp> {
    BROWSER_APPS.iter().find(|a| a.key == key)
}

/// macOS bundle ids, Windows ProgIds (prefixes) and Linux .desktop names of the supported browsers.
const HANDLERS: &[(&str, &str)] = &[
    ("com.brave.browser", "brave"),
    ("com.google.chrome", "chrome"),
    ("org.chromium.chromium", "chromium"),
    ("com.microsoft.edgemac", "edge"),
    ("org.mozilla.firefox", "firefox"),
    ("bravehtml", "brave"),
    ("chromehtml", "chrome"),
    ("chromiumhtm", "chromium"),
    ("msedgehtm", "edge"),
    ("firefoxurl", "firefox"),
    ("brave-browser.desktop", "brave"),
    ("google-chrome.desktop", "chrome"),
    ("chromium.desktop", "chromium"),
    ("chromium-browser.desktop", "chromium"),
    ("microsoft-edge.desktop", "edge"),
    ("firefox.desktop", "firefox"),
];

pub fn browser_for_handler(id: &str) -> Option<&'static str> {
    let key = super::actions::js_trim(id).to_lowercase();
    HANDLERS
        .iter()
        .find(|(prefix, _)| key == *prefix || key.starts_with(prefix))
        .map(|(_, browser)| *browser)
}

fn output_of(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The person's default web browser, when it is one we support. VIBY_BROWSER_DEFAULT overrides it
/// (empty: none), so tests and simulations do not depend on this machine.
pub fn default_browser(platform: &str, home: &Path) -> Option<String> {
    if let Some(forced) = std::env::var_os("VIBY_BROWSER_DEFAULT") {
        let forced = forced.to_string_lossy().into_owned();
        return (!forced.is_empty()).then_some(forced);
    }
    let found = match platform {
        "darwin" => {
            let plist = home.join("Library/Preferences/com.apple.LaunchServices/com.apple.launchservices.secure.plist");
            let out = output_of(
                "plutil",
                &["-convert", "json", "-o", "-", &plist.to_string_lossy()],
            )?;
            let parsed: Value = serde_json::from_str(&out).ok()?;
            let handler =
                parsed.get("LSHandlers")?.as_array()?.iter().find(|h| {
                    h.get("LSHandlerURLScheme").and_then(Value::as_str) == Some("https")
                })?;
            let role = super::actions::present(handler.get("LSHandlerRoleAll"))
                .map(|v| super::actions::js_string(Some(v)))
                .unwrap_or_default();
            browser_for_handler(&role)
        }
        "win32" => {
            let key = r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice";
            let out = output_of("reg", &["query", key, "/v", "ProgId"])?;
            let re = regex::Regex::new(r"(?i)ProgId\s+REG_SZ\s+(\S+)").ok()?;
            let prog_id = re.captures(&out)?.get(1)?.as_str().to_string();
            browser_for_handler(&prog_id)
        }
        _ => {
            let out = output_of("xdg-settings", &["get", "default-web-browser"])?;
            if out.is_empty() {
                return None;
            }
            browser_for_handler(&out)
        }
    };
    found.map(str::to_string)
}

const DELIMITER: char = if cfg!(windows) { ';' } else { ':' };

/// Folders searched for browser programs. VIBY_BROWSER_APP_ROOTS (path-list) replaces them, so a
/// fresh machine can be simulated on a real one. On macOS only /Applications and ~/Applications:
/// LaunchServices would also find test-runner copies (Playwright's Chromium in ~/Library/Caches).
fn app_roots(platform: &str, env: &Env, home: &Path) -> Vec<PathBuf> {
    let split = |list: &str| {
        list.split(DELIMITER)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect()
    };
    if let Some(roots) = env.get("VIBY_BROWSER_APP_ROOTS") {
        return split(roots);
    }
    match platform {
        "darwin" => vec![PathBuf::from("/Applications"), home.join("Applications")],
        "win32" => ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMFILES(X86)"]
            .iter()
            .filter_map(|k| env_get(env, k).map(PathBuf::from))
            .collect(),
        _ => split(env.get("PATH").map_or("", String::as_str)),
    }
}

/// The installed program for a browser (macOS .app, Windows .exe, Linux binary), or None.
pub fn installed_app(browser: &str, platform: &str, env: &Env, home: &Path) -> Option<PathBuf> {
    let target = browser_app(browser)?;
    let names: Vec<String> = match platform {
        "darwin" => vec![format!("{}.app", target.mac)],
        "win32" => target.win.iter().map(|s| s.to_string()).collect(),
        _ => target.linux.iter().map(|s| s.to_string()).collect(),
    };
    app_roots(platform, env, home)
        .iter()
        .flat_map(|root| names.iter().map(move |n| root.join(n)))
        .find(|p| p.exists())
}

/// installed_app for this machine.
pub fn installed_app_here(browser: &str) -> Option<PathBuf> {
    installed_app(browser, PLATFORM, &process_env(), &home_dir())
}

/// Safari is common on a Mac and cannot run this extension, so the guide names it.
pub fn safari_installed(platform: &str, env: &Env, home: &Path) -> bool {
    platform == "darwin"
        && app_roots(platform, env, home)
            .iter()
            .any(|r| r.join("Safari.app").exists())
}

/// Browsers installed (program found) or used (profile folder found), with where their native-host manifests go.
pub fn detect_browsers(home: &Path, platform: &str, env: &Env) -> Vec<BrowserTarget> {
    let mac = |p: &str| home.join("Library").join("Application Support").join(p);
    let linux = |p: &str| home.join(".config").join(p);
    let local = |p: &str| {
        env_get(env, "LOCALAPPDATA")
            .map_or_else(|| home.join("AppData").join("Local"), PathBuf::from)
            .join(p)
    };
    let roaming = |p: &str| {
        env_get(env, "APPDATA")
            .map_or_else(|| home.join("AppData").join("Roaming"), PathBuf::from)
            .join(p)
    };
    let hkcu = |p: &str| format!("HKCU\\Software\\{p}\\NativeMessagingHosts\\{NATIVE_HOST_NAME}");
    use Family::{Chromium, Firefox};
    /// Browser, family, profile folder, manifest folder when not the default, registry key.
    type Row = (
        &'static str,
        Family,
        PathBuf,
        Option<PathBuf>,
        Option<String>,
    );
    let table: Vec<Row> = match platform {
        "win32" => vec![
            (
                "brave",
                Chromium,
                local("BraveSoftware\\Brave-Browser\\User Data"),
                None,
                Some(hkcu("BraveSoftware\\Brave-Browser")),
            ),
            (
                "chrome",
                Chromium,
                local("Google\\Chrome\\User Data"),
                None,
                Some(hkcu("Google\\Chrome")),
            ),
            (
                "chromium",
                Chromium,
                local("Chromium\\User Data"),
                None,
                Some(hkcu("Chromium")),
            ),
            (
                "edge",
                Chromium,
                local("Microsoft\\Edge\\User Data"),
                None,
                Some(hkcu("Microsoft\\Edge")),
            ),
            (
                "firefox",
                Firefox,
                roaming("Mozilla\\Firefox"),
                None,
                Some(hkcu("Mozilla")),
            ),
        ],
        "darwin" => vec![
            (
                "brave",
                Chromium,
                mac("BraveSoftware/Brave-Browser"),
                None,
                None,
            ),
            ("chrome", Chromium, mac("Google/Chrome"), None, None),
            ("chromium", Chromium, mac("Chromium"), None, None),
            ("edge", Chromium, mac("Microsoft Edge"), None, None),
            (
                "firefox",
                Firefox,
                mac("Firefox"),
                Some(mac("Mozilla/NativeMessagingHosts")),
                None,
            ),
        ],
        _ => vec![
            (
                "brave",
                Chromium,
                linux("BraveSoftware/Brave-Browser"),
                None,
                None,
            ),
            ("chrome", Chromium, linux("google-chrome"), None, None),
            ("chromium", Chromium, linux("chromium"), None, None),
            ("edge", Chromium, linux("microsoft-edge"), None, None),
            (
                "firefox",
                Firefox,
                home.join(".mozilla").join("firefox"),
                Some(home.join(".mozilla").join("native-messaging-hosts")),
                None,
            ),
        ],
    };
    table
        .into_iter()
        .filter(|(browser, _, app_dir, _, _)| {
            app_dir.exists() || installed_app(browser, platform, env, home).is_some()
        })
        .map(|(browser, family, app_dir, host_dir, registry_key)| {
            let host_dir = if platform == "win32" {
                browser_home().join("native-hosts").join(browser)
            } else {
                host_dir.unwrap_or_else(|| app_dir.join("NativeMessagingHosts"))
            };
            BrowserTarget {
                browser: browser.to_string(),
                family,
                app_dir,
                host_dir,
                registry_key,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionState {
    Installed { profile: String },
    Disabled { profile: String },
    OtherFolder { profile: String, path: String },
    Missing,
}

/// Where the extension stands in a Chromium browser's profiles, read from each profile's
/// `Secure Preferences` / `Preferences`. Lets the guide say "turn it on" or "load it from the
/// right folder" instead of a generic "not connected". From jcode's browser_chromium_diag.rs.
///
/// Profiles are read in name order (the TypeScript took the OS's directory order).
pub fn chromium_extension_state(app_dir: &Path, expected_dir: &Path) -> ExtensionState {
    let Ok(entries) = fs::read_dir(app_dir) else {
        return ExtensionState::Missing;
    };
    let mut profiles: Vec<String> = entries
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|d| d == "Default" || d.starts_with("Profile "))
        .collect();
    profiles.sort();
    let mut best = ExtensionState::Missing;
    for profile in profiles {
        for file in ["Secure Preferences", "Preferences"] {
            let Ok(text) = fs::read_to_string(app_dir.join(&profile).join(file)) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let record = parsed.pointer(&format!("/extensions/settings/{CHROMIUM_EXTENSION_ID}"));
            if !super::actions::truthy(record) {
                continue;
            }
            let Some(record) = record else { continue };
            let reasons = record.get("disable_reasons");
            let disabled = reasons
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty())
                || reasons.and_then(Value::as_f64).is_some_and(|n| n != 0.0)
                || record.get("state").and_then(Value::as_f64) == Some(0.0);
            let path = record.get("path").and_then(Value::as_str);
            match path {
                Some(path)
                    if Path::new(path).is_absolute()
                        && !same_path(Path::new(path), expected_dir) =>
                {
                    best = ExtensionState::OtherFolder {
                        profile: profile.clone(),
                        path: path.to_string(),
                    };
                }
                _ if disabled => {
                    if best == ExtensionState::Missing {
                        best = ExtensionState::Disabled {
                            profile: profile.clone(),
                        };
                    }
                }
                _ => return ExtensionState::Installed { profile },
            }
        }
    }
    best
}

/// `path.resolve`: absolute, with `.` and `..` worked out, without touching the disk.
fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for part in absolute.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = resolve(p).to_string_lossy().into_owned();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

pub fn native_host_manifest(family: Family, host_path: &Path) -> Value {
    let mut m = json!({
        "name": NATIVE_HOST_NAME,
        "description": "Viby Browser native host (viby-toolkit)",
        "path": host_path.to_string_lossy(),
        "type": "stdio",
    });
    match family {
        Family::Chromium => {
            m["allowed_origins"] = json!([format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")])
        }
        Family::Firefox => m["allowed_extensions"] = json!([FIREFOX_EXTENSION_ID]),
    }
    m
}

/// The vendored extension's version: what a connected browser should report once it is up to date.
pub fn extension_version() -> String {
    serde_json::from_str::<Value>(EXTENSION_MANIFEST)
        .ok()
        .map(|m| super::actions::js_string(m.get("version")))
        .unwrap_or_else(|| "unknown".into())
}

pub fn plugin_version() -> String {
    serde_json::from_str::<Value>(PLUGIN_MANIFEST)
        .ok()
        .map(|m| super::actions::js_string(m.get("version")))
        .unwrap_or_else(|| "unknown".into())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Registered {
    pub browser: String,
    pub manifest: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Failed {
    pub browser: String,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallReport {
    pub home: PathBuf,
    pub version: String,
    pub extension: ExtensionDirs,
    pub registered: Vec<Registered>,
    pub failed: Vec<Failed>,
}

/// The copy of this binary the launcher runs, in `root`.
pub fn host_binary(root: &Path, platform: &str) -> PathBuf {
    root.join(if platform == "win32" {
        "viby.exe"
    } else {
        "viby"
    })
}

/// The launcher the browser runs. Browsers start native hosts with a bare PATH, so it names the
/// binary absolutely.
pub fn host_launcher(root: &Path, platform: &str) -> (PathBuf, String) {
    let exe = host_binary(root, platform);
    let exe = exe.display();
    // @echo off matters: anything a .cmd echoes lands on stdout, inside the native-messaging stream.
    if platform == "win32" {
        (
            root.join("host.cmd"),
            format!("@echo off\r\n\"{exe}\" browser-host %*\r\n"),
        )
    } else {
        (
            root.join("host.sh"),
            format!("#!/bin/sh\nexec \"{exe}\" browser-host \"$@\"\n"),
        )
    }
}

/// `reg add` arguments that point a browser at its manifest (Windows only).
pub fn registry_args(registry_key: &str, manifest: &str) -> Vec<String> {
    [
        "add",
        registry_key,
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        manifest,
        "/f",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Puts a copy of `exe` at `target` without ever writing into a file a running host has open:
/// the copy is written beside it and renamed over it, so a running host keeps its old file.
/// Windows will not rename over a running .exe, but it will rename the running one aside.
fn place_binary(exe: &Path, target: &Path) -> Result<(), String> {
    if let (Ok(a), Ok(b)) = (fs::canonicalize(exe), fs::canonicalize(target)) {
        if a == b {
            return Ok(()); // the installed copy is installing itself
        }
    }
    let dir = target.parent().unwrap_or(Path::new("."));
    let temp = dir.join(format!(".viby-{}.tmp", std::process::id()));
    fs::copy(exe, &temp).map_err(|e| write_err(e, "copyfile", exe))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o755))
            .map_err(|e| write_err(e, "chmod", &temp))?;
    }
    if let Err(first) = fs::rename(&temp, target) {
        let aside = dir.join(format!("viby-old-{}.exe", std::process::id()));
        let retried = fs::rename(target, &aside).and_then(|()| fs::rename(&temp, target));
        if retried.is_err() {
            let _ = fs::remove_file(&temp);
            return Err(write_err(first, "rename", target));
        }
    }
    // Copies set aside on Windows go once nothing runs them; one still running stays for now.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("viby-old-") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

/// One install at a time in this process. The server answers tool calls on threads, and two
/// installs at once would delete each other's extension folder while writing it; the
/// TypeScript's single thread never ran two.
static INSTALLING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn install_lock() -> std::sync::MutexGuard<'static, ()> {
    INSTALLING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Installs into browser_home() and registers the native host with every browser found.
/// `exe` is the viby binary to install: the running one, outside tests.
pub fn install(exe: &Path, home: &Path, platform: &str) -> Result<InstallReport, String> {
    let _one_at_a_time = install_lock();
    install_now(exe, home, platform)
}

/// Brings browser_home() up to this plugin's version, unless it is there already: its VERSION
/// matches and the host binary is in place. This is how the host follows a plugin update.
pub fn ensure_installed(exe: &Path, home: &Path, platform: &str) -> Result<(), String> {
    let _one_at_a_time = install_lock();
    let current = installed_version().as_deref() == Some(plugin_version().as_str())
        && host_binary(&browser_home(), platform).exists();
    if !current {
        install_now(exe, home, platform)?;
    }
    Ok(())
}

fn install_now(exe: &Path, home: &Path, platform: &str) -> Result<InstallReport, String> {
    let root = browser_home();
    fs::create_dir_all(&root).map_err(|e| write_err(e, "mkdir", &root))?;
    place_binary(exe, &host_binary(&root, platform))?;
    // What a TypeScript install left: the launcher no longer runs them.
    for old in ["host.ts", "protocol.ts"] {
        let _ = fs::remove_file(root.join(old));
    }
    let (launcher, text) = host_launcher(&root, platform);
    fs::write(&launcher, text).map_err(|e| write_err(e, "open", &launcher))?;
    #[cfg(unix)]
    if platform != "win32" {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
            .map_err(|e| write_err(e, "chmod", &launcher))?;
    }
    let extension = build_extension(&root.join("extension"))?;

    let mut registered = Vec::new();
    let mut failed = Vec::new();
    for target in detect_browsers(home, platform, &process_env()) {
        let manifest = target.host_dir.join(format!("{NATIVE_HOST_NAME}.json"));
        fs::create_dir_all(&target.host_dir)
            .map_err(|e| write_err(e, "mkdir", &target.host_dir))?;
        fs::write(
            &manifest,
            pretty_line(&native_host_manifest(target.family, &launcher)),
        )
        .map_err(|e| write_err(e, "open", &manifest))?;
        if let Some(key) = &target.registry_key {
            let reg = Command::new("reg")
                .args(registry_args(key, &manifest.to_string_lossy()))
                .stdin(Stdio::null())
                .output();
            let failure = match reg {
                Ok(out) if out.status.success() => None,
                Ok(out) => Some(String::from_utf8_lossy(&out.stderr).into_owned())
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| Some("reg add failed".into())),
                Err(e) => Some(e.to_string()),
            };
            if let Some(error) = failure {
                failed.push(Failed {
                    browser: target.browser,
                    error: error.trim().to_string(),
                });
                continue;
            }
        }
        registered.push(Registered {
            browser: target.browser,
            manifest,
        });
    }
    let version = plugin_version();
    let file = root.join("VERSION");
    fs::write(&file, format!("{version}\n")).map_err(|e| write_err(e, "open", &file))?;
    Ok(InstallReport {
        home: root,
        version,
        extension,
        registered,
        failed,
    })
}

pub fn installed_version() -> Option<String> {
    fs::read_to_string(browser_home().join("VERSION"))
        .ok()
        .map(|s| s.trim().to_string())
}
