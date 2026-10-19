//! Contract tests for the public-surface differ (`viby check-api-surface`), ported from
//! `tests/check-api-surface.test.ts`.
//!
//! Both halves pinned. The must-NOT half carries most of the weight here: a differ that calls
//! every change breaking is exactly as useless as one that calls none, because the whole point
//! is to tell major from minor. So a renamed parameter must NOT be reported as a signature
//! break, and an export inside a string fixture must NOT be reported at all.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use viby::checks::check_api_surface::{diff_surface, extract_surface};

/// Fixtures must not inherit the developer's global git config (a global `commit.gpgSign` or
/// `tag.gpgSign` breaks a non-interactive fixture). Identity is set locally in each repo.
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

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (rel, body) in files {
        let full = dir.join(rel);
        fs::create_dir_all(full.parent().expect("has parent")).expect("mkdir");
        fs::write(full, body).expect("write");
    }
}

fn repo(before: &[(&str, &str)], after: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix("surface-")
        .tempdir()
        .expect("tempdir");
    let p = dir.path();
    git(p, &["init", "-q"]);
    git(p, &["config", "user.email", "t@example.com"]);
    git(p, &["config", "user.name", "T"]);
    write(p, before);
    git(p, &["add", "-A"]);
    git(p, &["commit", "-qm", "base"]);
    write(p, after);
    dir
}

fn verdict_of(before: &[(&str, &str)], after: &[(&str, &str)]) -> (String, Vec<String>) {
    let dir = repo(before, after);
    let d = dir.path().to_string_lossy().into_owned();
    let r = diff_surface(&d, "HEAD", std::slice::from_ref(&d));
    (
        r.verdict.to_string(),
        r.changes.iter().map(|c| c.kind.to_string()).collect(),
    )
}

fn names(symbols: &[viby::checks::check_api_surface::Symbol]) -> Vec<&str> {
    symbols.iter().map(|s| s.name.as_str()).collect()
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let p = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("check-api-surface")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("viby runs");
    (
        p.status.code(),
        String::from_utf8_lossy(&p.stdout).into_owned(),
    )
}

#[test]
fn removing_an_exported_function_is_major() {
    let (verdict, kinds) = verdict_of(
        &[(
            "src/a.ts",
            "export function alpha(x: number) {\n  return x;\n}\nexport function beta() {}\n",
        )],
        &[(
            "src/a.ts",
            "export function alpha(x: number) {\n  return x;\n}\n",
        )],
    );
    assert_eq!(verdict, "major", "{kinds:?}");
    assert!(kinds.iter().any(|k| k == "removed"));
}

#[test]
fn adding_an_exported_function_is_minor_not_major() {
    let (verdict, kinds) = verdict_of(
        &[("src/a.ts", "export function alpha() {}\n")],
        &[(
            "src/a.ts",
            "export function alpha() {}\nexport function beta(y: string) {}\n",
        )],
    );
    assert_eq!(verdict, "minor", "{kinds:?}");
}

#[test]
fn an_internal_only_change_with_no_surface_movement_is_patch() {
    let (verdict, kinds) = verdict_of(
        &[("src/a.ts", "export function alpha() {\n  return 1;\n}\n")],
        &[(
            "src/a.ts",
            "export function alpha() {\n  const x = 2;\n  return x - 1;\n}\n",
        )],
    );
    assert_eq!(verdict, "patch", "{kinds:?}");
}

#[test]
fn changing_the_parameter_count_is_a_signature_break() {
    let (verdict, kinds) = verdict_of(
        &[("src/a.ts", "export function alpha(a: number) {}\n")],
        &[(
            "src/a.ts",
            "export function alpha(a: number, b: number) {}\n",
        )],
    );
    assert_eq!(verdict, "major");
    assert!(kinds.iter().any(|k| k == "signature"), "{kinds:?}");
}

#[test]
fn renaming_a_parameter_is_p2_not_a_major_break() {
    // Positionally compatible. Calling it major would make the tool cry wolf on a rename, and a
    // differ nobody believes gets ignored on the one release where it was right.
    let (verdict, kinds) = verdict_of(
        &[("src/a.ts", "export function alpha(oldName: number) {}\n")],
        &[("src/a.ts", "export function alpha(newName: number) {}\n")],
    );
    assert!(kinds.iter().any(|k| k == "param-name"), "{kinds:?}");
    assert_ne!(
        verdict, "major",
        "a positional rename must not be reported as MAJOR"
    );
}

#[test]
fn an_export_function_inside_a_string_fixture_is_not_part_of_the_surface() {
    let (symbols, _) = extract_surface(
        "const FIXTURE = `\nexport function ghost() {}\n`;\nexport function real() {}\n",
        "a.ts",
    );
    assert_eq!(names(&symbols), ["real"], "{symbols:?}");
}

#[test]
fn an_export_function_in_a_comment_is_not_part_of_the_surface() {
    let (symbols, _) = extract_surface(
        "// export function ghost() {}\nexport function real() {}\n",
        "a.ts",
    );
    assert_eq!(names(&symbols), ["real"]);
}

#[test]
fn types_interfaces_classes_and_consts_are_all_surface() {
    let (symbols, _) = extract_surface(
        "export type A = string;\nexport interface B { x: number }\nexport class C {}\nexport const d = 1;\n",
        "a.ts",
    );
    let mut kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
    kinds.sort();
    assert_eq!(kinds, ["class", "const", "interface", "type"]);
}

