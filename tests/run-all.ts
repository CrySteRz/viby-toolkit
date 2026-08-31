#!/usr/bin/env -S node --experimental-strip-types
/**
 * Run every viby-toolkit check and report a single verdict.
 *
 * Run:  node --experimental-strip-types --disable-warning=ExperimentalWarning tests/run-all.ts
 * Exit: 0 = everything passed, 1 = at least one check failed, with its output shown.
 *
 * This is the pre-push gate. The toolkit preaches evidence-gated completion, so it
 * holds itself to it: manifests validate, and every executable hook and script has a
 * contract test that pins both what it must do and what it must NOT do.
 */
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, delimiter } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const NODE_RUNNER = [
  process.execPath,
  "--experimental-strip-types",
  "--disable-warning=ExperimentalWarning",
];

type OkCodes = Set<number>;

type Check = {
  name: string;
  cmd: string[];
  ok: OkCodes;
  /** When present and returning false, the check is skipped instead of run. */
  skipUnless?: () => boolean;
  /** Printed alongside a skip so the reason is never a mystery. */
  skipNote?: string;
  /**
   * Minimum passing tests the suite must report. Guards against a gutted or emptied test
   * file still exiting 0. Raise it when you add cases; never lower it to make a run green.
   */
  minPassing?: number;
};

function which(name: string): boolean {
  const pathEnv = process.env.PATH || "";
  return pathEnv.split(delimiter).some((dir) => dir && existsSync(join(dir, name)));
}

const SCANNER = join("plugins", "viby-toolkit", "skills", "test", "scripts", "scan-test-quality.ts");

