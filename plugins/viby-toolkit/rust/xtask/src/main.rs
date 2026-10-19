//! Repo tasks: `cargo xtask check`, everything that must pass before a commit.

mod check;

use std::path::PathBuf;
use std::process::{Command, ExitCode};

/// The repository root, wherever the workspace sits inside it.
fn repo_root() -> PathBuf {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git");
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("check") => check::main(&repo_root()),
        _ => {
            eprintln!("usage: cargo xtask check");
            2
        }
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}
