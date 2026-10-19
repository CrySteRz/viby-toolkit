//! viby-toolkit statusline: makes the two resources that actually bind visible.
//!
//! Prints one line:
//!   <model> · ctx NN% · cache NN% · 5h NN% · 7d NN% · $C.CC
//!
//!   ctx    percentage of the context window in use (context_window.used_percentage,
//!          which is input-only: input + cache_creation + cache_read). Colour-banded
//!          green <60 / yellow <80 / red, matching the 40-60% target the toolkit aims for.
//!   cache  cache-read share of input tokens: confirms the prompt cache is being reused.
//!          On a subscription, cache hits are the cheapest tokens you have.
//!   5h/7d  rate-limit consumption (rate_limits.five_hour / .seven_day). On a Max plan the
//!          scarce resources are context and rate limit, not dollars, so these matter more
//!          than the cost figure. Only present for Pro/Max, after the first API response.
//!   $      client-side session cost estimate; omitted when not reported.
//!
//! Fully generic; never blocks. On any error it prints a minimal line so the statusline
//! never breaks. Fields that are absent or null (early in a session, or right after
//! /compact) are simply skipped rather than shown as 0.
//!
//! It runs on every UI update and reads nothing but stdin.
//!
//! Wire it up (statuslines live in settings.json, not in plugin hooks.json):
//!
//!   "statusLine": { "type": "command", "command": "viby statusline" }
//!
//! The numbers are formatted the way JavaScript formats them (`Math.round`, `Number(string)`,
//! `toFixed`, `String(number)`), so the line is byte for byte what the TypeScript printed.

use std::io::{Read, Write};

use serde_json::Value;

use crate::core::js::js_trim;

const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// What is printed when the payload cannot be read: the statusline must never break.
const FALLBACK: &str = "viby-toolkit";

/// Colour a percentage: green below `low`, yellow below `high`, red beyond. A NaN is red, as
/// in the TypeScript (every comparison is false).
fn band(pct: f64, low: f64, high: f64) -> &'static str {
    if pct < low {
        GREEN
    } else if pct < high {
        YELLOW
    } else {
        RED
    }
}

/// JavaScript `Math.round`: halves go toward +infinity (Rust's `round` goes away from zero).
fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// JavaScript `String(x)` for a whole number (every value printed here is whole): no exponent
/// below 1e21, `1e+21` style from there, and `0` for negative zero.
fn js_num(x: f64) -> String {
    if x.is_nan() {
        "NaN".into()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if x == 0.0 {
        "0".into()
    } else if x.abs() >= 1e21 {
        format!("{x:e}").replacen('e', "e+", 1)
    } else {
        format!("{x}")
    }
}

/// JavaScript `x.toFixed(2)` for a positive finite `x`. Rust breaks an exact tie to even,
/// JavaScript takes the larger neighbour (0.125 prints `0.13`, not `0.12`).
fn to_fixed_2(x: f64) -> String {
    if x >= 1e21 {
        return js_num(x);
    }
    let eighths = x * 8.0;
    let exact_tie = x < 1e12 && eighths.fract() == 0.0 && (eighths % 2.0) == 1.0;
    if exact_tie {
        // x is j/8 with j odd, so x * 100 is a half-cent: round it up.
        let cents = (x * 100.0 + 0.5).floor() as u64;
        return format!("{}.{:02}", cents / 100, cents % 100);
    }
    format!("{x:.2}")
}

/// Is `s` a JavaScript StrDecimalLiteral (sign, digits, fraction, exponent; no `inf`, no `nan`)?
fn is_decimal_literal(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let int = digits(i);
    i += int;
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        frac = digits(i + 1);
        i += 1 + frac;
    }
    if int + frac == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp = digits(i);
        if exp == 0 {
            return false;
        }
        i += exp;
    }
    i == b.len()
}

