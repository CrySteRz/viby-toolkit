//! Repo tasks: `cargo xtask check` (everything that must pass before a commit) and
//! `cargo xtask parity [filter]` (TypeScript and Rust checkers on the same inputs, until the
//! TypeScript is deleted).

mod parity;

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
        Some("parity") => parity::main(&repo_root(), args.get(1).map(String::as_str)),
        _ => {
            eprintln!("usage: cargo xtask <parity [filter]>");
            2
        }
    };
    ExitCode::from(code.clamp(0, 255) as u8)
}
