/**
 * Contract tests for the safety-net differ.
 *
 * Run: node --experimental-strip-types --test tests/check-test-drift.test.ts
 *
 * Both halves pinned. The must-NOT half is the one that decides whether anyone leaves this
 * checker switched on: adding tests, splitting a test in two, reformatting, and moving a file
 * must all be silent, because those are what a good refactor looks like.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkTestDrift, isTestFile, statsFor } from "../plugins/viby-toolkit/skills/adopt/scripts/check-test-drift.ts";

function repo(before: Record<string, string>, after: Record<string, string | null>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "drift-"));
  const run = (...args: string[]): void => {
    spawnSync("git", args, { cwd: dir, encoding: "utf8" });
  };
  run("init", "-q");
  run("config", "user.email", "t@example.com");
  run("config", "user.name", "T");
  for (const [rel, body] of Object.entries(before)) {
    const full = path.join(dir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  run("add", "-A");
  run("commit", "-qm", "base");
  for (const [rel, body] of Object.entries(after)) {
    const full = path.join(dir, rel);
    if (body === null) {
      fs.rmSync(full, { force: true });
      continue;
    }
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  return dir;
}

function cleanup(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

function drift(before: Record<string, string>, after: Record<string, string | null>): string[] {
  const dir = repo(before, after);
  try {
    return checkTestDrift(dir, "HEAD", [dir]).findings.map((f) => f.check);
  } finally {
    cleanup(dir);
  }
}

const TWO_TESTS = [
  'test("adds", () => {',
  "  expect(add(1, 2)).toBe(3);",
  "});",
  'test("subtracts", () => {',
  "  expect(sub(3, 1)).toBe(2);",
  "});",
  "",
].join("\n");

test("deleting a test file that took coverage with it is P1", () => {
  const f = drift({ "a.test.ts": TWO_TESTS }, { "a.test.ts": null });
  assert.ok(f.includes("test-file-deleted"), f.join() || "none");
});

test("MOVING a test file is not reported as deleted coverage", () => {
  // The suite-wide count is unchanged, so this is a rename. Reporting it as lost coverage on
  // every reorganisation is how a checker gets switched off.
  const f = drift({ "a.test.ts": TWO_TESTS }, { "a.test.ts": null, "tests/a.test.ts": TWO_TESTS });
  assert.ok(!f.includes("test-file-deleted"), `a move must not be a deletion: ${f.join()}`);
  assert.ok(!f.includes("suite-assertions-down"), f.join());
});

test("removing one test case from a file is P1", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS },
    { "a.test.ts": 'test("adds", () => {\n  expect(add(1, 2)).toBe(3);\n});\n' },
  );
  assert.ok(f.includes("tests-removed"), f.join() || "none");
});

test("removing assertions while keeping the test is P1 — the worst case, because it looks like cover", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS },
    {
      "a.test.ts": ['test("adds", () => {', "  add(1, 2);", "});", 'test("subtracts", () => {', "  sub(3, 1);", "});", ""].join("\n"),
    },
  );
  assert.ok(f.includes("assertions-removed"), f.join() || "none");
});

test("adding a skip is P1", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS },
    { "a.test.ts": TWO_TESTS.replace('test("subtracts"', 'test.skip("subtracts"') },
  );
  assert.ok(f.includes("skips-added"), f.join() || "none");
});

test("adding .only is P2 — it silences the rest of the file", () => {
  const dir = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": TWO_TESTS.replace('test("adds"', 'test.only("adds"') });
  try {
    const hit = checkTestDrift(dir, "HEAD", [dir]).findings.find((f) => f.check === "focus-added");
    assert.ok(hit, "expected focus-added");
    assert.equal(hit.severity, "P2");
  } finally {
    cleanup(dir);
  }
});

test("inserting a zero-status exit into the suite is P1", () => {
  // The documented harness escape: leave the runner successful without running anything.
  const f = drift(
    { "test_a.py": "def test_adds():\n    assert add(1, 2) == 3\n" },
    { "test_a.py": "import sys\nsys.exit(0)\n\ndef test_adds():\n    assert add(1, 2) == 3\n" },
  );
  assert.ok(f.includes("early-exit-added"), f.join() || "none");
});

test("ADDING tests and assertions is silent", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS },
    { "a.test.ts": TWO_TESTS + 'test("multiplies", () => {\n  expect(mul(2, 3)).toBe(6);\n});\n' },
  );
  assert.deepEqual(f, [], `a growing suite must be silent, got ${f.join()}`);
});

test("splitting one test into two, keeping every assertion, is silent", () => {
  const f = drift(
    { "a.test.ts": 'test("maths", () => {\n  expect(add(1, 2)).toBe(3);\n  expect(sub(3, 1)).toBe(2);\n});\n' },
    { "a.test.ts": TWO_TESTS },
  );
  assert.deepEqual(f, [], `a split is a refactor, not a loss: ${f.join()}`);
});

test("reformatting without changing counts is silent", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS },
    { "a.test.ts": TWO_TESTS.replace(/\n/g, "\n\n") },
  );
  assert.deepEqual(f, [], f.join());
});

test("changes to non-test files are ignored entirely", () => {
  const f = drift(
    { "a.test.ts": TWO_TESTS, "src/a.ts": "export const add = (a: number, b: number) => a + b;\n" },
    { "src/a.ts": "export const add = (a: number, b: number) => b + a;\n" },
  );
  assert.deepEqual(f, [], f.join());
});

test("an assertion inside a string fixture or comment is not counted", () => {
  // Decide on code, never raw text — the rule this repo has re-learned four times.
  const withFixture = [
    "const FIXTURE = `",
    '  test("ghost", () => { expect(1).toBe(1); });',
    "`;",
    "// expect(this).toBe(ignored)",
    'test("real", () => {',
    "  expect(add(1, 2)).toBe(3);",
    "});",
    "",
  ].join("\n");
  const s = statsFor("a.test.ts", withFixture);
  assert.equal(s.tests, 1, "only the real test counts");
  assert.equal(s.assertions, 1, "only the real assertion counts");
});

test("Python unittest assertions are counted", () => {
  const s = statsFor("test_x.py", "class T(TestCase):\n    def test_a(self):\n        self.assertEqual(1, 1)\n        self.assertTrue(True)\n");
  assert.equal(s.tests, 1);
  assert.equal(s.assertions, 2);
});

test("each assert shape is counted exactly once", () => {
  // Regression: overlapping patterns counted `self.assertEqual(...)` twice. A miscount that
  // differs between two refs invents drift that isn't there, or hides drift that is.
  assert.equal(statsFor("test_a.py", "def test_a():\n    assert add(1, 2) == 3\n").assertions, 1, "bare python assert");
  assert.equal(statsFor("test_a.py", "def test_a():\n    self.assertEqual(1, 1)\n").assertions, 1, "unittest member assert");
  assert.equal(statsFor("a_test.go", "func TestA(t *testing.T) {\n\tassert.Equal(t, 1, 1)\n}\n").assertions, 1, "testify");
  assert.equal(statsFor("lib_test.rs", "#[test]\nfn a() {\n    assert!(x);\n    assert_eq!(1, 1);\n}\n").assertions, 2, "rust macros");
  assert.equal(statsFor("a.test.ts", 'test("a", () => {\n  expect(1).toBe(1);\n});\n').assertions, 1, "jest expect");
});

test("Go table tests and t.Fatal are counted", () => {
  const s = statsFor("x_test.go", "func TestAdd(t *testing.T) {\n\tif add(1,2) != 3 {\n\t\tt.Fatalf(\"bad\")\n\t}\n}\n");
  assert.equal(s.tests, 1);
  assert.equal(s.assertions, 1);
});

test("Rust #[test] and assert_eq! are counted, and #[ignore] reads as a skip", () => {
  const s = statsFor("lib_test.rs", "#[test]\n#[ignore]\nfn adds() {\n    assert_eq!(add(1,2), 3);\n}\n");
  assert.equal(s.tests, 1);
  assert.equal(s.assertions, 1);
  assert.equal(s.skips, 1);
});

test("a variable named test is not a test declaration", () => {
  const s = statsFor("a.test.ts", "const test = 1;\nconst other = test + 1;\n");
  assert.equal(s.tests, 0, "only call heads count");
});

test("test-file detection covers the common layouts and excludes source files", () => {
  for (const p of ["a.test.ts", "src/b.spec.js", "tests/c.ts", "test_d.py", "e_test.go", "FooTest.java", "__tests__/g.tsx"]) {
    assert.ok(isTestFile(p), `${p} should be a test file`);
  }
  for (const p of ["src/index.ts", "README.md", "latest.ts", "contest.py"]) {
    assert.ok(!isTestFile(p), `${p} must NOT be treated as a test file`);
  }
});

test("a non-git directory returns no findings rather than inventing them", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nogit-drift-"));
  try {
    fs.writeFileSync(path.join(dir, "a.test.ts"), TWO_TESTS);
    const d = checkTestDrift(dir, "HEAD", [dir]);
    assert.deepEqual(d.findings, []);
    assert.equal(d.compared, 0);
  } finally {
    cleanup(dir);
  }
});

test("an unresolvable base ref is exit 2, NOT a clean pass", () => {
  // Regression, caught by running the tool for real: a tag that did not exist yielded an empty
  // baseline, so the report read "0 → 198 tests, the safety net grew" — a perfect score for a
  // typo. A comparison that never happened must never read as a pass.
  const dir = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": TWO_TESTS });
  try {
    const d = checkTestDrift(dir, "v99.0.0-does-not-exist", [dir]);
    assert.equal(d.baseResolved, false);
    assert.equal(d.before.tests, 0, "nothing was read at base");

    const script = path.join(
      path.dirname(import.meta.dirname),
      "plugins", "viby-toolkit", "skills", "adopt", "scripts", "check-test-drift.ts",
    );
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, ".", "--base", "v99.0.0-does-not-exist"],
      { cwd: dir, encoding: "utf8" },
    );
    assert.equal(p.status, 2, `must be nothing-to-check, not success: ${p.stdout}`);
    assert.match(p.stdout ?? "", /NOTHING was compared/);
  } finally {
    cleanup(dir);
  }
});

test("CLI: a weakened suite exits 1, an intact one exits 0", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "adopt", "scripts", "check-test-drift.ts",
  );
  const run = (dir: string): { status: number | null; stdout: string } => {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, ".", "--quiet"],
      { cwd: dir, encoding: "utf8" },
    );
    return { status: p.status, stdout: p.stdout ?? "" };
  };

  const weakened = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": 'test("adds", () => {\n  expect(add(1,2)).toBe(3);\n});\n' });
  try {
    assert.equal(run(weakened).status, 1, "a shrunken suite must fail");
  } finally {
    cleanup(weakened);
  }

  const intact = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": TWO_TESTS + 'test("more", () => {\n  expect(1).toBe(1);\n});\n' });
  try {
    assert.equal(run(intact).status, 0, "a grown suite must pass");
  } finally {
    cleanup(intact);
  }
});

test("CLI: the report states that counts are not proof of preserved behaviour", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "adopt", "scripts", "check-test-drift.ts",
  );
  const dir = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": TWO_TESTS });
  try {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, "."],
      { cwd: dir, encoding: "utf8" },
    );
    assert.match(p.stdout ?? "", /never that behaviour was\s+preserved/);
    assert.match(p.stdout ?? "", /weakened in place/);
  } finally {
    cleanup(dir);
  }
});

test("a moved test file is P3 when the suite grew on both counts, P2 otherwise", () => {
  // Measured on a real 30-commit range: 4 renames produced 4 P2 findings, none of them a problem.
  // "The net is bigger than it was" is strong evidence nothing was lost.
  const grown = repo(
    { "a.test.ts": TWO_TESTS },
    { "a.test.ts": null, "tests/a.test.ts": TWO_TESTS + 'test("extra", () => {\n  expect(1).toBe(1);\n});\n' },
  );
  try {
    const hit = checkTestDrift(grown, "HEAD", [grown]).findings.find((f) => f.check === "test-file-moved");
    assert.ok(hit, "the move is still reported");
    assert.equal(hit.severity, "P3", "but at P3, because the suite grew");
  } finally {
    cleanup(grown);
  }

  const flat = repo({ "a.test.ts": TWO_TESTS }, { "a.test.ts": null, "tests/a.test.ts": TWO_TESTS });
  try {
    const hit = checkTestDrift(flat, "HEAD", [flat]).findings.find((f) => f.check === "test-file-moved");
    assert.equal(hit?.severity, "P2", "a move with no growth stays P2 — confirm it really moved");
  } finally {
    cleanup(flat);
  }
});