const CHECKS: Check[] = [
  { name: "plugin manifests", cmd: ["claude", "plugin", "validate", "."], ok: new Set([0]) },
  {
    name: "statusline contract",
    cmd: [...NODE_RUNNER, "--test", "tests/statusline.test.ts"],
    ok: new Set([0]),
    minPassing: 13,
  },
  {
    name: "test-scanner contract",
    cmd: [...NODE_RUNNER, "--test", "tests/scanner.test.ts"],
    ok: new Set([0]),
    minPassing: 65,
  },
  {
    name: "release-preflight contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-release.test.ts"],
    ok: new Set([0]),
    minPassing: 20,
  },
  {
    name: "skill-library health",
    cmd: [...NODE_RUNNER, "--test", "tests/check-skills.test.ts"],
    ok: new Set([0]),
    minPassing: 19,
  },
  {
    name: "migration-linter contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-migration.test.ts"],
    ok: new Set([0]),
    minPassing: 19,
  },
  {
    name: "stack-detector contract",
    cmd: [...NODE_RUNNER, "--test", "tests/detect-stack.test.ts"],
    ok: new Set([0]),
    minPassing: 14,
  },
  {
    name: "read-cost-meter contract",
    cmd: [...NODE_RUNNER, "--test", "tests/measure-read-cost.test.ts"],
    ok: new Set([0]),
    minPassing: 18,
  },
  {
    name: "api-surface-differ contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-api-surface.test.ts"],
    ok: new Set([0]),
    minPassing: 20,
  },
  {
    name: "docs-auditor contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-docs.test.ts"],
    ok: new Set([0]),
    minPassing: 14,
  },
  {
    name: "logging-auditor contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-logging.test.ts"],
    ok: new Set([0]),
    minPassing: 17,
  },
  {
    name: "memory-auditor contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-memory.test.ts"],
    ok: new Set([0]),
    minPassing: 18,
  },
  {
    name: "plan-validator contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-plan.test.ts"],
    ok: new Set([0]),
    minPassing: 16,
  },
  {
    name: "skill-safety contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-skill-safety.test.ts"],
    ok: new Set([0]),
    minPassing: 17,
  },
  {
    name: "diff-hygiene contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-diff-hygiene.test.ts"],
    ok: new Set([0]),
    minPassing: 31,
  },
  {
    name: "study-auditor contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-study.test.ts"],
    ok: new Set([0]),
    minPassing: 37,
  },
  {
    name: "analytics-sql contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-analytics-sql.test.ts"],
    ok: new Set([0]),
    minPassing: 25,
  },
  {
    name: "test-drift contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-test-drift.test.ts"],
    ok: new Set([0]),
    minPassing: 23,
  },
  {
    name: "runner-shim contract",
    cmd: [...NODE_RUNNER, "--test", "tests/run-shim.test.ts"],
    ok: new Set([0]),
    minPassing: 6,
  },
  {
    name: "delegation-defaults contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-orchestration.test.ts"],
    ok: new Set([0]),
    minPassing: 23,
  },
  {
    name: "hook-wiring contract",
    cmd: [...NODE_RUNNER, "--test", "tests/check-hooks.test.ts"],
    ok: new Set([0]),
    minPassing: 32,
  },
  {
    name: "agent-contract checker",
    cmd: [...NODE_RUNNER, "--test", "tests/check-agents.test.ts"],
    ok: new Set([0]),
    minPassing: 17,
  },
  // The toolkit's own test files must survive its own auditor. 0 = clean, 2 = nothing
  // to scan; 1 (findings) is a failure here because we dogfood a clean suite.
  {
    name: "self-audit (scanner on own tests)",
    cmd: [...NODE_RUNNER, SCANNER, "--all", "--quiet"],
    ok: new Set([0, 2]),
  },
  // Uses the locally installed tsc. TypeScript is a dev-only dependency — nothing here
  // is needed to RUN the plugin — so when node_modules is absent this is skipped rather
  // than failed, and a fresh clone still passes every other check.
  {
    name: "typecheck",
    cmd: [join("node_modules", ".bin", "tsc"), "--noEmit"],
    ok: new Set([0]),
    skipUnless: () => existsSync(join(ROOT, "node_modules", ".bin", "tsc")),
    skipNote: "run `npm install` to enable typecheck (dev-only dependency)",
  },
  {
    name: "manifest version is not behind the newest tag",
    cmd: [...NODE_RUNNER, "tests/check-version.ts"],
    ok: new Set([0]),
  },
  // The lexical routing proxy that used to gate here is GONE, deliberately. Routing is now measured
  // for real by `tests/routing/` — a fresh `claude -p` per run against the installed listing, with the
  // outcome read from the actual Skill tool call. That is not a gate (it costs real time and tokens),
  // so routing is measured on demand and recorded in `tests/routing/README.md`.
  //
  // The proxy was retired rather than kept alongside, because it was measurably misleading: it ranked
  // `secure` as the threat to `review`, when the real competitor turned out to be Claude
  // Code's built-in `code-review` skill — which a similarity check over this library's own 31
  // descriptions cannot see at all. A gate that produces confident wrong guidance is worse than no
  // gate, which is the same rule applied to `listing-over-budget` above.
  // Catches the class of bug no other check can see: an instruction that points at a skill
  // Claude cannot invoke, or a path built from a variable that is empty in this context.
  {
    name: "cross-references resolve",
    cmd: [...NODE_RUNNER, "tests/check-references.ts"],
    ok: new Set([0]),
  },
  {
    name: "SessionStart emits valid JSON",
    cmd: [
      "sh",
      "-c",
      `sh plugins/viby-toolkit/hooks/session-start.sh | ${process.execPath} -e "JSON.parse(require('fs').readFileSync(0,'utf8'))"`,
    ],
    ok: new Set([0]),
  },
];

type Result = {
  name: string;
  status: "pass" | "FAIL" | "skip";
  output: string;
  code: number | null;
};

