//! Contract tests for the release pre-flight (`viby check-release`), ported from
//! `tests/check-release.test.ts`.
//!
//! Each case builds a throwaway git repo. Both halves pinned as always: what must be flagged,
//! and — the half that keeps a checker trustworthy — what must NOT be.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use viby::checks::check_release::check_release;

/// Fixtures must not inherit the developer's global git config. A global `tag.gpgSign = true`
/// turns `git tag <name>` into a signed annotated tag, which fails with "no tag message?" in a
/// non-interactive run — leaving the fixture silently without the tag it is testing for.
fn git(dir: &Path, args: &[&str]) {
    let r = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git runs");
    assert!(
        r.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&r.stderr).trim()
    );
}

fn write(dir: &Path, rel: &str, body: &str) {
    let full = dir.join(rel);
    fs::create_dir_all(full.parent().expect("has parent")).expect("mkdir");
    fs::write(full, body).expect("write");
}

const CI: (&str, &str) = (".github/workflows/ci.yml", "jobs: {}\n");

fn pkg(version: &str) -> String {
    format!("{{\"version\":\"{version}\"}}")
}

fn repo_with(files: &[(&str, &str)], commit: bool, tags: &[&str]) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("rel-")
        .tempdir()
        .expect("tempdir");
    let p = dir.path();
    git(p, &["init", "-q"]);
    git(p, &["config", "user.email", "t@example.com"]);
    git(p, &["config", "user.name", "T"]);
    for (rel, body) in files {
        write(p, rel, body);
    }
    if commit {
        git(p, &["add", "-A"]);
        git(p, &["commit", "-qm", "init"]);
    }
    for t in tags {
        git(p, &["tag", t]);
    }
    dir
}

fn repo(files: &[(&str, &str)]) -> TempDir {
    repo_with(files, true, &[])
}

fn checks(dir: &TempDir) -> Vec<String> {
    check_release(&dir.path().to_string_lossy())
        .findings
        .into_iter()
        .map(|f| f.check)
        .collect()
}

fn has(found: &[String], check: &str) -> bool {
    found.iter().any(|c| c == check)
}

#[test]
fn version_drift_across_manifests_is_flagged() {
    let dir = repo(&[
        ("package.json", &pkg("1.2.0")),
        ("pyproject.toml", "[project]\nname='x'\nversion = '1.1.0'\n"),
        CI,
    ]);
    let found = checks(&dir);
    assert!(
        has(&found, "version-drift"),
        "expected version-drift, got {found:?}"
    );
}

#[test]
fn consistent_versions_across_manifests_are_not_flagged() {
    let dir = repo(&[
        ("package.json", &pkg("1.2.0")),
        ("pyproject.toml", "[project]\nname='x'\nversion = '1.2.0'\n"),
        ("VERSION", "1.2.0\n"),
        CI,
    ]);
    let found = checks(&dir);
    assert!(
        !has(&found, "version-drift"),
        "must not flag matching versions, got {found:?}"
    );
}

#[test]
fn a_nested_plugin_marketplace_manifest_is_compared_with_the_root() {
    let dir = repo(&[
        ("package.json", &pkg("0.9.0")),
        (
            ".claude-plugin/marketplace.json",
            "{\"plugins\":[{\"name\":\"p\",\"version\":\"0.8.0\"}]}",
        ),
        CI,
    ]);
    assert!(
        has(&checks(&dir), "version-drift"),
        "a stale marketplace version must be caught"
    );
}

#[test]
fn monorepo_per_package_versions_are_expected_not_drift() {
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        ("pnpm-workspace.yaml", "packages:\n  - packages/*\n"),
        ("packages/a/package.json", &pkg("2.3.1")),
        ("packages/b/package.json", &pkg("0.4.0")),
        CI,
    ]);
    let found = checks(&dir);
    assert!(
        !has(&found, "version-drift"),
        "independent package versions are normal in a monorepo, got {found:?}"
    );
}

#[test]
fn an_uncommitted_change_is_flagged_because_it_would_not_ship() {
    let dir = repo(&[("package.json", &pkg("1.0.0")), CI]);
    write(dir.path(), "src.ts", "export const x = 1;\n");
    assert!(has(&checks(&dir), "dirty-tree"));
}

#[test]
fn a_clean_committed_tree_is_not_flagged_as_dirty() {
    let dir = repo(&[("package.json", &pkg("1.0.0")), CI]);
    assert!(!has(&checks(&dir), "dirty-tree"));
}

#[test]
fn an_existing_tag_for_this_version_is_flagged_with_or_without_a_v_prefix() {
    for tag in ["1.0.0", "v1.0.0"] {
        let dir = repo_with(&[("package.json", &pkg("1.0.0")), CI], true, &[tag]);
        assert!(
            has(&checks(&dir), "tag-exists"),
            "tag {tag} should be detected"
        );
    }
}

#[test]
fn an_unrelated_existing_tag_is_not_mistaken_for_this_version() {
    let dir = repo_with(&[("package.json", &pkg("2.0.0")), CI], true, &["v1.0.0"]);
    assert!(!has(&checks(&dir), "tag-exists"));
}

#[test]
fn a_changelog_that_omits_this_version_is_flagged() {
    let dir = repo(&[
        ("package.json", &pkg("2.0.0")),
        ("CHANGELOG.md", "# Changelog\n\n## 1.9.0\n- old stuff\n"),
        CI,
    ]);
    assert!(has(&checks(&dir), "changelog-stale"));
}

