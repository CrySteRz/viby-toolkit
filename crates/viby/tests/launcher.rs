//! The POSIX launcher (`plugins/viby-toolkit/launcher/viby`) against a fake plugin, a fake release
//! served from a `file://` folder, and a fake `cargo`: no network and no real build.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const LAUNCHER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plugins/viby-toolkit/launcher/viby"
);

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    data: PathBuf,
    release: PathBuf,
    path_dir: PathBuf,
}

fn write_exe(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn sha256(path: &Path) -> String {
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .or_else(|_| Command::new("sha256sum").arg(path).output())
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", _)
            if Command::new("sysctl")
                .args(["-n", "hw.optional.arm64"])
                .output()
                .is_ok_and(|o| o.stdout.starts_with(b"1")) =>
        {
            "aarch64-apple-darwin"
        }
        ("macos", _) => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    }
}

/// A plugin at version 9.9.9 whose release has a `viby` that prints its tag and arguments.
fn fixture(version: &str) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugin");
    fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    fs::write(
        root.join(".claude-plugin/plugin.json"),
        format!("{{\n  \"name\": \"viby-toolkit\",\n  \"version\": \"{version}\"\n}}\n"),
    )
    .unwrap();
    fs::create_dir_all(root.join("launcher")).unwrap();
    fs::copy(LAUNCHER, root.join("launcher/viby")).unwrap();

    let release = tmp.path().join("release");
    let stage = tmp.path().join("stage");
    write_exe(&stage.join("viby"), "#!/bin/sh\necho \"downloaded $*\"\n");
    let asset = format!("viby-{}.tar.gz", target());
    fs::create_dir_all(release.join(format!("v{version}"))).unwrap();
    let tarball = release.join(format!("v{version}/{asset}"));
    assert!(Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(&stage)
        .arg("viby")
        .status()
        .unwrap()
        .success());
    fs::write(
        root.join("launcher/checksums.txt"),
        format!("{}  {asset}\n", sha256(&tarball)),
    )
    .unwrap();

    // A PATH with the basics but no cargo, unless a test adds one.
    let path_dir = tmp.path().join("path");
    fs::create_dir_all(&path_dir).unwrap();
    Fixture {
        data: tmp.path().join("data"),
        root,
        release,
        path_dir,
        _tmp: tmp,
    }
}

impl Fixture {
    fn run(&self, args: &[&str]) -> Output {
        Command::new("sh")
            .arg(self.root.join("launcher/viby"))
            .args(args)
            .env_clear()
            .env("HOME", self._tmp.path())
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.path_dir.display()),
            )
            .env("CLAUDE_PLUGIN_DATA", &self.data)
            .env(
                "VIBY_RELEASE_URL",
                format!("file://{}", self.release.display()),
            )
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn first_call_downloads_verifies_and_runs_then_uses_the_cache() {
    let f = fixture("9.9.9");
    let first = f.run(&["check-plan", "a.md"]);
    assert!(first.status.success(), "{}", stderr(&first));
    assert_eq!(stdout(&first), "downloaded check-plan a.md\n");
    let exe = f.data.join("bin/9.9.9/viby");
    assert!(exe.exists());
    // Second call: the release is gone, so it can only have come from the cache.
    fs::remove_dir_all(&f.release).unwrap();
    assert_eq!(stdout(&f.run(&["x"])), "downloaded x\n");
}

#[test]
fn a_tampered_asset_is_refused_and_never_run() {
    let f = fixture("9.9.9");
    let asset = f.release.join(format!("v9.9.9/viby-{}.tar.gz", target()));
    let stage = f._tmp.path().join("evil");
    write_exe(&stage.join("viby"), "#!/bin/sh\necho evil\n");
    assert!(Command::new("tar")
        .arg("-czf")
        .arg(&asset)
        .arg("-C")
        .arg(&stage)
        .arg("viby")
        .status()
        .unwrap()
        .success());
    let out = f.run(&["check-plan"]);
    assert!(!stdout(&out).contains("evil"));
    assert!(stderr(&out).contains("refused"), "{}", stderr(&out));
    assert!(!out.status.success(), "no cargo either, so it gives up");
    assert!(!f.data.join("bin/9.9.9/viby").exists());
}

#[test]
fn without_a_build_or_cargo_it_says_what_to_run_and_the_statusline_still_prints() {
    let f = fixture("9.9.9");
    fs::write(f.root.join("launcher/checksums.txt"), "").unwrap(); // nothing published for this version
    let out = f.run(&["check-plan"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("sh.rustup.rs"), "{}", stderr(&out));
    let status = f.run(&["statusline"]);
    assert!(status.status.success());
    assert_eq!(stdout(&status), "viby-toolkit\n");
}

#[test]
fn with_no_published_build_it_builds_with_cargo() {
    let f = fixture("9.9.9");
    fs::write(f.root.join("launcher/checksums.txt"), "").unwrap();
    // A cargo that "builds" by writing a binary where --target-dir says.
    write_exe(
        &f.path_dir.join("cargo"),
        "#!/bin/sh\nwhile [ $# -gt 0 ]; do [ \"$1\" = --target-dir ] && dir=$2; shift; done\nmkdir -p \"$dir/release\"\nprintf '#!/bin/sh\\necho \"built $*\"\\n' > \"$dir/release/viby\"\nchmod +x \"$dir/release/viby\"\n",
    );
    let out = f.run(&["detect-stack", "."]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "built detect-stack .\n");
    assert!(stderr(&out).contains("building viby 9.9.9"));
}

#[test]
fn an_update_fetches_the_new_version_and_keeps_only_the_previous_one() {
    let f = fixture("9.9.9");
    for old in ["1.0.0", "2.0.0"] {
        write_exe(
            &f.data.join(format!("bin/{old}/viby")),
            "#!/bin/sh\necho old\n",
        );
        std::thread::sleep(std::time::Duration::from_millis(1100)); // distinct mtimes for `ls -t`
    }
    assert_eq!(stdout(&f.run(&["x"])), "downloaded x\n");
    let mut left: Vec<String> = fs::read_dir(f.data.join("bin"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["2.0.0", "9.9.9"],
        "the newest older version stays for sessions still running it"
    );
}

#[test]
fn install_only_fetches_and_runs_nothing() {
    let f = fixture("9.9.9");
    let out = f.run(&["--install"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "", "the binary was installed, not run");
    assert!(f.data.join("bin/9.9.9/viby").exists());
    assert!(
        f.run(&["--install"]).status.success(),
        "and again, from the cache"
    );
}
