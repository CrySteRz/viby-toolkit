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
    // P0 regression: a stray triple quote in a NON-Python file used to open a fake
    // triple-quoted region with no closing guard, blanking the rest of the file. A skipped
    // test and a tautology both vanished and the scanner reported clean at exit 0.
    "ts: a stray triple quote must not blank the rest of the file",
    "triple.test.ts",
    `
const marker = ''';
test.skip("skipped and asserts nothing", () => {
  log("never runs");
});
test("tautology", () => {
  expect(true).toBe(true);
});
`,
    // Three findings, not two: the skipped test also asserts nothing, which is correct —
    // the point is that NONE of them are invisible any more.
    ["focused-or-skipped", "no-assertion", "tautology"],
  ],

  [
    // The other half: in Python, triple quotes ARE delimiters and must still blank.
    "py: a real docstring is still blanked",
    "test_doc.py",
    `
def test_documented():
    """Mentions DROP COLUMN and time.sleep(5) and it.skip deliberately."""
    total = compute()
    assert total == 42
`,
    [],
  ],

  [
    // The multi-line handler form had NO coverage, despite its own comment calling it
    // "far more common than the single-line ones".
    "ts: multi-line empty catch is a swallowed error",
    "multiline-catch.test.ts",
    `
it("handles bad input", () => {
  try {
    parse("garbage");
  } catch (e) {
  }
  expect(state()).toBe("clean");
});
`,
    ["swallowed-error"],
  ],

  [
    // over-mocking boundary: exactly MOCK_DENSITY_MAX must NOT fire (4 > 4 is false).
    "py: exactly four mocks is at the boundary and must not fire",
    "test_mock_boundary.py",
    `
def test_checkout():
    a = MagicMock()
    b = MagicMock()
    c = MagicMock()
    d = MagicMock()
    checkout(a, b, c, d)
    assert result.ok
`,
    [],
  ],

  [
    // ...and above the max it must still not fire when assertions outnumber the mocks.
    "py: mock-heavy but assertion-richer must not fire",
    "test_mock_asserts.py",
    `
def test_checkout():
    a = MagicMock()
    b = MagicMock()
    c = MagicMock()
    d = MagicMock()
    e = MagicMock()
    r = checkout(a, b, c, d, e)
    assert r.status == "paid"
    assert r.total == 100
    assert r.currency == "EUR"
    assert r.id is not None
    assert r.receipt is not None
    assert r.settled is True
`,
    [],
  ],

  [
    // Regression: a quote-free regex literal was scanned as code, so the pattern TEXT
    // `/it.skip/` was reported as a focused test.
    "ts: regex literal containing it.skip is not a focused test",
    "regex3.test.ts",
    `
it("finds the text", () => {
  expect(screen.getByText(/it.skip/)).toBeTruthy();
});
`,
    [],
  ],

  [
    "ts: division is not mistaken for a regex literal",
    "div.test.ts",
    `
it("computes a ratio", () => {
  const ratio = width / height / 2;
  expect(ratio).toBeCloseTo(0.5);
});
`,
    [],
  ],

  [
    // ava's assertion family matched nothing at all before.
    "ts: ava t.is() counts as an assertion",
    "ava.test.ts",
    `
test("adds", (t) => {
  const total = compute();
  const other = compute2();
  t.is(total, 3);
});
`,
    [],
  ],

  [
    // A chain split across lines: no line holds `expect(`, so recognition depended on the
    // matcher whitelist, which missed toStrictEqual. 9 of vite's 14 findings were this.
    "ts: multi-line expect.poll chain counts as an assertion",
    "poll.test.ts",
    `
test("eventually matches", async () => {
  const allResult = build();
  await expect
    .poll(async () => JSON.parse(await page.textContent(".result")))
    .toStrictEqual(allResult);
});
`,
    [],
  ],

  [
    // `try { x(); expect.unreachable() } catch {}` is a manual raise-assertion.
    "ts: expect.unreachable + empty catch is not a swallowed error",
    "unreach.test.ts",
    `
test("throws on bad input", async () => {
  try {
    await load("virtual:test");
    expect.unreachable();
  } catch {}
  expect(spy.lastCall[0]).toContain("failed");
});
`,
    [],
  ],

  [
    // python equivalent: the `else: self.fail(...)` branch IS the assertion.
    "py: except/pass with else/fail is a raise-assertion, not a swallow",
    "test_raises.py",
    `
def test_join_rejects_ints():
    try:
        "".join([0])
    except TypeError:
        pass
    else:
        self.fail("''.join([0]) did not raise TypeError")
`,
    [],
  ],

  [
    "py: draining a generator to StopIteration is not a swallowed error",
    "test_drain.py",
    `
def test_generator_exhausts():
    g = make_gen()
    try:
        while True:
            g.send(None)
    except StopIteration:
        pass
    assert g.closed
`,
    [],
  ],

  [
    // A zero-delay timer is a deterministic tick flush, not an arbitrary wait.
    "ts: zero-delay setTimeout tick-flush is not a sleep-wait",
    "tick.test.ts",
    `
it("flushes effects", async () => {
  mount(App);
  await new Promise((r) => setTimeout(r));
  expect(container.innerHTML).toBe("<div>ok</div>");
});
`,
    [],
  ],

  [
    "ts: a real delayed wait is still flagged",
    "realwait.test.ts",
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
    // `.mock.calls` INSPECTS a spy inside an assertion; counting it as mocking made a
    // carefully-asserted single-spy test look over-mocked.
    "ts: spy introspection is not mock density",
    "spy.test.ts",
    `
test("records every error", () => {
  const onError = vi.fn();
  doThing(onError);
  expect(onError.mock.calls[0]).toEqual(["a"]);
  expect(onError.mock.calls[1]).toEqual(["b"]);
  expect(onError.mock.calls[2]).toEqual(["c"]);
  expect(onError.mock.calls[3]).toEqual(["d"]);
  expect(onError.mock.calls[4]).toEqual(["e"]);
});
`,
    [],
  ],

  [
    // `check = self.check_match` — an alias whose own name says nothing about asserting.
    "py: custom-named alias to an asserting helper is recognised",
    "test_alias2.py",
    `
def check_match(self, pattern, text):
    assert fnmatch(text, pattern)

def test_range():
    check = self.check_match
    check("[a-z]", "q")
    check("[!a-z]", "0")
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

function runCli(args: string[]): { status: number | null; stderr: string; stdout: string } {
  const result = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", SCANNER, ...args],
    { encoding: "utf8", cwd: ROOT },
  );
  return { status: result.status, stderr: result.stderr, stdout: result.stdout ?? "" };
}

test("self-scan of tests/ is CLEAN, not merely non-crashing", () => {
  // This previously accepted exit 1 — which means "findings were found" — while being named
  // "runs cleanly". It would have stayed green if a real defect appeared under tests/.
  const result = runCli(["--json", path.join(ROOT, "tests")]);
  assert.equal(
    result.status,
    0,
    `expected a clean scan (exit 0); exit ${result.status} means defects were found: ${result.stdout.slice(0, 400)}`,
  );
});

test("--all on the repo is CLEAN (0) or nothing-to-scan (2) — never 1", () => {
  // Same defect: exit 1 is a finding, not a pass. This repo dogfoods a clean suite.
  const result = runCli(["--all", "--quiet"]);
  assert.ok(
    result.status === 0 || result.status === 2,
    `expected 0 or 2, got ${result.status} — exit 1 means real findings: ${result.stdout.slice(0, 400)}`,
  );
});

// ---------------------------------------------------------------------------
// Cross-file delegation. A test whose assertions live in a base class or mixin in ANOTHER
// module was the largest single cause of false `no-assertion` — one pair of CPython files
// accounted for ~10% of all findings. These cases need two files on disk, so they can't use
// the single-file CASES table above.
// ---------------------------------------------------------------------------

function scanIn(files: Record<string, string>, target: string): string[] {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "xfile-"));
  try {
    for (const [rel, body] of Object.entries(files)) {
      const full = path.join(dir, rel);
      fs.mkdirSync(path.dirname(full), { recursive: true });
      fs.writeFileSync(full, body.replace(/^\n/, ""));
    }
    const r = spawnSync(
      process.execPath,
      [
        "--experimental-strip-types",
        "--disable-warning=ExperimentalWarning",
        SCANNER,
        "--json",
        path.join(dir, target),
      ],
      { encoding: "utf8" },
    );
    return (JSON.parse(r.stdout).findings as Array<{ check: string }>).map((f) => f.check);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test("py: assertion inherited from a mixin in a sibling module is recognised", () => {
  const checks = scanIn(
    {
      "widget_tests.py": `
class AbstractWidgetTest:
    def checkParam(self, widget, name, value):
        widget[name] = value
        assert widget[name] == value
`,
      "test_widgets.py": `
from widget_tests import AbstractWidgetTest

class ButtonTest(AbstractWidgetTest):
    def test_text_param(self):
        widget = self.create()
        self.checkParam(widget, "text", "hello")
`,
    },
    "test_widgets.py",
  );
  assert.deepEqual(checks, [], `expected no findings, got ${JSON.stringify(checks)}`);
});

test("py: mixin reached through a dotted package path is recognised", () => {
  const checks = scanIn(
    {
      "pkg/test_tk/widget_tests.py": `
class AbstractWidgetTest:
    def checkParam(self, widget, name, value):
        assert widget[name] == value
`,
      "pkg/test_ttk/test_widgets.py": `
from pkg.test_tk.widget_tests import AbstractWidgetTest

class ComboTest(AbstractWidgetTest):
    def test_values_param(self):
        widget = self.create()
        self.checkParam(widget, "values", "a b c")
`,
    },
    "pkg/test_ttk/test_widgets.py",
  );
  assert.deepEqual(checks, [], `expected no findings, got ${JSON.stringify(checks)}`);
});

test("ts: assertion inherited from a relative import is recognised", () => {
  const checks = scanIn(
    {
      "helpers.ts": `
export function expectRendered(el: HTMLElement, html: string) {
  expect(el.innerHTML).toBe(html);
}
`,
      "widget.test.ts": `
import { expectRendered } from "./helpers";

class Base {
  assertRendered(el: HTMLElement, html: string) {
    expectRendered(el, html);
  }
}

test("renders", () => {
  const el = mount(App);
  flush();
  this.assertRendered(el, "<div>ok</div>");
});
`,
    },
    "widget.test.ts",
  );
  assert.deepEqual(checks, [], `expected no findings, got ${JSON.stringify(checks)}`);
});

test("cross-file delegation does NOT excuse a helper that never asserts", () => {
  const checks = scanIn(
    {
      "helpers.py": `
class SupportBase:
    def prepare(self, widget):
        widget.pack()
        return widget
`,
      "test_thing.py": `
from helpers import SupportBase

class ThingTest(SupportBase):
    def test_prepares(self):
        widget = make_widget()
        self.prepare(widget)
        log_event(widget)
`,
    },
    "test_thing.py",
  );
  assert.deepEqual(
    checks,
    ["no-assertion"],
    `a non-asserting inherited helper must not suppress the finding, got ${JSON.stringify(checks)}`,
  );
});

test("cross-file names only count via a receiver, not a bare call", () => {
  const checks = scanIn(
    {
      "utils.py": `
def check(value):
    assert value is not None
`,
      "test_bare.py": `
from utils import check

def test_unrelated_bare_call():
    widget = make_widget()
    other = build()
    render(widget, other)
`,
    },
    "test_bare.py",
  );
  // `render(...)` is a bare call to an unrelated name; the imported `check` must not
  // excuse it. Guards against the over-suppression that name-only matching caused.
  assert.deepEqual(checks, ["no-assertion"], `got ${JSON.stringify(checks)}`);
});

test("py: calling a locally-declared abstract method suppresses no-assertion", () => {
  const checks = scanIn(
    {
      "util.py": `
class CommonTests:
    def execute(self, package, path):
        """Subclasses implement this and assert on the result."""

    def test_package_object(self):
        package = make_package()
        path = make_path()
        self.execute(package, path)
`,
    },
    "util.py",
  );
  assert.deepEqual(checks, [], `abstract-method delegation, got ${JSON.stringify(checks)}`);
});
