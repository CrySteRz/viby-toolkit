/**
 * viby-toolkit logging auditor — the executable half of /viby-toolkit:observe.
 *
 * `observe` says: log decisions and outcomes rather than arrivals, structured fields rather than
 * sentences, a correlation key on every event, and high cardinality is the point for logs but the
 * cost trap for metric labels. That was all prose. This checks the mechanically decidable parts —
 * and the one that is not a style question at all: **personal data and secrets reaching the logs.**
 *
 * Usage:
 *   node check-logging.ts [paths...] [--all] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no source found.
 *
 * WHY PII FIRST. A log line is the least access-controlled artifact a team produces: it fans out to
 * aggregators, alerting, screenshots and third-party dashboards, and it is retained long after the
 * request is gone. Nothing errors when an email address or a bearer token lands in it. That makes it
 * the same shape as every other rule in this repo — a silent failure with an expensive tail.
 *
 * Decides on parsed code: strings and comments are blanked first, so a comment describing a logging
 * mistake, or a fixture containing one, is not a finding.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";
import { stripNoncode } from "../../../lib/strip-noncode.ts";

export type Finding = {
  file: string;
  line: number;
  rule: string;
  severity: "P1" | "P2" | "P3";
  problem: string;
  fix: string;
};

const SOURCE_EXT = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rb", ".java", ".kt", ".cs", ".php"]);
const SKIP_DIRS = new Set([".git", "node_modules", "vendor", "venv", ".venv", "__pycache__", "dist", "build", "out", "target", "coverage", ".next"]);
const TEST_PATH = /(^|\/)(tests?|specs?|__tests__)\/|[._-](test|spec)\.[a-z]+$|(^|\/)test_[^/]*\.py$|_test\.[a-z]+$/i;

/** A logging call in the common frameworks. */
const LOG_CALL = /\b(?:logger|log|logging|slog|console|klog|Log)\s*\.\s*(?:trace|debug|info|warn|warning|error|fatal|critical|exception|log)\s*\(|\blog\.(?:Print|Fatal|Panic)\w*\s*\(|\bprintln!\s*\(/;

/**
 * Field names that carry personal data or credentials.
 *
 * `sessionId` is deliberately NOT here. Measured on a real payments codebase, the only
 * `sensitive-in-log` finding was a Stripe **checkout** session id — a resource identifier that is
 * necessary for debugging a payment, not an auth credential. Only `session_token` / `session_secret`
 * / cookies qualify. Conflating a resource id with a credential is how this rule would have taught
 * its reader to ignore it.
 */
const SENSITIVE_FIELD =
  /\b(password|passwd|pwd|secret|api[_-]?key|apikey|access[_-]?token|refresh[_-]?token|bearer|authorization|auth[_-]?header|session[_-]?(?:token|secret|cookie)|cookie|ssn|social[_-]?security|dob|date[_-]?of[_-]?birth|credit[_-]?card|card[_-]?number|cvv|iban|routing[_-]?number|passport|email[_-]?address|\bemail\b|phone[_-]?number|home[_-]?address|postcode|zip[_-]?code|donor[_-]?name|patient|medical)\b/i;

/** Whole objects that habitually contain everything. */
const WHOLE_OBJECT = /\b(?:req|request|res|response|ctx|context|user|customer|account|profile|payload|body|headers|event|record|row)\b\s*[),]|JSON\.stringify\s*\(\s*(?:req|request|user|customer|account|profile|payload|body|headers)\b/;

/** An identifier-shaped value — fine in a log, ruinous as a metric label. */
const ID_VALUE = /\b(\w*(?:id|uuid|guid|email|user|customer|account|session|request|trace|order|sku)\w*)\b/i;
const METRIC_CALL = /\b(?:metrics?|statsd|counter|gauge|histogram|timing|prometheus|otel|meter)\s*\.\s*\w+\s*\(|\.\s*(?:labels|withLabelValues|tags)\s*\(|\bincrement\s*\(/i;

/**
 * A CLI's stdout IS its interface. `console.log` in a command-line tool is output, not observability,
 * and a CLI has no request to correlate — measured on this repo, that mistake produced 44 findings
 * across 17 files, every one of them a checker printing its own results. PII rules still apply:
 * printing a secret to stdout is a disclosure wherever it happens.
 */
function isCliContext(file: string, code: string): boolean {
  if (/(^|\/)(scripts?|bin|tools?|cli|tasks?|jobs?)\//i.test(file)) return true;
  return (
    /\bprocess\.argv\b|\bparseArgs\s*\(|if\s+__name__\s*==\s*.__main__.|^func\s+main\s*\(/m.test(code) ||
    /^#!/.test(code)
  );
}

/**
 * A logging library's OWN implementation. Measured false positive: every finding in a 2,000-file
 * real corpus landed inside a file named `logger.mjs`/`logger.mts`, on lines like
 * `logger.debug(event, { ...context, error: serializeErrorForLog(error) })`. Forwarding whatever the
 * caller passed IS a logger's job — flagging it is like telling a database driver not to run SQL.
 * The rules about HOW you call a logger are meaningless in the thing being called.
 */
function isLoggerImplementation(file: string, code: string): boolean {
  const base = file.split("/").pop() ?? "";
  if (/^(logger|logging|log)\.[cm]?[jt]sx?$/i.test(base) || /(^|\/)logging\//i.test(file)) return true;
  // Or it defines the logger rather than using one: a level table plus a level-keyed dispatch.
  return /\b(LOG_?LEVELS?|LEVEL_INDEX|logLevel)\b/.test(code) && /\b(createLogger|makeLogger|getLogger)\b/.test(code);
}

type Rule = {
  rule: string;
  severity: "P1" | "P2" | "P3";
  /** False for rules that are meaningless in a command-line tool. */
  appliesInCli?: boolean;
  /** False for rules that are meaningless inside a logging library's own implementation. */
  appliesInLoggerImpl?: boolean;
  /**
   * True for rules whose signal lives INSIDE the string literal — the interpolation markers of an
   * unstructured message, the word "entering" in an arrival log. Blanking removes exactly that, so
   * these rules read the RAW line. Safe because the line is only offered to them when the *blanked*
   * line still contains a live log call, which keeps comments and fixtures inert. This is the sixth
   * time this repo has met the same shape: locate on parsed code, read the value from raw text.
   */
  usesRaw?: boolean;
  test: (line: string, whole: string, file: string) => boolean;
  problem: string;
  fix: string;
};

export const RULES: Rule[] = [
  {
    rule: "sensitive-in-log",
    severity: "P1",
    test: (l) => LOG_CALL.test(l) && SENSITIVE_FIELD.test(l),
    problem:
      "logs a field that carries personal data or a credential — a log line is the least access-controlled artifact you produce, it fans out to aggregators and dashboards, and it is retained long after the request",
    fix: "log an identifier instead of the value, redact at the logging boundary, and treat anything already shipped as disclosed",
  },
  {
    rule: "whole-object-in-log",
    appliesInLoggerImpl: false,
    severity: "P1",
    test: (l) => LOG_CALL.test(l) && WHOLE_OBJECT.test(l),
    problem:
      "logs a whole request/user/payload object, so whatever it happens to contain today ends up in the logs — including the fields somebody adds next month",
    fix: "name the two or three fields you actually need; an allow-list survives schema changes, a whole object does not",
  },
  {
    rule: "unstructured-log",
    appliesInLoggerImpl: false,
    appliesInCli: false,
    usesRaw: true,
    severity: "P2",
    test: (l) => LOG_CALL.test(l) && (/\$\{/.test(l) || /["'`]\s*\+\s*\w/.test(l) || /%[sdv]/.test(l)) && !/\{[^}]*:/.test(l),
    problem:
      "builds the message by interpolation with no structured fields, so the value cannot be filtered, grouped or alerted on — only grepped",
    fix: "pass fields as data: `logger.info({ orderId, durationMs }, 'settled')` rather than embedding them in the sentence",
  },
  {
    rule: "error-without-cause",
    appliesInCli: false,
    severity: "P2",
    test: (l, whole) => {
      if (!/\.(error|exception|fatal|critical|warn)\s*\(/i.test(l)) return false;
      const start = whole.indexOf(l);
      const before = start === -1 ? "" : whole.slice(Math.max(0, start - 300), start);
      if (!/\bcatch\s*(\(|\{|\s+\w)|\bexcept\b|\brescue\b|err\s*!=\s*nil/i.test(before)) return false;
      // The caught binding must appear in the log call.
      const m = /catch\s*\(\s*(\w+)|except\s+\w+\s+as\s+(\w+)|(\berr\b)/i.exec(before.slice(-160));
      const bind = m?.[1] ?? m?.[2] ?? m?.[3];
      return bind !== undefined && !new RegExp(`\\b${bind}\\b`).test(l);
    },
    problem: "logs inside a catch without including the caught error, so the stack and the cause are gone and the line says only that something failed",
    fix: "pass the error itself alongside the message — the stack is the whole value of the log at 3am",
  },
  {
    rule: "arrival-log",
    appliesInCli: false,
    usesRaw: true,
    severity: "P3",
    test: (l) => LOG_CALL.test(l) && /\b(entering|entered|start(ing|ed)?\b|called with|begin|>>>|handler (hit|called)|in function)\b/i.test(l),
    problem: "logs an arrival rather than a decision or an outcome — arrivals are the bulk of log volume and almost never the thing you needed",
    fix: "log what was decided and what resulted, with the inputs that drove it",
  },
  {
    rule: "high-cardinality-metric",
    severity: "P2",
    test: (l) => METRIC_CALL.test(l) && ID_VALUE.test(l),
    problem:
      "puts an identifier-shaped value in a metric label — high cardinality is the point for logs and traces, and the cost trap for metrics: one series per distinct value",
    fix: "keep the id in the log or the trace and label the metric with a bounded dimension (route, status class, region)",
  },
  {
    rule: "log-in-tight-loop",
    appliesInCli: false,
    severity: "P3",
    test: (l, whole) => {
      if (!LOG_CALL.test(l)) return false;
      const start = whole.indexOf(l);
      if (start === -1) return false;
      const before = whole.slice(Math.max(0, start - 200), start);
      return /\b(for|while|forEach|map)\s*[({]/.test(before) && !/\bif\b/.test(before.slice(-80));
    },
    problem: "logs unconditionally inside a loop — the volume scales with the data, and the one interesting line is buried in ten thousand",
    fix: "log once with a count and a sample, or log only the cases that are actually interesting",
  },
];

export function scanFile(file: string, raw?: string): Finding[] {
  let text: string;
  if (raw !== undefined) text = raw;
  else {
    try {
      text = fs.readFileSync(file, "utf8");
    } catch {
      return [{ file, line: 1, rule: "unreadable", severity: "P3", problem: "could not read this file, so it was NOT checked", fix: "fix the path and re-run" }];
    }
  }
  const code = stripNoncode(text, path.extname(file).toLowerCase());
  const cli = isCliContext(file, code);
  const loggerImpl = isLoggerImplementation(file, code);
  const lines = code.split("\n");
  const rawLines = text.split("\n");
  const findings: Finding[] = [];
  // Raw-reading rules test the whole log CALL, not a physical line. Every one of them begins with
  // `LOG_CALL.test(l)`, so a call whose message sat on a continuation line matched nothing — and a
  // long message is exactly the message that gets wrapped. Join each call onto its opening line and
  // mark the continuation lines consumed, so the finding still lands at the call site and once only.
  const callText = new Map<number, string>();
  const consumed = new Set<number>();
  for (let i = 0; i < lines.length; i += 1) {
    if (!LOG_CALL.test(lines[i] ?? "")) continue;
    let depth = 0;
    for (let j = i; j < Math.min(lines.length, i + 12); j += 1) {
      const blanked = lines[j] ?? "";
      depth += (blanked.match(/\(/g) ?? []).length - (blanked.match(/\)/g) ?? []).length;
      if (j > i) consumed.add(j);
      if (depth <= 0) {
        if (j > i) callText.set(i, rawLines.slice(i, j + 1).join(" "));
        break;
      }
    }
  }
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    if (line.trim() === "") continue;
    for (const r of RULES) {
      if (cli && r.appliesInCli === false) continue;
      if (loggerImpl && r.appliesInLoggerImpl === false) continue;
      // A raw-reading rule only sees the line if the blanked line proves it is live code.
      // A log call spanning several lines put its message on a line with no `logger.` on it, so
      // gating on the physical line silently dropped every finding for the multi-line form — which
      // is the form long messages actually take. `inLogCall` stays true until the call's parens close.
      if (r.usesRaw === true && consumed.has(i)) continue;
      if (r.usesRaw === true && !LOG_CALL.test(line)) continue;
      const subject = r.usesRaw === true ? callText.get(i) ?? rawLines[i] ?? "" : line;
      if (r.test(subject, code, file)) {
        findings.push({ file, line: i + 1, rule: r.rule, severity: r.severity, problem: r.problem, fix: r.fix });
      }
    }
  }
  // File-level: several log calls and nothing to stitch them together with.
  const logCalls = lines.filter((l) => LOG_CALL.test(l)).length;
  // Same exemption: a logger implementation has no request to correlate — the caller supplies that.
  if (!cli && !loggerImpl && logCalls >= 3 && !/\b(request[_-]?id|correlation[_-]?id|trace[_-]?id|span[_-]?id|requestId|correlationId|traceId|reqId)\b/i.test(code)) {
    findings.push({
      file,
      line: 1,
      rule: "no-correlation-key",
      severity: "P2",
      problem: `${logCalls} log calls and no request/trace/correlation id anywhere — the lines cannot be stitched into one request, which is the first thing you need at 3am`,
      fix: "thread a correlation id through and include it on every event",
    });
  }
  return findings;
}

function* walk(root: string): Generator<string> {
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      const full = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name)) stack.push(full);
      } else if (SOURCE_EXT.has(path.extname(e.name).toLowerCase()) && !TEST_PATH.test(full)) {
        yield full;
      }
    }
  }
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { all: { type: "boolean", default: false }, json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  let targets: string[] = [];
  if (values.all || positionals.length === 0) targets = [...walk(".")];
  else {
    for (const p of positionals) {
      try {
        if (fs.statSync(p).isDirectory()) targets.push(...walk(p));
        else targets.push(p);
      } catch {
        /* skip */
      }
    }
  }
  targets = [...new Set(targets)].sort();
  if (targets.length === 0) {
    if (values.json) console.log(JSON.stringify({ scanned: 0, findings: [] }));
    else if (!values.quiet) console.log("no source files found (tests are excluded — logging in a test is not a finding)");
    return 2;
  }

  const findings = targets.flatMap((t) => scanFile(t));
  if (values.json) {
    console.log(JSON.stringify({ scanned: targets.length, findings }, null, 2));
    return findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file))) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.rule}]`);
    console.log(`    ${f.problem}`);
    console.log(`    fix: ${f.fix}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(findings.length === 0 ? `clean: ${targets.length} file(s)` : `${findings.length} finding(s) across ${targets.length} file(s)`);
    console.log(
      "Field names only — it cannot see that a variable called `data` holds an email address, and it\n" +
        "cannot tell whether your aggregator redacts. A clean run is not a privacy review. See\n" +
        "/viby-toolkit:observe.",
    );
  }
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
