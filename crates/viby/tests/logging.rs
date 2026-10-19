//! Contract tests for the logging auditor (port of `tests/check-logging.test.ts`).
//!
//! Both halves. Two must-NOT cases are real false positives measured on real repositories: a CLI
//! printing its own results is not observability, and a Stripe checkout session id is not a credential.

use viby::checks::check_logging::scan_file;

fn rules_in(code: &str, file: &str) -> Vec<String> {
    scan_file(file, Some(code))
        .into_iter()
        .map(|f| f.rule)
        .collect()
}

fn rules(code: &str) -> Vec<String> {
    rules_in(code, "src/app/route.ts")
}

fn has(found: &[String], rule: &str) -> bool {
    found.iter().any(|r| r == rule)
}

#[test]
fn logging_a_password_token_or_email_is_p1() {
    for line in [
        "logger.info({ password }, 'login');",
        "logger.info({ access_token: t }, 'issued');",
        "logger.info({ email: user.email }, 'signup');",
    ] {
        assert!(has(&rules(line), "sensitive-in-log"), "should flag: {line}");
    }
}

#[test]
fn a_stripe_checkout_session_id_is_not_a_credential() {
    // The only sensitive-in-log finding on a real payments codebase, and it was wrong: a checkout
    // session id is a resource identifier that debugging a payment requires.
    let f = rules(r#"console.warn("failed to expire orphaned Stripe session:", sessionId);"#);
    assert!(
        !has(&f, "sensitive-in-log"),
        "resource ids are fine: {}",
        f.join(",")
    );
}

#[test]
fn but_a_session_token_or_cookie_is_still_flagged() {
    assert!(has(
        &rules("logger.info({ session_token: s }, 'x');"),
        "sensitive-in-log"
    ));
    assert!(has(
        &rules("logger.debug({ cookie: req.headers.cookie }, 'x');"),
        "sensitive-in-log"
    ));
}

#[test]
fn logging_a_whole_request_or_user_object_is_p1() {
    assert!(has(&rules("logger.info(req);"), "whole-object-in-log"));
    assert!(has(
        &rules("logger.info(JSON.stringify(user));"),
        "whole-object-in-log"
    ));
}

#[test]
fn logging_two_named_fields_is_not_a_whole_object_finding() {
    let f = rules("logger.info({ orderId, durationMs }, 'settled');");
    assert_eq!(
        f,
        Vec::<String>::new(),
        "the good shape must be silent, got {}",
        f.join(",")
    );
}

#[test]
fn an_interpolated_message_with_no_structured_fields_is_flagged() {
    assert!(has(
        &rules("logger.info(`order ${id} settled in ${ms}ms`);"),
        "unstructured-log"
    ));
}

#[test]
fn a_catch_that_logs_without_the_caught_error_is_flagged() {
    let code = [
        "try {",
        "  await save();",
        "} catch (err) {",
        "  logger.error('save failed');",
        "}",
    ]
    .join("\n");
    assert!(has(&rules(&code), "error-without-cause"));
}

#[test]
fn a_catch_that_logs_with_the_error_is_not() {
    let code = [
        "try {",
        "  await save();",
        "} catch (err) {",
        "  logger.error({ err }, 'save failed');",
        "}",
    ]
    .join("\n");
    assert!(!has(&rules(&code), "error-without-cause"));
}

#[test]
fn an_identifier_in_a_metric_label_is_flagged_the_documented_cost_trap() {
    assert!(has(
        &rules("metrics.increment('checkout', { userId });"),
        "high-cardinality-metric"
    ));
}

#[test]
fn a_bounded_metric_dimension_is_not_flagged() {
    let f = rules("metrics.increment('checkout', { route: '/pay', statusClass: '2xx' });");
    assert!(!has(&f, "high-cardinality-metric"), "{}", f.join(","));
}

#[test]
fn an_arrival_log_is_p3() {
    assert!(has(
        &rules("logger.debug('entering handleCheckout');"),
        "arrival-log"
    ));
}

#[test]
fn three_log_calls_with_no_correlation_id_is_flagged() {
    let code = [
        "logger.info({ a: 1 }, 'x');",
        "logger.info({ b: 2 }, 'y');",
        "logger.info({ c: 3 }, 'z');",
    ]
    .join("\n");
    assert!(has(&rules(&code), "no-correlation-key"));
}

#[test]
fn the_same_file_with_a_correlation_id_is_not() {
    let code = [
        "const requestId = ctx.requestId;",
        "logger.info({ requestId, a: 1 }, 'x');",
        "logger.info({ requestId, b: 2 }, 'y');",
        "logger.info({ requestId, c: 3 }, 'z');",
    ]
    .join("\n");
    assert!(!has(&rules(&code), "no-correlation-key"));
}

#[test]
fn a_cli_printing_its_own_results_is_not_an_observability_finding() {
    // Measured on this repo: 44 findings across 17 files, every one a checker printing results.
    let cli = [
        "import { parseArgs } from 'node:util';",
        "for (const f of findings) {",
        "  console.log(`${f.file}:${f.line}`);",
        "}",
    ]
    .join("\n");
    let f = rules_in(&cli, "skills/x/scripts/check-thing.ts");
    assert_eq!(
        f,
        Vec::<String>::new(),
        "a CLI's stdout is its interface, got {}",
        f.join(",")
    );
}

#[test]
fn but_a_cli_printing_a_secret_is_still_flagged() {
    let cli = [
        "import { parseArgs } from 'node:util';",
        "console.log({ api_key: key });",
    ]
    .join("\n");
    assert!(
        has(&rules_in(&cli, "scripts/deploy.ts"), "sensitive-in-log"),
        "disclosure is disclosure"
    );
}

#[test]
fn a_logging_pattern_inside_a_comment_or_string_is_not_a_finding() {
    let code = [
        "// logger.info({ password }, 'never do this');",
        "const doc = `logger.info(req)`;",
        "export const ok = 1;",
    ]
    .join("\n");
    assert_eq!(
        rules(&code),
        Vec::<String>::new(),
        "decide on parsed code, never raw text"
    );
}

#[test]
fn an_unreadable_file_is_reported_never_silently_clean() {
    assert_eq!(
        scan_file("/nope/missing.ts", None)
            .first()
            .map(|f| f.rule.as_str()),
        Some("unreadable")
    );
}

#[test]
fn p2_regression_a_multi_line_log_call_is_still_audited() {
    // The usesRaw gate tested the current PHYSICAL line for a log call, so a call whose message sat
    // on a continuation line lost every finding — and that is the form long messages actually take.
    let code = [
        "export function handler(user: string) {",
        "  logger.info(",
        "    `entering checkout for ${user}`,",
        "  );",
        "}",
    ]
    .join("\n");
    assert!(
        has(&rules(&code), "unstructured-log"),
        "the message on a continuation line must be seen"
    );
}

#[test]
fn a_logging_librarys_own_implementation_is_not_audited_on_how_it_calls_a_logger() {
    // Measured false positive: every finding in a 2,000-file real corpus landed inside a file named
    // `logger.mjs`, on lines that forward the caller's fields. That IS a logger's job.
    let code = [
        "export function logCaughtError(logger, event, error, context = {}) {",
        "  logger.debug(event, { ...context, error: serializeErrorForLog(error) });",
        "}",
        "export function logDecision(logger, fields) {",
        "  logger.debug(`decision:${fields.event}`, fields);",
        "}",
    ]
    .join("\n");
    assert_eq!(
        rules_in(&code, "hooks/logger.mjs"),
        Vec::<String>::new(),
        "a logger implementation forwards, it does not misuse"
    );
}

#[test]
fn but_a_request_handler_that_does_the_same_thing_is_flagged() {
    // The must-NOT half needs its counterpart, or the exemption above could swallow the rule whole.
    let code = [
        "export async function POST(req) {",
        "  logger.debug(`decision:${body.event}`, body);",
        "}",
    ]
    .join("\n");
    assert!(
        !rules_in(&code, "src/app/api/route.ts").is_empty(),
        "the exemption must be scoped to the logger itself"
    );
}
