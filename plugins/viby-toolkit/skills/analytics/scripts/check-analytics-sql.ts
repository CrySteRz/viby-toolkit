/**
 * viby-toolkit analytics SQL linter — the executable half of /viby-toolkit:analytics.
 *
 * A KPI is a number a client makes decisions with, and the ways it goes quietly wrong are a short,
 * well-known list. None of them fail loudly: the query runs, returns a plausible number, and the
 * dashboard looks fine. That is the whole problem — a wrong KPI is indistinguishable from a right
 * one until someone reconciles it against the business, usually in a meeting.
 *
 * Usage:
 *   node check-analytics-sql.ts [paths...] [--all] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no SQL found.
 *
 * DESIGN, same as the migration linter: every rule names the danger AND the safe alternative,
 * because "don't do that" with no replacement gets ignored under deadline. Severity is by how
 * silently it corrupts a number: P1 = the number is wrong and nothing tells you, P2 = wrong under
 * conditions that will eventually occur, P3 = fragile or unreproducible.
 *
 * Engine behaviour differs (BETWEEN, DATE_TRUNC and timezone semantics vary), so findings say what
 * to verify rather than asserting a universal truth. Decide on parsed SQL, never raw text: strings
 * and `--` comments are blanked first, so a column named `revenue_between_dates` or a commented-out
 * draft query is not a finding.
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
  danger: string;
  instead: string;
};

const SQL_EXT = new Set([".sql", ".dbt", ".ddl"]);
const SKIP_DIRS = new Set([".git", "node_modules", "target", "dbt_packages", "venv", ".venv", "dist", "build", "logs"]);

/** Looks like a date/time thing — a column name, a cast, or a date function. */
const TEMPORAL = /\b(\w*(?:date|time|timestamp|_at|_ts|day|month|week|year)\w*)\b|::\s*(?:date|timestamp\w*)|\b(?:current_date|now|getdate|sysdate)\b/i;
/** A quoted date-shaped literal: '2026-01-01', '2026-01-01 00:00:00'. */
const DATE_LITERAL_BLANKED = /'\s*'|''/; // strings are blanked, so a literal survives only as quotes

type Rule = {
  rule: string;
  severity: "P1" | "P2" | "P3";
  /** line = the blanked line, whole = the blanked file, raw = the original file. */
  test: (line: string, whole: string, raw: string) => boolean;
  danger: string;
  instead: string;
};

