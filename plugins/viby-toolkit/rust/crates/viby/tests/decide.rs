//! `viby decide` and `viby prompt-hint`: the opt-in gate, the model fallback, the time budget, and
//! silence on every failure.
//!
//! `curl` here is `tests/fixtures/decide/curl`, a fake shell script first on PATH. It answers each
//! model from a `plan` file (`<model> <behaviour>`) and logs its argv and the config it read from
//! stdin, so a test can see which models were tried, and that the key went through stdin and never
//! through argv.
//!
//! The two wall-clock tests take `GATE` exclusively: with fifteen other suites' processes spawning
//! alongside, process start-up alone adds seconds and the budget would be measured against load.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

static GATE: RwLock<()> = RwLock::new(());

enum Guard {
    Shared(#[allow(dead_code)] RwLockReadGuard<'static, ()>),
    Exclusive(#[allow(dead_code)] RwLockWriteGuard<'static, ()>),
}

const KEY: &str = "vck_test_key_must_not_reach_argv";

struct Fake {
    bin: tempfile::TempDir,
    skills: tempfile::TempDir,
    _guard: Guard,
}

impl Fake {
    fn new(plan: &[(&str, &str)]) -> Fake {
        let guard = Guard::Shared(GATE.read().unwrap_or_else(|e| e.into_inner()));
        Fake::with(plan, guard)
    }

    /// For a wall-clock test: nothing else in this suite runs meanwhile.
    fn alone(plan: &[(&str, &str)]) -> Fake {
        let guard = Guard::Exclusive(GATE.write().unwrap_or_else(|e| e.into_inner()));
        Fake::with(plan, guard)
    }

    fn with(plan: &[(&str, &str)], guard: Guard) -> Fake {
        let bin = tempfile::tempdir().unwrap();
        let lines: String = plan.iter().map(|(m, b)| format!("{m} {b}\n")).collect();
        std::fs::write(bin.path().join("plan"), lines).unwrap();
        let skills = tempfile::tempdir().unwrap();
        for (name, desc) in [
            ("test", "Writing or auditing tests."),
            ("perf", "Speed and memory."),
        ] {
            let dir = skills.path().join(name);
            std::fs::create_dir(&dir).unwrap();
            std::fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: {desc}\n---\n\nBody.\n"),
            )
            .unwrap();
        }
        Fake {
            bin,
            skills,
            _guard: guard,
        }
    }

    fn log(&self, name: &str) -> String {
        std::fs::read_to_string(self.bin.path().join(name)).unwrap_or_default()
    }

    fn models_tried(&self) -> Vec<String> {
        self.log("models.log").lines().map(String::from).collect()
    }

    /// The checked-in fake curl first. Writing it per test would race other tests' spawns
    /// ("text file busy" on exec).
    fn path(&self) -> String {
        let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decide");
        format!("{}:/usr/bin:/bin", fake.display())
    }

    /// `viby <cmd>` with only `env` set besides PATH: (exit status, stdout).
    fn run(&self, cmd: &str, input: &str, env: &[(&str, &str)]) -> (Option<i32>, String) {
        let mut c = Command::new(env!("CARGO_BIN_EXE_viby"));
        c.arg(cmd);
        if cmd == "prompt-hint" {
            c.arg(self.skills.path());
        }
        c.env_clear()
            .env("PATH", self.path())
            .env("FAKE_CURL_DIR", self.bin.path());
        for (k, v) in env {
            c.env(k, v);
        }
        let mut child = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn viby");
        // Not opted in, viby exits without reading stdin; on Linux this write then fails with a
        // broken pipe, which is the behaviour under test, not an error.
        let _ = child.stdin.take().unwrap().write_all(input.as_bytes());
        let out = child.wait_with_output().unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }
}

fn ok(choice: &str, confidence: f64) -> String {
    format!(
        r#"ok:{{"answers":{{"skill":{{"type":"choice","choice":"{choice}","confidence":{confidence}}}}}}}"#
    )
}

const PROMPT: &str = r#"{"prompt":"write tests for the pricing module","cwd":"/tmp"}"#;

fn on(models: &str) -> Vec<(&'static str, String)> {
    vec![
        ("AI_GATEWAY_API_KEY", KEY.to_string()),
        ("VIBY_DECIDE", "on".to_string()),
        ("VIBY_DECIDE_MODELS", models.to_string()),
    ]
}

fn hint(fake: &Fake, models: &str, input: &str) -> (Option<i32>, String) {
    let env = on(models);
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    fake.run("prompt-hint", input, &env)
}

#[test]
fn a_confident_answer_becomes_one_hint_line() {
    let a = ok("test", 0.97);
    let fake = Fake::new(&[("m/a", &a)]);
    let (status, out) = hint(&fake, "m/a", PROMPT);
    assert_eq!(status, Some(0));
    assert_eq!(
        out,
        "Skill hint: viby-toolkit:test (confidence 0.97, from a decision model; use it if it fits)\n"
    );
}

#[test]
fn every_http_failure_moves_on_to_the_next_model_in_order() {
    let e = ok("perf", 0.95);
    let fake = Fake::new(&[
        ("m/a", "403"),
        ("m/b", "404"),
        ("m/c", "429"),
        ("m/d", "500"),
        ("m/e", &e),
    ]);
    let (status, out) = hint(&fake, "m/a,m/b,m/c,m/d,m/e", PROMPT);
    assert_eq!(status, Some(0));
    assert!(out.starts_with("Skill hint: viby-toolkit:perf "), "{out}");
    assert_eq!(fake.models_tried(), ["m/a", "m/b", "m/c", "m/d", "m/e"]);
}

#[test]
fn garbage_bodies_and_a_crashed_curl_move_on_too() {
    let c = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", "garbage"), ("m/b", "crash"), ("m/c", &c)]);
    let (_, out) = hint(&fake, "m/a,m/b,m/c", PROMPT);
    assert!(out.starts_with("Skill hint: viby-toolkit:test "), "{out}");
}

#[test]
fn a_200_without_answers_is_not_an_answer() {
    let b = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", r#"ok:{"error":"odd"}"#), ("m/b", &b)]);
    let (_, out) = hint(&fake, "m/a,m/b", PROMPT);
    assert!(out.starts_with("Skill hint: viby-toolkit:test "), "{out}");
    assert_eq!(fake.models_tried(), ["m/a", "m/b"]);
}

#[test]
fn a_hung_model_is_killed_and_the_next_one_answers_within_the_budget() {
    let b = ok("perf", 0.93);
    let fake = Fake::alone(&[("m/a", "sleep"), ("m/b", &b)]);
    let start = Instant::now();
    let (status, out) = hint(&fake, "m/a,m/b", PROMPT);
    let took = start.elapsed();
    assert_eq!(status, Some(0));
    assert!(out.starts_with("Skill hint: viby-toolkit:perf "), "{out}");
    assert!(took < Duration::from_millis(3500), "took {took:?}");
}

#[test]
fn hung_models_cannot_outlast_the_overall_budget() {
    let fake = Fake::alone(&[("m/a", "sleep"), ("m/b", "sleep"), ("m/c", "sleep")]);
    let start = Instant::now();
    let (status, out) = hint(&fake, "m/a,m/b,m/c", PROMPT);
    let took = start.elapsed();
    assert_eq!((status, out.as_str()), (Some(0), ""));
    assert!(took < Duration::from_millis(3800), "took {took:?}");
    assert!(fake.models_tried().len() < 3, "{:?}", fake.models_tried());
}

#[test]
fn when_every_model_fails_the_hook_prints_nothing_and_exits_0() {
    let fake = Fake::new(&[("m/a", "429"), ("m/b", "500"), ("m/c", "garbage")]);
    assert_eq!(hint(&fake, "m/a,m/b,m/c", PROMPT), (Some(0), String::new()));
    assert_eq!(fake.models_tried().len(), 3);
}

#[test]
fn low_confidence_and_none_say_nothing() {
    for answer in [ok("test", 0.89), ok("none", 0.99)] {
        let fake = Fake::new(&[("m/a", &answer)]);
        assert_eq!(
            hint(&fake, "m/a", PROMPT),
            (Some(0), String::new()),
            "{answer}"
        );
    }
}

#[test]
fn confidence_falls_back_to_the_chosen_options_probability() {
    let a =
        r#"ok:{"answers":{"skill":{"choice":"perf","probabilities":{"perf":0.96,"test":0.04}}}}"#;
    let fake = Fake::new(&[("m/a", a)]);
    let (_, out) = hint(&fake, "m/a", PROMPT);
    assert!(
        out.starts_with("Skill hint: viby-toolkit:perf (confidence 0.96"),
        "{out}"
    );
}

#[test]
fn slash_commands_and_empty_prompts_never_reach_curl() {
    let a = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    for input in [
        r#"{"prompt":"/viby-toolkit:review"}"#,
        r#"{"prompt":"   "}"#,
        r#"{"prompt":""}"#,
    ] {
        assert_eq!(
            hint(&fake, "m/a", input),
            (Some(0), String::new()),
            "{input}"
        );
    }
    assert!(fake.models_tried().is_empty());
}

#[test]
fn without_both_the_key_and_viby_decide_on_curl_never_runs() {
    let a = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    let cases: [&[(&str, &str)]; 6] = [
        &[],
        &[("VIBY_DECIDE", "on")],
        &[("AI_GATEWAY_API_KEY", KEY)],
        &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE", "off")],
        &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE", "yes")],
        &[("AI_GATEWAY_API_KEY", "  "), ("VIBY_DECIDE", "on")],
    ];
    for env in cases {
        let mut env = env.to_vec();
        env.push(("VIBY_DECIDE_MODELS", "m/a"));
        assert_eq!(
            fake.run("prompt-hint", PROMPT, &env),
            (Some(0), String::new()),
            "{env:?}"
        );
    }
    assert!(fake.models_tried().is_empty());
}

#[test]
fn the_key_goes_through_stdin_never_argv() {
    let a = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    hint(&fake, "m/a", PROMPT);
    let argv = fake.log("argv.log");
    assert_eq!(argv.trim(), "-q -K -");
    assert!(!argv.contains(KEY) && !argv.contains("pricing"));
    let config = fake.log("config.log");
    assert!(config.contains(&format!("Bearer {KEY}")), "{config}");
    assert!(config.contains("pricing module"), "{config}");
}

#[test]
fn unreadable_input_is_silent() {
    let a = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    for input in ["", "not json", "null", "[]", "{}", r#"{"prompt":7}"#] {
        assert_eq!(
            hint(&fake, "m/a", input),
            (Some(0), String::new()),
            "{input}"
        );
    }
}

#[test]
fn decide_prints_the_answering_model_and_its_answers() {
    let b = ok("test", 0.9);
    let fake = Fake::new(&[("m/a", "429"), ("m/b", &b)]);
    let req = r#"{"state":"x","questions":{"skill":{"type":"choice","instructions":"y","criteria":{"test":"t"}}}}"#;
    let env = [
        ("AI_GATEWAY_API_KEY", KEY),
        ("VIBY_DECIDE_MODELS", "m/a,m/b"),
    ];
    let (status, out) = fake.run("decide", req, &env);
    assert_eq!(status, Some(0));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["model"], "m/b");
    assert_eq!(v["answers"]["skill"]["choice"], "test");
}

