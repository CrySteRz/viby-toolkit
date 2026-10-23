//! viby-toolkit in one binary: the skill checkers, the hooks and both MCP servers.
//!
//! Each subcommand keeps the contract of the TypeScript script it replaces: the same arguments,
//! output and exit codes (0 clean, 1 findings, 2 nothing to check).

pub mod agentgrep;
pub mod browser;
pub mod checks;
pub mod core;
pub mod hooks;
pub mod host_port;
pub mod mcp;

/// A subcommand's entry point: its arguments (without the subcommand) to an exit code.
pub type Entry = fn(&[String]) -> i32;

/// Every subcommand, named after the script it replaces.
pub const COMMANDS: &[(&str, Entry)] = &[
    ("check-test-drift", checks::check_test_drift::main),
    ("check-analytics-sql", checks::check_analytics_sql::main),
    ("check-api-surface", checks::check_api_surface::main),
    ("check-memory", checks::check_memory::main),
    ("check-docs", checks::check_docs::main),
    ("measure-read-cost", checks::measure_read_cost::main),
    ("check-hooks", checks::check_hooks::main),
    ("check-logging", checks::check_logging::main),
    ("check-diff-hygiene", checks::check_diff_hygiene::main),
    ("check-plan", checks::check_plan::main),
    ("check-agents", checks::check_agents::main),
    ("check-orchestration", checks::check_orchestration::main),
    ("check-skills", checks::check_skills::main),
    ("check-release", checks::check_release::main),
    ("check-migration", checks::check_migration::main),
    ("check-skill-safety", checks::check_skill_safety::main),
    ("check-study", checks::check_study::main),
    ("scan-test-quality", checks::scan_test_quality::main),
    ("detect-stack", checks::detect_stack::main),
    ("statusline", hooks::statusline::main),
    ("format-hook", hooks::format::main),
    ("decide", hooks::decide::main),
    ("prompt-hint", hooks::prompt_hint::main),
    ("browser-server", browser::server::main),
    ("browser-host", browser::host::main),
    ("agentgrep-server", agentgrep::main),
    ("host-port", host_port::main),
];

pub fn not_ported(name: &str) -> i32 {
    eprintln!("viby {name}: not ported yet");
    2
}
