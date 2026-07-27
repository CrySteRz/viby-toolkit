#!/usr/bin/env -S node --experimental-strip-types
/**
 * viby-code test-quality scanner — the executable half of /viby-code:test.
 *
 * Scans test files for the defect classes that empirical studies find in
 * agent-written tests, and that a passing suite cannot reveal:
 *
 *   no-assertion        a test that asserts nothing — it passes if the code does anything
 *   tautology           an assertion that cannot fail (expect(true).toBe(true))
 *   over-mocking        mock density so high the test exercises mocks, not code
 *   focused-or-skipped  .only / .skip / @Ignore left in, silently shrinking the suite
 *   sleep-wait          waiting on a timer instead of an observable condition
 *   swallowed-error     except/catch that discards the failure it was meant to catch
 *
 * Usage:
 *   node scan-test-quality.ts                 # test files changed vs HEAD
 *   node scan-test-quality.ts <path> [...]    # explicit files or directories
 *   node scan-test-quality.ts --all           # every test file in the repo
 *   node scan-test-quality.ts --json          # machine-readable findings
 *   node scan-test-quality.ts --quiet         # findings only, no summary
 *
 * Exit: 0 = clean, 1 = findings, 2 = nothing to scan (not a failure).
 *
 * PRECISION IS THE CONTRACT. Every check here is one an agent can act on without
 * arguing. Noisy-but-classic smells were deliberately left out — magic numbers and
 * conditional test logic fire constantly on legitimate table-driven tests, and a
 * scanner that cries wolf gets ignored, taking the real findings with it. Each
 * finding is a heuristic over text, not a proof: confirm before "fixing".
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { stripNoncode } from "../../../lib/strip-noncode.ts";

// ---------------------------------------------------------------- file detection

// Filename conventions — conclusive on their own, wherever the file lives.
const NAME_PATTERNS: RegExp[] = [
  /(^|\/)test_[^/]+\.py$/,
  /(^|\/)[^/]+_test\.py$/,
  /\.(test|spec)\.[jt]sx?$/,
  // Cypress's own convention. Without it, a repo using idiomatic Cypress layout produced
  // zero findings — not because it was clean, but because every file was invisible.
  /\.cy\.[jt]sx?$/,
  /(^|\/)cypress\/(e2e|integration|component)\/.*\.[jt]sx?$/,
  /(^|\/)[^/]+_test\.go$/,
  /(^|\/)[^/]*Tests?\.(java|kt|cs)$/,
  /(^|\/)[^/]+_spec\.rb$/,
  /(^|\/)[^/]+_test\.(rs|php|swift)$/,
];
// A test directory — suggestive, but only when the file isn't support code.
const DIR_PATTERNS: RegExp[] = [/(^|\/)(tests?|specs?|__tests__)\/.*\.(py|[jt]sx?|go|rb|rs|java|kt)$/];
// Support code that lives under a test directory but contains no tests. Applied ONLY
// to the directory rule, so `tests/utils/parse_test.go` still counts on its filename.
const DIR_DISQUALIFY = /(^|\/)(scripts?|fixtures?|helpers?|utils?|mocks?|__mocks__|__snapshots__|testdata|factories)\//;
// Support FILES that live in a test directory but contain no tests of their own — shared
// browser/DOM helpers, setup files, and config. Measured against real suites, these were a
// steady false-positive source: a polling utility's timeout guard is not a flaky test.
const SUPPORT_FILE =
  /(^|\/)([\w.-]*[Uu]tils?|[\w.-]*[Hh]elpers?|setup(Tests?)?|test-setup|globalSetup|\w+\.config|conftest|serve|commonTests|fixtures?|testServer)\.[jt]sx?$/;

const SKIP_DIRS = new Set([
  ".git", "node_modules", "venv", ".venv", "dist", "build", "target",
  "__pycache__", ".pytest_cache", ".next", "vendor", "coverage", ".tox",
]);

export function isTestFile(p0: string): boolean {
  const p = p0.split(path.sep).join("/");
  if (SUPPORT_FILE.test(p)) return false;
  if (NAME_PATTERNS.some((pat) => pat.test(p))) return true;
  if (DIR_PATTERNS.some((pat) => pat.test(p)) && !DIR_DISQUALIFY.test(p)) return true;
  return false;
}

function* walkRepo(root = "."): Generator<string> {
  const stack: string[] = [root];
  while (stack.length) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) stack.push(path.join(dir, entry.name));
      } else {
        const full = path.join(dir, entry.name);
        if (isTestFile(full)) yield path.normalize(full);
      }
    }
  }
}

/** Test files touched vs HEAD, including untracked. Empty if not a git repo. */
function changedFiles(): string[] {
  const out = new Set<string>();
  const cmds: string[][] = [
    ["git", "diff", "--name-only", "HEAD"],
    ["git", "diff", "--name-only", "--staged"],
    ["git", "ls-files", "--others", "--exclude-standard"],
  ];
  for (const cmd of cmds) {
    try {
      const [bin, ...rest] = cmd;
      if (bin === undefined) continue;
      const r = spawnSync(bin, rest, { encoding: "utf8", timeout: 10_000 });
      if (r.status === 0 && r.stdout) {
        for (const l of r.stdout.split("\n")) {
          if (l.trim()) out.add(l);
        }
      }
    } catch {
      // ignore
    }
  }
  return [...out].filter((f) => isTestFile(f) && fs.existsSync(f) && fs.statSync(f).isFile()).sort();
}

