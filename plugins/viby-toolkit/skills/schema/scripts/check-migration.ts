/**
 * viby-toolkit migration safety linter — the executable half of /viby-toolkit:schema.
 *
 * A bad schema migration is one of the very few genuinely unrecoverable software mistakes:
 * a dropped column takes the data with it, and a lock held on a hot table takes the service
 * down while it does. Almost all of it comes from a short list of operations whose danger is
 * well established and each of which has a safe alternative. This finds them.
 *
 * Usage:
 *   node check-migration.ts [paths...] [--all] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no migration files found.
 *
 * DESIGN: each rule names the DANGER and the SAFE ALTERNATIVE, because "don't do that" with
 * no replacement gets ignored under deadline. Severity is by irreversibility first and lock
 * impact second — a lock costs minutes of downtime, dropped data costs it permanently.
 *
 * Engine behaviour cited here (ACCESS EXCLUSIVE locks, table rewrites, CONCURRENTLY) is
 * standard documented PostgreSQL/MySQL behaviour, not a research claim. Specifics vary by
 * engine and version: treat every finding as "check this against YOUR engine", which is why
 * the messages say what to verify rather than asserting a universal truth.
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

/** Paths that hold schema migrations across the common tools. */
const MIGRATION_PATH = new RegExp(
  [
    /(^|\/)migrations?\//.source, // django, alembic, knex, generic
    /(^|\/)db\/migrate\//.source, // rails
    /(^|\/)versions\//.source, // alembic
    /(^|\/)prisma\/migrations\//.source,
    /(^|\/)supabase\/migrations\//.source,
    /(^|\/)V\d+[\w.]*__.*\.sql$/.source, // flyway
    /(^|\/)changelog.*\.(xml|ya?ml|sql)$/.source, // liquibase
    /(^|\/)schema\.rb$/.source,
    /(^|\/)structure\.sql$/.source,
  ].join("|"),
);
const MIGRATION_EXT = new Set([".sql", ".py", ".rb", ".ts", ".js", ".go", ".php", ".xml", ".yaml", ".yml"]);
const SKIP_DIRS = new Set([".git", "node_modules", "venv", ".venv", "dist", "build", "target", "__pycache__"]);

export function isMigrationFile(p: string): boolean {
  const norm = p.split(path.sep).join("/");
  if (!MIGRATION_EXT.has(path.extname(norm).toLowerCase())) return false;
  return MIGRATION_PATH.test(norm);
}

type Rule = {
  rule: string;
  severity: "P1" | "P2" | "P3";
  test: (line: string, whole: string) => boolean;
  danger: string;
  instead: string;
};

