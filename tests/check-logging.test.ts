/**
 * Contract tests for the logging auditor.
 *
 * Run: node --experimental-strip-types --test tests/check-logging.test.ts
 *
 * Both halves. Two must-NOT cases are real false positives measured on real repositories: a CLI
 * printing its own results is not observability, and a Stripe checkout session id is not a credential.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { scanFile } from "../plugins/viby-toolkit/skills/observe/scripts/check-logging.ts";

function rules(code: string, file = "src/app/route.ts"): string[] {
  return scanFile(file, code).map((f) => f.rule);
}

test("logging a password, token or email is P1", () => {
  for (const line of [
    "logger.info({ password }, 'login');",
    "logger.info({ access_token: t }, 'issued');",
    "logger.info({ email: user.email }, 'signup');",
  ]) {
    assert.ok(rules(line).includes("sensitive-in-log"), `should flag: ${line}`);
  }
});

test("a Stripe checkout sessionId is NOT a credential", () => {
  // The only sensitive-in-log finding on a real payments codebase, and it was wrong: a checkout
  // session id is a resource identifier that debugging a payment requires.
  const f = rules('console.warn("failed to expire orphaned Stripe session:", sessionId);');
  assert.ok(!f.includes("sensitive-in-log"), `resource ids are fine: ${f.join()}`);
});

test("but a session TOKEN or cookie is still flagged", () => {
  assert.ok(rules("logger.info({ session_token: s }, 'x');").includes("sensitive-in-log"));
  assert.ok(rules("logger.debug({ cookie: req.headers.cookie }, 'x');").includes("sensitive-in-log"));
});

test("logging a whole request or user object is P1", () => {
  assert.ok(rules("logger.info(req);").includes("whole-object-in-log"));
  assert.ok(rules("logger.info(JSON.stringify(user));").includes("whole-object-in-log"));
});

test("logging two named fields is NOT a whole-object finding", () => {
  const f = rules("logger.info({ orderId, durationMs }, 'settled');");
  assert.deepEqual(f, [], `the good shape must be silent, got ${f.join()}`);
});

test("an interpolated message with no structured fields is flagged", () => {
  assert.ok(rules("logger.info(`order ${id} settled in ${ms}ms`);").includes("unstructured-log"));
});

test("a catch that logs without the caught error is flagged", () => {
  const code = ["try {", "  await save();", "} catch (err) {", "  logger.error('save failed');", "}"].join("\n");
  assert.ok(rules(code).includes("error-without-cause"));
});

test("a catch that logs WITH the error is not", () => {
  const code = ["try {", "  await save();", "} catch (err) {", "  logger.error({ err }, 'save failed');", "}"].join("\n");
  assert.ok(!rules(code).includes("error-without-cause"));
});

test("an identifier in a metric label is flagged — the documented cost trap", () => {
  assert.ok(rules("metrics.increment('checkout', { userId });").includes("high-cardinality-metric"));
});

test("a bounded metric dimension is not flagged", () => {
  const f = rules("metrics.increment('checkout', { route: '/pay', statusClass: '2xx' });");
  assert.ok(!f.includes("high-cardinality-metric"), f.join());
});

test("an arrival log is P3", () => {
  assert.ok(rules("logger.debug('entering handleCheckout');").includes("arrival-log"));
});

test("three log calls with no correlation id is flagged", () => {
  const code = ["logger.info({ a: 1 }, 'x');", "logger.info({ b: 2 }, 'y');", "logger.info({ c: 3 }, 'z');"].join("\n");
  assert.ok(rules(code).includes("no-correlation-key"));
});

test("the same file WITH a correlation id is not", () => {
  const code = ["const requestId = ctx.requestId;", "logger.info({ requestId, a: 1 }, 'x');", "logger.info({ requestId, b: 2 }, 'y');", "logger.info({ requestId, c: 3 }, 'z');"].join("\n");
  assert.ok(!rules(code).includes("no-correlation-key"));
});

test("a CLI printing its own results is NOT an observability finding", () => {
  // Measured on this repo: 44 findings across 17 files, every one a checker printing results.
  const cli = ["import { parseArgs } from 'node:util';", "for (const f of findings) {", "  console.log(`${f.file}:${f.line}`);", "}"].join("\n");
  const f = rules(cli, "skills/x/scripts/check-thing.ts");
  assert.deepEqual(f, [], `a CLI's stdout is its interface, got ${f.join()}`);
});

test("but a CLI printing a secret is still flagged", () => {
  const cli = ["import { parseArgs } from 'node:util';", "console.log({ api_key: key });"].join("\n");
  assert.ok(rules(cli, "scripts/deploy.ts").includes("sensitive-in-log"), "disclosure is disclosure");
});

test("a logging pattern inside a comment or string is not a finding", () => {
  const code = ["// logger.info({ password }, 'never do this');", "const doc = `logger.info(req)`;", "export const ok = 1;"].join("\n");
  assert.deepEqual(rules(code), [], "decide on parsed code, never raw text");
});

test("an unreadable file is reported, never silently clean", () => {
  assert.equal(scanFile("/nope/missing.ts")[0]?.rule, "unreadable");
});
