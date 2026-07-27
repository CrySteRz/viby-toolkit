/**
 * Contract tests for the read-cost meter.
 *
 * Run: node --experimental-strip-types --test tests/measure-read-cost.test.ts
 *
 * Both halves pinned, as always: what must be counted, and what must NOT be. The second
 * half matters more here than usual — a baseline inflated by `node_modules` or a lockfile
 * makes every savings ratio computed against it a fiction, which is precisely the failure
 * mode /viby-toolkit:evaluate exists to prevent.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  estimateTokens,
  kindOf,
  measureReadCost,
} from "../plugins/viby-toolkit/skills/evaluate/scripts/measure-read-cost.ts";

const SCRIPT = path.join(
  path.dirname(import.meta.dirname),
  "plugins",
  "viby-toolkit",
  "skills",
  "evaluate",
  "scripts",
  "measure-read-cost.ts",
);

function tree(files: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "readcost-"));
  for (const [rel, body] of Object.entries(files)) {
    const full = path.join(dir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  return dir;
}

function cleanup(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

function runCli(args: string[]): { status: number | null; stdout: string; stderr: string } {
  const p = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", SCRIPT, ...args],
    { encoding: "utf8" },
  );
  return { status: p.status, stdout: p.stdout ?? "", stderr: p.stderr ?? "" };
}

test("an ASCII code file lands within the stated error bar of chars/ratio", () => {
  const body = "export function add(a: number, b: number): number {\n  return a + b;\n}\n".repeat(20);
  const est = estimateTokens(body, "code");
  const naive = body.length / 3.6;
  assert.ok(
    Math.abs(est - naive) / naive < 0.02,
    `the estimator must apply the documented ratio: got ${est}, ratio implies ${naive.toFixed(0)}`,
  );
});

test("prose and code of the same text cost different amounts — the ratio is per-kind", () => {
  const text = "The quick brown fox jumps over the lazy dog. ".repeat(50);
  assert.ok(
    estimateTokens(text, "prose") < estimateTokens(text, "code"),
    "prose tokenizes less densely than code, so the same characters must cost fewer tokens",
  );
});

test("non-ASCII content is charged at roughly one token per character", () => {
  const cjk = "設定を読み込む処理".repeat(30);
  const est = estimateTokens(cjk, "code");
  assert.ok(est >= cjk.length, `non-ASCII must not be divided by the ASCII ratio: got ${est} for ${cjk.length} chars`);
});

test("a directory is walked recursively and every source file counted", () => {
  const dir = tree({
    "src/a.ts": "const a = 1;\n".repeat(10),
    "src/deep/b.ts": "const b = 2;\n".repeat(10),
    "README.md": "hello\n".repeat(10),
  });
  try {
    const m = measureReadCost([dir]);
    assert.equal(m.files.length, 3, `expected 3 files, got ${m.files.map((f) => f.file).join(", ")}`);
    assert.ok(m.tokens > 0);
  } finally {
    cleanup(dir);
  }
});

test("node_modules is NOT counted — a baseline that includes it is worthless", () => {
  const dir = tree({
    "src/a.ts": "const a = 1;\n",
    "node_modules/big/index.js": "x".repeat(500_000),
    "dist/bundle.js": "y".repeat(500_000),
  });
  try {
    const m = measureReadCost([dir]);
    assert.deepEqual(
      m.files.map((f) => path.basename(f.file)),
      ["a.ts"],
      "only real source may enter the total",
    );
    assert.ok(m.tokens < 100, `total must not include vendored or built output, got ${m.tokens}`);
  } finally {
    cleanup(dir);
  }
});

test("a binary file is skipped with a reason, not counted as text", () => {
  const dir = tree({ "src/a.ts": "const a = 1;\n" });
  try {
    fs.writeFileSync(path.join(dir, "logo.png"), Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x01, 0x02, 0x03]));
    const m = measureReadCost([dir]);
    assert.equal(m.files.length, 1, "the binary must not be measured as text");
    assert.ok(
      m.skipped.some((s) => s.file.endsWith("logo.png") && s.reason === "binary"),
      `the skip must be reported, not silent: ${JSON.stringify(m.skipped)}`,
    );
  } finally {
    cleanup(dir);
  }
});

test("a lockfile is counted but classified as generated, and reported separately", () => {
  const dir = tree({
    "src/a.ts": "const a = 1;\n",
    "package-lock.json": JSON.stringify({ packages: Array.from({ length: 500 }, (_, i) => `p${i}`) }),
  });
  try {
    const m = measureReadCost([dir]);
    assert.equal(kindOf("package-lock.json"), "generated");
    assert.ok(m.generatedTokens > 0, "the lockfile's share must be broken out");
    assert.ok(m.generatedTokens < m.tokens, "it is part of the total, just attributed");
  } finally {
    cleanup(dir);
  }
});

test("a minified file carries a caveat instead of a silent estimate", () => {
  const dir = tree({ "app.min.js": `const x=${"1+".repeat(400)}1;` });
  try {
    const m = measureReadCost([dir]);
    assert.equal(m.files.length, 1);
    assert.match(m.files[0]?.caveat ?? "", /longest line/, "a long-line file must declare that its estimate is weaker");
  } finally {
    cleanup(dir);
  }
});

test("an ordinary short-lined source file carries NO caveat", () => {
  // The must-not half of the caveat rule: warning on normal code would train the reader to
  // ignore the warning that matters.
  const dir = tree({ "src/a.ts": "const a = 1;\nconst b = 2;\n" });
  try {
    const m = measureReadCost([dir]);
    assert.equal(m.files[0]?.caveat, undefined, `normal code must not be flagged: ${m.files[0]?.caveat}`);
  } finally {
    cleanup(dir);
  }
});

test("repeat multiplies the total — cadence is charged, not just payload", () => {
  const dir = tree({ "src/a.ts": "const a = 1;\n".repeat(100) });
  try {
    const once = measureReadCost([dir]);
    const thrice = measureReadCost([dir], { repeat: 3 });
    assert.equal(thrice.tokens, once.tokens, "one pass is unchanged");
    assert.equal(thrice.totalTokens, once.tokens * 3, "three passes cost three times as much");
  } finally {
    cleanup(dir);
  }
});

test("a path that does not exist is recorded as skipped, never silently ignored", () => {
  const m = measureReadCost([path.join(os.tmpdir(), "definitely-not-here-9f3a")]);
  assert.equal(m.files.length, 0);
  assert.ok(m.skipped.some((s) => s.reason === "does not exist"), JSON.stringify(m.skipped));
});

test("an empty file is measured at zero rather than dropped", () => {
  const dir = tree({ "src/empty.ts": "" });
  try {
    const m = measureReadCost([dir]);
    assert.equal(m.files.length, 1, "an empty file still exists and still gets read");
    assert.equal(m.files[0]?.tokens, 0);
  } finally {
    cleanup(dir);
  }
});

test("CLI: nothing measurable exits 2, distinct from a clean measurement", () => {
  const r = runCli([path.join(os.tmpdir(), "definitely-not-here-9f3a"), "--quiet"]);
  assert.equal(r.status, 2, `expected 2 (nothing to measure), got ${r.status}: ${r.stdout}${r.stderr}`);
});

test("CLI: --budget below the total exits 1 and says what to do instead", () => {
  const dir = tree({ "src/a.ts": "const a = 1;\n".repeat(2000) });
  try {
    const over = runCli([dir, "--budget", "100"]);
    assert.equal(over.status, 1, `over budget must fail: ${over.stdout}${over.stderr}`);
    assert.match(over.stdout, /over budget/);
    assert.match(over.stdout, /subagent/, "an over-budget verdict must name the alternative, not just refuse");

    const under = runCli([dir, "--budget", "10000000"]);
    assert.equal(under.status, 0, `within budget must pass: ${under.stdout}${under.stderr}`);
    assert.match(under.stdout, /within budget/);
  } finally {
    cleanup(dir);
  }
});

test("CLI: no budget given means measure-and-report, exit 0", () => {
  const dir = tree({ "src/a.ts": "const a = 1;\n".repeat(500) });
  try {
    const r = runCli([dir]);
    assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
    assert.match(r.stdout, /tokens to read once/);
  } finally {
    cleanup(dir);
  }
});

test("CLI: the estimate always ships with its error bar", () => {
  // A cheap estimate that hides its uncertainty is the fast-and-wrong answer this whole
  // skill exists to prevent. The disclaimer is part of the contract, not decoration.
  const dir = tree({ "src/a.ts": "const a = 1;\n" });
  try {
    const r = runCli([dir]);
    assert.match(r.stdout, /Estimate, not a tokenizer/);
  } finally {
    cleanup(dir);
  }
});

test("CLI: --json emits the same numbers as the text report", () => {
  const dir = tree({ "src/a.ts": "const a = 1;\n".repeat(300), "docs/x.md": "words ".repeat(300) });
  try {
    const r = runCli([dir, "--json"]);
    assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
    const parsed = JSON.parse(r.stdout) as { tokens: number; files: unknown[] };
    const direct = measureReadCost([dir]);
    assert.equal(parsed.tokens, direct.tokens, "the JSON and library paths must not diverge");
    assert.equal(parsed.files.length, 2);
  } finally {
    cleanup(dir);
  }
});

test("CLI: the biggest contributor is listed first, so trimming has a target", () => {
  const dir = tree({
    "small.ts": "const a = 1;\n",
    "huge.ts": "const b = 2;\n".repeat(400),
  });
  try {
    const r = runCli([dir, "--quiet"]);
    const huge = r.stdout.indexOf("huge.ts");
    const small = r.stdout.indexOf("small.ts");
    assert.ok(huge !== -1 && small !== -1, r.stdout);
    assert.ok(huge < small, `contributors must be ranked by cost, got:\n${r.stdout}`);
  } finally {
    cleanup(dir);
  }
});
