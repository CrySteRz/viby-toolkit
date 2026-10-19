//! viby-toolkit read-cost meter — the executable half of /viby-toolkit:evaluate.
//!
//! Answers one question before you spend the context: **what would it cost to just read
//! this?** That number is the baseline every "this tool saves you tokens" claim has to beat,
//! and the denominator of every savings ratio. Without it, a savings claim is a vendor
//! adjective. It also answers the plainer daily question — "does this read set fit in the
//! budget I have left, or do I need a subagent?" (`/viby-toolkit:principles` §2).
//!
//! Usage:
//!   viby measure-read-cost [paths...] [--repeat N] [--budget N] [--window N]
//!                          [--top N] [--json] [--quiet]
//! Exit: 0 = measured (and within budget if one was given), 1 = over budget,
//!       2 = nothing measurable found.
//!
//! ACCURACY — MEASURED, not asserted. This is an estimate, not a tokenizer: it counts characters
//! and divides by a per-kind ratio, charging non-ASCII at ~1 token each.
//!
//! Calibrated 2026-07-29 against **tiktoken `cl100k_base` on 400 real files** (TypeScript, TSX,
//! Python, SQL, YAML, JSON, Markdown, shell, drawn from four working repositories):
//!
//!   kind        median error   p90 |error|   within ±15%
//!   code            -1.1%         13.5%        93%
//!   prose           +0.1%          5.4%       100%
//!   data (yml/json)  0.0%         39.7%        68%   ← wide: token density varies enormously
//!   overall         -0.5%         17.5%        85%   (95% within ±25%)
//!
//! The first version of this file claimed "±15%" from reasoning alone. Measured, that claim was
//! false: 33% of files fell outside it and every ratio was biased low, over-estimating by ~9%.
//! The ratios here are the corrected ones, and `tests/read_cost.rs` pins four fixtures against
//! token counts produced by the real tokenizer so a future edit cannot silently decalibrate them.
//!
//! Fit for: "is this read set 7k or 87k", "does this fit in the remaining budget", "is that
//! savings claim real". NOT fit for a precise published number, and NOT reliable per-file on
//! JSON/YAML — use it on a set, where the errors cancel, rather than on one config file.
//!
//! Port notes: lengths are JavaScript string lengths (UTF-16 units), as the calibration was
//! made with them. Numbers on the command line are read with JS `Number()` rules, and `--top`
//! goes through `Array.prototype.slice` semantics (negative and fractional values included).

use crate::core::args::{flag, parse_or_exit, value};
use crate::core::js::{js_trim, sort_utf16, utf16_len};
use crate::core::nodepath::{basename, extname, join};
use serde::Serialize;
use serde_json::{json, Map, Number, Value};
use std::collections::BTreeSet;
use std::fs;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Code,
    Prose,
    Data,
    Generated,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Code => "code",
            Kind::Prose => "prose",
            Kind::Data => "data",
            Kind::Generated => "generated",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FileCost {
    pub file: String,
    pub bytes: usize,
    pub chars: usize,
    pub tokens: u64,
    pub kind: Kind,
    /// Set when the estimate for this file is less trustworthy than the headline error bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Skipped {
    pub file: String,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct Measurement {
    pub files: Vec<FileCost>,
    pub skipped: Vec<Skipped>,
    /// Estimated tokens for ONE pass over every measured file.
    pub tokens: u64,
    /// tokens × repeat — what a tool that re-sends its whole state each step actually costs.
    pub total_tokens: f64,
    pub repeat: f64,
    /// Tokens attributable to generated/lock files, which are almost never worth reading.
    pub generated_tokens: u64,
}

/// Characters per token, by content kind. Code carries more punctuation and more identifier
/// fragments than prose, so it tokenizes denser; JSON/YAML denser again (quotes, colons,
/// brackets). MEASURED against cl100k_base on 400 real files, not reasoned about — see the
/// accuracy table in the header. Do not change these without re-running that calibration.
fn chars_per_token(kind: Kind) -> f64 {
    match kind {
        Kind::Code => 3.95,
        Kind::Prose => 4.25,
        Kind::Data => 3.55,
        Kind::Generated => 3.1,
    }
}

/// Per-extension overrides for kinds whose measured ratio is far from their class. SQL tokenises
/// much less densely than general code (long uppercase keywords, few short identifiers) and was
/// over-estimated by ~16% while sitting in the `code` bucket.
fn ext_ratio(ext: &str) -> Option<f64> {
    (ext == ".sql").then_some(4.15)
}

const PROSE_EXT: &[&str] = &[".md", ".markdown", ".txt", ".rst", ".adoc", ".mdx"];
const DATA_EXT: &[&str] = &[
    ".json",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".csv",
    ".tsv",
    ".xml",
    ".properties",
];

/// Directories never worth counting: an agent does not read them, and including them would
/// inflate every baseline into meaninglessness. Being absent from the total is the point.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "venv",
    ".venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    ".next",
    ".nuxt",
    ".turbo",
    ".gradle",
    ".terraform",
];