#[test]
fn a_changelog_mentioning_this_version_is_clean() {
    let dir = repo(&[
        ("package.json", &pkg("2.0.0")),
        ("CHANGELOG.md", "# Changelog\n\n## 2.0.0\n- the new thing\n"),
        CI,
    ]);
    assert!(!has(&checks(&dir), "changelog-stale"));
}

#[test]
fn a_real_focused_test_is_flagged_as_a_debug_artifact() {
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        (
            "a.test.ts",
            "describe.only(\"payments\", () => {\n  it(\"charges\", () => {});\n});\n",
        ),
        CI,
    ]);
    assert!(has(&checks(&dir), "debug-artifact"));
}

#[test]
fn a_focused_test_inside_a_string_fixture_is_not_a_debug_artifact() {
    // The exact false positive this checker produced on its own repo before the shared
    // code-blanking pass was wired in.
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        (
            "meta.test.ts",
            concat!(
                "const FIXTURE = `\ndescribe.only(\"payments\", () => {});\n`;\n",
                "test(\"scanner flags focused tests\", () => {\n  expect(scan(FIXTURE)).toContain(\"focused\");\n});\n",
            ),
        ),
        CI,
    ]);
    let found = checks(&dir);
    assert!(
        !has(&found, "debug-artifact"),
        "a fixture mentioning describe.only is not a focused test, got {found:?}"
    );
}

#[test]
fn a_debugger_statement_is_flagged() {
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        (
            "src.ts",
            "export function f() {\n  debugger;\n  return 1;\n}\n",
        ),
        CI,
    ]);
    assert!(has(&checks(&dir), "debug-artifact"));
}

#[test]
fn a_missing_ci_config_is_reported_at_low_severity() {
    let dir = repo(&[("package.json", &pkg("1.0.0"))]);
    let r = check_release(&dir.path().to_string_lossy());
    let no_ci = r
        .findings
        .iter()
        .find(|f| f.check == "no-ci")
        .expect("expected a no-ci finding");
    assert_eq!(
        no_ci.severity, "P3",
        "missing CI is informational, not a blocker"
    );
}

#[test]
fn a_non_git_directory_reports_is_repo_false_rather_than_crashing() {
    let dir = tempfile::Builder::new()
        .prefix("norepo-")
        .tempdir()
        .expect("tempdir");
    write(dir.path(), "package.json", &pkg("1.0.0"));
    let r = check_release(&dir.path().to_string_lossy());
    assert!(!r.is_repo);
    assert!(
        !r.findings.iter().any(|f| f.check == "dirty-tree"),
        "git checks must be skipped, not guessed"
    );
}

#[test]
fn a_version_is_matched_as_a_token_not_a_substring() {
    // Regression: "12.0.1".includes("2.0") is true, so an unrelated older release silently
    // satisfied the changelog check.
    let dir = repo(&[
        ("package.json", &pkg("2.0")),
        ("CHANGELOG.md", "# Changelog\n\n## 12.0.1\n- unrelated\n"),
        CI,
    ]);
    assert!(
        has(&checks(&dir), "changelog-stale"),
        "2.0 is not documented by 12.0.1"
    );
}

#[test]
fn every_debug_artifact_line_is_reported_not_only_the_first() {
    // Regression: findIndex reported one hit per pattern per file, so a second focused test
    // shipped silently even after the first was fixed.
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        (
            "a.test.js",
            "describe.only(\"a\", () => {});\nconst x = 1;\nit.only(\"b\", () => {});\n",
        ),
        CI,
    ]);
    let r = check_release(&dir.path().to_string_lossy());
    let f = r
        .findings
        .iter()
        .find(|x| x.check == "debug-artifact")
        .expect("expected a debug-artifact finding");
    let detail = f.detail.clone().unwrap_or_default();
    assert!(detail.contains("a.test.js:1"), "first artifact reported");
    assert!(
        detail.contains("a.test.js:3"),
        "the second artifact on line 3 must also be reported"
    );
}

#[test]
fn a_manifest_inside_a_singular_fixture_directory_does_not_contribute_to_version_drift() {
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        ("tests/routing/fixture/package.json", &pkg("9.9.9")),
        CI,
    ]);
    let found = checks(&dir);
    assert!(
        !has(&found, "version-drift"),
        "a throwaway fixture manifest must not count as drift, got {found:?}"
    );
}

#[test]
fn a_real_manifest_at_the_repo_root_still_contributes_to_version_drift() {
    let dir = repo(&[
        ("package.json", &pkg("1.0.0")),
        ("other/package.json", &pkg("2.0.0")),
        CI,
    ]);
    assert!(
        has(&checks(&dir), "version-drift"),
        "a genuine second manifest must still be caught"
    );
}

#[test]
fn a_manifest_with_a_utf8_bom_is_still_parsed_so_drift_is_still_caught() {
    // Regression: Node keeps the BOM under "utf8", JSON.parse threw, the catch swallowed it,
    // and the manifest silently dropped out of version detection entirely.
    let dir = repo(&[
        ("package.json", &format!("\u{feff}{}", pkg("1.0.0"))),
        ("other/package.json", &pkg("2.0.0")),
        CI,
    ]);
    assert!(
        has(&checks(&dir), "version-drift"),
        "a BOM must not hide a real version drift"
    );
}
