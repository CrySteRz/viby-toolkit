//! Contract tests for the stack detector.
//!
//! Each case builds a synthetic repo in a temp dir. The contract has three halves:
//!  1. it finds the right commands for each ecosystem,
//!  2. it ranks CI above task-runner above convention, and
//!  3. it says "unknown" instead of inventing a command — the property that makes it safe
//!     to trust, and the one a plausible-sounding guess would quietly destroy.

use std::fs;
use tempfile::TempDir;
use viby::checks::detect_stack::{detect_stack, Source, Stack};

fn build(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    dir
}

fn stack(files: &[(&str, &str)]) -> Stack {
    let dir = build(files);
    detect_stack(dir.path())
}

fn commands_for(files: &[(&str, &str)], role: &str) -> Vec<String> {
    stack(files)
        .commands
        .get(role)
        .map(|v| v.iter().map(|c| c.command.clone()).collect())
        .unwrap_or_default()
}

fn names(s: &Stack, role: &str) -> Option<Vec<String>> {
    s.commands
        .get(role)
        .map(|v| v.iter().map(|c| c.command.clone()).collect())
}

#[test]
fn node_pnpm_lockfile_picks_pnpm_and_scripts_map_to_roles() {
    let package_json = serde_json::json!({
        "scripts": { "test": "vitest run", "build": "tsc -p .", "lint": "eslint .", "typecheck": "tsc --noEmit" },
        "devDependencies": { "vitest": "^1.0.0" },
    })
    .to_string();
    let s = stack(&[
        ("pnpm-lock.yaml", "lockfileVersion: 6.0\n"),
        ("package.json", &package_json),
        ("src/index.ts", "export const x = 1;\n"),
    ]);
    assert!(
        s.package_managers.iter().any(|p| p == "pnpm"),
        "expected pnpm, got {}",
        s.package_managers.join(",")
    );
    assert_eq!(names(&s, "test"), Some(vec!["pnpm test".to_string()]));
    assert_eq!(
        names(&s, "typecheck"),
        Some(vec!["pnpm typecheck".to_string()])
    );
    assert!(s.test_frameworks.iter().any(|f| f == "vitest"));
    assert_eq!(s.languages[0].name, "TypeScript");
}

#[test]
fn rust_cargo_conventions_are_found_from_cargo_toml_alone() {
    let cmds = commands_for(
        &[
            ("Cargo.toml", "[package]\nname='x'\n"),
            ("src/main.rs", "fn main() {}\n"),
        ],
        "test",
    );
    assert_eq!(cmds, vec!["cargo test"]);
}

#[test]
fn go_go_mod_yields_go_test() {
    let cmds = commands_for(
        &[
            ("go.mod", "module x\n\ngo 1.22\n"),
            ("main.go", "package main\n"),
        ],
        "test",
    );
    assert_eq!(cmds, vec!["go test ./..."]);
}

#[test]
fn python_pyproject_yields_pytest_and_uv_is_detected_from_uv_lock() {
    let s = stack(&[
        (
            "pyproject.toml",
            "[project]\nname='x'\ndependencies=['pytest']\n",
        ),
        ("uv.lock", "version = 1\n"),
        ("src/app.py", "x = 1\n"),
    ]);
    assert_eq!(names(&s, "test"), Some(vec!["pytest".to_string()]));
    assert!(s.package_managers.iter().any(|p| p == "uv"));
    assert!(s.test_frameworks.iter().any(|f| f == "pytest"));
}

#[test]
fn ruby_php_elixir_java_swift_gradle_conventions_each_resolve() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "Gemfile",
            "source 'https://rubygems.org'\n",
            "bundle exec rspec",
        ),
        ("composer.json", "{}", "./vendor/bin/phpunit"),
        ("mix.exs", "defmodule X do\nend\n", "mix test"),
        ("pom.xml", "<project/>", "mvn test"),
        (
            "Package.swift",
            "// swift-tools-version:5.9\n",
            "swift test",
        ),
        ("build.gradle", "plugins {}\n", "./gradlew test"),
    ];
    for (file, body, expected) in cases {
        let cmds = commands_for(&[(file, body)], "test");
        assert!(
            cmds.iter().any(|c| c == expected),
            "{file}: expected {expected}, got {}",
            if cmds.is_empty() {
                "none".to_string()
            } else {
                cmds.join(",")
            }
        );
    }
}

#[test]
fn ci_outranks_the_task_runner_and_is_labelled_as_authoritative() {
    let s = stack(&[
        (
            "package.json",
            &serde_json::json!({ "scripts": { "test": "vitest run" } }).to_string(),
        ),
        ("package-lock.json", "{}"),
        (
            ".github/workflows/ci.yml",
            "
name: ci
jobs:
  build:
    steps:
      - run: npm ci
      - run: npm run test:integration
",
        ),
        ("src/a.ts", "export {};\n"),
    ]);
    let tests = s.commands.get("test").cloned().unwrap_or_default();
    assert!(
        tests.len() >= 2,
        "expected CI and task-runner commands, got {}",
        tests.len()
    );
    assert_eq!(
        tests[0].source,
        Source::Ci,
        "the CI command must rank first"
    );
    assert_eq!(tests[0].command, "npm run test:integration");
    assert!(
        tests.iter().any(|c| c.source == Source::TaskRunner),
        "the package.json script must still be reported as a lower-authority option"
    );
    assert!(s.ci_files.iter().any(|f| f == ".github/workflows/ci.yml"));
}