/// Files that exist but are not read by a human or an agent on purpose. They are MEASURED
/// (they really are in the directory you pointed at) but reported separately, because a
/// baseline dominated by a lockfile is measuring the wrong thing.
const GENERATED_NAMES: &[&str] = &[
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lockb",
    "cargo.lock",
    "poetry.lock",
    "pdm.lock",
    "uv.lock",
    "composer.lock",
    "gemfile.lock",
    "go.sum",
    "packages.lock.json",
    "flake.lock",
];

fn is_generated(file: &str) -> bool {
    let base = basename(file).to_lowercase();
    if GENERATED_NAMES.contains(&base.as_str()) {
        return true;
    }
    [".min.js", ".min.css", ".min.mjs", ".min.cjs"]
        .iter()
        .any(|s| base.ends_with(s))
        || base.ends_with(".map")
        || base.ends_with(".snap")
}

pub fn kind_of(file: &str) -> Kind {
    if is_generated(file) {
        return Kind::Generated;
    }
    let ext = extname(file).to_lowercase();
    if PROSE_EXT.contains(&ext.as_str()) {
        return Kind::Prose;
    }
    if DATA_EXT.contains(&ext.as_str()) {
        return Kind::Data;
    }
    Kind::Code
}

/// (UTF-16 length, how many of those units are ASCII).
fn utf16_counts(text: &str) -> (usize, usize) {
    let ascii = text.bytes().filter(|b| *b < 128).count();
    (utf16_len(text), ascii)
}

/// Estimate tokens for a string. ASCII goes through the per-kind ratio; non-ASCII is charged
/// at ~1 token per character, which is roughly right for CJK and deliberately pessimistic
/// for accented Latin — over-estimating a budget is the safe direction to be wrong in.
pub fn estimate_tokens(text: &str, kind: Kind, ext: &str) -> u64 {
    let (len, ascii) = utf16_counts(text);
    tokens_for(len, ascii, kind, ext)
}

fn tokens_for(len: usize, ascii: usize, kind: Kind, ext: &str) -> u64 {
    let non_ascii = len - ascii;
    let ratio = ext_ratio(ext).unwrap_or_else(|| chars_per_token(kind));
    (ascii as f64 / ratio + non_ascii as f64).ceil() as u64
}

fn looks_binary(buf: &[u8]) -> bool {
    buf.iter().take(4096).any(|b| *b == 0)
}

fn caveat_for(text: &str, non_ascii_heavy: bool, data_kind: bool) -> Option<String> {
    let mut longest = 0usize;
    let mut current = 0usize;
    for c in text.chars() {
        if c == '\n' {
            longest = longest.max(current);
            current = 0;
        } else {
            current += c.len_utf16();
        }
    }
    longest = longest.max(current);
    if longest > 500 {
        return Some(format!(
            "longest line is {longest} chars (minified or generated?) — estimate less reliable"
        ));
    }
    if data_kind {
        return Some(
            "JSON/YAML: measured p90 error ~40% per file — reliable in aggregate, not alone"
                .to_string(),
        );
    }
    if non_ascii_heavy {
        return Some(
            "mostly non-ASCII — charged at ~1 token/char, real cost may differ substantially"
                .to_string(),
        );
    }
    None
}

fn walk(root: &str, out: &mut Vec<String>) {
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            let full = join(&[&dir, &name]);
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(full);
                }
            } else if t.is_file() {
                out.push(full);
            }
        }
    }
}

pub enum Measured {
    Cost(FileCost),
    Skip(Skipped),
}

pub fn measure_file(file: &str) -> Measured {
    let Ok(buf) = fs::read(file) else {
        return Measured::Skip(Skipped {
            file: file.to_string(),
            reason: "unreadable".to_string(),
        });
    };
    if looks_binary(&buf) {
        return Measured::Skip(Skipped {
            file: file.to_string(),
            reason: "binary".to_string(),
        });
    }
    let text = String::from_utf8_lossy(&buf);
    let kind = kind_of(file);
    let (len, ascii) = utf16_counts(&text);
    let non_ascii_heavy = len > 0 && ((len - ascii) as f64 / len as f64) > 0.05;
    Measured::Cost(FileCost {
        file: file.to_string(),
        bytes: buf.len(),
        chars: len,
        tokens: tokens_for(len, ascii, kind, &extname(file).to_lowercase()),
        kind,
        caveat: caveat_for(&text, non_ascii_heavy, kind == Kind::Data),
    })
}

