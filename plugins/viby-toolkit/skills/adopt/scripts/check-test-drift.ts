/**
 * viby-toolkit safety-net differ — the executable half of /viby-toolkit:adopt.
 *
 * Answers one question after a refactor, especially an agent-driven one: **did the tests get
 * weaker?** Not "do they pass" — a suite passes beautifully once you delete the tests that
 * failed. This compares the test suite between two git refs and reports any shrinkage: files
 * gone, test cases gone, assertions gone, skips added, or an early exit inserted.
 *
 * Usage:
 *   node check-test-drift.ts [--base <ref>] [paths...] [--json] [--quiet]
 * Exit: 0 = the net held or grew, 1 = it shrank, 2 = no tests found to compare.
 *
 * WHY THIS IS THE GATE. Repository-level refactoring is genuinely hard for models — on a
 * benchmark of 1,099 developer-written behaviour-preserving refactorings the best model
 * succeeded 41.58% of the time, and an agent scored 39.4% on compound cases. Under that much
 * failure pressure, the cheapest path to a green run is to edit the check rather than the code:
 * documented shortcuts include modifying or deleting the test file, and inserting sys.exit(0) to
 * leave the harness with a success code. The measured payoff of watching for it is the reason
 * this script exists rather than a paragraph of advice: trajectory-level behaviour monitoring
 * "reduces average hacked-resolved rate from 28.57% to 0.56%, while improving clean resolved
 * rate from 40.22% to 60.53%" (arXiv 2606.26300, fetched 2026-07-29). Watching for the shortcut
 * did not just stop the cheating — it made the honest work substantially more likely to land.
 *
 * WHAT IT IS NOT: it counts declarations and assertions. It cannot see an assertion that was
 * weakened in place (`toEqual(3)` → `toBeTruthy()`) while the count held, and it does not run
 * anything. A clean result means the net did not shrink, never that behaviour is preserved —
 * that is what the characterization suite and the held-out acceptance suite are for.
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { stripNoncode } from "../../../lib/strip-noncode.ts";

export type FileStats = {
  file: string;
  tests: number;
  assertions: number;
  skips: number;
  focus: number;
  earlyExit: number;
};

export type Drift = {
  base: string;
  findings: Finding[];
  before: Totals;
  after: Totals;
  compared: number;
  /** False when the base ref does not exist — the comparison never happened. */
  baseResolved: boolean;
};

export type Totals = { files: number; tests: number; assertions: number; skips: number; focus: number };

export type Finding = {
  file: string;
  check: string;
  severity: "P1" | "P2" | "P3";
  message: string;
  fix: string;
};

const TEST_EXT = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".py", ".go", ".rs", ".java", ".rb", ".php", ".cs", ".kt", ".swift"]);
const SKIP_DIRS = new Set([".git", "node_modules", "vendor", "venv", ".venv", "__pycache__", "dist", "build", "out", "target", "coverage"]);

export function isTestFile(p: string): boolean {
  const norm = p.split(path.sep).join("/").toLowerCase();
  if (!TEST_EXT.has(path.extname(norm))) return false;
  return (
    /(^|\/)(tests?|specs?|__tests__)\//.test(norm) ||
    /[._-](test|tests|spec|specs)\.[a-z]+$/.test(norm) ||
    /(^|\/)test_[^/]*\.py$/.test(norm) ||
    /[^/]*_test\.[a-z]+$/.test(norm) ||
    /[^/]*Test(s)?\.(java|kt|cs)$/i.test(norm)
  );
}

/** Count occurrences of a pattern, on code with strings and comments blanked. */
function count(code: string, re: RegExp): number {
  return [...code.matchAll(re)].length;
}

