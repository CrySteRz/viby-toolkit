//! viby-toolkit logging auditor — the executable half of /viby-toolkit:observe.
//!
//! `observe` says: log decisions and outcomes rather than arrivals, structured fields rather than
//! sentences, a correlation key on every event, and high cardinality is the point for logs but the
//! cost trap for metric labels. That was all prose. This checks the mechanically decidable parts —
//! and the one that is not a style question at all: **personal data and secrets reaching the logs.**
//!
//! Usage:
//!   viby check-logging [paths...] [--all] [--json] [--quiet]
//! Exit: 0 = clean, 1 = findings, 2 = no source found.
//!
//! WHY PII FIRST. A log line is the least access-controlled artifact a team produces: it fans out to
//! aggregators, alerting, screenshots and third-party dashboards, and it is retained long after the
//! request is gone. Nothing errors when an email address or a bearer token lands in it. That makes it
//! the same shape as every other rule in this repo — a silent failure with an expensive tail.
//!
//! Decides on parsed code: strings and comments are blanked first, so a comment describing a logging
//! mistake, or a fixture containing one, is not a finding.

use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::Serialize;
use serde_json::json;

use super::check_analytics_sql::{
    back_utf16, collect_targets, js_extname, js_trim, locale_compare, read_text, rx,
    severity_order, walk_files,
};
use crate::core::args::{flag, parse_or_exit};
use crate::core::strip::strip_noncode;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub rule: String,
    pub severity: String,
    pub problem: String,
    pub fix: String,
}

const SOURCE_EXT: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rb", ".java", ".kt",
    ".cs", ".php",
];
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "vendor",
    "venv",
    ".venv",
    "__pycache__",
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    ".next",
];

fn is_test_path(p: &str) -> bool {
    rx!(
        r"(^|\/)(tests?|specs?|__tests__)\/|[._-](test|spec)\.[a-z]+$|(^|\/)test_[^/]*\.py$|_test\.[a-z]+$",
        "i"
    )
    .is_match(p)
}

/// A logging call in the common frameworks.
fn log_call() -> &'static Regex {
    rx!(
        r"\b(?:logger|log|logging|slog|console|klog|Log)\s*\.\s*(?:trace|debug|info|warn|warning|error|fatal|critical|exception|log)\s*\(|\blog\.(?:Print|Fatal|Panic)\w*\s*\(|\bprintln!\s*\("
    )
}

/// Field names that carry personal data or credentials.
///
/// `sessionId` is deliberately NOT here. Measured on a real payments codebase, the only
/// `sensitive-in-log` finding was a Stripe **checkout** session id — a resource identifier that is
/// necessary for debugging a payment, not an auth credential. Only `session_token` / `session_secret`
/// / cookies qualify. Conflating a resource id with a credential is how this rule would have taught
/// its reader to ignore it.
fn sensitive_field() -> &'static Regex {
    rx!(
        r"\b(password|passwd|pwd|secret|api[_-]?key|apikey|access[_-]?token|refresh[_-]?token|bearer|authorization|auth[_-]?header|session[_-]?(?:token|secret|cookie)|cookie|ssn|social[_-]?security|dob|date[_-]?of[_-]?birth|credit[_-]?card|card[_-]?number|cvv|iban|routing[_-]?number|passport|email[_-]?address|\bemail\b|phone[_-]?number|home[_-]?address|postcode|zip[_-]?code|donor[_-]?name|patient|medical)\b",
        "i"
    )
}

/// Whole objects that habitually contain everything.
fn whole_object() -> &'static Regex {
    rx!(
        r"\b(?:req|request|res|response|ctx|context|user|customer|account|profile|payload|body|headers|event|record|row)\b\s*[),]|JSON\.stringify\s*\(\s*(?:req|request|user|customer|account|profile|payload|body|headers)\b"
    )
}

/// An identifier-shaped value — fine in a log, ruinous as a metric label.
fn id_value() -> &'static Regex {
    rx!(
        r"\b(\w*(?:id|uuid|guid|email|user|customer|account|session|request|trace|order|sku)\w*)\b",
        "i"
    )
}

fn metric_call() -> &'static Regex {
    rx!(
        r"\b(?:metrics?|statsd|counter|gauge|histogram|timing|prometheus|otel|meter)\s*\.\s*\w+\s*\(|\.\s*(?:labels|withLabelValues|tags)\s*\(|\bincrement\s*\(",
        "i"
    )
}