export const RULES: Rule[] = [
  {
    rule: "between-on-time",
    severity: "P1",
    // BETWEEN is closed-closed. Temporal binning needs a half-open interval or the boundary row is
    // counted in two periods — the classic "our monthly numbers don't add up to the year".
    test: (l) => /\bBETWEEN\b/i.test(l) && TEMPORAL.test(l),
    danger:
      "BETWEEN on a date/time range is inclusive at BOTH ends, so a row exactly on the boundary lands in two buckets and monthly figures stop summing to the annual one",
    instead: "use a half-open interval: `ts >= '<start>' AND ts < '<next start>'` — never BETWEEN for time",
  },
  {
    rule: "divide-without-guard",
    severity: "P1",
    // A rate whose denominator can be zero or NULL: either an error or, worse, a silent NULL that
    // a dashboard renders as a gap or a zero.
    test: (l) => {
      if (!/\//.test(l) || /^\s*--/.test(l)) return false;
      if (/\b(?:NULLIF|COALESCE|CASE|IFNULL|DIV0|SAFE_DIVIDE)\b/i.test(l)) return false;
      // Only when the denominator is an expression, not a literal (x / 100 is fine).
      return /\/\s*(?![0-9.]+\b)[a-z_(]/i.test(l);
    },
    danger:
      "division with a non-literal denominator: a zero denominator errors or returns NULL, and a NULL rate is rendered as a gap or a zero rather than 'no data'",
    instead: "wrap it — `numerator / NULLIF(denominator, 0)` — and decide explicitly what an empty denominator should display as",
  },
  {
    rule: "count-star-with-join",
    severity: "P1",
    // A join that fans out multiplies the rows, and COUNT(*) counts the multiplication.
    test: (l, whole) => /\bCOUNT\s*\(\s*\*\s*\)/i.test(l) && /\b(?:LEFT|RIGHT|INNER|FULL|CROSS)?\s*JOIN\b/i.test(whole),
    danger:
      "COUNT(*) in a query that joins: if any join matches more than one row, the count is multiplied and the metric is silently inflated",
    instead: "count the grain you mean — `COUNT(DISTINCT <primary key of the grain>)` — or aggregate the joined side in a subquery first",
  },
  {
    rule: "now-in-definition",
    severity: "P2",
    // The word boundary goes only where a word actually ends. `\b` AFTER `NOW()` can never match —
    // `)` is not a word character, so there is no boundary to find, and the rule silently never
    // fired. Caught by its own contract test.
    test: (l, whole) => {
      if (!/\bNOW\s*\(\s*\)|\bCURRENT_TIMESTAMP\b|\bCURRENT_DATE\b|\bGETDATE\s*\(|\bSYSDATE\b/i.test(l)) return false;
      // Only in a METRIC query. Reading the clock is correct and normal in a migration
      // (`created_at DEFAULT NOW()`, a dated backfill) and in DDL. Measured on a real repo of 62
      // migrations this rule fired 117 times — 93% of all findings, every one of them legitimate
      // SQL. A rule that floods on correct code takes the whole linter down with it.
      if (/\b(?:INSERT\s+INTO|UPDATE\s+\w|ALTER\s+TABLE|CREATE\s+(?:TABLE|INDEX)|DEFAULT\b)/i.test(whole)) return false;
      return /\bSELECT\b/i.test(whole) && /\b(?:SUM|COUNT|AVG|MIN|MAX|PERCENTILE\w*)\s*\(/i.test(whole);
    },
    danger:
      "a metric that reads the clock cannot be reproduced: the same query returns a different number tomorrow, so nobody can verify last week's figure or re-run a report",
    instead: "take the window as a parameter (or a dbt var) so a run is reproducible, and record the window on the output",
  },
  {
    rule: "select-star",
    severity: "P3",
    test: (l) => /\bSELECT\s+\*/i.test(l) && !/\bCOUNT\s*\(\s*\*/i.test(l),
    danger: "SELECT * in a model or metric query silently changes shape when an upstream column is added, renamed or reordered",
    instead: "list the columns you depend on — the query then fails loudly when the contract changes, which is what you want",
  },
  {
    rule: "union-dedupes",
    severity: "P2",
    test: (l) => /\bUNION\b/i.test(l) && !/\bUNION\s+ALL\b/i.test(l),
    danger:
      "plain UNION removes duplicate rows, so genuinely repeated events (two identical purchases in a second) are silently collapsed and the count is under-reported",
    instead: "UNION ALL unless you can state why identical rows must be collapsed",
  },
  {
    rule: "distinct-over-aggregate",
    severity: "P2",
    test: (l, whole) =>
      /\bSELECT\s+DISTINCT\b/i.test(l) && /\b(?:SUM|COUNT|AVG|MIN|MAX)\s*\(/i.test(whole) && /\bJOIN\b/i.test(whole),
    danger:
      "SELECT DISTINCT over a joined aggregate usually papers over a fan-out join rather than fixing it — it hides duplicate rows but the SUMs were already computed on them",
    instead: "find the join that fans out and aggregate it to the right grain first; DISTINCT at the end cannot undo an inflated SUM",
  },
  {
    rule: "date-trunc-without-zone",
    severity: "P2",
    test: (l) =>
      /\bDATE_TRUNC\s*\(/i.test(l) && !/\bAT\s+TIME\s+ZONE\b/i.test(l) && !/\btimezone\s*\(/i.test(l) && TEMPORAL.test(l),
    danger:
      "truncating a timestamp without naming a timezone buckets by UTC day; a client in another timezone sees events shifted into the wrong day, and their 'yesterday' never matches yours",
    instead: "convert explicitly — `DATE_TRUNC('day', ts AT TIME ZONE '<the reporting zone>')` — and state the reporting timezone on the dashboard",
  },
  {
    rule: "null-equality",
    severity: "P1",
    // Comparison only. `SET col = NULL` is an ASSIGNMENT and perfectly correct — flagging it fired
    // on real UPDATE statements that were doing exactly the right thing.
    test: (l) => /(?:<>|!=)\s*NULL\b/i.test(l) || (/=\s*NULL\b/i.test(l) && !/\bSET\b[^;]*=\s*NULL\b/i.test(l)),
    danger: "comparing to NULL with = or <> is never true, so the filter silently matches nothing and the metric quietly excludes every row it should have caught",
    instead: "use IS NULL / IS NOT NULL",
  },
  {
    rule: "float-money",
    severity: "P2",
    test: (l) => /\b(?:FLOAT|REAL|DOUBLE(?:\s+PRECISION)?)\b/i.test(l) && /\b(\w*(?:amount|price|revenue|cost|total|fee|balance|salary)\w*)\b/i.test(l),
    danger: "binary floating point cannot represent decimal money exactly, so sums drift by cents and a revenue KPI stops reconciling with finance",
    instead: "use NUMERIC/DECIMAL with an explicit scale for anything monetary",
  },
  {
    rule: "unbounded-fact-scan",
    severity: "P3",
    test: (l, whole) => {
      if (!/\bFROM\s+[\w."]*\b(?:events?|fact_\w+|\w+_events?|logs?|clicks?|pageviews?)\b/i.test(l)) return false;
      if (/\bWHERE\b[\s\S]{0,400}?/i.test(whole) && TEMPORAL.test(whole.slice(whole.search(/\bWHERE\b/i)))) return false;
      return /\b(?:SUM|COUNT|AVG)\s*\(/i.test(whole);
    },
    danger:
      "an aggregate over an event/fact table with no time predicate scans the whole history: it gets slower every day and quietly changes meaning as old data ages in",
    instead: "always bound the window explicitly, even for an 'all time' figure — then 'all time' is a stated range rather than 'whatever exists today'",
  },
];

export function scanSql(filePath: string, rawInput?: string): Finding[] {
  let raw: string;
  if (rawInput !== undefined) raw = rawInput;
  else {
    try {
      raw = fs.readFileSync(filePath, "utf8");
    } catch {
      return [
        {
          file: filePath,
          line: 1,
          rule: "unreadable",
          severity: "P2",
          danger: "could not read this file, so it was NOT checked",
          instead: "fix the path and re-run — do not treat this run as clean for it",
        },
      ];
    }
  }
  // Decide on SQL, not on text: string literals and `--`/`/* */` comments are blanked.
  const code = stripNoncode(raw, ".sql");
  const lines = code.split("\n");
  const findings: Finding[] = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    if (line.trim() === "") continue;
    for (const r of RULES) {
      if (r.test(line, code, raw)) {
        findings.push({ file: filePath, line: i + 1, rule: r.rule, severity: r.severity, danger: r.danger, instead: r.instead });
      }
    }
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
      } else if (SQL_EXT.has(path.extname(e.name).toLowerCase())) {
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
    else if (!values.quiet) console.log("no .sql files found");
    return 2;
  }

  const findings = targets.flatMap((t) => scanSql(t));

  if (values.json) {
    console.log(JSON.stringify({ scanned: targets.length, findings }, null, 2));
    return findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file))) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.rule}]`);
    console.log(`    risk:    ${f.danger}`);
    console.log(`    instead: ${f.instead}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(
      findings.length === 0
        ? `clean: ${targets.length} SQL file(s), none of the known silent-corruption patterns`
        : `${findings.length} finding(s) across ${targets.length} SQL file(s)`,
    );
    console.log(
      "None of these fail loudly — the query runs and returns a plausible number, which is why they\n" +
        "survive to production. Engine semantics differ, so verify each against YOUR warehouse, and\n" +
        "remember this checks syntax, not whether the metric answers the question. See\n" +
        "/viby-toolkit:analytics.",
    );
  }
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
