//! `viby decide`: one typed question to a decision model on Vercel AI Gateway, and nothing on failure.
//!
//! Off unless `AI_GATEWAY_API_KEY` is set, and `VIBY_DECIDE=off` turns it off regardless. Models are
//! tried in order (`VIBY_DECIDE_MODELS`, comma-separated); any non-200, timeout or unusable answer moves
//! on to the next, inside one overall budget. The free tier limits each model separately, so a 429
//! on one model says nothing about the next.
//!
//! curl does the HTTP, so the crate needs no client. curl reads its whole config from stdin, which
//! keeps the key and the judged text out of argv (and so out of `ps`) and off the disk.
//!
//! CLI: stdin `{"state": ..., "questions": {...}}`, stdout `{"model": ..., "answers": {...}}`, exit 0;
//! exit 2 with no output when no model answered.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const DEFAULT_MODELS: &str = "baseten/mammoth-1,baseten/elephant-1,liquid/d1-ultra";
const DEFAULT_URL: &str = "https://ai-gateway.vercel.sh/v1/evaluate";
const PER_CALL: Duration = Duration::from_millis(1500);
const BUDGET: Duration = Duration::from_millis(3000);
/// Not worth starting a call with less than this left.
const MIN_CALL: Duration = Duration::from_millis(200);

pub struct Decision {
    pub model: String,
    pub answers: Value,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// A key is present and the person has not switched decisions off.
pub fn available() -> bool {
    env("AI_GATEWAY_API_KEY").is_some()
        && env("VIBY_DECIDE").is_none_or(|v| !v.eq_ignore_ascii_case("off"))
}

/// For calls the person did not ask for (the prompt hint, the agentgrep pick): they send what the
/// person is working on to a third party, so they also need `VIBY_DECIDE=on`.
pub fn opted_in() -> bool {
    env("VIBY_DECIDE").is_some_and(|v| v.eq_ignore_ascii_case("on")) && available()
}

/// Ask `questions` about `state`; the first model to give usable answers wins.
pub fn evaluate(state: &str, questions: &Value) -> Option<Decision> {
    if !available() {
        return None;
    }
    let key = env("AI_GATEWAY_API_KEY")?;
    let url = env("VIBY_DECIDE_URL").unwrap_or_else(|| DEFAULT_URL.to_string());
    let models = env("VIBY_DECIDE_MODELS").unwrap_or_else(|| DEFAULT_MODELS.to_string());
    let start = Instant::now();
    for model in models.split(',').map(str::trim).filter(|m| !m.is_empty()) {
        let left = BUDGET.saturating_sub(start.elapsed());
        if left < MIN_CALL {
            break;
        }
        let body = json!({"model": model, "state": state, "questions": questions}).to_string();
        let Some(response) = post(&url, &key, &body, left.min(PER_CALL)) else {
            continue;
        };
        if let Some(answers) = response.get("answers").filter(|a| a.is_object()) {
            return Some(Decision {
                model: model.to_string(),
                answers: answers.clone(),
            });
        }
    }
    None
}

/// A double-quoted value in a curl config file.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// POST `body`; the parsed JSON of a 200 response, else None.
fn post(url: &str, key: &str, body: &str, limit: Duration) -> Option<Value> {
    let config = format!(
        "url = {}\nrequest = \"POST\"\nsilent\nmax-time = {:.2}\nheader = {}\nheader = \"Content-Type: application/json\"\ndata-binary = {}\nwrite-out = \"\\n%{{http_code}}\"\n",
        quoted(url),
        limit.as_secs_f64(),
        quoted(&format!("Authorization: Bearer {key}")),
        quoted(body),
    );
    // -q first, or the person's ~/.curlrc applies too: it can reshape the output or log the key.
    let mut child = Command::new("curl")
        .args(["-q", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    ignore_sigpipe();
    let mut stdin = child.stdin.take()?;
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(config.as_bytes());
    });
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = stdout.read_to_end(&mut s);
        s
    });
    // curl's own --max-time should end it first; this kill is the backstop.
    let deadline = Instant::now() + limit + MIN_CALL;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => break None,
        }
    };
    let _ = writer.join();
    let out = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    if !status?.success() {
        return None;
    }
    let (body, code) = out.rsplit_once('\n')?;
    if code.trim() != "200" {
        return None;
    }
    serde_json::from_str(body).ok()
}

/// main() restores SIGPIPE's default, under which writing to a curl that exited early kills viby
/// itself instead of failing that one write. Every write here already tolerates an error.
fn ignore_sigpipe() {
    #[cfg(unix)]
    // SAFETY: setting a signal disposition to SIG_IGN has no preconditions.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}

pub fn main(_args: &[String]) -> i32 {
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let Ok(request) = serde_json::from_slice::<Value>(&input) else {
        return 2;
    };
    let (Some(state), Some(questions)) = (
        request.get("state").and_then(Value::as_str),
        request.get("questions").filter(|q| q.is_object()),
    ) else {
        return 2;
    };
    match evaluate(state, questions) {
        Some(d) => {
            let line = json!({"model": d.model, "answers": d.answers});
            let _ = writeln!(std::io::stdout().lock(), "{line}");
            0
        }
        None => 2,
    }
}
