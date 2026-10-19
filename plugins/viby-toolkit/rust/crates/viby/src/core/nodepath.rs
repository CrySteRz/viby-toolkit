//! Node's `path` module (POSIX) and the directory reads built on it, as the TypeScript used them.

use std::fs;
use std::path::Path;

use super::js::sort_utf16;

fn cwd() -> String {
    std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `path.normalize`.
pub fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let absolute = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let mut out = parts.join("/");
    if out.is_empty() {
        if absolute {
            return "/".to_string();
        }
        return if trailing { "./" } else { "." }.to_string();
    }
    if trailing {
        out.push('/');
    }
    if absolute {
        format!("/{out}")
    } else {
        out
    }
}

/// `path.join(...parts)`.
pub fn join(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return ".".to_string();
    }
    normalize(&parts.join("/"))
}

/// `path.resolve(...parts)`: right to left until a part is absolute, then against the working
/// directory; normalized, with no trailing slash.
pub fn resolve(parts: &[&str]) -> String {
    let mut combined = String::new();
    for p in parts.iter().rev() {
        if p.is_empty() {
            continue;
        }
        combined = if combined.is_empty() {
            p.to_string()
        } else {
            format!("{p}/{combined}")
        };
        if p.starts_with('/') {
            break;
        }
    }
    if !combined.starts_with('/') {
        combined = format!("{}/{combined}", cwd());
    }
    let out = normalize(&combined);
    match out.strip_suffix('/') {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => out,
    }
}

/// `path.relative(from, to)`.
pub fn relative(from: &str, to: &str) -> String {
    let (from, to) = (resolve(&[from]), resolve(&[to]));
    let f: Vec<&str> = from.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = to.split('/').filter(|s| !s.is_empty()).collect();
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; f.len() - common];
    out.extend(&t[common..]);
    out.join("/")
}

/// `path.dirname`.
pub fn dirname(p: &str) -> String {
    let b = p.as_bytes();
    if b.is_empty() {
        return ".".to_string();
    }
    let has_root = b[0] == b'/';
    let mut end = None;
    let mut matched_slash = true;
    for i in (1..b.len()).rev() {
        if b[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None => if has_root { "/" } else { "." }.to_string(),
        Some(1) if has_root => "//".to_string(),
        Some(e) => p[..e].to_string(),
    }
}

/// `path.basename(p)`: the last segment, ignoring trailing slashes.
pub fn basename(p: &str) -> &str {
    p.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// `path.extname`.
pub fn extname(p: &str) -> &str {
    let b = p.as_bytes();
    let (mut start_dot, mut start_part, mut end) = (None, 0, None);
    let mut matched_slash = true;
    let mut pre_dot_state = 0;
    for i in (0..b.len()).rev() {
        let c = b[i];
        if c == b'/' {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            continue;
        }
        if end.is_none() {
            matched_slash = false;
            end = Some(i + 1);
        }
        if c == b'.' {
            if start_dot.is_none() {
                start_dot = Some(i);
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot.is_some() {
            pre_dot_state = -1;
        }
    }
    match (start_dot, end) {
        (Some(d), Some(e))
            if pre_dot_state != 0 && !(pre_dot_state == 1 && d + 1 == e && d == start_part + 1) =>
        {
            &p[d..e]
        }
        _ => "",
    }
}

/// `readdirSync(dir)` names, sorted as `.sort()` sorts them; `None` when the directory is
/// unreadable.
pub fn read_dir_sorted(dir: &Path) -> Option<Vec<String>> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    sort_utf16(&mut names);
    Some(names)
}

/// Files under `root`, depth first, as the TypeScript's `readdirSync(dir, { withFileTypes })`
/// loops walked them. Directories named by `skip_dir` are not entered and symlinks are not
/// followed. `want(name, full, is_file)` sees every other entry: a symlink has `is_file` false,
/// as `Dirent.isFile()` does. Entries are visited in name order, so the result is deterministic.
pub fn walk_files(
    root: &str,
    skip_dir: impl Fn(&str) -> bool,
    want: impl Fn(&str, &str, bool) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<(String, bool, bool)> = rd
            .flatten()
            .map(|e| {
                let t = e.file_type();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    t.as_ref().is_ok_and(|t| t.is_dir()),
                    t.as_ref().is_ok_and(|t| t.is_file()),
                )
            })
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, is_dir, is_file) in entries {
            let full = join(&[&dir, &name]);
            if is_dir {
                if !skip_dir(&name) {
                    stack.push(full);
                }
            } else if want(&name, &full, is_file) {
                out.push(full);
            }
        }
    }
    out
}
