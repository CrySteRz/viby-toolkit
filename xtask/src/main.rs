//! Repo tasks: `cargo xtask check` (T14) and `cargo xtask parity` (T13).
fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    eprintln!("xtask {task}: not built yet (see docs/plans/2026-10-19-rust-port.md)");
    std::process::exit(2);
}