/// A CLI's stdout IS its interface. `console.log` in a command-line tool is output, not observability,
/// and a CLI has no request to correlate — measured on this repo, that mistake produced 44 findings
/// across 17 files, every one of them a checker printing its own results. PII rules still apply:
/// printing a secret to stdout is a disclosure wherever it happens.
fn is_cli_context(file: &str, code: &str) -> bool {
    if rx!(r"(^|\/)(scripts?|bin|tools?|cli|tasks?|jobs?)\/", "i").is_match(file) {
        return true;
    }
    rx!(
        r"\bprocess\.argv\b|\bparseArgs\s*\(|if\s+__name__\s*==\s*.__main__.|^func\s+main\s*\(",
        "m"
    )
    .is_match(code)
        || rx!(r"^#!").is_match(code)
}

/// A logging library's OWN implementation. Measured false positive: every finding in a 2,000-file
/// real corpus landed inside a file named `logger.mjs`/`logger.mts`, on lines like
/// `logger.debug(event, { ...context, error: serializeErrorForLog(error) })`. Forwarding whatever the
/// caller passed IS a logger's job — flagging it is like telling a database driver not to run SQL.
/// The rules about HOW you call a logger are meaningless in the thing being called.
fn is_logger_implementation(file: &str, code: &str) -> bool {
    let base = file.rsplit('/').next().unwrap_or("");
    if rx!(r"^(logger|logging|log)\.[cm]?[jt]sx?$", "i").is_match(base)
        || rx!(r"(^|\/)logging\/", "i").is_match(file)
    {
        return true;
    }
    // Or it defines the logger rather than using one: a level table plus a level-keyed dispatch.
    rx!(r"\b(LOG_?LEVELS?|LEVEL_INDEX|logLevel)\b").is_match(code)
        && rx!(r"\b(createLogger|makeLogger|getLogger)\b").is_match(code)
}

