//! Shared constants and framing for the browser bridge.
//!
//!   MCP server ⇄ host:       newline-delimited JSON over a unix socket in run_dir() (mode 0700),
//!                            or a per-user named pipe on Windows.
//!   host ⇄ extension:        native messaging — u32 little-endian length, then UTF-8 JSON.

use serde_json::{json, Value};
use std::io;
use std::path::{Path, PathBuf};

pub const NATIVE_HOST_NAME: &str = "viby_browser";
pub const FIREFOX_EXTENSION_ID: &str = "browser@viby-toolkit";
/// Public half of the key that pins the Chromium extension id; the private half was never kept.
pub const CHROMIUM_PUBLIC_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAyFP02O7ihyoVtbK3tVNmAczpGUoqac65DNzJIzUVNjJDUcFwHcreRhlgXh642qmM8APDbZ7LCf/TCeUQhtYKqyvUp2R4CGvnQcyDFiIrJ+EPUOFrgAuJvBBZor96AtnheJsLnWQFKYkY2aDdkfplLgmgPXqkRnY6tWTekBoqOQBWd1WjBCsiQJY1NSvnKtRDu32ns87sxMRC8/qMUtrIPp5C7KJ1El+dqvHXqO8WcseOWkcR0I2rUiyA4y26xdgE2h9bzjlgylsjgW6DY+c8rSmaBk24l+M0LLKx0mxu593+/3A1W7pdgv10pm3NaUqwaXa4PISmESGo8hPYSqIJJQIDAQAB";
pub const CHROMIUM_EXTENSION_ID: &str = "jndceenifmkbbhlpfamnkokbdjildmme";

/// Chrome rejects host→extension messages over 1 MB, so file payloads above this are chunked.
pub const CHUNK_SIZE: usize = 750_000;
pub const REQUEST_TIMEOUT_MS: u64 = 30_000;

/// The platform name as Node's `process.platform` gives it; the install and guide code is
/// written against these names so a test can ask for another platform's answer.
pub const PLATFORM: &str = if cfg!(windows) {
    "win32"
} else if cfg!(target_os = "macos") {
    "darwin"
} else {
    "linux"
};

/// The person's home folder, as Node's `os.homedir()` finds it.
pub fn home_dir() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn browser_home() -> PathBuf {
    match std::env::var_os("VIBY_BROWSER_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home_dir().join(".viby").join("browser"),
    }
}

pub fn run_dir() -> PathBuf {
    browser_home().join("run")
}

/// Names a host can register under: what the extension's detectBrowserName() reports.
pub const BROWSER_NAMES: [&str; 6] = ["brave", "chrome", "chromium", "edge", "firefox", "unknown"];

fn alnum_dash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

pub fn socket_path(browser: &str) -> String {
    socket_path_for(browser, PLATFORM)
}

pub fn socket_path_for(browser: &str, platform: &str) -> String {
    let name = alnum_dash(browser);
    if platform == "win32" {
        // Pipes share one machine-wide namespace, so the user name keeps two accounts apart.
        let user = alnum_dash(&user_name());
        return format!(r"\\.\pipe\viby-browser-{user}-{name}");
    }
    run_dir()
        .join(format!("{name}.sock"))
        .to_string_lossy()
        .into_owned()
}

/// The account name, as Node's `os.userInfo().username` gives it.
fn user_name() -> String {
    ["USERNAME", "USER", "LOGNAME"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .or_else(|| {
            home_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

pub fn encode_native(message: &Value) -> Vec<u8> {
    let body = message.to_string().into_bytes();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

/// Incremental decoder for native-messaging frames.
#[derive(Default)]
pub struct NativeDecoder {
    buffer: Vec<u8>,
}

impl NativeDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.buffer.extend_from_slice(chunk);
        let mut out = Vec::new();
        while self.buffer.len() >= 4 {
            let length = u32::from_le_bytes([
                self.buffer[0],
                self.buffer[1],
                self.buffer[2],
                self.buffer[3],
            ]) as usize;
            if self.buffer.len() < 4 + length {
                break;
            }
            let frame: Vec<u8> = self.buffer.drain(..4 + length).skip(4).collect();
            // A frame we cannot parse is dropped; the request it carried times out.
            if let Ok(message) = serde_json::from_str(&String::from_utf8_lossy(&frame)) {
                out.push(message);
            }
        }
        out
    }
}

/// Incremental decoder for newline-delimited JSON.
#[derive(Default)]
pub struct LineDecoder {
    pending: Vec<u8>,
}

impl LineDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Value> {
        self.pending.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line[..end]).into_owned();
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str(&line).unwrap_or_else(|_| json!({ "__invalid": line })));
        }
        out
    }
}

/// A file-system error worded as Node words it (`ENOENT: no such file or directory, open
/// '<path>'`), so what reaches the person reads the same as before the port.
pub fn node_error(err: &io::Error, syscall: &str, path: &Path) -> String {
    use io::ErrorKind as K;
    let (code, text) = match err.kind() {
        K::NotFound => ("ENOENT", "no such file or directory"),
        K::PermissionDenied => ("EACCES", "permission denied"),
        // Node opens a directory fine and fails on the read, so it names neither open nor a path.
        K::IsADirectory if syscall == "open" => {
            return "EISDIR: illegal operation on a directory, read".into()
        }
        K::IsADirectory => ("EISDIR", "illegal operation on a directory"),
        K::NotADirectory => ("ENOTDIR", "not a directory"),
        K::AlreadyExists => ("EEXIST", "file already exists"),
        K::DirectoryNotEmpty => ("ENOTEMPTY", "directory not empty"),
        K::ReadOnlyFilesystem => ("EROFS", "read-only file system"),
        K::StorageFull => ("ENOSPC", "no space left on device"),
        _ => return format!("{err}, {syscall} '{}'", path.display()),
    };
    format!("{code}: {text}, {syscall} '{}'", path.display())
}
