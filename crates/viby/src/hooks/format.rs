//! viby-toolkit PostToolUse auto-format (OPT-IN: not registered by default).
//!
//! After a Write/Edit, run the file's formatter IF one is already installed and the
//! project actually uses it. Conservative by design:
//!   - Only runs a formatter that exists on PATH AND has project config present, so it
//!     never imposes a style the project didn't opt into.
//!   - NEVER installs anything (no `npx --yes`, no network).
//!   - NEVER blocks Claude: always exits 0, failures are silent, and it prints `{}`.
//!
//! To enable, add to viby-toolkit's hooks/hooks.json (or your settings.json) a PostToolUse
//! matcher "Write|Edit" running `viby format-hook`. Left off by default because
//! auto-format is a matter of taste and can create surprise diffs.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// A formatter that has not finished by then is killed.
const TIMEOUT: Duration = Duration::from_secs(30);

const WEB_EXTS: &[&str] = &[
    ".js", ".jsx", ".ts", ".tsx", ".json", ".md", ".css", ".scss", ".html", ".yaml", ".yml",
];

const PRETTIER_CONFIGS: &[&str] = &[
    ".prettierrc",
    ".prettierrc.json",
    ".prettierrc.js",
    ".prettierrc.cjs",
    ".prettierrc.yaml",
    ".prettierrc.yml",
    "prettier.config.js",
    "prettier.config.cjs",
];

fn run(cmd: &[&str], cwd: &Path) {
    let mut command = Command::new(cmd[0]);
    command
        .args(&cmd[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // An empty cwd makes the spawn fail, as it did for Node: swallowed, nothing runs.
    command.current_dir(cwd);
    // swallow every failure: never block
    let Ok(mut child) = command.spawn() else {
        return;
    };
    let start = Instant::now();
    let mut nap = Duration::from_millis(1);
    loop {
        match child.try_wait() {
            Ok(None) if start.elapsed() < TIMEOUT => {
                std::thread::sleep(nap);
                nap = (nap * 2).min(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Ok(Some(_)) | Err(_) => return,
        }
    }
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

/// Equivalent of Python's `shutil.which(name) is not None`, which checks
/// `exists AND executable AND not a directory`. Plain `exists` is not enough: a
/// directory named `prettier` on PATH, or a non-executable file, would claim the tool is
/// installed and make this hook try to format with something that cannot run.
fn has(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .any(|dir| {
            // missing, not a file, or not executable: keep looking
            std::fs::metadata(dir.join(name)).is_ok_and(|m| m.is_file() && is_executable(&m))
        })
}

fn any_exists(cwd: &Path, names: &[&str]) -> bool {
    names.iter().any(|n| cwd.join(n).exists())
}

/// Node's `path.extname`, lowercased: the text from the last dot of the last path segment,
/// empty for a leading-dot name (`.bashrc`) and for `..`.
fn extname(path: &str) -> String {
    let sep = |c: u8| c == b'/' || (cfg!(windows) && c == b'\\');
    let b = path.as_bytes();
    let end = match b.iter().rposition(|&c| !sep(c)) {
        Some(i) => i + 1,
        None => return String::new(),
    };
    let start = b[..end].iter().rposition(|&c| sep(c)).map_or(0, |i| i + 1);
    let name = &path[start..end];
    match name.rfind('.') {
        Some(0) | None => String::new(),
        Some(_) if name == ".." => String::new(),
        Some(dot) => name[dot..].to_lowercase(),
    }
}

fn str_field<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get(key).and_then(Value::as_str)
}

fn format_file(input: &str) {
    let Ok(d) = serde_json::from_str::<Value>(input) else {
        return;
    };
    let ti = d.get("tool_input").unwrap_or(&Value::Null);
    let Some(fp) = str_field(ti, "file_path")
        .filter(|s| !s.is_empty())
        .or_else(|| str_field(ti, "path").filter(|s| !s.is_empty()))
    else {
        return;
    };
    let cwd = match str_field(&d, "cwd") {
        Some(c) => PathBuf::from(c),
        None => std::env::current_dir().unwrap_or_default(),
    };
    if !Path::new(fp).exists() {
        return;
    }
    let ext = extname(fp);

    if WEB_EXTS.contains(&ext.as_str()) && has("prettier") && any_exists(&cwd, PRETTIER_CONFIGS) {
        run(&["prettier", "--write", fp], &cwd);
    } else if ext == ".py" {
        if has("ruff") && any_exists(&cwd, &["pyproject.toml", "ruff.toml", ".ruff.toml"]) {
            run(&["ruff", "check", "--fix", "--exit-zero", "-q", fp], &cwd);
            run(&["ruff", "format", "-q", fp], &cwd);
        } else if has("black") && any_exists(&cwd, &["pyproject.toml", "setup.cfg", "tox.ini"]) {
            run(&["black", "-q", fp], &cwd);
        }
    } else if ext == ".go" && has("gofmt") {
        run(&["gofmt", "-w", fp], &cwd);
    } else if ext == ".rs"
        && has("rustfmt")
        && any_exists(&cwd, &["Cargo.toml", "rustfmt.toml", ".rustfmt.toml"])
    {
        run(&["rustfmt", fp], &cwd);
    }
}

pub fn main(_args: &[String]) -> i32 {
    let mut input = Vec::new();
    if std::io::stdin().lock().read_to_end(&mut input).is_ok() {
        format_file(&String::from_utf8_lossy(&input));
    }
    // A closed pipe must not change the exit code: the hook never blocks.
    let _ = writeln!(std::io::stdout().lock(), "{{}}");
    0
}