pub struct Rule {
    pub rule: &'static str,
    pub severity: &'static str,
    /// False for rules that are meaningless in a command-line tool.
    pub applies_in_cli: bool,
    /// False for rules that are meaningless inside a logging library's own implementation.
    pub applies_in_logger_impl: bool,
    /// True for rules whose signal lives INSIDE the string literal — the interpolation markers of an
    /// unstructured message, the word "entering" in an arrival log. Blanking removes exactly that, so
    /// these rules read the RAW line. Safe because the line is only offered to them when the *blanked*
    /// line still contains a live log call, which keeps comments and fixtures inert. This is the sixth
    /// time this repo has met the same shape: locate on parsed code, read the value from raw text.
    pub uses_raw: bool,
    /// `(subject, whole blanked file, whole raw file, path)`.
    pub test: fn(line: &str, whole: &str, raw: &str, file: &str) -> bool,
    pub problem: &'static str,
    pub fix: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        rule: "sensitive-in-log",
        severity: "P1",
        applies_in_cli: true,
        applies_in_logger_impl: true,
        uses_raw: false,
        test: |l, _, _, _| log_call().is_match(l) && sensitive_field().is_match(l),
        problem: "logs a field that carries personal data or a credential — a log line is the least access-controlled artifact you produce, it fans out to aggregators and dashboards, and it is retained long after the request",
        fix: "log an identifier instead of the value, redact at the logging boundary, and treat anything already shipped as disclosed",
    },
    Rule {
        rule: "whole-object-in-log",
        severity: "P1",
        applies_in_cli: true,
        applies_in_logger_impl: false,
        uses_raw: false,
        test: |l, _, _, _| log_call().is_match(l) && whole_object().is_match(l),
        problem: "logs a whole request/user/payload object, so whatever it happens to contain today ends up in the logs — including the fields somebody adds next month",
        fix: "name the two or three fields you actually need; an allow-list survives schema changes, a whole object does not",
    },
    Rule {
        rule: "unstructured-log",
        severity: "P2",
        applies_in_cli: false,
        applies_in_logger_impl: false,
        uses_raw: true,
        test: |l, _, _, _| {
            log_call().is_match(l)
                && (rx!(r"\$\{").is_match(l)
                    || rx!(r#"["'`]\s*\+\s*\w"#).is_match(l)
                    || rx!(r"%[sdv]").is_match(l))
                && !rx!(r"\{[^}]*:").is_match(l)
        },
        problem: "builds the message by interpolation with no structured fields, so the value cannot be filtered, grouped or alerted on — only grepped",
        fix: "pass fields as data: `logger.info({ orderId, durationMs }, 'settled')` rather than embedding them in the sentence",
    },
    Rule {
        rule: "error-without-cause",
        severity: "P2",
        applies_in_cli: false,
        applies_in_logger_impl: true,
        uses_raw: false,
        test: |l, whole, raw, _| {
            if !rx!(r"\.(error|exception|fatal|critical|warn)\s*\(", "i").is_match(l) {
                return false;
            }
            let Some(start) = whole.find(l) else {
                return false;
            };
            // `before` is the 300 UTF-16 units ahead of the line, counted on the raw text (the blanked
            // text spends several bytes where one JavaScript unit was).
            let before_start = back_utf16(raw, start, 300);
            let before = &whole[before_start..start];
            if !rx!(
                r"\bcatch\s*(\(|\{|\s+\w)|\bexcept\b|\brescue\b|err\s*!=\s*nil",
                "i"
            )
            .is_match(before)
            {
                return false;
            }
            // The caught binding must appear in the log call.
            let tail = &whole[back_utf16(raw, start, 160).max(before_start)..start];
            let Some(m) = rx!(
                r"catch\s*\(\s*(\w+)|except\s+\w+\s+as\s+(\w+)|(\berr\b)",
                "i"
            )
            .captures(tail) else {
                return false;
            };
            let Some(bind) = m.get(1).or_else(|| m.get(2)).or_else(|| m.get(3)) else {
                return false;
            };
            match Regex::new(&format!("(?-u:\\b){}(?-u:\\b)", regex::escape(bind.as_str()))) {
                Ok(re) => !re.is_match(l),
                Err(_) => false,
            }
        },
        problem: "logs inside a catch without including the caught error, so the stack and the cause are gone and the line says only that something failed",
        fix: "pass the error itself alongside the message — the stack is the whole value of the log at 3am",
    },
    Rule {
        rule: "arrival-log",
        severity: "P3",
        applies_in_cli: false,
        applies_in_logger_impl: true,
        uses_raw: true,
        test: |l, _, _, _| {
            log_call().is_match(l)
                && rx!(
                    r"\b(entering|entered|start(ing|ed)?\b|called with|begin|>>>|handler (hit|called)|in function)\b",
                    "i"
                )
                .is_match(l)
        },
        problem: "logs an arrival rather than a decision or an outcome — arrivals are the bulk of log volume and almost never the thing you needed",
        fix: "log what was decided and what resulted, with the inputs that drove it",
    },
    Rule {
        rule: "high-cardinality-metric",
        severity: "P2",
        applies_in_cli: true,
        applies_in_logger_impl: true,
        uses_raw: false,
        test: |l, _, _, _| metric_call().is_match(l) && id_value().is_match(l),
        problem: "puts an identifier-shaped value in a metric label — high cardinality is the point for logs and traces, and the cost trap for metrics: one series per distinct value",
        fix: "keep the id in the log or the trace and label the metric with a bounded dimension (route, status class, region)",
    },
    Rule {
        rule: "log-in-tight-loop",
        severity: "P3",
        applies_in_cli: false,
        applies_in_logger_impl: true,
        uses_raw: false,
        test: |l, whole, raw, _| {
            if !log_call().is_match(l) {
                return false;
            }
            let Some(start) = whole.find(l) else {
                return false;
            };
            let before = &whole[back_utf16(raw, start, 200)..start];
            let tail = &whole[back_utf16(raw, start, 80).max(back_utf16(raw, start, 200))..start];
            rx!(r"\b(for|while|forEach|map)\s*[({]").is_match(before)
                && !rx!(r"\bif\b").is_match(tail)
        },
        problem: "logs unconditionally inside a loop — the volume scales with the data, and the one interesting line is buried in ten thousand",
        fix: "log once with a count and a sample, or log only the cases that are actually interesting",
    },
];

