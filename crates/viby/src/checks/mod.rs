//! The skill checkers, one module per former `skills/*/scripts/*.ts`.

pub mod check_test_drift;
pub mod check_analytics_sql;
pub mod check_api_surface;
pub mod check_memory;
pub mod check_docs;
pub mod measure_read_cost;
pub mod check_hooks;
pub mod check_logging;
pub mod check_diff_hygiene;
pub mod check_plan;
pub mod check_agents;
pub mod check_orchestration;
pub mod check_skills;
pub mod check_release;
pub mod check_migration;
pub mod check_skill_safety;
pub mod check_study;
pub mod scan_test_quality;
pub mod detect_stack;
