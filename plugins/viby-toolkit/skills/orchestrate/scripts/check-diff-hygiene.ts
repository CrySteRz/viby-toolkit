/**
 * viby-toolkit diff hygiene — the executable half of /viby-toolkit:orchestrate.
 *
 * The build phase was the last uninstrumented part of this toolkit: deciding and proving had
 * checkers, writing code had prose. This closes it by auditing the artifact the build phase
 * actually produces — the diff — for the things that make a change hard to review or unsafe to
 * land, none of which a compiler or a test run objects to.
 *
 * Usage:
 *   node check-diff-hygiene.ts [--base <ref>] [--staged] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no diff to check.
 *
 * WHY SIZE IS A FINDING, not a style opinion. The largest study of code review — SmartBear at
 * Cisco, 2,500 reviews over 3.2 million lines — found reviewers spot defects most effectively at
 * **200–400 changed lines**, with detection dropping off past 400. Reported detection falls from
 * ~87% on changes under 100 lines to ~28% on changes over 1,000. A 2,000-line diff is not a
 * bigger review, it is an unreviewed one, and splitting it is the highest-leverage thing available
 * before anyone reads it.
 *
 * DESIGN: diff-scoped, so it judges what you ADDED rather than what the file already contained —
 * `check-release.ts` scans the tree and will fairly complain about pre-existing debris; this only
 * complains about debris you are introducing. Every finding names the fix.
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import path from "node:path";

export type Finding = {
  file: string;
  line: number;
  check: string;
  severity: "P1" | "P2" | "P3";
  problem: string;
  fix: string;
};

export type DiffFile = {
  file: string;
  added: Array<{ line: number; text: string }>;
  removedCount: number;
  /** Added lines that differ from a removed line only in whitespace. */
  whitespaceOnly: number;
};

const TEST_PATH = /(^|\/)(tests?|specs?|__tests__)\/|[._-](test|tests|spec|specs)\.[a-z]+$|(^|\/)test_[^/]*\.py$|_test\.[a-z]+$|Test(s)?\.(java|kt|cs)$/i;
const SOURCE_EXT = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rs", ".java", ".rb", ".php", ".cs", ".kt", ".swift", ".scala", ".c", ".cc", ".cpp", ".h", ".hpp"]);
/**
 * A script's stdout IS its interface. `console.log` in a CLI, a migration script, a seed file or a
 * task runner is progress output for the human running it, not leftover debugging — and measured on
 * four real repositories, every single `debug-added` finding (20 of them) was exactly that. Path is
 * the reliable signal; the message text is not.
 */
const SCRIPT_PATH = /(^|\/)(scripts?|bin|tools?|cli|tasks?|jobs?|migrations?|seeds?)\//i;
const SCRIPT_FILE = /(^|\/)(seed|migrate|backfill|bootstrap|setup|deploy|codemod)[\w.-]*\.[a-z]+$/i;

const LOCKFILES = new Set(["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", "poetry.lock", "cargo.lock", "uv.lock", "composer.lock", "gemfile.lock", "go.sum"]);
const MANIFESTS = new Set(["package.json", "pyproject.toml", "requirements.txt", "cargo.toml", "go.mod", "composer.json", "gemfile", "build.gradle", "pom.xml"]);

function isSourceFile(file: string): boolean {
  return SOURCE_EXT.has(path.extname(file).toLowerCase());
}

/** Review-size thresholds, from the Cisco/SmartBear measurement quoted in the header. */
const REVIEWABLE_MAX = 400;
const UNREVIEWABLE = 1000;

export function parseDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = [];
  let current: DiffFile | null = null;
  let lineNo = 0;
  let pendingRemovals: string[] = [];

  for (const raw of diff.split("\n")) {
    if (raw.startsWith("diff --git ")) {
      if (current) files.push(current);
      const m = / b\/(.+)$/.exec(raw);
      current = { file: m?.[1] ?? "?", added: [], removedCount: 0, whitespaceOnly: 0 };
      pendingRemovals = [];
      continue;
    }
    if (current === null) continue;
    if (raw.startsWith("@@")) {
      const m = /@@ -\d+(?:,\d+)? \+(\d+)/.exec(raw);
      lineNo = m?.[1] !== undefined ? Number(m[1]) - 1 : 0;
      pendingRemovals = [];
      continue;
    }
    if (raw.startsWith("---") || raw.startsWith("+++")) continue;
    if (raw.startsWith("-")) {
      current.removedCount += 1;
      pendingRemovals.push(raw.slice(1));
      continue;
    }
    if (raw.startsWith("+")) {
      lineNo += 1;
      const text = raw.slice(1);
      current.added.push({ line: lineNo, text });
      // Whitespace-only churn: an added line matching a removed one once spaces are collapsed.
      const norm = text.replace(/\s+/g, "");
      if (norm !== "" && pendingRemovals.some((r) => r.replace(/\s+/g, "") === norm)) current.whitespaceOnly += 1;
      continue;
    }
    if (raw.startsWith(" ")) {
      lineNo += 1;
      pendingRemovals = [];
    }
  }
  if (current) files.push(current);
  return files;
}