#[test]
fn decide_exits_2_silently_when_off_unanswered_or_malformed() {
    let a = ok("test", 0.9);
    let fake = Fake::new(&[("m/a", &a), ("m/x", "500")]);
    let req = r#"{"state":"x","questions":{"q":{"type":"boolean","instructions":"y"}}}"#;
    let cases: [(&str, &[(&str, &str)]); 5] = [
        (req, &[("VIBY_DECIDE_MODELS", "m/a")]),
        (
            req,
            &[
                ("AI_GATEWAY_API_KEY", KEY),
                ("VIBY_DECIDE", "OFF"),
                ("VIBY_DECIDE_MODELS", "m/a"),
            ],
        ),
        (
            req,
            &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE_MODELS", "m/x")],
        ),
        (
            "not json",
            &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE_MODELS", "m/a")],
        ),
        (
            r#"{"state":"x"}"#,
            &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE_MODELS", "m/a")],
        ),
    ];
    for (input, env) in cases {
        assert_eq!(
            fake.run("decide", input, env),
            (Some(2), String::new()),
            "{input} {env:?}"
        );
    }
    assert_eq!(fake.models_tried(), ["m/x"]);
}

fn hook_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../hooks/prompt-hint.sh")
}

#[test]
fn the_shell_gate_prints_nothing_unless_opted_in() {
    let fake = Fake::new(&[]);
    let cases: [&[(&str, &str)]; 4] = [
        &[],
        &[("VIBY_DECIDE", "on")],
        &[("AI_GATEWAY_API_KEY", KEY)],
        &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE", "off")],
    ];
    for env in cases {
        let mut c = Command::new("sh");
        c.arg(hook_script())
            .env_clear()
            .env("PATH", fake.path())
            .env("FAKE_CURL_DIR", fake.bin.path());
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = out;
        // The gate exits without reading stdin, so this write may meet a closed pipe.
        let _ = child.stdin.take().unwrap().write_all(PROMPT.as_bytes());
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{env:?}");
        assert!(out.stdout.is_empty() && out.stderr.is_empty(), "{env:?}");
    }
    assert!(fake.models_tried().is_empty());
}