#[test]
fn a_repo_with_no_test_tooling_reports_unknown_rather_than_inventing_a_command() {
    let s = stack(&[("README.md", "# docs only\n"), ("notes.txt", "hello\n")]);
    assert!(
        s.commands.get("test").is_none(),
        "must not invent a test command"
    );
    assert!(
        s.unknowns.iter().any(|u| u.contains("no test command")),
        "expected an explicit unknown, got {:?}",
        s.unknowns
    );
}

#[test]
fn polyglot_repos_are_flagged_because_one_test_command_will_not_cover_them() {
    let s = stack(&[
        ("a.ts", "export {};\n"),
        ("b.py", "x = 1\n"),
        ("c.go", "package main\n"),
        ("d.rs", "fn main() {}\n"),
        ("e.rb", "puts 1\n"),
    ]);
    assert!(s.languages.len() >= 4);
    assert!(
        s.unknowns.iter().any(|u| u.contains("polyglot")),
        "expected a polyglot warning, got {:?}",
        s.unknowns
    );
}

#[test]
fn monorepo_tooling_is_identified_and_a_plain_cargo_toml_is_not_a_workspace() {
    let nx = stack(&[("nx.json", "{}"), ("package.json", "{}")]);
    let plain = stack(&[("Cargo.toml", "[package]\nname='x'\n")]);
    let ws = stack(&[("Cargo.toml", "[workspace]\nmembers=['a']\n")]);
    assert_eq!(nx.monorepo.as_ref().map(|m| m.tool.as_str()), Some("Nx"));
    assert!(
        plain.monorepo.is_none(),
        "a non-workspace Cargo.toml is not a monorepo"
    );
    assert_eq!(
        ws.monorepo.as_ref().map(|m| m.tool.as_str()),
        Some("Cargo workspace")
    );
}

#[test]
fn makefile_targets_are_picked_up() {
    let cmds = commands_for(
        &[
            ("Makefile", "build:\n\tcc -o a a.c\ntest:\n\t./a\n"),
            ("a.c", "int main(){}\n"),
        ],
        "test",
    );
    assert!(
        cmds.iter().any(|c| c == "make test"),
        "got {}",
        cmds.join(",")
    );
}

#[test]
fn a_ci_publish_script_is_not_mistaken_for_an_install_step() {
    let s = stack(&[
        ("package.json", "{}"),
        (
            ".github/workflows/publish.yml",
            "jobs:\n  p:\n    steps:\n      - run: pnpm run ci-publish\n",
        ),
    ]);
    let installs = names(&s, "install").unwrap_or_default();
    assert!(
        !installs.iter().any(|c| c.contains("ci-publish")),
        "`ci-publish` must not classify as install, got {}",
        installs.join(",")
    );
}

#[test]
fn skips_vendor_directories_when_taking_the_language_census() {
    let s = stack(&[
        ("src/a.ts", "export {};\n"),
        ("node_modules/pkg/index.js", "module.exports = 1;\n"),
        ("node_modules/pkg/b.js", "module.exports = 2;\n"),
        ("dist/bundle.js", "console.log(1);\n"),
    ]);
    assert_eq!(
        s.languages.len(),
        1,
        "only src should count, got {:?}",
        s.languages
    );
    assert_eq!(s.languages[0].name, "TypeScript");
}

#[test]
fn a_config_infra_repo_is_labelled_as_one_not_as_its_one_stray_script() {
    // Found by running this detector on a real Kubernetes GitOps repo: 183 YAML manifests and one
    // Python file produced the headline "Python 100%" — technically true, and a confident wrong
    // answer about what the repo IS, which is the worst output this tool can give.
    let mut files: Vec<(String, String)> = vec![("scripts/rotate.py".into(), "print(1)\n".into())];
    for i in 0..30 {
        files.push((
            format!("apps/svc{i}/kustomization.yaml"),
            "resources:\n  - deploy.yaml\n".into(),
        ));
    }
    files.push((
        "charts/web/Chart.yaml".into(),
        "name: web\nversion: 1.0.0\n".into(),
    ));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let s = stack(&refs);
    assert!(
        s.shape.config_files > s.shape.code_files * 10,
        "expected config-dominated, got {:?}",
        s.shape
    );
    assert!(
        s.shape
            .infra
            .iter()
            .any(|i| i.to_lowercase().contains("kustomize") || i.to_lowercase().contains("helm")),
        "the infra flavour must be named, got {}",
        if s.shape.infra.is_empty() {
            "none".to_string()
        } else {
            s.shape.infra.join(", ")
        }
    );
    assert!(
        s.unknowns.iter().any(|u| u.contains("CONFIG/INFRA REPO")),
        "the reader must be warned before trusting the language line"
    );
}

#[test]
fn an_ordinary_code_repo_with_some_config_is_not_labelled_config_infra() {
    // The must-not half: every real project has a package.json, a tsconfig and a CI file. Flagging
    // those as "this is a config repo" would fire on essentially everything.
    let mut files: Vec<(String, String)> = vec![
        ("package.json".into(), "{\"name\":\"x\"}\n".into()),
        ("tsconfig.json".into(), "{}\n".into()),
        (".github/workflows/ci.yml".into(), "jobs: {}\n".into()),
    ];
    for i in 0..20 {
        files.push((format!("src/mod{i}.ts"), "export const x = 1;\n".into()));
    }
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let s = stack(&refs);
    assert!(
        !s.unknowns.iter().any(|u| u.contains("CONFIG/INFRA REPO")),
        "a normal TS project must not be called a config repo: {:?}",
        s.shape
    );
}