/// `repeat` is clamped like `Math.max(1, Math.floor(repeat))`.
pub fn measure_read_cost(paths: &[String], repeat: Option<f64>) -> Measurement {
    let repeat = repeat.unwrap_or(1.0).floor().max(1.0);
    let mut targets: Vec<String> = Vec::new();
    let mut skipped: Vec<Skipped> = Vec::new();

    for p in paths {
        let Ok(stat) = fs::metadata(p) else {
            skipped.push(Skipped {
                file: p.clone(),
                reason: "does not exist".to_string(),
            });
            continue;
        };
        if stat.is_dir() {
            walk(p, &mut targets);
        } else {
            targets.push(p.clone());
        }
    }

    // `[...new Set(targets)].sort()`: the default sort compares UTF-16 code units.
    let mut unique: Vec<String> = targets
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sort_utf16(&mut unique);

    let mut files: Vec<FileCost> = Vec::new();
    for t in unique {
        match measure_file(&t) {
            Measured::Skip(s) => skipped.push(s),
            Measured::Cost(c) => files.push(c),
        }
    }

    let tokens: u64 = files.iter().map(|f| f.tokens).sum();
    let generated_tokens: u64 = files
        .iter()
        .filter(|f| f.kind == Kind::Generated)
        .map(|f| f.tokens)
        .sum();
    Measurement {
        files,
        skipped,
        tokens,
        total_tokens: tokens as f64 * repeat,
        repeat,
        generated_tokens,
    }
}