#[test]
fn replies_too_short_to_route_and_prompts_that_name_a_skill_never_reach_curl() {
    let a = ok("test", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    for prompt in [
        "yes",
        "go on",
        "ok, do it",
        "use viby-toolkit:review on this diff please",
        "run the Viby-Toolkit:test skill for pricing",
    ] {
        let input = serde_json::json!({ "prompt": prompt }).to_string();
        assert_eq!(
            hint(&fake, "m/a", &input),
            (Some(0), String::new()),
            "{prompt}"
        );
    }
    assert!(fake.models_tried().is_empty(), "{:?}", fake.models_tried());
}

/// A small project for `find`: two files about charging a card, one unrelated. It sits in `repo/`
/// because a temp dir's own name starts with a dot, and agentgrep skips hidden paths.
struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn path(&self) -> &Path {
        &self.root
    }
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let files = [
        ("card_billing.rs", "/// Charge the customer's card.\npub fn charge_card(amount: u64) -> bool {\n    amount > 0\n}\n"),
        ("card_refunds.rs", "/// Give a card charge back.\npub fn refund_card_charge(amount: u64) -> bool {\n    amount > 0\n}\n"),
        ("logging.rs", "pub fn log_line(text: &str) -> usize {\n    text.len()\n}\n"),
    ];
    for (name, body) in files {
        std::fs::write(root.join(name), body).unwrap();
    }
    Project { _dir: dir, root }
}

