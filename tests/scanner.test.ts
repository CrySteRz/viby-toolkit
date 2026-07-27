/**
 * Contract tests for the viby-code test-quality scanner.
 *
 * Run: node --experimental-strip-types --test tests/scanner.test.ts
 *
 * Each case is a synthetic test file written to a temp dir and scanned. Both halves
 * of the contract are pinned: snippets that MUST produce a given check, and
 * good-practice snippets that must produce NOTHING. A scanner that flags healthy
 * tests gets ignored, which costs more than it ever saves.
 *
 * assertion-roulette and magic-number checks are deliberately not implemented (see
 * scan-test-quality.ts) — cases that would have exercised them are pinned to expect
 * nothing instead.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isTestFile } from "../plugins/viby-code/skills/test/scripts/scan-test-quality.ts";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.dirname(__dirname);
const SCANNER = path.join(ROOT, "plugins", "viby-code", "skills", "test", "scripts", "scan-test-quality.ts");

type Case = [name: string, filename: string, source: string, expected: string[]];

// (name, filename, source, expected_checks) — expected is the exact set of check
// names the scanner must report. Empty array = must be silent.
const CASES: Case[] = [
  // ---------------------------------------------------------- no-assertion
  [
    "py: test with no assertion",
    "test_a.py",
    `
def test_creates_user():
    user = make_user("ana")
    save(user)
`,
    ["no-assertion"],
  ],

  [
    "py: test with assertion is clean",
    "test_b.py",
    `
def test_creates_user():
    user = make_user("ana")
    save(user)
    assert user.id is not None
`,
    [],
  ],

  [
    "ts: it() with no expect",
    "a.test.ts",
    `
it("creates a user", async () => {
  const user = await makeUser("ana");
  await save(user);
});
`,
    ["no-assertion"],
  ],

  [
    // node:test + node:assert style. The scanner must recognise `assert.<method>(...)`,
    // not just bare `assert x` / `assertEqual(...)` — this is the style the toolkit's own
    // test suite uses, and missing it made the scanner flag its own tests.
    "ts: node:assert style counts as an assertion",
    "n.test.ts",
    `
test("exit code is tolerated", () => {
  const result = spawnSync(process.execPath, [SCANNER, "--all"], { encoding: "utf8" });
  const allowed = [0, 1, 2];
  assert.ok(allowed.includes(result.status), "unexpected exit code");
});
`,
    [],
  ],

  [
    "ts: node:assert deepStrictEqual counts as an assertion",
    "o.test.ts",
    `
test("shapes match", () => {
  const got = collect();
  const want = { a: 1, b: 2 };
  assert.deepStrictEqual(got, want);
});
`,
    [],
  ],

  [
    "ts: it() with expect is clean",
    "b.test.ts",
    `
it("creates a user", async () => {
  const user = await makeUser("ana");
  expect(user.id).toBeDefined();
});
`,
    [],
  ],

  [
    "go: test with t.Fatal is clean",
    "x_test.go",
    `
func TestCreatesUser(t *testing.T) {
	u, err := MakeUser("ana")
	if err != nil {
		t.Fatalf("MakeUser failed: %v", err)
	}
	_ = u
}
`,
    [],
  ],

  [
    "ts: single-line smoke test is clean",
    "smoke.test.ts",
    'it("does not throw", () => { doThing(); });\n',
    [],
  ],

  // ---------------------------------------------------------- tautology
  [
    "py: assertTrue(True)",
    "test_c.py",
    `
def test_placeholder():
    result = compute()
    assertTrue(True)
`,
    ["tautology"],
  ],

  [
    "ts: expect(true).toBe(true)",
    "c.test.ts",
    `
it("works", () => {
  doThing();
  expect(true).toBe(true);
});
`,
    ["tautology"],
  ],

  [
    "ts: expect(x).toBe(x) self-comparison",
    "d.test.ts",
    `
it("works", () => {
  const total = compute();
  expect(total).toBe(total);
});
`,
    ["tautology"],
  ],

  [
    "py: real equality assertion is clean",
    "test_d.py",
    `
def test_total():
    total = compute()
    assert total == 42
`,
    [],
  ],

  // ---------------------------------------------------------- focused / skipped
  [
    "ts: describe.only left in",
    "e.test.ts",
    `
describe.only("payments", () => {
  it("charges", () => {
    expect(charge(100)).toBe(100);
  });
});
`,
    ["focused-or-skipped"],
  ],

  [
    "ts: it.skip left in",
    "f.test.ts",
    `
it.skip("charges the card", () => {
  expect(charge(100)).toBe(100);
});
`,
    ["focused-or-skipped"],
  ],

  [
    "py: pytest skip mark",
    "test_e.py",
    `
@pytest.mark.skip(reason="flaky")
def test_charges():
    assert charge(100) == 100
`,
    ["focused-or-skipped"],
  ],

  [
    "go: t.Skip",
    "y_test.go",
    `
func TestCharges(t *testing.T) {
	t.Skip("not ready")
	if Charge(100) != 100 {
		t.Fatalf("wrong")
	}
}
`,
    ["focused-or-skipped"],
  ],

  [
    "ts: ordinary describe/it is clean",
    "g.test.ts",
    `
describe("payments", () => {
  it("charges", () => {
    expect(charge(100)).toBe(100);
  });
});
`,
    [],
  ],

  // ---------------------------------------------------------- sleep-wait
  [
    "py: time.sleep in test",
    "test_f.py",
    `
def test_eventually_ready():
    start_server()
    time.sleep(2)
    assert is_ready()
`,
    ["sleep-wait"],
  ],

  [
    "ts: setTimeout promise wait",
    "h.test.ts",
    `
it("becomes ready", async () => {
  startServer();
  await new Promise((r) => setTimeout(r, 2000));
  expect(isReady()).toBe(true);
});
`,
    ["sleep-wait"],
  ],

  [
    "ts: setTimeout with fake timers is clean",
    "i.test.ts",
    `
beforeEach(() => { jest.useFakeTimers(); });
it("debounces", () => {
  const fn = debounce(cb, 100);
  fn();
  setTimeout(() => {}, 100);
  expect(cb).toHaveBeenCalled();
});
`,
    [],
  ],

  [
    "py: polling for a condition is clean",
    "test_g.py",
    `
def test_eventually_ready():
    start_server()
    wait_until(lambda: is_ready(), timeout=5)
    assert is_ready()
`,
    [],
  ],

  [
    // `eq = self.assertEqual` then `eq(a, b)` — the assertion-alias idiom. Without this,
    // every test in suites that use it (CPython does, throughout) looks assertion-free.
    "py: assertion bound to a local alias counts as an assertion",
    "test_alias.py",
    `
def test_percents():
    eq = self.assertEqual
    s = Template("%(foo)s $foo")
    d = dict(foo="baz")
    eq(s.substitute(d), "%(foo)s baz")
`,
    [],
  ],

  [
    // `self.fail(...)` is how you assert "we should not have reached here".
    "py: self.fail in an except branch counts as an assertion",
    "test_fail.py",
    `
def test_map_chunksize():
    try:
        self.pool.map_async(sqr, [], chunksize=1).get(timeout=1)
    except TimeoutError:
        self.fail("map_async stalled on a null list")
`,
    [],
  ],

  [
    // Transitive delegation: the helper the test calls does not assert directly, but the
    // helper IT calls does. Resolved to a fixpoint.
    "py: transitive helper chain counts as delegation",
    "test_chain.py",
    `
def _really_check(x):
    assert x > 0

def check_value(x):
    _really_check(x)

def test_uses_chain():
    v = compute()
    save(v)
    check_value(v)
`,
    [],
  ],

  [
    // Regression: a quote inside a regex literal used to open "string mode", blanking the
    // rest of the line and erasing the real assertion after it.
    "ts: regex literal containing a quote does not erase the line",
    "regex.test.ts",
    `
it("validates format", () => {
  const ok = /['"]/.test(x);
  expect(ok).toBe(true);
});
`,
    [],
  ],

  [
    "ts: regex literal with a quote, assertion on the same line",
    "regex2.test.ts",
    `
it("validates format", () => {
  const ok = /["']/.test(x); expect(ok).toBe(true);
});
`,
    [],
  ],

  [
    // Regression: delegation was keyed on the helper's NAME only, so a test calling an
    // unrelated `cache.get(...)` was excused because some fixture class defined `get`.
    "py: unrelated local name does not excuse a missing assertion",
    "test_deleg.py",
    `
class FakeCache:
    def get(self, key):
        return self.data.get(key)


def test_creates_user():
    user = make_user("ana")
    cache.get("some_key")
    log_event(user)
`,
    ["no-assertion"],
  ],

  [
    "py: delegating to a helper that really asserts is clean",
    "test_deleg2.py",
    `
def check_user(user):
    assert user.id is not None
    assert user.name


def test_creates_user():
    user = make_user("ana")
    save(user)
    check_user(user)
`,
    [],
  ],

  [
    // Regression: fake timers anywhere in the file used to suppress sleep-wait everywhere,
    // hiding a genuine flaky wait in an unrelated suite.
    "ts: fake timers in one suite do not excuse a real wait in another",
    "mixed-timers.test.ts",
    `
describe("debounce", () => {
  beforeEach(() => { jest.useFakeTimers(); });
  it("debounces", () => {
    const fn = debounce(cb, 100);
    setTimeout(() => {}, 100);
    expect(cb).toHaveBeenCalled();
  });
});

describe("server readiness", () => {
  it("becomes ready eventually", async () => {
    startServer();
    await new Promise((r) => setTimeout(r, 2000));
    expect(isReady()).toBe(true);
  });
});
`,
    ["sleep-wait"],
  ],

  [
    "py: freeze_time does not excuse a blocking time.sleep",
    "test_freeze.py",
    `
def test_a():
    freeze_time("2020-01-01")
    assert now() == "2020-01-01"

def test_b():
    start_server()
    time.sleep(5)
    assert is_ready()
`,
    ["sleep-wait"],
  ],

  [
    "ts: template literal mentioning time.sleep is not flagged",
    "k.test.ts",
    "it(\"builds a message\", () => {\n  const msg = `retrying, use time.sleep(2) if flaky`;\n  expect(msg).toContain(\"retrying\");\n});\n",
    [],
  ],

  [
    "ts: multi-line template literal mentioning banned patterns is not flagged",
    "l.test.ts",
    `
it("documents the banned patterns", () => {
  const help = \`
    Do not write:
      it.skip("...", () => {});
      time.sleep(2);
      expect(true).toBe(true);
  \`;
  expect(help).toContain("Do not write");
});
`,
    [],
  ],

  // ---------------------------------------------------------- swallowed-error
  [
    "py: except pass in test",
    "test_h.py",
    `
def test_handles_bad_input():
    try:
        parse("garbage")
    except ValueError:
        pass
    assert True
`,
    ["swallowed-error", "tautology"],
  ],

  [
    "ts: empty catch block",
    "j.test.ts",
    `
it("handles bad input", () => {
  try {
    parse("garbage");
  } catch (e) {}
  expect(state()).toBe("clean");
});
`,
    ["swallowed-error"],
  ],

  [
    "py: pytest.raises is clean",
    "test_i.py",
    `
def test_handles_bad_input():
    with pytest.raises(ValueError):
        parse("garbage")
`,
    [],
  ],

  // ---------------------------------------------------------- over-mocking
  [
    "py: mock-heavy test",
    "test_j.py",
    `
def test_checkout_flow():
    gateway = MagicMock()
    mailer = MagicMock()
    ledger = MagicMock()
    inventory = MagicMock()
    audit = MagicMock()
    checkout(gateway, mailer, ledger, inventory, audit)
    assert gateway.charge.called
`,
    ["over-mocking"],
  ],

  [
    "py: two mocks and real assertions is clean",
    "test_k.py",
    `
def test_checkout_charges_once():
    gateway = MagicMock()
    order = build_order(total=100)
    result = checkout(order, gateway)
    assert result.status == "paid"
    assert result.total == 100
    assert gateway.charge.call_count == 1
`,
    [],
  ],

  // ------------------------------------------------- assertion-roulette (not implemented)
  [
    "py: many bare assertions is not flagged (assertion-roulette deliberately removed)",
    "test_l.py",
    `
def test_user_shape():
    u = make_user("ana")
    assert u.name == "ana"
    assert u.email is not None
    assert u.id > 0
    assert u.active is True
    assert u.role == "member"
`,
    [],
  ],

  [
    "py: assertions with messages are clean",
    "test_m.py",
    `
def test_user_shape():
    u = make_user("ana")
    assert u.name == "ana", "name should round-trip"
    assert u.email is not None, "email must be generated"
    assert u.id > 0, "id must be assigned by the store"
    assert u.active is True, "new users start active"
    assert u.role == "member", "default role is member"
`,
    [],
  ],

  [
    "py: three bare assertions is under threshold",
    "test_n.py",
    `
def test_user_shape():
    u = make_user("ana")
    assert u.name == "ana"
    assert u.email is not None
    assert u.id > 0
`,
    [],
  ],

  // ---------------------------------------------------------- non-test files
  [
    "non-test file is not scanned",
    "helpers.py",
    `
def build():
    try:
        risky()
    except Exception:
        pass
`,
    [],
  ],

  // ---------------------------------------------------------- comments
  [
    "commented-out smell is ignored",
    "test_o.py",
    `
def test_real():
    # time.sleep(5)  -- removed, we poll now
    # assertTrue(True)
    wait_until(is_ready)
    assert is_ready()
`,
    [],
  ],

];

// (path, is_test) — which files the scanner considers tests. Load-bearing: the
// checks are meaningless on production or support code.
const DETECTION_CASES: Array<[string, boolean]> = [
  ["test_auth.py", true],
  ["auth_test.py", true],
  ["src/auth.test.ts", true],
  ["src/auth.spec.tsx", true],
  ["pkg/auth_test.go", true],
  ["src/AuthTest.java", true],
  ["spec/auth_spec.rb", true],
  ["tests/user_flows.py", true], // unconventional name, but in tests/
  ["tests/utils/parse_test.go", true], // filename wins over the utils/ dir
  ["src/auth.py", false],
  ["src/authenticate.ts", false],
  ["tests/scripts/run_all.py", false], // support code under tests/
  ["tests/fixtures/sample.py", false],
  ["tests/__mocks__/api.ts", false],
  ["skills/test/scripts/scan.py", false], // the bug this pins: a tool, not a test
  ["testdata/golden.go", false],
];

test("file detection", () => {
  for (const [p, want] of DETECTION_CASES) {
    assert.equal(isTestFile(p), want, `is_test_file(${p})`);
  }
});

type ScanResult = { checks: Set<string> | null; status: number | null; stdout: string; stderr: string };

function runScanner(paths: string[]): ScanResult {
  const result = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", SCANNER, "--json", ...paths],
    { encoding: "utf8", cwd: ROOT },
  );
  if (result.status === 2) {
    return { checks: new Set(), status: result.status, stdout: result.stdout, stderr: result.stderr };
  }
  try {
    const data = JSON.parse(result.stdout) as { findings: Array<{ check: string }> };
    return {
      checks: new Set(data.findings.map((f) => f.check)),
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr,
    };
  } catch {
    return { checks: null, status: result.status, stdout: result.stdout, stderr: result.stderr };
  }
}

for (const [name, filename, source, expected] of CASES) {
  test(name, () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "scanner-test-"));
    try {
      const file = path.join(dir, filename);
      fs.writeFileSync(file, source.replace(/^\n/, ""));
      const { checks, stdout, stderr } = runScanner([file]);
      assert.notEqual(checks, null, `scanner did not emit valid JSON: ${stdout.slice(0, 200)} ${stderr.slice(0, 200)}`);
      const got = [...(checks ?? new Set<string>())].sort();
      const want = [...expected].sort();
      assert.deepEqual(got, want, `${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
}

function runCli(args: string[]): { status: number | null; stderr: string } {
  const result = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", SCANNER, ...args],
    { encoding: "utf8", cwd: ROOT },
  );
  return { status: result.status, stderr: result.stderr };
}

test("self-scan of tests/ runs cleanly", () => {
  const result = runCli(["--json", path.join(ROOT, "tests")]);
  assert.ok(
    result.status === 0 || result.status === 1,
    `expected exit 0 or 1, got ${result.status}: ${result.stderr.slice(0, 200)}`,
  );
});

test("--all on repo runs cleanly", () => {
  const result = runCli(["--all", "--quiet"]);
  assert.ok(
    result.status === 0 || result.status === 1 || result.status === 2,
    `expected exit 0, 1 or 2, got ${result.status}: ${result.stderr.slice(0, 200)}`,
  );
});