/// JavaScript `Number(s)`: NaN when `s` is not a numeric literal; an empty string is 0.
fn js_string_to_number(s: &str) -> f64 {
    // `Number(string)` trims JavaScript white space and line terminators, as `trim()` does.
    let s = js_trim(s);
    if s.is_empty() {
        return 0.0;
    }
    let radix = match s.get(..2) {
        Some("0x" | "0X") => 16,
        Some("0o" | "0O") => 8,
        Some("0b" | "0B") => 2,
        _ => 0,
    };
    if radix != 0 {
        let digits = &s[2..];
        if digits.is_empty() {
            return f64::NAN;
        }
        let mut n = 0.0_f64;
        for c in digits.chars() {
            match c.to_digit(radix) {
                Some(d) => n = n * f64::from(radix) + f64::from(d),
                None => return f64::NAN,
            }
        }
        return n;
    }
    match s {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ if is_decimal_literal(s) => s.parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

/// Read a percentage field, returning `None` when absent or null.
///
/// Clamped to 0-100: an out-of-range value is upstream nonsense, and rendering it raw was
/// actively misleading: a `-50` would print as a reassuring green "ctx -50%" because the
/// colour band only asks whether the number is below the green threshold.
fn pct_of(node: Option<&Value>) -> Option<f64> {
    let n = match node?.get("used_percentage")? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => js_string_to_number(s),
        _ => f64::NAN,
    };
    n.is_finite().then(|| js_round(n).clamp(0.0, 100.0))
}

fn num_or_0(node: Option<&Value>, key: &str) -> f64 {
    node.and_then(|n| n.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

/// JavaScript truthiness of a number.
fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

fn non_empty_str<'a>(node: Option<&'a Value>, key: &str) -> Option<&'a str> {
    node.and_then(|n| n.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Render the line (without its newline) for one stdin payload. Anything that is not JSON
/// gives the minimal `viby-toolkit` line.
pub fn render(input: &[u8]) -> String {
    // Node decodes stdin as UTF-8 and replaces bad bytes; do the same.
    let input = String::from_utf8_lossy(input);
    let Ok(d) = serde_json::from_str::<Value>(&input) else {
        return FALLBACK.into();
    };

    let model_node = d.get("model");
    let model = non_empty_str(model_node, "display_name")
        .or_else(|| non_empty_str(model_node, "id"))
        .unwrap_or("claude");
    let mut line = format!("{DIM}{model}{RESET}");

    let cw = d.get("context_window");
    let usage = cw.and_then(|c| c.get("current_usage"));
    let inp = num_or_0(usage, "input_tokens");
    let cc = num_or_0(usage, "cache_creation_input_tokens");
    let cr = num_or_0(usage, "cache_read_input_tokens");
    let total_in = inp + cc + cr;

    // --- context window
    let pct = pct_of(cw).or_else(|| {
        let cap = num_or_0(cw, "context_window_size");
        (truthy(cap) && truthy(total_in)).then(|| js_round(total_in * 100.0 / cap))
    });
    if let Some(pct) = pct {
        let colour = band(pct, 60.0, 80.0);
        line.push_str(&format!(" · {colour}ctx {}%{RESET}", js_num(pct)));
    }

    // --- prompt-cache reuse share
    if truthy(total_in) {
        let share = js_num(js_round(cr * 100.0 / total_in));
        line.push_str(&format!(" · {DIM}cache {share}%{RESET}"));
    }

    // --- rate limits: the real ceiling on a subscription
    let rl = d.get("rate_limits");
    for (label, key) in [("5h", "five_hour"), ("7d", "seven_day")] {
        if let Some(rpct) = pct_of(rl.and_then(|r| r.get(key))) {
            let colour = band(rpct, 50.0, 75.0);
            line.push_str(&format!(" · {colour}{label} {}%{RESET}", js_num(rpct)));
        }
    }

    // --- cost, last: informational on a subscription
    let cost = d
        .get("cost")
        .and_then(|c| c.get("total_cost_usd"))
        .and_then(Value::as_f64);
    if let Some(cost) = cost.filter(|c| *c > 0.0) {
        line.push_str(&format!(" · {DIM}${}{RESET}", to_fixed_2(cost)));
    }
    line
}

pub fn main(_args: &[String]) -> i32 {
    let mut input = Vec::new();
    let line = match std::io::stdin().lock().read_to_end(&mut input) {
        Ok(_) => render(&input),
        Err(_) => FALLBACK.into(),
    };
    // A closed pipe must not change the exit code: the statusline never blocks.
    let _ = writeln!(std::io::stdout().lock(), "{line}");
    0
}