// ---------------------------------------------------------------- patterns

// A line that starts a new test case. Containers (describe/class) deliberately excluded.
const TEST_START = new RegExp(
  [
    /^\s*(?:async\s+)?def\s+test\w*\s*\(/.source, // python
    /^\s*(?:it|test|specify)\s*(?:\.\w+)*\s*[(<]/.source, // js/ts jest/mocha/vitest
    /^\s*(?:it|specify)\s+['"]/.source, // ruby rspec
    /^\s*func\s+(?:Test|Benchmark|Fuzz)\w*\s*\(/.source, // go
    /^\s*#\[test\]/.source, // rust
    /^\s*@Test\b/.source, // java/kotlin junit
    /^\s*\[(?:Test|Fact|Theory)\]/.source, // c#
  ].join("|"),
);

const ASSERT = new RegExp(
  [
    /\bassert\w*\s*[(\s]/.source,
    // `assert.ok(...)` / `assert.deepStrictEqual(...)` — the node:assert and chai style.
    // The pattern above cannot match it (it needs `(` or space right after `assert`),
    // which made the scanner miss the assertion style of its own test framework.
    /\bassert\.\w+\s*\(/.source,
    /\bexpect\s*\(/.source,
    /\bassertThat\s*\(/.source,
    /\bverify\s*\(/.source,
    /\bshould\b/.source,
    /\brequire\.\w+\s*\(/.source,
    /\bt\.(?:Error|Fatal|Errorf|Fatalf)\s*\(/.source,
    // ava: `t.is(...)`, `t.deepEqual(...)`, `t.throws(...)` — an entire framework's
    // assertion family that matched nothing at all.
    /\bt\.(?:is|not|true|false|deepEqual|notDeepEqual|like|throws\w*|notThrows\w*|regex|notRegex|pass|fail|snapshot)\s*\(/.source,
    // Any `.toSomething(` matcher, plus the resolves/rejects modifiers. The old fixed list
    // (toBe|toEqual|toMatch|toHave*|toThrow|toContain|toBeCalled*) missed toStrictEqual,
    // toBeTruthy, toBeVisible, toBeCloseTo, toMatchObject, toMatchSnapshot and more. That
    // list is the ONLY thing that can recognise an assertion chain split across lines, so
    // every gap in it became a false `no-assertion` — 9 of vite's 14 came from exactly this.
    /\.to[A-Z]\w*\s*\(/.source,
    /\.(?:resolves|rejects)\b/.source,
    // `expect.poll(...)`, `expect.soft(...)`, `expect.unreachable()` — `expect` not
    // immediately followed by `(`, so the plain `expect\s*\(` alternative cannot see it.
    /\bexpect\s*\.\s*\w+/.source,
    /\bassert_\w+/.source,
    /\bXCTAssert\w*\s*\(/.source,
    /\bpytest\.raises\s*\(/.source, // expecting a raise IS an assertion
    /\bassertRaises\w*\s*\(/.source,
    /\bassertWarns\w*\s*\(/.source,
    /\bpytest\.warns\s*\(/.source,
    /\bshould_panic\b/.source,
    /\bassert!\s*\(/.source, // rust
    /\bassert_eq!\s*\(/.source,
    /\bexpect_err\s*\(/.source,
    /\bhypothesis\b/.source,
    // `self.fail("...")` / `pytest.fail(...)` — an unconditional failure IS an assertion
    // mechanism, and is the standard way to assert "we should not have reached here".
    /\bfail\s*\(/.source,
  ].join("|"),
);

/**
 * An assertion function bound to a short local alias, e.g. `eq = self.assertEqual`, then
 * called as `eq(a, b)`. Extremely common in large suites (CPython uses it throughout), and
 * without it every such test looks assertion-free. Matches the BINDING (no call parens).
 */
const ASSERT_ALIAS_BINDING =
  /^\s*(\w+)\s*=\s*(?:self\.|cls\.|this\.)?(\w*(?:[aA]ssert|[eE]xpect)\w*|fail)\s*$/;

/**
 * A name bound to ANY method reference or partial application, e.g. `check = self.check_match`
 * or `raises = partial(self.assertRaises, ValueError)`. Only counted as an assertion alias
 * when the target is independently known to assert, so this cannot excuse an arbitrary call.
 * Measured on CPython, custom-named aliases like `check = self.check_match` were a distinct
 * false-positive class the assert/expect-named pattern above could never catch.
 */
const GENERIC_ALIAS_BINDING = /^\s*(\w+)\s*=\s*(?:partial\s*\(\s*)?(?:self\.|cls\.|this\.)?(\w+)/;

/**
 * `except X: pass` paired with an `else:` branch that fails — the manual raise-assertion
 * idiom (`try: bad() / except ValueError: pass / else: self.fail("no raise")`). The `pass`
 * is the SUCCESS path and the `else` is the assertion, so this is the opposite of a
 * swallowed failure. On CPython this single pattern accounted for a third of all
 * swallowed-error findings, and a 12-sample audit found zero true positives overall.
 */
const RAISE_ASSERTION_ELSE = /^\s*else\s*:/;
const FAIL_CALL = /\b(?:self\.fail|pytest\.fail|fail)\s*\(/;

/**
 * The JS equivalent: `try { await x(); expect.unreachable() } catch {}` asserts that the
 * call throws, then checks the error's effects after the block. The empty catch is the
 * success path, not a discarded failure.
 */
const UNREACHABLE_ASSERTION = /\b(?:expect\s*\.\s*unreachable|assert\s*\.\s*fail|fail)\s*\(/;

const MOCK = new RegExp(
  [
    /\bMock\w*\s*\(/.source,
    /\bMagicMock\b/.source,
    /\bAsyncMock\b/.source,
    // `mock.patch(...)` etc. — but NOT `.mock.calls` / `.mock.results`, which INSPECT a
    // mock inside an assertion. Counting those as mocking inflated the mock density of
    // tests that were, in fact, asserting carefully on a single spy.
    /\bmock\.(?!calls\b|results\b|instances\b|lastCall\b|invocationCallOrder\b)\w+/.source,
    /\bpatch\s*\(/.source,
    /@patch\b/.source,
    /\bjest\.(?:mock|fn|spyOn)\b/.source,
    /\bvi\.(?:mock|fn|spyOn)\b/.source,
    /\bsinon\.\w+/.source,
    /\bmockito\b/.source,
    /\bwhen\s*\(/.source,
    /\bgomock\b/.source,
    /\bcreateMock\w*\s*\(/.source,
    /\bstub\w*\s*\(/.source,
    /\bspyOn\s*\(/.source,
    /\bmockReturnValue\b/.source,
    /\bmockResolvedValue\b/.source,
    /\bmockImplementation\b/.source,
  ].join("|"),
);

const TAUTOLOGY = new RegExp(
  [
    /\bassert(?:True|_true)\s*\(\s*(?:True|true|1)\s*[,)]/.source,
    /\bassert\s+(?:True|true)\s*(?:$|#)/.source,
    /\bassertFalse\s*\(\s*(?:False|false|0)\s*[,)]/.source,
    /\bexpect\s*\(\s*(?:true|1)\s*\)\s*\.\s*(?:toBe|toEqual)\s*\(\s*(?:true|1)\s*\)/.source,
    /\bassert\s+1\s*==\s*1\b/.source,
    /\bassertEquals?\s*\(\s*(?<ae>\w+)\s*,\s*\k<ae>\s*\)/.source,
    /\bexpect\s*\(\s*(?<ex>\w+)\s*\)\s*\.\s*(?:toBe|toEqual)\s*\(\s*\k<ex>\s*\)/.source,
    /\bassert\s+(?<eq>\w+)\s*==\s*\k<eq>\s*(?:$|#)/.source,
  ].join("|"),
  "m",
);

const FOCUSED_OR_SKIPPED = new RegExp(
  [
    /\b(?:describe|it|test|context)\s*\.\s*(?:only|skip|todo|failing)\b/.source,
    /\bf(?:describe|it|context)\s*\(/.source,
    /\bx(?:describe|it|test|context)\s*\(/.source,
    /@pytest\.mark\.(?:skip|xfail)\b/.source,
    /@unittest\.(?:skip|expectedFailure)\b/.source,
    /\bt\.Skip\s*\(/.source,
    /#\[ignore\]/.source,
    /@(?:Ignore|Disabled)\b/.source,
    /\.pending\b/.source,
    /\bpytest\.skip\s*\(/.source,
  ].join("|"),
);

// A real, blocking wait. Fake timers NEVER excuse these: a fake-timer clock does not make
// `time.sleep` non-blocking, and an *awaited* real-timer promise would simply hang under
// fake timers — so its presence means the wait is genuine. Keeping these unconditional
// fixes a silent false negative: one suite calling `useFakeTimers()` used to suppress the
// flaky real waits in every other, unrelated suite in the same file.
const SLEEP_WAIT_HARD = new RegExp(
  [
    /\btime\.sleep\s*\(/.source,
    /\bThread\.sleep\s*\(/.source,
    // Bounded `.` rather than `[^)]*`: the idiomatic form is
    // `await new Promise((r) => setTimeout(r, 2000))`, whose arrow-function parameter list
    // closes a paren before `setTimeout` is reached.
    /\bawait\s+new\s+Promise\s*\(.{0,120}?setTimeout/.source,
    /\bawait\s+(?:delay|sleep|wait)\s*\(\s*\d/.source,
    /\btime\.Sleep\s*\(/.source,
    /\busleep\s*\(/.source,
  ].join("|"),
);
// A bare timer call, which fake timers DO make deterministic — so it is excused when the
// file installs them.
const SLEEP_WAIT_SOFT = /\b(?:setTimeout|setInterval)\s*\(/;

/**
 * Timer uses that are NOT arbitrary waits, measured against real TypeScript suites where
 * they accounted for most of the `sleep-wait` findings:
 *
 *  - `await new Promise(r => setTimeout(r))` / `setTimeout(r, 0)` — a macrotask "tick
 *    flush". Deterministic, idiomatic, and the opposite of a flaky sleep: there is no
 *    duration to race against.
 *  - a `setTimeout` whose callback rejects or throws — that is a timeout *guard* wrapping a
 *    poll loop, i.e. exactly the pattern this check is supposed to recommend.
 */
const TICK_FLUSH = /\b(?:setTimeout|setImmediate)\s*\(\s*[^,()]*(?:\([^)]*\)\s*=>\s*[^,]*)?\s*(?:,\s*0\s*)?\)/;
const TIMEOUT_GUARD = /\b(?:reject|throw)\b/;
const FAKE_TIMERS = /\b(useFakeTimers|sinon\.useFakeTimers|vi\.useFakeTimers|freeze_time|freezegun)\b/;

// Single-line forms: `except ValueError: pass`, `catch (e) {}`.
const SWALLOWED = new RegExp(
  [
    /\bexcept\b[^:\n]*:\s*pass\s*(?:$|#)/.source,
    /\bcatch\s*\([^)]*\)\s*\{\s*\}/.source,
    /\bcatch\s*\{\s*\}/.source,
    /\bexcept\b[^:\n]*:\s*\.\.\.\s*$/.source,
  ].join("|"),
  "m",
);
// Multi-line forms, far more common than the single-line ones:
//     except ValueError:        }  catch (e) {
//         pass                  }
const HANDLER_OPEN_PY = /^\s*except\b[^:]*:\s*(?:#.*)?$/;
const HANDLER_OPEN_JS = /^\s*\}?\s*catch\s*(?:\([^)]*\))?\s*\{\s*(?:\/\/.*)?$/;
const HANDLER_NOOP_PY = /^\s*(?:pass|\.\.\.)\s*(?:#.*)?$/;
const HANDLER_NOOP_JS = /^\s*\}\s*(?:\/\/.*)?$/;

// Lines that are comments or strings-only are poor evidence; skip the obvious ones.
const COMMENT_ONLY = /^\s*(#|\/\/|\*|\/\*|"""|'''|--)/;

// Callables defined in the file under scan, so delegated assertions can be recognised.
const LOCAL_DEF = new RegExp(
  [
    /^\s*(?:async\s+)?(?:def|function|fn|sub)\s+(\w+)/.source,
    /^\s*(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:\(|function)/.source,
    /^\s*(?:public|private|protected|static|\s)*[\w<>[\],\s]+?\s+(\w+)\s*\([^)]*\)\s*\{/.source,
  ].join("|"),
);

const CALL_NAME = /\b(\w+)\s*\(/g;
/** A method call through a receiver: `self.checkParam(...)`, `this.expectOk(...)`. */
const RECEIVER_CALL = /\b(?:self|this|cls)\s*\.\s*(\w+)\s*\(/g;

/**
 * A declaration whose body does nothing — `pass`, `...`, `raise NotImplementedError`, or
 * only a docstring. It is an abstract or stub method, so the real implementation (and its
 * assertions) live somewhere this scanner cannot see. Calling one is not evidence that a
 * test asserts nothing.
 */
const STUB_BODY = /^\s*(?:pass|\.\.\.|raise\s+NotImplementedError|return\s+NotImplemented|throw\s+new\s+Error)\b/;

/** Modules that hold shared test infrastructure, indexed even without a resolvable import. */
const SHARED_INFRA =
  /(^|\/)(\w*_tests?|util|utils|support|helpers?|base|common\w*|testcase|mixins?|shared|fixtures?)\.(py|[jt]sx?)$/;

/** Import specifiers, read from RAW text — a stripped file has its specifiers blanked. */
const JS_IMPORT = /(?:from|import)\s*\(?\s*["']([^"']+)["']|require\(\s*["']([^"']+)["']\s*\)/g;
const PY_IMPORT = /^[ \t]*(?:from[ \t]+([\w.]+)[ \t]+import|import[ \t]+([\w.]+))/gm;

const MAX_RELATED_FILES = 12; // per scanned file, so a wide import graph can't blow up runtime

// Capability probes AND loop-termination sentinels. `except ImportError: pass` is a
// compatibility idiom; `except StopIteration: pass` after draining a generator, or
// `except Empty: pass` draining a queue, is how those loops are DOCUMENTED to end. Measured
// on CPython, "drain until exhausted" was one of the largest remaining false-positive
// classes for this check.
const COMPAT_EXCEPTIONS =
  /\b(AttributeError|ImportError|ModuleNotFoundError|NotImplementedError|SkipTest|StopIteration|StopAsyncIteration|Empty|EOFError|BlockingIOError|BrokenPipeError|ConnectionResetError)\b/;

const MOCK_DENSITY_MAX = 4; // mock/stub calls in one test before it tests mocks
const NO_ASSERTION_MIN_BODY = 2; // code lines a test needs before "no assertion" is meaningful

// Deliberately NOT checked mechanically, though /viby-code:test still teaches both:
//   assertion-roulette  — 4+ unexplained assertions. Measured against CPython's own suite
//                         this fired ~6.2 times per file on idiomatic code (four
//                         consecutive assertRaises calls is normal, readable style).
//   magic-number-test   — fires constantly on legitimate table-driven tests.
// Both are real authoring smells and useless as gates. A scanner that cries wolf gets
// ignored, which costs more than the findings it would have surfaced.

type Severity = "P1" | "P2" | "P3";

type Finding = {
  file: string;
  line: number;
  check: string;
  message: string;
  severity: Severity;
};

function makeFinding(
  file: string,
  line: number,
  check: string,
  message: string,
  severity: Severity = "P2",
): Finding {
  return { file, line, check, message, severity };
}

type LineEntry = [number, string];

/**
 * Yield [nameLineIndex, body] for each test case in the file.
 *
 * Nested declarations are NOT new tests: a table-driven test commonly defines a local
 * helper called `test(...)`, and treating that as a separate test case truncates the
 * real one — which then looks assertion-free because its assertions live in the helper.
 * Indentation decides: a declaration indented deeper than the test it sits inside is
 * part of that test.
 */
function* splitTests(lines: string[]): Generator<[number, LineEntry[]]> {
  const starts: Array<[number, number]> = [];
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] ?? "";
    if (!TEST_START.test(line)) continue;
    const indent = line.length - line.trimStart().length;
    const last = starts[starts.length - 1];
    if (last && indent > last[1]) continue;
    starts.push([i, indent]);
  }

  for (let n = 0; n < starts.length; n++) {
    const entry = starts[n];
    if (!entry) continue;
    const [start, indent] = entry;
    const next = starts[n + 1];
    const hardEnd = next ? next[0] : lines.length;
    // A test body also ends at the next SIBLING definition — a helper method declared
    // between two tests belongs to neither. Without this, a helper's `except: pass`
    // and its assertions both get attributed to the preceding test.
    let end = hardEnd;
    for (let j = start + 1; j < hardEnd; j++) {
      const line = lines[j] ?? "";
      if (!line.trim()) continue;
      if (LOCAL_DEF.test(line) && line.length - line.trimStart().length <= indent) {
        end = j;
        break;
      }
    }
    const body: LineEntry[] = [];
    for (let i = start; i < end; i++) body.push([i, lines[i] ?? ""]);
    yield [start, body];
  }
}

type Helpers = { asserting: Set<string>; stubs: Set<string> };

/**
 * Names declared in `lines` that provably assert, plus names that are abstract stubs.
 *
 * Extracted from scanFile so the same analysis can run over a RELATED file — a base class
 * or mixin in another module. Resolved to a fixpoint, because helper chains are normal.
 */
function computeHelpers(lines: string[]): Helpers {
  const aliases = new Set<string>();
  for (const line of lines) {
    const m = ASSERT_ALIAS_BINDING.exec(line);
    if (m?.[1] !== undefined) aliases.add(m[1]);
  }
  const asserts = (t: string): boolean => {
    if (ASSERT.test(t)) return true;
    if (aliases.size === 0) return false;
    for (const m of t.matchAll(CALL_NAME)) if (m[1] !== undefined && aliases.has(m[1])) return true;
    return false;
  };

  const defs: Array<{ name: string; line: number; indent: number }> = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    const m = LOCAL_DEF.exec(line);
    if (!m) continue;
    const name = m.slice(1).find((g) => g !== undefined);
    if (name) defs.push({ name, line: i, indent: line.length - line.trimStart().length });
  }
  const allLocals = new Set(defs.map((d) => d.name));
  const info = new Map<string, { direct: boolean; calls: Set<string> }>();
  const stubs = new Set<string>();

  for (let d = 0; d < defs.length; d += 1) {
    const def = defs[d];
    if (!def) continue;
    let end = lines.length;
    for (let e = d + 1; e < defs.length; e += 1) {
      const next = defs[e];
      if (next && next.indent <= def.indent) {
        end = next.line;
        break;
      }
    }
    const entry = info.get(def.name) ?? { direct: false, calls: new Set<string>() };
    const bodyCode: string[] = [];
    for (let j = def.line; j < end; j += 1) {
      const line = lines[j] ?? "";
      if (j > def.line && line.trim() && !COMMENT_ONLY.test(line)) bodyCode.push(line);
      if (asserts(line)) entry.direct = true;
      for (const m of line.matchAll(CALL_NAME)) {
        const callee = m[1];
        if (callee !== undefined && callee !== def.name && allLocals.has(callee)) entry.calls.add(callee);
      }
    }
    // Abstract/stub: nothing but a docstring (blanked to filler), `pass`, `...`, or a raise.
    const meaningful = bodyCode.filter((l) => !/^[\s\x00"'`]*$/.test(l));
    if (meaningful.length === 0 || (meaningful.length === 1 && STUB_BODY.test(meaningful[0] ?? ""))) {
      stubs.add(def.name);
    }
    info.set(def.name, entry);
  }

  const asserting = new Set<string>();
  for (const [name, entry] of info) if (entry.direct) asserting.add(name);
  let grew = true;
  while (grew) {
    grew = false;
    for (const [name, entry] of info) {
      if (asserting.has(name)) continue;
      for (const callee of entry.calls) {
        if (asserting.has(callee)) {
          asserting.add(name);
          grew = true;
          break;
        }
      }
    }
  }
  return { asserting, stubs };
}

/** Memoized per-file helper analysis, so a shared mixin is parsed once per run. */
const helperCache = new Map<string, Helpers>();
function helpersInFile(filePath: string): Helpers {
  const cached = helperCache.get(filePath);
  if (cached) return cached;
  let out: Helpers = { asserting: new Set(), stubs: new Set() };
  try {
    const raw = fs.readFileSync(filePath, "utf8");
    out = computeHelpers(stripNoncode(raw, path.extname(filePath).toLowerCase()).split("\n"));
  } catch {
    // unreadable — treat as contributing nothing
  }
  helperCache.set(filePath, out);
  return out;
}

/**
 * Files whose declarations a test file could plausibly inherit or import: resolvable
 * relative/`from x import y` targets, plus shared test-infrastructure modules sitting in
 * the same directory. Import specifiers are read from RAW text, since the blanking pass
 * (correctly) erases string contents.
 */
function relatedFiles(filePath: string, raw: string): string[] {
  const dir = path.dirname(filePath);
  const found = new Set<string>();
  const tryAdd = (base: string): void => {
    for (const e of [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py"]) {
      const c = base + e;
      try {
        if (fs.statSync(c).isFile()) {
          found.add(c);
          return;
        }
      } catch {
        /* keep looking */
      }
    }
    for (const idx of ["/index.ts", "/index.tsx", "/index.js"]) {
      try {
        if (fs.statSync(base + idx).isFile()) {
          found.add(base + idx);
          return;
        }
      } catch {
        /* keep looking */
      }
    }
  };

  for (const m of raw.matchAll(JS_IMPORT)) {
    const spec = m[1] ?? m[2];
    if (spec === undefined || !spec.startsWith(".")) continue; // relative only — skip packages
    tryAdd(path.resolve(dir, spec.replace(/\.[jt]sx?$/, "")));
  }
  // Dotted Python modules: `from test.test_tkinter.widget_tests import AbstractWidgetTest`
  // resolves relative to whichever ancestor directory the package root sits in — often NOT
  // the importing file's own directory (test_ttk imports a mixin from test_tkinter). Try the
  // full dotted path against each ancestor, then progressively drop leading components.
  for (const m of raw.matchAll(PY_IMPORT)) {
    const parts = (m[1] ?? m[2] ?? "").split(".").filter(Boolean);
    if (parts.length === 0) continue;
    for (let up = 0; up <= 4; up += 1) {
      const ancestor = path.join(dir, ...Array(up).fill(".."));
      for (let drop = 0; drop < parts.length; drop += 1) {
        tryAdd(path.join(ancestor, ...parts.slice(drop)));
      }
    }
  }
  try {
    for (const entry of fs.readdirSync(dir)) {
      const full = path.join(dir, entry);
      if (full !== filePath && SHARED_INFRA.test(full.split(path.sep).join("/"))) found.add(full);
    }
  } catch {
    /* unreadable directory */
  }

  found.delete(filePath);
  return [...found].slice(0, MAX_RELATED_FILES);
}

function scanFile(filePath: string): Finding[] {
  const findings: Finding[] = [];
  let raw: string;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
    // A file we could not read must NOT be reported as clean. This checker's whole claim is
    // that "clean" means checked-and-clean; silently returning no findings turns an
    // unreadable file into a passing one, which is the exact silent-pass mode it exists to
    // catch elsewhere. Reproduced: chmod 000 on a file with a real defect printed
    // "clean: 1 test file(s), no quality defects found" and exited 0.
    findings.push(
      makeFinding(
        filePath,
        1,
        "unreadable",
        "could not read this file, so it was NOT checked — do not treat this run as clean for it",
        "P2",
      ),
    );
    return findings;
  }

  // Match against code only — string literals and comments are blanked first.
  const fileText = stripNoncode(raw, path.extname(filePath).toLowerCase());
  const lines = fileText.split("\n");
  const hasFakeTimers = FAKE_TIMERS.test(fileText);

  function codeLines(body: LineEntry[]): LineEntry[] {
    return body.filter(([, t]) => t.trim() && !COMMENT_ONLY.test(t));
  }

  // Local aliases bound to assertion functions, so calls through them count as assertions.
  const assertAliases = new Set<string>();
  for (const line of lines) {
    const m = ASSERT_ALIAS_BINDING.exec(line);
    if (m?.[1] !== undefined) assertAliases.add(m[1]);
  }
  function assertsOnLine(text: string): boolean {
    if (ASSERT.test(text)) return true;
    if (assertAliases.size === 0) return false;
    for (const m of text.matchAll(CALL_NAME)) {
      if (m[1] !== undefined && assertAliases.has(m[1])) return true;
    }
    return false;
  }

  const tests = [...splitTests(lines)];

  // Callables defined in this file whose own body contains an assertion. A test that
  // calls one of these may be asserting through it (`self.check_parse(...)`,
  // `assertRoundTrips(...)`) — very common, and invisible to a per-test scan. Without
  // this, delegation reads as "asserts nothing"; measured against CPython's suite it was
  // the single largest false-positive source.
  //
  // The body check matters: keying on the NAME alone suppressed `no-assertion` whenever a
  // test called anything sharing a name with any local definition. Since fixture and mock
  // classes routinely define `get`, `run`, `close`, `load`, a test calling an unrelated
  // `cache.get(...)` was silently excused, hiding real defects in the scanner's largest
  // category. Only helpers that actually assert can be carrying an assertion.
  // Resolved to a FIXPOINT, because helper chains are normal: `check_parse` calls `_check`
  // which does the asserting. Requiring a *direct* assertion in the called helper doubled
  // the no-assertion count on CPython's suite (1037 -> 2050) — almost all of it delegation
  // one level deeper. A helper counts as asserting if its own body asserts, or if it calls
  // another helper that does.
  const local = computeHelpers(lines);
  const assertingLocals = local.asserting;
  // Locally-declared abstract stubs: `self.execute(...)` whose body here is only a
  // docstring, with the real implementation in a subclass elsewhere.
  const stubNames = new Set(local.stubs);

  // ---- cross-file delegation
  //
  // The scanner reads one file at a time, so an assertion living in a base class or mixin
  // in ANOTHER module was invisible: `self.checkParam(...)` in test_widgets.py asserts via
  // widget_tests.py. Measured on CPython this was the single largest cause of false
  // `no-assertion` — one pair of files accounted for ~10% of all findings.
  //
  // Names from related files are accepted ONLY when called through a receiver
  // (`self.x()` / `this.x()` / `cls.x()`). Inherited helpers are always invoked that way,
  // and the restriction stops a generically-named helper in a shared module (`run`, `check`)
  // from excusing an unrelated bare call — the same over-suppression trap that keying on
  // names alone caused within a single file.
  const inheritedAsserting = new Set<string>();
  const inheritedStubs = new Set<string>();
  for (const related of relatedFiles(filePath, raw)) {
    const h = helpersInFile(related);
    for (const n of h.asserting) inheritedAsserting.add(n);
    for (const n of h.stubs) inheritedStubs.add(n);
  }

  // Second alias pass, now that we know which locals assert: a name bound to an asserting
  // method or a partial of one counts as an assertion alias even when its own name says
  // nothing (`check = self.check_match`, `raises = partial(self.assertRaises, ValueError)`).
  // Requiring the target to provably assert is what makes this safe.
  for (const line of lines) {
    const m = GENERIC_ALIAS_BINDING.exec(line);
    const alias = m?.[1];
    const target = m?.[2];
    if (alias === undefined || target === undefined) continue;
    if (assertingLocals.has(target) || /(?:[aA]ssert|[eE]xpect)/.test(target) || target === "fail") {
      assertAliases.add(alias);
    }
  }

  function delegatesToLocal(text: string): boolean {
    for (const m of text.matchAll(CALL_NAME)) {
      const n = m[1];
      if (n !== undefined && (assertingLocals.has(n) || stubNames.has(n))) return true;
    }
    // A helper can also be PASSED rather than called — `test.each(cases)(name, assertThing)`
    // hands the asserting function to the runner, so it never appears with parentheses.
    for (const m of text.matchAll(/\b(\w+)\b/g)) {
      if (m[1] !== undefined && assertingLocals.has(m[1])) return true;
    }
    // Inherited from a base class or mixin in another file — receiver form only.
    for (const m of text.matchAll(RECEIVER_CALL)) {
      const n = m[1];
      if (n !== undefined && (inheritedAsserting.has(n) || inheritedStubs.has(n))) return true;
    }
    return false;
  }
  // Line indices that sit inside a test body. `except: pass` in a shared helper or in
  // setUp is usually deliberate cleanup; inside a test it discards the failure.
  const inTest = new Set<number>();
  for (const [, body] of tests) {
    for (const [i] of body) inTest.add(i);
  }

  // ---- file-level checks (apply anywhere in the file)
  for (let i = 0; i < lines.length; i++) {
    const text = lines[i] ?? "";
    if (COMMENT_ONLY.test(text)) continue;
    if (FOCUSED_OR_SKIPPED.test(text)) {
      findings.push(
        makeFinding(
          filePath,
          i + 1,
          "focused-or-skipped",
          "focused or skipped test left in place — the suite silently shrinks and " +
            "still reports green",
          "P1",
        ),
      );
    }
    if (TAUTOLOGY.test(text)) {
      findings.push(
        makeFinding(
          filePath,
          i + 1,
          "tautology",
          "assertion cannot fail — it passes regardless of the code under test",
          "P1",
        ),
      );
    }
    let swallowed = SWALLOWED.test(text);
    if (!swallowed && (HANDLER_OPEN_PY.test(text) || HANDLER_OPEN_JS.test(text))) {
      const noop = HANDLER_OPEN_PY.test(text) ? HANDLER_NOOP_PY : HANDLER_NOOP_JS;
      for (const nxt of lines.slice(i + 1)) {
        if (!nxt.trim() || COMMENT_ONLY.test(nxt)) continue;
        swallowed = noop.test(nxt);
        break;
      }
    }
    // The manual raise-assertion idiom, in both dialects:
    //   python: try/except X: pass  +  else: self.fail("did not raise")
    //   js:     try { x(); expect.unreachable() } catch {}
    // In both, the swallowing branch is the SUCCESS path and the assertion is elsewhere.
    let raiseAssertion = false;
    for (let j = i + 1; j < Math.min(lines.length, i + 10); j += 1) {
      const nxt = lines[j] ?? "";
      if (RAISE_ASSERTION_ELSE.test(nxt)) {
        for (let k = j; k < Math.min(lines.length, j + 4); k += 1) {
          if (FAIL_CALL.test(lines[k] ?? "")) {
            raiseAssertion = true;
            break;
          }
        }
        break;
      }
    }
    // Look BACKWARD for the js form: the assertion sits inside the try, above the catch.
    if (!raiseAssertion) {
      for (let j = Math.max(0, i - 6); j < i; j += 1) {
        if (UNREACHABLE_ASSERTION.test(lines[j] ?? "")) {
          raiseAssertion = true;
          break;
        }
      }
    }
    if (swallowed && inTest.has(i) && !COMPAT_EXCEPTIONS.test(text) && !raiseAssertion) {
      findings.push(
        makeFinding(
          filePath,
          i + 1,
          "swallowed-error",
          "exception discarded — check whether the failure this test exists to catch is " +
            "being thrown away (lowest-confidence check: many languages have legitimate " +
            "swallow idioms, so confirm before changing anything)",
          // Deliberately P2, not P1: a 12-sample audit against CPython found zero true
          // positives before the raise-assertion and drain-sentinel exclusions were added.
          // The check earns its place on empty `catch {}` in JS, but it is the least
          // trustworthy finding this scanner emits and should not read as high-confidence.
          "P2",
        ),
      );
    }
    const timerIsBenign = TICK_FLUSH.test(text) || TIMEOUT_GUARD.test(text);
    const realWait = SLEEP_WAIT_HARD.test(text) && !TICK_FLUSH.test(text);
    const softWait = SLEEP_WAIT_SOFT.test(text) && !hasFakeTimers && !timerIsBenign;
    if (realWait || softWait) {
      findings.push(
        makeFinding(
          filePath,
          i + 1,
          "sleep-wait",
          "waiting on a timer, not an observable condition — this is how flaky tests " +
            "are born; poll for the state or await the event",
          "P2",
        ),
      );
    }
  }

  // ---- per-test checks
  for (const [start, body] of tests) {
    const real = codeLines(body);
    const asserts = real.filter(([, t]) => assertsOnLine(t));
    const mocks = real.filter(([, t]) => MOCK.test(t));

    // A test with a real body but nothing that can fail. A one- or two-line body is
    // often a deliberate "just don't raise" smoke test, so require some substance;
    // and a call into a local helper may be carrying the assertion.
    const delegates = real.some(([i, t]) => i !== start && delegatesToLocal(t));
    // A body that is entirely a string literal is a doctest: the assertion mechanism is
    // doctest's own output comparison, and `stripNoncode` has (correctly) blanked it to
    // filler, so there is no code here to judge either way.
    const doctestOnly =
      real.length > 1 &&
      real.slice(1).every(([, t]) => t.trim() === "" || /^[\s\x00"'`]*$/.test(t));
    if (asserts.length === 0 && !delegates && !doctestOnly && real.length > NO_ASSERTION_MIN_BODY) {
      findings.push(
        makeFinding(
          filePath,
          start + 1,
          "no-assertion",
          "test asserts nothing — it passes as long as the code does not raise",
          "P1",
        ),
      );
    }

    if (mocks.length > MOCK_DENSITY_MAX && mocks.length > asserts.length) {
      findings.push(
        makeFinding(
          filePath,
          start + 1,
          "over-mocking",
          `${mocks.length} mock/stub calls vs ${asserts.length} assertions — this exercises ` +
            `the mocks, not the code; mocks only hold while they match the real ` +
            `implementation`,
          "P2",
        ),
      );
    }
  }

  findings.sort((a, b) => (a.file === b.file ? a.line - b.line : a.file < b.file ? -1 : 1));
  return findings;
}

type Targets = { targets: string[]; skipped: string[] };

/**
 * Checks are test-specific, so non-test files are skipped unless --any says the
 * project uses unconventional naming.
 */
function collectTargets(args: { all: boolean; any: boolean; paths: string[] }): Targets {
  if (args.all) {
    return { targets: [...walkRepo(".")].sort(), skipped: [] };
  }
  if (args.paths.length > 0) {
    const out: string[] = [];
    const skipped: string[] = [];
    for (const p of args.paths) {
      let stat: fs.Stats | undefined;
      try {
        stat = fs.statSync(p);
      } catch {
        continue;
      }
      if (stat.isDirectory()) {
        out.push(...walkRepo(p));
      } else if (stat.isFile()) {
        if (args.any || isTestFile(p)) {
          out.push(path.normalize(p));
        } else {
          skipped.push(path.normalize(p));
        }
      }
    }
    return { targets: [...new Set(out)].sort(), skipped };
  }
  return { targets: changedFiles(), skipped: [] };
}

function main(): number {
  const { values, positionals } = parseArgs({
    args: process.argv.slice(2),
    options: {
      all: { type: "boolean", default: false },
      any: { type: "boolean", default: false },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
    allowPositionals: true,
  });

  const args = { all: values.all ?? false, any: values.any ?? false, paths: positionals };
  const { targets, skipped } = collectTargets(args);

  if (targets.length === 0) {
    if (values.json) {
      console.log(JSON.stringify({ scanned: 0, findings: [], skipped_non_test: skipped }));
    } else if (!values.quiet) {
      if (skipped.length > 0) {
        console.log(
          "no test files among the given paths — skipped as non-test: " + skipped.join(", "),
        );
        console.log("these checks only make sense on tests; pass --any to scan them anyway");
      } else {
        console.log("no test files to scan (pass paths explicitly, or --all)");
      }
    }
    return 2;
  }
  if (skipped.length > 0 && !values.json && !values.quiet) {
    console.log(`note: skipped ${skipped.length} non-test file(s): ${skipped.join(", ")}\n`);
  }

  const findings: Finding[] = [];
  for (const t of targets) findings.push(...scanFile(t));

  if (values.json) {
    console.log(JSON.stringify({ scanned: targets.length, findings }, null, 2));
    return findings.length ? 1 : 0;
  }

  const order: Record<string, number> = { P1: 0, P2: 1, P3: 2 };
  const sorted = [...findings].sort((a, b) => {
    const oa = order[a.severity] ?? 3;
    const ob = order[b.severity] ?? 3;
    if (oa !== ob) return oa - ob;
    if (a.file !== b.file) return a.file < b.file ? -1 : 1;
    return a.line - b.line;
  });
  for (const f of sorted) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.check}]  ${f.message}`);
  }

  if (!values.quiet) {
    if (findings.length > 0) {
      const by = new Map<string, number>();
      for (const f of findings) by.set(f.check, (by.get(f.check) ?? 0) + 1);
      const breakdown = [...by.entries()]
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
        .map(([k, v]) => `${k} ${v}`)
        .join(", ");
      console.log(
        `\n${findings.length} finding(s) across ${targets.length} test file(s): ${breakdown}`,
      );
      console.log("Each is a text heuristic — confirm against the code before changing anything.");
    } else {
      console.log(`clean: ${targets.length} test file(s), no quality defects found`);
    }
  }
  return findings.length ? 1 : 0;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exitCode = main();
}
