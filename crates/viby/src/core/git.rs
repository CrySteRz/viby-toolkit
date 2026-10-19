//! Running git, as the checkers' `spawnSync("git", …)` did.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct Output {
    /// Exited 0 within the time limit.
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// `git <args>` in `cwd`, with no time limit.
pub fn git(cwd: &Path, args: &[&str]) -> Output {
    run(cwd, args, None)
}

/// `git <args>` in `cwd`, killed after `limit` (reported as not ok).
pub fn git_within(cwd: &Path, args: &[&str], limit: Duration) -> Output {
    run(cwd, args, Some(limit))
}

fn run(cwd: &Path, args: &[&str], limit: Option<Duration>) -> Output {
    let Ok(mut child) = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return Output::default();
    };
    // Read both pipes on their own threads, so a large output cannot fill a pipe and stall git.
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let out_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out.read_to_end(&mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.read_to_end(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if limit.is_some_and(|l| start.elapsed() > l) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break None,
        }
    };
    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();
    Output {
        ok: status.is_some_and(|s| s.success()),
        stdout,
        stderr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_git_and_reports_failure() {
        let ok = git(Path::new("."), &["--version"]);
        assert!(ok.ok && ok.stdout.starts_with("git version"));
        assert!(!git(Path::new("."), &["no-such-subcommand"]).ok);
    }
}