function runCheck(check: Check): Result {
  const p = spawnSync(check.cmd[0]!, check.cmd.slice(1), {
    cwd: ROOT,
    encoding: "utf8",
  });
  const code = p.status;
  const out = (p.stdout || "") + (p.stderr || "");

  // Exit 0 alone is a weak proof for a test suite: `node --test` exits 0 for a file whose
  // tests assert nothing, and also for a file that was emptied or lost its cases. A gate
  // that says "safe to push" must not go green on a suite that stopped testing anything,
  // so assert the suite still reports at least the number of passing tests we expect.
  if (check.minPassing !== undefined && code !== null && check.ok.has(code)) {
    // ⚠️ BOTH REPORTER FORMATS, because this gate silently inverted once already. `node --test`
    // emitted `# pass N` (TAP) through Node 22; Node 23+ switched the default reporter to `spec`,
    // which emits `\u2139 pass N`. The regex matched only the old form, so on a current Node EVERY
    // suite here reported "no '# pass N' line at all" and 21 checks failed while `npm test` itself
    // was green 479/479 — a gate that fails closed on its own parser is noise, and noise is how a
    // real lost-cases regression gets waved through.
    const m = /^(?:#|\u2139) pass (\d+)$/m.exec(out);
    const passed = m?.[1] !== undefined ? Number(m[1]) : -1;
    if (passed < check.minPassing) {
      return {
        name: check.name,
        status: "FAIL",
        output:
          `expected at least ${check.minPassing} passing tests, saw ` +
          `${passed < 0 ? "no 'pass N' line at all" : passed} — the suite lost cases, ` +
          `or its output format changed\n\n${out}`,
        code,
      };
    }
  }

  if (code !== null && check.ok.has(code)) {
    return { name: check.name, status: "pass", output: "", code };
  }
  return { name: check.name, status: "FAIL", output: out, code };
}

function runRunnerShimCheck(): Result {
  const name = "runner shim resolves a runtime";
  const dir = mkdtempSync(join(tmpdir(), "viby-run-all-"));
  const script = join(dir, "sentinel.ts");
  const sentinel = "VIBY_RUN_ALL_SENTINEL_OK";
  try {
    // A type annotation, deliberately: a sentinel of plain JS would pass under a runtime that
    // cannot strip types at all, which is the exact failure this check exists to catch.
    writeFileSync(script, `const ok: string = ${JSON.stringify(sentinel)};\nconsole.log(ok);\n`);
    const p = spawnSync("sh", [join(ROOT, "plugins", "viby-toolkit", "hooks", "run.sh"), script], {
      cwd: ROOT,
      encoding: "utf8",
    });
    const out = (p.stdout || "") + (p.stderr || "");
    if (p.status === 0 && out.includes(sentinel)) {
      return { name, status: "pass", output: "", code: p.status };
    }
    return { name, status: "FAIL", output: out, code: p.status };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function main(): number {
  if (!which("claude")) {
    console.log("note: `claude` CLI not on PATH — skipping manifest validation\n");
  }

  const results: Result[] = [];
  const failed: Result[] = [];

  for (const check of CHECKS) {
    let result: Result;
    const missingTool = check.cmd[0] === "claude" && !which("claude");
    const skipped = check.skipUnless !== undefined && !check.skipUnless();
    if (missingTool || skipped) {
      result = { name: check.name, status: "skip", output: "", code: null };
    } else {
      // Deliberately no leniency here: every check now runs a locally available tool, so
      // a failure is a real failure. Nothing gets downgraded to "skip" after the fact.
      result = runCheck(check);
    }
    results.push(result);
    if (result.status === "FAIL") failed.push(result);
    const note = result.status === "skip" && check.skipNote ? `  (${check.skipNote})` : "";
    console.log(`${result.status.padStart(4)}  ${result.name}${note}`);
  }

  const shimResult = runRunnerShimCheck();
  results.push(shimResult);
  if (shimResult.status === "FAIL") failed.push(shimResult);
  console.log(`${shimResult.status.padStart(4)}  ${shimResult.name}`);

  console.log("\n" + "─".repeat(74));
  if (failed.length > 0) {
    console.log(`✗ ${failed.length} check(s) failed\n`);
    for (const r of failed) {
      console.log(`── ${r.name} (exit ${r.code}) ` + "─".repeat(Math.max(0, 50 - r.name.length)));
      const tail = r.output.trimEnd().slice(-2000);
      console.log(tail || "(no output)");
      console.log();
    }
    return 1;
  }

  const passed = results.filter((r) => r.status === "pass").length;
  const skipped = results.filter((r) => r.status === "skip").length;
  const tail = skipped ? ` (${skipped} skipped)` : "";
  console.log(`✓ all ${passed} check(s) passed${tail} — safe to commit and push`);
  return 0;
}

process.exit(main());