/// One agentgrep `find` through `viby agentgrep-server`, in its own process so its env is its own.
fn find(fake: &Fake, root: &Path, args: serde_json::Value, env: &[(&str, &str)]) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_viby"));
    c.arg("agentgrep-server")
        .current_dir(root)
        .env_clear()
        .env("PATH", fake.path())
        .env("FAKE_CURL_DIR", fake.bin.path());
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut call = serde_json::json!({"mode": "find", "query": "card", "path": root});
    call.as_object_mut()
        .unwrap()
        .extend(args.as_object().unwrap().clone());
    let messages = [
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "agentgrep", "arguments": call}}),
    ];
    let mut stdin = child.stdin.take().unwrap();
    for m in messages {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let reply = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|v| v["id"] == 2)
        .expect("a reply to the tool call");
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

fn pick(choice: &str, confidence: f64) -> String {
    format!(
        r#"ok:{{"answers":{{"file":{{"type":"choice","choice":"{choice}","confidence":{confidence}}}}}}}"#
    )
}

const ON: [(&str, &str); 3] = [
    ("AI_GATEWAY_API_KEY", KEY),
    ("VIBY_DECIDE", "on"),
    ("VIBY_DECIDE_MODELS", "m/a"),
];

#[test]
fn find_leads_with_a_confident_pick_and_is_otherwise_unchanged() {
    let a = pick("card_billing.rs", 0.97);
    let fake = Fake::new(&[("m/a", &a)]);
    let root = project();
    let plain = find(&fake, root.path(), serde_json::json!({}), &[]);
    let picked = find(&fake, root.path(), serde_json::json!({}), &ON);
    assert_eq!(
        picked,
        format!(
            "Decision model's pick: card_billing.rs (confidence 0.97; open it first if it fits)\n\n{plain}"
        )
    );
    // The model chose among the candidates agentgrep ranked, all of them, not only those shown.
    let config = fake.log("config.log");
    assert!(
        config.contains("card_billing.rs") && config.contains("card_refunds.rs"),
        "{config}"
    );
}

