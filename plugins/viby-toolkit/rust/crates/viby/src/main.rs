//! `viby <command> [args]`. The launcher in `plugins/viby-toolkit/bin/` starts this binary.

use std::process::ExitCode;

fn main() -> ExitCode {
    // Die quietly when the reader goes away (`viby scan-test-quality --all | head`), as other
    // command-line tools do, instead of panicking on the write.
    #[cfg(unix)]
    // SAFETY: restoring the default action for SIGPIPE before any thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();
    let Some((_, entry)) = viby::COMMANDS.iter().find(|(name, _)| *name == command) else {
        eprintln!(
            "usage: viby <command> [args]\ncommands: {}",
            viby::COMMANDS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(2);
    };
    ExitCode::from(entry(&rest).clamp(0, 255) as u8)
}