const RULES: Rule[] = [
  // ---- irreversible: the data is gone
  {
    rule: "drop-column",
    severity: "P1",
    test: (l) => /\bDROP\s+COLUMN\b/i.test(l) || /\bremove_column\b/i.test(l),
    danger: "dropping a column destroys its data irreversibly, and any deployed code still selecting it starts erroring immediately",
    instead:
      "expand-contract: stop writing it, deploy, confirm nothing reads it (logs/metrics, not assumption), THEN drop in a later migration",
  },
  {
    rule: "drop-table",
    severity: "P1",
    test: (l) => /\bDROP\s+TABLE\b/i.test(l) || /\bdrop_table\b/i.test(l),
    danger: "dropping a table destroys the data irreversibly",
    instead: "rename it out of the way first and drop it a release later, once you can prove nothing references it",
  },
  {
    rule: "truncate",
    severity: "P1",
    test: (l) => /\bTRUNCATE\b/i.test(l),
    danger: "TRUNCATE deletes every row and is not transactional on all engines",
    instead: "if this is intentional in a migration, say so explicitly and take a verified backup first",
  },
  {
    rule: "unbounded-dml",
    severity: "P1",
    // Checks the whole STATEMENT, not just the opening line: ordinary SQL style puts WHERE
    // on a later line, and the single-line version fired a false P1 on
    //   UPDATE foo\n  SET x = 1\n  WHERE id > 100;
    // A wrong P1 on safe, common code is the fastest way to get a linter switched off.
    test: (line, whole) => {
      if (!/^\s*(UPDATE|DELETE)\b/i.test(line)) return false;
      const start = whole.indexOf(line);
      const rest = start === -1 ? line : whole.slice(start);
      const end = rest.indexOf(";");
      const statement = end === -1 ? rest : rest.slice(0, end);
      return !/\bWHERE\b/i.test(statement);
    },
    danger: "an UPDATE/DELETE with no WHERE touches every row: one long transaction, held locks, and no way back",
    instead: "backfill in bounded batches with a WHERE on a key range, committing between batches",
  },

  // ---- locks: the service goes down while it runs
  {
    rule: "index-without-concurrently",
    severity: "P1",
    test: (l) => /\bCREATE\s+(UNIQUE\s+)?INDEX\b/i.test(l) && !/\bCONCURRENTLY\b/i.test(l),
    danger: "CREATE INDEX takes a lock that blocks writes for the whole build — minutes on a large table",
    instead: "CREATE INDEX CONCURRENTLY (postgres), outside a transaction; on MySQL confirm ALGORITHM=INPLACE applies",
  },
  {
    rule: "drop-index-without-concurrently",
    severity: "P2",
    test: (l) => /\bDROP\s+INDEX\b/i.test(l) && !/\bCONCURRENTLY\b/i.test(l),
    danger: "DROP INDEX takes an exclusive lock on the table",
    instead: "DROP INDEX CONCURRENTLY (postgres)",
  },
  {
    rule: "add-not-null-column",
    severity: "P1",
    test: (l) => /\bADD\s+COLUMN\b/i.test(l) && /\bNOT\s+NULL\b/i.test(l) && !/\bDEFAULT\b/i.test(l),
    danger: "adding NOT NULL without a default has to prove every existing row complies, and fails outright if the table is non-empty",
    instead: "add it nullable, backfill in batches, then add the NOT NULL constraint separately once the data is clean",
  },
  {
    rule: "set-not-null",
    severity: "P2",
    test: (l) => /\bALTER\s+COLUMN\b.*\bSET\s+NOT\s+NULL\b/i.test(l),
    danger: "SET NOT NULL scans the whole table under a lock to validate it",
    instead: "add a validated CHECK (col IS NOT NULL) NOT VALID, VALIDATE it separately, then SET NOT NULL (postgres 12+)",
  },
  {
    rule: "column-type-change",
    severity: "P1",
    test: (l) => /\bALTER\s+(COLUMN\s+\w+\s+)?TYPE\b/i.test(l) || /\bMODIFY\s+COLUMN\b/i.test(l),
    danger: "changing a column type can rewrite the entire table under an exclusive lock, and may silently truncate values",
    instead: "add a new column, dual-write, backfill in batches, switch reads, then drop the old one in a later migration",
  },
  {
    rule: "constraint-without-not-valid",
    severity: "P2",
    test: (l) =>
      /\bADD\s+(CONSTRAINT\b.*)?(FOREIGN\s+KEY|CHECK)\b/i.test(l) && !/\bNOT\s+VALID\b/i.test(l),
    danger: "adding a FOREIGN KEY or CHECK validates every existing row while holding a lock",
    instead: "ADD ... NOT VALID first, then VALIDATE CONSTRAINT in a separate migration (takes a weaker lock)",
  },
  {
    rule: "rename",
    severity: "P1",
    test: (l) =>
      /\bRENAME\s+(COLUMN|TO)\b/i.test(l) || /\brename_column\b/i.test(l) || /\brename_table\b/i.test(l),
    danger: "a rename is not backward compatible: code deployed before the migration breaks the instant it lands, and code after breaks until it lands",
    instead: "add the new name, dual-write, migrate readers, drop the old name a release later — never rename in place on a live system",
  },
  {
    rule: "vacuum-full",
    severity: "P2",
    test: (l) => /\bVACUUM\s+FULL\b/i.test(l) || (/\bREINDEX\b/i.test(l) && !/\bCONCURRENTLY\b/i.test(l)),
    danger: "VACUUM FULL and plain REINDEX hold an exclusive lock for the duration",
    instead: "pg_repack, or REINDEX CONCURRENTLY",
  },
];