export function statsFor(file: string, raw: string): FileStats {
  const code = stripNoncode(raw, path.extname(file).toLowerCase());

  // Test declarations across the common runners. `it`/`test`/`describe` are matched only as
  // call heads so a variable named `test` does not count.
  const tests =
    count(code, /\b(?:it|test)\s*(?:\.\s*(?:each|concurrent|failing)\s*(?:\([^)]*\))?)?\s*\(/g) +
    count(code, /^\s*(?:async\s+)?def\s+test\w*\s*\(/gm) +
    count(code, /^\s*func\s+Test\w*\s*\(/gm) +
    count(code, /#\[\s*(?:tokio::)?test\s*\]/g) +
    count(code, /@Test\b/g) +
    count(code, /^\s*(?:it|specify)\s+['"]/gm);

  // The assert shapes are deliberately mutually exclusive. Overlapping patterns double-counted
  // `self.assertEqual(...)` as two assertions, which matters more than it sounds: this checker
  // compares counts across two refs, so a miscount that differs between refs invents drift that
  // isn't there, or hides drift that is.
  const assertions =
    count(code, /\bexpect\s*\(/g) +
    count(code, /(?<![.\w])assert\w*\s*\(/g) + // assert(, assertEqual(, assertThrows(
    count(code, /(?<![.\w])assert\w*\s*!/g) + // Rust: assert!, assert_eq!
    count(code, /(?<![.\w])assert\s+[^\s=]/g) + // Python/Rust bare: assert x == 1
    count(code, /(?<![.\w])assert\s*\.\s*\w+\s*\(/g) + // testify: assert.Equal(
    count(code, /\.\s*assert\w*\s*\(/g) + // self.assertEqual(, this.assertX(
    count(code, /\bshould\s*\.\s*\w+/g) +
    count(code, /\bt\s*\.\s*(?:Error|Errorf|Fatal|Fatalf)\s*\(/g) +
    count(code, /\brequire\s*\.\s*\w+\s*\(/g) +
    count(code, /\bverify\s*\(/g);

  const skips =
    count(code, /\b(?:it|test|describe|context)\s*\.\s*(?:skip|todo)\b/g) +
    count(code, /\bx(?:it|test|describe)\s*\(/g) +
    count(code, /@(?:pytest\.mark\.)?skip\w*/g) +
    count(code, /\bt\s*\.\s*Skip\s*\(/g) +
    count(code, /#\[\s*ignore\s*\]/g) +
    count(code, /@(?:Disabled|Ignore)\b/g) +
    count(code, /\bunittest\s*\.\s*skip/g);

  const focus = count(code, /\b(?:it|test|describe|context)\s*\.\s*only\b/g) + count(code, /\bf(?:it|test|describe)\s*\(/g);

  // The documented harness escape: leave the runner with a success code before anything runs.
  const earlyExit =
    count(code, /\bsys\s*\.\s*exit\s*\(\s*0\s*\)/g) +
    count(code, /\bprocess\s*\.\s*exit\s*\(\s*0\s*\)/g) +
    count(code, /\bos\s*\.\s*_?exit\s*\(\s*0\s*\)/g) +
    count(code, /\bpytest\s*\.\s*exit\s*\(/g);

  return { file, tests, assertions, skips, focus, earlyExit };
}

function git(args: string[], cwd: string): { ok: boolean; out: string } {
  const p = spawnSync("git", args, { cwd, encoding: "utf8" });
  return { ok: p.status === 0, out: p.stdout ?? "" };
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
      } else if (e.isFile() && isTestFile(full)) {
        yield full;
      }
    }
  }
}

function empty(): Totals {
  return { files: 0, tests: 0, assertions: 0, skips: 0, focus: 0 };
}

function add(t: Totals, s: FileStats): void {
  t.files += 1;
  t.tests += s.tests;
  t.assertions += s.assertions;
  t.skips += s.skips;
  t.focus += s.focus;
}

export function checkTestDrift(cwd: string, base: string, targets: string[]): Drift {
  const findings: Finding[] = [];
  const before = empty();
  const after = empty();

  if (!git(["rev-parse", "--git-dir"], cwd).ok) {
    return { base, findings, before, after, compared: 0, baseResolved: false };
  }
  // A base ref that does not exist used to yield an empty baseline, which reported "0 → 198
  // tests, the safety net grew" — a perfect score for a typo. A comparison that did not happen
  // must never read as a pass.
  if (!git(["rev-parse", "--verify", "--quiet", `${base}^{commit}`], cwd).ok) {
    return { base, findings, before, after, compared: 0, baseResolved: false };
  }

  // Test files as they exist NOW.
  const now = new Map<string, FileStats>();
  for (const t of targets) {
    let files: string[];
    try {
      files = fs.statSync(t).isDirectory() ? [...walk(t)] : isTestFile(t) ? [t] : [];
    } catch {
      files = [];
    }
    for (const f of files) {
      const rel = path.relative(cwd, path.resolve(f)) || f;
      try {
        const s = statsFor(rel, fs.readFileSync(f, "utf8"));
        now.set(rel, s);
        add(after, s);
      } catch {
        /* unreadable: falls through as a deletion if it existed at base */
      }
    }
  }

  // Test files as they existed at BASE — listed from git, so a file deleted entirely is seen.
  const listed = git(["ls-tree", "-r", "--name-only", base], cwd);
  const baseFiles = listed.out.split("\n").filter((f) => f !== "" && isTestFile(f));
  const wasThere = new Map<string, FileStats>();
  for (const rel of baseFiles) {
    const old = git(["show", `${base}:${rel}`], cwd);
    if (!old.ok) continue;
    const s = statsFor(rel, old.out);
    wasThere.set(rel, s);
    add(before, s);
  }

  // Suite-wide movement first: a file that moved is not a loss, and reporting it as one on every
  // rename would make this checker the boy who cried wolf.
  const suiteLostTests = before.tests - after.tests;
  const suiteLostAssertions = before.assertions - after.assertions;

  for (const [rel, was] of wasThere) {
    const isNow = now.get(rel);
    if (isNow === undefined) {
      if (was.tests === 0) continue;
      findings.push({
        file: rel,
        check: suiteLostTests > 0 ? "test-file-deleted" : "test-file-moved",
        // A move is P3 rather than P2 when the suite grew on BOTH counts: "the net is bigger than
        // it was" is strong evidence nothing was lost, and over a long history window this rule
        // otherwise fires once per rename. Measured on a real 30-commit range: 4 renames, 4
        // findings, zero of them a problem.
        severity: suiteLostTests > 0 ? "P1" : suiteLostAssertions <= 0 && after.tests > before.tests ? "P3" : "P2",
        message:
          suiteLostTests > 0
            ? `test file gone, and the suite lost ${suiteLostTests} test(s) overall — coverage was removed, not relocated`
            : "test file gone but the suite-wide test count held — looks relocated; confirm it really moved",
        fix: "restore it, or point at where the same cases now live",
      });
      continue;
    }
    if (isNow.tests < was.tests) {
      findings.push({
        file: rel,
        check: "tests-removed",
        severity: "P1",
        message: `${was.tests} test(s) → ${isNow.tests}: ${was.tests - isNow.tests} removed`,
        fix: "a refactor does not delete test cases; restore them, or state which behaviour was deliberately dropped and why",
      });
    }
    if (isNow.assertions < was.assertions) {
      findings.push({
        file: rel,
        check: "assertions-removed",
        severity: "P1",
        message: `${was.assertions} assertion(s) → ${isNow.assertions}: the tests may still run, but they check less`,
        fix: "restore the assertions — a passing test that asserts nothing is worse than a deleted one, because it looks like cover",
      });
    }
    if (isNow.skips > was.skips) {
      findings.push({
        file: rel,
        check: "skips-added",
        severity: "P1",
        message: `${isNow.skips - was.skips} test(s) newly skipped — a skipped test reports green while checking nothing`,
        fix: "fix the code so it passes, or delete the test deliberately with a stated reason; never park it as skipped to get a green run",
      });
    }
    if (isNow.focus > was.focus) {
      findings.push({
        file: rel,
        check: "focus-added",
        severity: "P2",
        message: `${isNow.focus - was.focus} focused test(s) added — .only silently stops the rest of the file from running`,
        fix: "remove the focus before this lands",
      });
    }
    if (isNow.earlyExit > was.earlyExit) {
      findings.push({
        file: rel,
        check: "early-exit-added",
        severity: "P1",
        message: "a zero-status exit was added inside the test suite — this leaves the harness successful without running the tests",
        fix: "remove it; this is the documented shortcut for faking a green run, and it must never survive review",
      });
    }
  }

  if (suiteLostAssertions > 0 && !findings.some((f) => f.check === "assertions-removed")) {
    findings.push({
      file: "(suite)",
      check: "suite-assertions-down",
      severity: "P1",
      message: `the suite lost ${suiteLostAssertions} assertion(s) overall even though no single file dropped — check moved or rewritten files`,
      fix: "account for every assertion that disappeared",
    });
  }

  return { base, findings, before, after, compared: wasThere.size, baseResolved: true };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      base: { type: "string", default: "HEAD" },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });

  const cwd = process.cwd();
  const d = checkTestDrift(cwd, values.base ?? "HEAD", positionals.length > 0 ? positionals : ["."]);

  if (!d.baseResolved) {
    if (values.json) console.log(JSON.stringify(d, null, 2));
    else {
      console.log(
        `cannot resolve base ref "${d.base}" (or this is not a git repository) — NOTHING was compared.\n` +
          "This is deliberately not a pass: an unresolvable ref used to report an empty baseline,\n" +
          "which looked like a suite that had grown from zero.",
      );
    }
    return 2;
  }
  if (d.compared === 0 && d.after.files === 0) {
    if (values.json) console.log(JSON.stringify(d, null, 2));
    else if (!values.quiet) console.log("no test files found at either ref — nothing to compare");
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify(d, null, 2));
    return d.findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of d.findings.sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`${f.file}  [${f.severity} ${f.check}]`);
    console.log(`    ${f.message}`);
    console.log(`    fix: ${f.fix}`);
  }

  if (!values.quiet) {
    console.log("");
    console.log(
      `vs ${d.base}: ${d.before.tests} → ${d.after.tests} tests, ` +
        `${d.before.assertions} → ${d.after.assertions} assertions, ` +
        `${d.before.skips} → ${d.after.skips} skipped, ${d.before.focus} → ${d.after.focus} focused`,
    );
    console.log(
      d.findings.length === 0
        ? "the safety net held or grew"
        : `${d.findings.length} sign(s) that the safety net shrank`,
    );
    console.log(
      "Counts only. An assertion weakened in place keeps the count and passes this check, and\n" +
        "nothing here was executed — so this proves the net was not cut, never that behaviour was\n" +
        "preserved. That is what the characterization and held-out suites are for. See\n" +
        "/viby-toolkit:adopt.",
    );
  }
  return d.findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