#[test]
fn a_widened_exported_type_is_a_signature_change_because_callers_depend_on_it() {
    let (_, kinds) = verdict_of(
        &[("src/a.ts", "export type Mode = 'on' | 'off';\n")],
        &[("src/a.ts", "export type Mode = 'on';\n")],
    );
    assert!(kinds.iter().any(|k| k == "signature"), "{kinds:?}");
}

#[test]
fn export_a_as_b_exports_the_alias_which_is_what_a_caller_imports() {
    let (symbols, _) = extract_surface("function a() {}\nexport { a as publicName };\n", "a.ts");
    assert!(
        symbols.iter().any(|s| s.name == "publicName"),
        "{symbols:?}"
    );
}

#[test]
fn an_unresolved_export_star_from_barrel_is_reported_never_silently_excluded() {
    // The tool cannot follow it without module resolution. Staying quiet would make the report
    // claim a completeness it does not have — the exact failure the source spike documented.
    let (_, barrels) = extract_surface("export * from \"./inner.ts\";\n", "a.ts");
    assert_eq!(barrels, ["./inner.ts"]);
    let (_, kinds) = verdict_of(
        &[("src/a.ts", "export const x = 1;\n")],
        &[(
            "src/a.ts",
            "export const x = 1;\nexport * from \"./inner.ts\";\n",
        )],
    );
    assert!(
        kinds.iter().any(|k| k == "unresolved-reexport"),
        "{kinds:?}"
    );
}

#[test]
fn python_a_leading_underscore_def_is_private_and_not_surface() {
    let (symbols, _) = extract_surface(
        "def public(a):\n    pass\n\ndef _private(b):\n    pass\n",
        "m.py",
    );
    assert_eq!(names(&symbols), ["public"]);
}

#[test]
fn python_an_explicit_all_is_the_surface_overriding_convention() {
    let (symbols, _) = extract_surface(
        "__all__ = [\"only_this\"]\n\ndef only_this():\n    pass\n\ndef also_public():\n    pass\n",
        "m.py",
    );
    assert_eq!(names(&symbols), ["only_this"]);
}

#[test]
fn go_a_lowercase_func_is_unexported_and_not_surface() {
    let (symbols, _) = extract_surface(
        "func Exported(a int) {}\nfunc unexported(b int) {}\n",
        "m.go",
    );
    assert_eq!(names(&symbols), ["Exported"]);
}

#[test]
fn rust_only_pub_items_are_surface() {
    let (symbols, _) = extract_surface(
        "pub fn open(path: String) {}\nfn hidden() {}\npub struct Handle;\n",
        "m.rs",
    );
    let mut n = names(&symbols);
    n.sort();
    assert_eq!(n, ["Handle", "open"]);
}

#[test]
fn a_language_it_does_not_understand_yields_no_symbols_rather_than_a_guess() {
    let (symbols, _) = extract_surface("PROCEDURE DIVISION.\nDISPLAY 'HELLO'.\n", "legacy.cbl");
    assert!(symbols.is_empty());
}

#[test]
fn a_non_git_directory_reports_unknown_instead_of_inventing_a_verdict() {
    let dir = tempfile::Builder::new()
        .prefix("nogit-")
        .tempdir()
        .expect("tempdir");
    write(dir.path(), &[("a.ts", "export const x = 1;\n")]);
    let d = dir.path().to_string_lossy().into_owned();
    let r = diff_surface(&d, "HEAD", std::slice::from_ref(&d));
    assert_eq!(r.verdict, "unknown");
    assert_eq!(
        r.changes.len(),
        0,
        "no repo means nothing to compare, not 'no changes'"
    );
}

#[test]
fn a_brand_new_files_exports_are_additions_not_a_false_removed_on_the_old_side() {
    let (verdict, kinds) = verdict_of(
        &[("src/a.ts", "export const x = 1;\n")],
        &[
            ("src/a.ts", "export const x = 1;\n"),
            ("src/b.ts", "export const y = 2;\n"),
        ],
    );
    assert_eq!(verdict, "minor", "{kinds:?}");
    assert!(!kinds.iter().any(|k| k == "removed"));
}

#[test]
fn cli_a_breaking_change_exits_1_an_additive_one_exits_0() {
    let broken = repo(
        &[("a.ts", "export function alpha() {}\n")],
        &[("a.ts", "export function renamed() {}\n")],
    );
    let (status, stdout) = run(broken.path(), &[".", "--quiet"]);
    assert_eq!(status, Some(1), "a removal must exit 1: {stdout}");

    let additive = repo(
        &[("a.ts", "export function alpha() {}\n")],
        &[(
            "a.ts",
            "export function alpha() {}\nexport function beta() {}\n",
        )],
    );
    let (status, stdout) = run(additive.path(), &[".", "--quiet"]);
    assert_eq!(status, Some(0), "an addition must exit 0: {stdout}");
}

#[test]
fn the_report_always_states_that_behavioural_breaks_are_invisible_to_it() {
    let dir = repo(
        &[("a.ts", "export function alpha() {}\n")],
        &[("a.ts", "export function alpha() {}\nexport const b = 2;\n")],
    );
    let (_, stdout) = run(dir.path(), &["."]);
    assert!(stdout.contains("Syntactic surface only"), "{stdout}");
    assert!(stdout.contains("MEANING changed"), "{stdout}");
}