pub fn scan_file(file: &str, raw: Option<&str>) -> Vec<Finding> {
    let text: String = match raw {
        Some(r) => r.to_string(),
        None => match read_text(file) {
            Some(t) => t,
            None => {
                return vec![Finding {
                    file: file.to_string(),
                    line: 1,
                    rule: "unreadable".to_string(),
                    severity: "P3".to_string(),
                    problem: "could not read this file, so it was NOT checked".to_string(),
                    fix: "fix the path and re-run".to_string(),
                }]
            }
        },
    };
    let code = strip_noncode(&text, &js_extname(file).to_lowercase());
    let cli = is_cli_context(file, &code);
    let logger_impl = is_logger_implementation(file, &code);
    let lines: Vec<&str> = code.split('\n').collect();
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let mut findings = Vec::new();
    // Raw-reading rules test the whole log CALL, not a physical line. Every one of them begins with
    // `LOG_CALL.test(l)`, so a call whose message sat on a continuation line matched nothing — and a
    // long message is exactly the message that gets wrapped. Join each call onto its opening line and
    // mark the continuation lines consumed, so the finding still lands at the call site and once only.
    let mut call_text: HashMap<usize, String> = HashMap::new();
    let mut consumed: HashSet<usize> = HashSet::new();
    for (i, first) in lines.iter().enumerate() {
        if !log_call().is_match(first) {
            continue;
        }
        let mut depth: i64 = 0;
        for (j, blanked) in lines.iter().enumerate().take(i + 12).skip(i) {
            depth += blanked.matches('(').count() as i64 - blanked.matches(')').count() as i64;
            if j > i {
                consumed.insert(j);
            }
            if depth <= 0 {
                if j > i {
                    let joined = raw_lines
                        .get(i..=j)
                        .map(|s| s.join(" "))
                        .unwrap_or_default();
                    call_text.insert(i, joined);
                }
                break;
            }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if js_trim(line).is_empty() {
            continue;
        }
        for r in RULES {
            if cli && !r.applies_in_cli {
                continue;
            }
            if logger_impl && !r.applies_in_logger_impl {
                continue;
            }
            // A raw-reading rule only sees the line if the blanked line proves it is live code.
            // A log call spanning several lines put its message on a line with no `logger.` on it, so
            // gating on the physical line silently dropped every finding for the multi-line form — which
            // is the form long messages actually take. `inLogCall` stays true until the call's parens close.
            if r.uses_raw && consumed.contains(&i) {
                continue;
            }
            if r.uses_raw && !log_call().is_match(line) {
                continue;
            }
            let subject: &str = if r.uses_raw {
                match call_text.get(&i) {
                    Some(t) => t,
                    None => raw_lines.get(i).copied().unwrap_or(""),
                }
            } else {
                line
            };
            if (r.test)(subject, &code, &text, file) {
                findings.push(Finding {
                    file: file.to_string(),
                    line: i + 1,
                    rule: r.rule.to_string(),
                    severity: r.severity.to_string(),
                    problem: r.problem.to_string(),
                    fix: r.fix.to_string(),
                });
            }
        }
    }
    // File-level: several log calls and nothing to stitch them together with.
    let log_calls = lines.iter().filter(|l| log_call().is_match(l)).count();
    // Same exemption: a logger implementation has no request to correlate — the caller supplies that.
    if !cli
        && !logger_impl
        && log_calls >= 3
        && !rx!(
            r"\b(request[_-]?id|correlation[_-]?id|trace[_-]?id|span[_-]?id|requestId|correlationId|traceId|reqId)\b",
            "i"
        )
        .is_match(&code)
    {
        findings.push(Finding {
            file: file.to_string(),
            line: 1,
            rule: "no-correlation-key".to_string(),
            severity: "P2".to_string(),
            problem: format!("{log_calls} log calls and no request/trace/correlation id anywhere — the lines cannot be stitched into one request, which is the first thing you need at 3am"),
            fix: "thread a correlation id through and include it on every event".to_string(),
        });
    }
    findings
}

fn walk(root: &str) -> Vec<String> {
    walk_files(root, &|name| SKIP_DIRS.contains(&name), &|name, full| {
        SOURCE_EXT.contains(&js_extname(name).to_lowercase().as_str()) && !is_test_path(full)
    })
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[flag("all"), flag("json"), flag("quiet")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let targets = collect_targets(&parsed.positionals, parsed.flag("all"), &walk);
    if targets.is_empty() {
        if json_out {
            println!("{}", json!({ "scanned": 0, "findings": [] }));
        } else if !quiet {
            println!(
                "no source files found (tests are excluded — logging in a test is not a finding)"
            );
        }
        return 2;
    }

    let mut findings: Vec<Finding> = targets.iter().flat_map(|t| scan_file(t, None)).collect();
    if json_out {
        let report = json!({ "scanned": targets.len(), "findings": findings });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
        return i32::from(!findings.is_empty());
    }

    findings.sort_by(|a, b| {
        severity_order(&a.severity)
            .cmp(&severity_order(&b.severity))
            .then_with(|| locale_compare(&a.file, &b.file))
    });
    for f in &findings {
        println!("{}:{}  [{} {}]", f.file, f.line, f.severity, f.rule);
        println!("    {}", f.problem);
        println!("    fix: {}", f.fix);
    }
    if !quiet {
        println!();
        if findings.is_empty() {
            println!("clean: {} file(s)", targets.len());
        } else {
            println!(
                "{} finding(s) across {} file(s)",
                findings.len(),
                targets.len()
            );
        }
        println!(
            "Field names only — it cannot see that a variable called `data` holds an email address, and it\n\
             cannot tell whether your aggregator redacts. A clean run is not a privacy review. See\n\
             /viby-toolkit:observe."
        );
    }
    i32::from(!findings.is_empty())
}