#[test]
fn find_is_byte_identical_when_off_unsure_failing_or_off_list() {
    let root = project();
    let baseline = {
        let fake = Fake::new(&[]);
        find(&fake, root.path(), serde_json::json!({}), &[])
    };
    let unsure = pick("card_billing.rs", 0.89);
    let off_list = pick("elsewhere.rs", 0.99);
    for (plan, env) in [
        (
            pick("card_billing.rs", 0.99),
            &[("AI_GATEWAY_API_KEY", KEY), ("VIBY_DECIDE_MODELS", "m/a")][..],
        ),
        (unsure, &ON[..]),
        ("500".to_string(), &ON[..]),
        (off_list, &ON[..]),
    ] {
        let fake = Fake::new(&[("m/a", &plan)]);
        assert_eq!(
            find(&fake, root.path(), serde_json::json!({}), env),
            baseline,
            "{plan}"
        );
    }
    assert!(baseline.contains("card_billing.rs"), "{baseline}");
}

#[test]
fn find_with_paths_only_never_asks() {
    let a = pick("card_billing.rs", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    let root = project();
    let out = find(
        &fake,
        root.path(),
        serde_json::json!({"paths_only": true}),
        &ON,
    );
    assert!(!out.contains("Decision model"), "{out}");
    assert!(fake.models_tried().is_empty());
}

#[test]
fn a_skill_that_was_not_offered_is_never_named() {
    let a = ok("rm -rf everything", 0.99);
    let fake = Fake::new(&[("m/a", &a)]);
    assert_eq!(hint(&fake, "m/a", PROMPT), (Some(0), String::new()));
}

#[test]
fn off_with_surrounding_whitespace_still_turns_decide_off() {
    let a = ok("test", 0.9);
    let fake = Fake::new(&[("m/a", &a)]);
    let req = r#"{"state":"x","questions":{"q":{"type":"boolean","instructions":"y"}}}"#;
    for off in ["off ", " OFF", "off\n"] {
        let env = [
            ("AI_GATEWAY_API_KEY", KEY),
            ("VIBY_DECIDE", off),
            ("VIBY_DECIDE_MODELS", "m/a"),
        ];
        assert_eq!(
            fake.run("decide", req, &env),
            (Some(2), String::new()),
            "{off:?}"
        );
    }
    assert!(fake.models_tried().is_empty());
}

#[test]
fn find_candidates_keeps_paths_with_spaces_and_their_own_descriptions() {
    let out = "query: card\ntop files: 2\n\n1. My App/card billing.rs\n   role: implementation\n\n2. card_refunds.rs\n   role: generic\n";
    let files = viby::agentgrep::find_candidates(out);
    let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(names, ["My App/card billing.rs", "card_refunds.rs"]);
    assert_eq!(files[0].1, "role: implementation");
    assert_eq!(files[1].1, "role: generic");
}

/// The hook as Claude Code runs it, opted in, with `data` as the launcher's cache root.
fn hook_opted_in(data: &Path) -> std::process::Output {
    let mut child = Command::new("sh")
        .arg(hook_script())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", data)
        .env("CLAUDE_PLUGIN_DATA", data)
        .env("AI_GATEWAY_API_KEY", KEY)
        .env("VIBY_DECIDE", "on")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(PROMPT.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn the_hook_never_downloads_or_builds_on_a_cache_miss() {
    let data = tempfile::tempdir().unwrap();
    let start = Instant::now();
    let out = hook_opted_in(data.path());
    assert_eq!(out.status.code(), Some(0));
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
    assert!(!data.path().join("bin").exists());
}

#[test]
fn the_hook_exits_0_even_when_the_cached_viby_exits_2() {
    // A cached binary from before prompt-hint existed answers "unknown command" with exit 2,
    // which on UserPromptSubmit would block the prompt.
    // It writes an executable and runs it: exclusive, or another test's spawn can hold the file
    // open for writing at exec ("text file busy").
    let _alone = GATE.write().unwrap_or_else(|e| e.into_inner());
    let data = tempfile::tempdir().unwrap();
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.claude-plugin/plugin.json");
    let plugin: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
    let dir = data
        .path()
        .join("bin")
        .join(plugin["version"].as_str().unwrap());
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join("viby");
    std::fs::write(
        &old,
        "#!/bin/sh\necho 'usage: viby <command>' >&2\nexit 2\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out = hook_opted_in(data.path());
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
}
