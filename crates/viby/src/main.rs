//! `viby <command> [args]`. The launcher in `plugins/viby-toolkit/bin/` starts this binary.

use std::process::ExitCode;

fn main() -> ExitCode {
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