/** Whole-file checks, rather than per-line. */
function fileLevel(file: string, text: string, isSql: boolean): Finding[] {
  const out: Finding[] = [];
  const ddl = /\b(ALTER\s+TABLE|CREATE\s+TABLE|CREATE\s+INDEX|DROP\s+)/i.test(text);
  const dml = /^\s*(UPDATE|INSERT|DELETE)\b/im.test(text);

  if (ddl && dml) {
    out.push({
      file,
      line: 1,
      rule: "ddl-and-backfill-together",
      severity: "P2",
      danger:
        "schema change and data backfill in one migration means one long transaction: the DDL lock is held for as long as the backfill takes",
      instead: "split them — ship the schema change, then backfill in batches in a separate migration or a job",
    });
  }
  if (isSql && ddl && !/\block_timeout\b/i.test(text)) {
    out.push({
      file,
      line: 1,
      rule: "no-lock-timeout",
      severity: "P2",
      danger:
        "with no lock_timeout, a migration that cannot get its lock queues behind a long query and every request queues behind it — a short migration becomes an outage",
      instead: "SET lock_timeout = '5s' (and a statement_timeout) so it fails fast and can be retried",
    });
  }
  // Reversibility: only for tools that have an explicit down/rollback concept.
  const isRailsOrAlembic = /(^|\/)(db\/migrate|versions)\//.test(file) || /\.rb$/.test(file);
  if (isRailsOrAlembic && /\b(def\s+up|def\s+change|def\s+upgrade)\b/.test(text)) {
    if (!/\b(def\s+down|def\s+downgrade)\b/.test(text) && !/\bdef\s+change\b/.test(text)) {
      out.push({
        file,
        line: 1,
        rule: "no-rollback",
        severity: "P2",
        danger: "no down/downgrade path, so this migration cannot be reversed if the deploy goes wrong",
        instead: "write the inverse, or state in a comment why it is deliberately irreversible",
      });
    }
  }
  return out;
}

export function scanMigration(filePath: string): Finding[] {
  let raw: string;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
    // Never report an unreadable migration as safe — see the same fix in
    // scan-test-quality.ts. Silence here would mean "no dangerous patterns found" for a
    // file nobody looked at, on the one class of change that cannot be undone.
    return [
      {
        file: filePath,
        line: 1,
        rule: "unreadable",
        severity: "P2",
        danger: "could not read this migration, so it was NOT checked for dangerous patterns",
        instead: "fix the permissions or path and re-run — do not treat this run as clean for it",
      },
    ];
  }
  const ext = path.extname(filePath).toLowerCase();
  // Match CODE, not raw text — a comment or a docstring describing a DROP COLUMN is not one.
  const text = stripNoncode(raw, ext);
  const lines = text.split("\n");
  const findings: Finding[] = [];

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    if (line.trim() === "") continue;
    for (const r of RULES) {
      if (r.test(line, text)) {
        findings.push({ file: filePath, line: i + 1, rule: r.rule, severity: r.severity, danger: r.danger, instead: r.instead });
      }
    }
  }
  findings.push(...fileLevel(filePath, text, ext === ".sql"));
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
      } else if (isMigrationFile(full)) {
        yield full;
      }
    }
  }
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      all: { type: "boolean", default: false },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });

  let targets: string[] = [];
  if (values.all || positionals.length === 0) {
    targets = [...walk(".")];
  } else {
    for (const p of positionals) {
      try {
        if (fs.statSync(p).isDirectory()) targets.push(...walk(p));
        else targets.push(p);
      } catch {
        /* skip unreadable */
      }
    }
  }
  targets = [...new Set(targets)].sort();

  if (targets.length === 0) {
    if (values.json) console.log(JSON.stringify({ scanned: 0, findings: [] }));
    else if (!values.quiet) console.log("no migration files found (looked for migrations/, db/migrate/, versions/, flyway, prisma)");
    return 2;
  }

  const findings = targets.flatMap(scanMigration);

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
    if (findings.length === 0) {
      console.log(`clean: ${targets.length} migration file(s), no dangerous patterns found`);
    } else {
      console.log(`${findings.length} finding(s) across ${targets.length} migration file(s)`);
    }
    console.log(
      "Engine and version change the details — verify each against YOUR database before acting,\n" +
        "and never run an unreviewed migration against production data. See /viby-toolkit:schema.",
    );
  }
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
