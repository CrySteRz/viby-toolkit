//! `viby format-hook`: the behaviour the TypeScript header documents. It always prints `{}` and
//! exits 0, and it runs a formatter only if one is on PATH and the project configures it.
//!
//! The formatters here are fake shell scripts that leave a marker file, found through a PATH
//! that holds nothing else.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Run the hook with `path` as its whole PATH: (exit status, stdout).
fn run(input: &[u8], path: &Path) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_viby"))
        .arg("format-hook")
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn viby format-hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn payload(file: &Path, cwd: &Path) -> Value {
    json!({
        "tool_input": { "file_path": file.to_str().unwrap() },
        "cwd": cwd.to_str().unwrap(),
    })
}

#[test]
fn no_formatter_on_path_is_a_no_op() {
    let project = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("Cargo.toml"), "").unwrap();
    let file = project.path().join("lib.rs");
    std::fs::write(&file, "fn  main(){}").unwrap();
    let (status, out) = run(
        payload(&file, project.path()).to_string().as_bytes(),
        bin.path(),
    );
    assert_eq!((status, out.as_str()), (Some(0), "{}\n"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "fn  main(){}");
}

#[test]
fn unreadable_or_irrelevant_input_still_prints_braces_and_exits_0() {
    let bin = tempfile::tempdir().unwrap();
    let inputs: [&[u8]; 9] = [
        b"",
        b"not json",
        b"null",
        b"[]",
        b"{}",
        br#"{"tool_input": null}"#,
        br#"{"tool_input": {"file_path": ""}}"#,
        br#"{"tool_input": {"file_path": "/no/such/file.rs"}}"#,
        br#"{"tool_input": {"file_path": 7}, "cwd": 7}"#,
    ];
    for input in inputs {
        let (status, out) = run(input, bin.path());
        assert_eq!((status, out.as_str()), (Some(0), "{}\n"), "{input:?}");
    }
}

#[cfg(unix)]
mod fake_formatters {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake formatter named `name` in `bin` that writes `<name>.ran` into the directory it
    /// runs in, with its arguments.
    fn install(bin: &Path, name: &str) {
        let script = format!("#!/bin/sh\necho \"$@\" > {name}.ran\n");
        let file = bin.join(name);
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn ran(project: &Path, name: &str) -> bool {
        project.join(format!("{name}.ran")).exists()
    }

    fn hook(project: &Path, bin: &Path, file: &Path) {
        let (status, out) = run(payload(file, project).to_string().as_bytes(), bin);
        assert_eq!((status, out.as_str()), (Some(0), "{}\n"));
    }

    #[test]
    fn runs_the_formatter_when_it_is_installed_and_configured() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        install(bin.path(), "rustfmt");
        std::fs::write(project.path().join("Cargo.toml"), "").unwrap();
        let file = project.path().join("lib.rs");
        std::fs::write(&file, "").unwrap();
        hook(project.path(), bin.path(), &file);
        let args = std::fs::read_to_string(project.path().join("rustfmt.ran")).unwrap();
        assert_eq!(args.trim(), file.to_str().unwrap());
    }

    #[test]
    fn leaves_a_project_without_the_config_alone() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        install(bin.path(), "rustfmt");
        install(bin.path(), "prettier");
        let rs = project.path().join("lib.rs");
        let ts = project.path().join("app.ts");
        std::fs::write(&rs, "").unwrap();
        std::fs::write(&ts, "").unwrap();
        hook(project.path(), bin.path(), &rs);
        hook(project.path(), bin.path(), &ts);
        assert!(!ran(project.path(), "rustfmt"));
        assert!(!ran(project.path(), "prettier"));
    }

    #[test]
    fn prettier_needs_its_config_and_a_web_extension() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        install(bin.path(), "prettier");
        std::fs::write(project.path().join(".prettierrc"), "{}").unwrap();
        let txt = project.path().join("notes.txt");
        std::fs::write(&txt, "").unwrap();
        hook(project.path(), bin.path(), &txt);
        assert!(!ran(project.path(), "prettier"));
        let ts = project.path().join("APP.TS");
        std::fs::write(&ts, "").unwrap();
        hook(project.path(), bin.path(), &ts);
        let args = std::fs::read_to_string(project.path().join("prettier.ran")).unwrap();
        assert_eq!(args.trim(), format!("--write {}", ts.to_str().unwrap()));
    }

    #[test]
    fn go_needs_no_config_and_python_prefers_ruff_over_black() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        install(bin.path(), "gofmt");
        install(bin.path(), "ruff");
        install(bin.path(), "black");
        std::fs::write(project.path().join("pyproject.toml"), "").unwrap();
        let go = project.path().join("main.go");
        let py = project.path().join("tool.py");
        std::fs::write(&go, "").unwrap();
        std::fs::write(&py, "").unwrap();
        hook(project.path(), bin.path(), &go);
        hook(project.path(), bin.path(), &py);
        assert!(ran(project.path(), "gofmt"));
        assert!(ran(project.path(), "ruff"));
        assert!(!ran(project.path(), "black"));
    }

    #[test]
    fn ignores_a_directory_and_a_non_executable_file_named_like_the_tool() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        // a directory named `rustfmt` and a plain file named `gofmt`: neither can run
        std::fs::create_dir(bin.path().join("rustfmt")).unwrap();
        std::fs::write(bin.path().join("gofmt"), "#!/bin/sh\ntouch gofmt.ran\n").unwrap();
        std::fs::write(project.path().join("Cargo.toml"), "").unwrap();
        let rs = project.path().join("lib.rs");
        let go = project.path().join("main.go");
        std::fs::write(&rs, "").unwrap();
        std::fs::write(&go, "").unwrap();
        hook(project.path(), bin.path(), &rs);
        hook(project.path(), bin.path(), &go);
        assert!(!ran(project.path(), "rustfmt"));
        assert!(!ran(project.path(), "gofmt"));
    }

    #[test]
    fn an_empty_cwd_runs_nothing_as_the_typescript_did() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        install(bin.path(), "gofmt");
        let go = project.path().join("main.go");
        std::fs::write(&go, "").unwrap();
        let input = json!({ "tool_input": { "file_path": go.to_str().unwrap() }, "cwd": "" });
        let (status, out) = run(input.to_string().as_bytes(), bin.path());
        assert_eq!((status, out.as_str()), (Some(0), "{}\n"));
        assert!(!ran(project.path(), "gofmt"));
    }

    #[test]
    fn a_failing_formatter_does_not_change_the_result() {
        let project = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let file = bin.path().join("gofmt");
        std::fs::write(&file, "#!/bin/sh\nexit 3\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let go = project.path().join("main.go");
        std::fs::write(&go, "").unwrap();
        hook(project.path(), bin.path(), &go);
    }
}