type LineRule = {
  check: string;
  severity: "P1" | "P2" | "P3";
  /**
   * Which files the rule applies to. Comment-shaped rules must not run on prose: `#` is a heading
   * in Markdown, so `commented-out-code` fired on six SKILL.md files in this repo's own dogfood.
   * A credential or a conflict marker, by contrast, matters in every file type there is.
   */
  appliesTo?: (file: string) => boolean;
  test: (text: string, file: string) => boolean;
  problem: string;
  fix: string;
};

const LINE_RULES: LineRule[] = [
  {
    check: "conflict-marker",
    severity: "P1",
    test: (t) => /^(<{7}|={7}|>{7})(\s|$)/.test(t),
    problem: "a merge conflict marker is being committed — the file is in a half-merged state",
    fix: "resolve the conflict and remove the markers",
  },
  {
    check: "secret-shaped",
    severity: "P1",
    // Deliberately only the shapes that are almost never a false positive. /viby-toolkit:secure is
    // the real pass; this catches the obvious one at the moment it would enter history.
    test: (t) =>
      /-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(t) ||
      /\bAKIA[0-9A-Z]{16}\b/.test(t) ||
      /\bgh[pousr]_[A-Za-z0-9]{36,}\b/.test(t) ||
      /\bsk-[A-Za-z0-9]{32,}\b/.test(t) ||
      /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/.test(t),
    problem: "this line looks like a real credential, and a secret in a commit is compromised even if you remove it later",
    fix: "remove it, rotate the credential, and load it from the environment — see /viby-toolkit:secure",
  },
  {
    check: "debug-added",
    severity: "P2",
    appliesTo: isSourceFile,
    test: (t, f) => {
      const ext = path.extname(f).toLowerCase();
      if (/^\s*(\/\/|#|\*)/.test(t)) return false; // a commented-out one is a different rule
      if (SCRIPT_PATH.test(f) || SCRIPT_FILE.test(f)) return false; // a script's output is not debug
      if (/\bdebugger\s*;?\s*$/.test(t)) return true;
      if (/\bconsole\.(log|debug|dir)\s*\(/.test(t) && ![".test.ts"].some((s) => f.endsWith(s))) return true;
      if (ext === ".py" && /^\s*print\s*\(/.test(t)) return true;
      if (ext === ".go" && /\bfmt\.Print(ln|f)?\s*\(/.test(t)) return true;
      if (ext === ".rs" && /\b(dbg!|println!)\s*\(/.test(t)) return true;
      if (ext === ".php" && /\b(var_dump|dd)\s*\(/.test(t)) return true;
      return false;
    },
    problem: "a debug print or breakpoint is being added — these are for the ten minutes you were debugging, not for the branch",
    fix: "remove it, or replace it with real instrumentation via /viby-toolkit:observe if the signal is worth keeping",
  },
  {
    check: "todo-added",
    severity: "P3",
    appliesTo: isSourceFile,
    test: (t) => /\b(TODO|FIXME|XXX|HACK)\b/.test(t),
    problem: "a new TODO/FIXME — it will outlive the branch, and nobody is assigned to it",
    fix: "do it, delete it, or file it with an owner; a marker with no owner is a wish",
  },
  {
    check: "commented-out-code",
    severity: "P3",
    appliesTo: isSourceFile,
    test: (t) => /^\s*(\/\/|#)\s*[\w$.[\]]+\s*[({=]/.test(t) && /[;){]\s*$/.test(t),
    problem: "commented-out code is being added — the next reader cannot tell if it is a spare part or a mistake",
    fix: "delete it; git remembers, and a commented block does not",
  },
];

function git(args: string[], cwd: string): { ok: boolean; out: string } {
  const p = spawnSync("git", args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  return { ok: p.status === 0, out: p.stdout ?? "" };
}

export function checkDiffHygiene(diff: string): { findings: Finding[]; addedLines: number; files: number } {
  const parsed = parseDiff(diff);
  const findings: Finding[] = [];

  for (const f of parsed) {
    for (const a of f.added) {
      for (const r of LINE_RULES) {
        if (r.appliesTo !== undefined && !r.appliesTo(f.file)) continue;
        if (r.test(a.text, f.file)) {
          findings.push({ file: f.file, line: a.line, check: r.check, severity: r.severity, problem: r.problem, fix: r.fix });
        }
      }
    }
  }

  const addedLines = parsed.reduce((n, f) => n + f.added.length, 0);

  // ---- diff-level checks
  if (addedLines + parsed.reduce((n, f) => n + f.removedCount, 0) > UNREVIEWABLE) {
    findings.push({
      file: "(diff)",
      line: 1,
      check: "unreviewable-size",
      severity: "P1",
      problem: `${addedLines} added lines across ${parsed.length} file(s): measured defect detection falls to ~28% past 1,000 changed lines, so this will not be reviewed so much as skimmed`,
      fix: "split it — mechanical/formatting changes in their own commit, then structural moves, then behaviour",
    });
  } else if (addedLines > REVIEWABLE_MAX) {
    findings.push({
      file: "(diff)",
      line: 1,
      check: "oversized-diff",
      severity: "P2",
      problem: `${addedLines} added lines: reviewers spot defects best at 200–400 changed lines and detection drops past that`,
      fix: "split it if the parts are separable; if it genuinely is not, say so in the description and point the reviewer at the risky part",
    });
  }

  // Formatting churn mixed with real work makes a diff unreadable, and hides the real change.
  const churnFiles = parsed.filter((f) => f.added.length > 0 && f.whitespaceOnly / f.added.length > 0.6 && f.added.length > 5);
  const realFiles = parsed.filter((f) => f.added.length > 0 && f.whitespaceOnly / f.added.length <= 0.6);
  if (churnFiles.length > 0 && realFiles.length > 0) {
    findings.push({
      file: churnFiles.map((f) => f.file).slice(0, 3).join(", "),
      line: 1,
      check: "mixed-concerns",
      severity: "P2",
      problem: `${churnFiles.length} file(s) are almost entirely whitespace/formatting churn while ${realFiles.length} carry real changes — the reformatting hides the change that matters`,
      fix: "commit the formatting separately, so the behavioural diff is readable on its own",
    });
  }

  const touchedSource = parsed.filter((f) => SOURCE_EXT.has(path.extname(f.file).toLowerCase()) && !TEST_PATH.test(f.file));
  const touchedTests = parsed.filter((f) => TEST_PATH.test(f.file));
  if (touchedSource.length > 0 && touchedTests.length === 0 && addedLines > 20) {
    findings.push({
      file: touchedSource.map((f) => f.file).slice(0, 3).join(", "),
      line: 1,
      check: "code-without-test",
      severity: "P2",
      problem: `${touchedSource.length} source file(s) changed and no test file touched`,
      fix: "add or update a test, or state explicitly why this change is not testable — 'no test' is a decision, not an oversight",
    });
  }

  const lock = parsed.filter((f) => LOCKFILES.has(path.basename(f.file).toLowerCase()));
  const manifest = parsed.filter((f) => MANIFESTS.has(path.basename(f.file).toLowerCase()));
  if (lock.length > 0 && manifest.length === 0) {
    findings.push({
      file: lock[0]?.file ?? "lockfile",
      line: 1,
      check: "lockfile-without-manifest",
      severity: "P2",
      problem: "a lockfile changed with no manifest change — usually an accidental resolution drift from someone's local install rather than an intended upgrade",
      fix: "revert it, or make the version change explicit in the manifest — see /viby-toolkit:deps",
    });
  }

  return { findings, addedLines, files: parsed.length };
}

function main(): number {
  const { values } = parseArgs({
    options: {
      base: { type: "string" },
      staged: { type: "boolean", default: false },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });
  const cwd = process.cwd();
  if (!git(["rev-parse", "--git-dir"], cwd).ok) {
    if (!values.quiet) console.log("not a git repository — nothing to diff");
    return 2;
  }

  let diff = "";
  let what = "";
  if (values.base !== undefined) {
    if (!git(["rev-parse", "--verify", "--quiet", `${values.base}^{commit}`], cwd).ok) {
      console.log(`cannot resolve base ref "${values.base}" — NOTHING was compared, which is deliberately not a pass`);
      return 2;
    }
    diff = git(["diff", "--unified=0", `${values.base}...HEAD`], cwd).out;
    what = `${values.base}...HEAD`;
  } else if (values.staged) {
    diff = git(["diff", "--unified=0", "--staged"], cwd).out;
    what = "staged changes";
  } else {
    diff = git(["diff", "--unified=0"], cwd).out + git(["diff", "--unified=0", "--staged"], cwd).out;
    what = "working tree + staged";
  }

  if (diff.trim() === "") {
    if (values.json) console.log(JSON.stringify({ findings: [], addedLines: 0, files: 0 }));
    else if (!values.quiet) console.log(`no changes in ${what}`);
    return 2;
  }

  const r = checkDiffHygiene(diff);

  if (values.json) {
    console.log(JSON.stringify({ ...r, target: what }, null, 2));
    return r.findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of r.findings.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file))) {
    console.log(`${f.file}${f.line > 1 ? `:${f.line}` : ""}  [${f.severity} ${f.check}]`);
    console.log(`    ${f.problem}`);
    console.log(`    fix: ${f.fix}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(
      r.findings.length === 0
        ? `clean: ${r.addedLines} added line(s) across ${r.files} file(s) in ${what}`
        : `${r.findings.length} finding(s) — ${r.addedLines} added line(s) across ${r.files} file(s) in ${what}`,
    );
    console.log(
      "This judges what the diff LOOKS like, not whether it is correct — a perfectly hygienic diff\n" +
        "can be entirely wrong. Correctness is /viby-toolkit:review-cluster and /viby-toolkit:verify.",
    );
  }
  return r.findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