/// JS `Number(string)`: whitespace-trimmed, empty is 0, `0x`/`0o`/`0b`, `Infinity`, and NaN for
/// anything else.
fn js_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let radix = |digits: &str, base: u32| -> f64 {
        if digits.is_empty() {
            return f64::NAN;
        }
        let mut v = 0.0;
        for c in digits.chars() {
            match c.to_digit(base) {
                Some(d) => v = v * base as f64 + d as f64,
                None => return f64::NAN,
            }
        }
        v
    };
    if let Some(r) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return radix(r, 16);
    }
    if let Some(r) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return radix(r, 8);
    }
    if let Some(r) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return radix(r, 2);
    }
    // StrDecimalLiteral: [+-] (digits [. [digits]] | . digits) [e [+-] digits]
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mant_ok = match mantissa.split_once('.') {
        None => digits(mantissa),
        Some((a, b)) => (digits(a) && (b.is_empty() || digits(b))) || (a.is_empty() && digits(b)),
    };
    let exp_ok = exp.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)));
    if !mant_ok || !exp_ok {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Number(x) || fallback`: NaN and 0 both fall back.
fn number_or(s: Option<&str>, fallback: f64) -> f64 {
    let n = s.map_or(0.0, js_number);
    if n.is_nan() || n == 0.0 {
        fallback
    } else {
        n
    }
}

/// `toFixed(0)`: ties go to the larger value, unlike Rust's round-half-even formatting.
fn to_fixed0(x: f64) -> String {
    let f = x.floor();
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    format!("{r:.0}")
}

fn pct(part: f64, whole: f64) -> String {
    if whole <= 0.0 {
        return "—".to_string();
    }
    format!("{}%", to_fixed0(part / whole * 100.0))
}

/// `Number.prototype.toLocaleString()` in en-US: grouped, at most three decimals.
fn locale(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞" } else { "-∞" }.to_string();
    }
    let s = format!("{:.3}", x.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let frac = frac.trim_end_matches('0');
    let neg = x < 0.0 && (int != "0" || !frac.is_empty());
    format!(
        "{}{grouped}{}{frac}",
        if neg { "-" } else { "" },
        if frac.is_empty() { "" } else { "." }
    )
}

/// A JS number printed as `String(x)` (whole numbers without a fraction).
fn js_string(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_string()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else if x == x.trunc() && x.abs() < 1e21 {
        format!("{x:.0}")
    } else {
        format!("{x}")
    }
}

/// A JSON number as `JSON.stringify` writes it: whole values as integers, NaN and infinity null.
fn num(x: f64) -> Value {
    if x.is_finite() && x == x.trunc() && x.abs() < 9e15 {
        return Value::Number(Number::from(x as i64));
    }
    Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// `Array.prototype.slice` bounds: truncate toward zero, negatives count from the end.
fn slice_index(x: f64, len: usize) -> usize {
    let t = if x.is_nan() { 0.0 } else { x.trunc() };
    if t < 0.0 {
        (len as f64 + t).max(0.0) as usize
    } else {
        t.min(len as f64) as usize
    }
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(
        args,
        &[
            value("repeat", Some("1")),
            value("budget", None),
            value("window", Some("200000")),
            value("top", Some("10")),
            flag("json"),
            flag("quiet"),
        ],
    ) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));

    let repeat = number_or(parsed.value("repeat"), 1.0);
    let top = number_or(parsed.value("top"), 10.0);
    let window = number_or(parsed.value("window"), 200000.0);
    let budget = parsed.value("budget").map(js_number);
    let paths: Vec<String> = if parsed.positionals.is_empty() {
        vec![".".to_string()]
    } else {
        parsed.positionals.clone()
    };

    let m = measure_read_cost(&paths, Some(repeat));

    if m.files.is_empty() {
        if json_out {
            println!(
                "{}",
                json!({ "files": [], "tokens": 0, "skipped": to_json(&m.skipped) })
            );
        } else if !quiet {
            println!("nothing measurable found (binary, unreadable, or all paths skipped)");
        }
        return 2;
    }

    let over_budget = budget.is_some_and(|b| m.total_tokens > b);

    if json_out {
        let mut obj = Map::new();
        obj.insert("files".into(), to_json(&m.files));
        obj.insert("skipped".into(), to_json(&m.skipped));
        obj.insert("tokens".into(), json!(m.tokens));
        obj.insert("totalTokens".into(), num(m.total_tokens));
        obj.insert("repeat".into(), num(m.repeat));
        obj.insert("generatedTokens".into(), json!(m.generated_tokens));
        obj.insert("window".into(), num(window));
        obj.insert("budget".into(), budget.map_or(Value::Null, num));
        obj.insert("overBudget".into(), json!(over_budget));
        match serde_json::to_string_pretty(&Value::Object(obj)) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                return 1;
            }
        }
        return i32::from(over_budget);
    }

    let mut ranked: Vec<&FileCost> = m.files.iter().collect();
    ranked.sort_by_key(|f| std::cmp::Reverse(f.tokens));
    let tokens = m.tokens as f64;
    println!(
        "{} file(s), ~{} tokens to read once ({} of a {}-token window)",
        m.files.len(),
        locale(tokens),
        pct(tokens, window),
        locale(window)
    );
    if m.repeat > 1.0 {
        println!(
            "× {} passes = ~{} tokens ({} of the window) — cadence, not payload, is what a re-sending tool costs",
            js_string(m.repeat),
            locale(m.total_tokens),
            pct(m.total_tokens, window)
        );
    }
    if m.generated_tokens > 0 {
        println!(
            "of which ~{} ({}) is generated/lock content — almost never worth reading; exclude it before quoting a baseline",
            locale(m.generated_tokens as f64),
            pct(m.generated_tokens as f64, tokens)
        );
    }

    println!();
    let shown = slice_index(top, ranked.len());
    for f in &ranked[..shown] {
        println!("  {:>7}  {:<9} {}", f.tokens, f.kind.as_str(), f.file);
        if let Some(c) = &f.caveat {
            println!("           ⚠ {c}");
        }
    }
    if ranked.len() as f64 > top {
        let rest: u64 = ranked[shown..].iter().map(|f| f.tokens).sum();
        println!(
            "  {:>7}  ({} more file(s))",
            rest,
            js_string(ranked.len() as f64 - top)
        );
    }

    if !m.skipped.is_empty() && !quiet {
        let list = m
            .skipped
            .iter()
            .take(5)
            .map(|s| format!("{} ({})", s.file, s.reason))
            .collect::<Vec<_>>()
            .join(", ");
        println!("\nskipped {}: {list}", m.skipped.len());
    }

    if let Some(b) = budget {
        println!();
        if over_budget {
            println!(
                "✗ over budget: ~{} > {}. Send a subagent to read it and return the conclusion, or narrow the set — do not spend the main window on it.",
                locale(m.total_tokens),
                locale(b)
            );
        } else {
            println!(
                "✓ within budget: ~{} ≤ {}",
                locale(m.total_tokens),
                locale(b)
            );
        }
    }

    if !quiet {
        println!(
            "\nEstimate, not a tokenizer — calibrated against cl100k_base on 400 real files:\n\
             median error -0.5%, 85% of files within ±15%, 95% within ±25%. Code and prose are tight\n\
             (p90 13.5% / 5.4%); JSON and YAML are not (p90 40%), so trust it on a SET rather than on\n\
             one config file. See /viby-toolkit:evaluate."
        );
    }

    i32::from(over_budget)
}
