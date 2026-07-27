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

// ---------------------------------------------------------------- file detection

// Filename conventions — conclusive on their own, wherever the file lives.
const NAME_PATTERNS: RegExp[] = [
  /(^|\/)test_[^/]+\.py$/,
  /(^|\/)[^/]+_test\.py$/,
  /\.(test|spec)\.[jt]sx?$/,
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

const SKIP_DIRS = new Set([
  ".git", "node_modules", "venv", ".venv", "dist", "build", "target",
  "__pycache__", ".pytest_cache", ".next", "vendor", "coverage", ".tox",
]);

export function isTestFile(p0: string): boolean {
  const p = p0.split(path.sep).join("/");
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
    /\.(?:toBe|toEqual|toMatch|toHave\w*|toThrow|toContain|toBeCalled\w*)\b/.source,
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

const MOCK = new RegExp(
  [
    /\bMock\w*\s*\(/.source,
    /\bMagicMock\b/.source,
    /\bAsyncMock\b/.source,
    /\bmock\.\w+/.source,
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

/**
 * Does an unescaped `delim` close on the same line, starting after position `start`?
 * Used to distinguish a real string literal from a stray quote inside a regex literal.
 */
function closesOnSameLine(text: string, start: number, delim: string): boolean {
  for (let j = start + 1; j < text.length; j += 1) {
    const ch = text[j];
    if (ch === "\n") return false;
    if (ch === "\\") {
      j += 1;
      continue;
    }
    if (ch === delim) return true;
  }
  return false;
}

const BLANK = "\x00"; // neutral filler: matches no pattern, preserves offsets
const HASH_COMMENT_EXTS = new Set([".py", ".rb", ".sh", ".pl", ".r", ".yaml", ".yml"]);

/**
 * Blank out string-literal contents and comments, preserving line and column offsets.
 *
 * The lesson is to decide on code, not on raw text. A test
 * fixture, a regex pattern, or a docstring that *mentions* `it.skip` or `time.sleep` is
 * not a focused test or a sleep — and a scanner that can't tell the difference floods
 * any repo containing meta-tests. String *lengths* are preserved because one check
 * (assertion-with-message, taught but not gated) legitimately cares whether a message
 * string exists.
 */
function stripNoncode(text: string, ext: string): string {
  const out: string[] = [];
  let i = 0;
  const n = text.length;
  const hashComments = HASH_COMMENT_EXTS.has(ext);
  const slashComments = !hashComments;
  while (i < n) {
    const c = text[i];

    // line comments
    if (hashComments && c === "#") {
      while (i < n && text[i] !== "\n") {
        out.push(BLANK);
        i += 1;
      }
      continue;
    }
    if (slashComments && c === "/" && i + 1 < n && text[i + 1] === "/") {
      while (i < n && text[i] !== "\n") {
        out.push(BLANK);
        i += 1;
      }
      continue;
    }
    // block comments
    if (slashComments && c === "/" && i + 1 < n && text[i + 1] === "*") {
      while (i < n && !(text[i] === "*" && i + 1 < n && text[i + 1] === "/")) {
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      out.push(BLANK.repeat(Math.min(2, n - i)));
      i += 2;
      continue;
    }

    // triple-quoted strings (python)
    if (c !== undefined && "\"'".includes(c) && text.slice(i, i + 3) === c.repeat(3) && (c === '"' || c === "'")) {
      const delim = text.slice(i, i + 3);
      out.push(delim);
      i += 3;
      while (i < n && text.slice(i, i + 3) !== delim) {
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      out.push(delim.slice(0, Math.max(0, Math.min(3, n - i))));
      i += 3;
      continue;
    }

    // template literals: legitimately multi-line, so — like triple-quoted strings —
    // they continue across newlines until the closing backtick. ${...} interpolation
    // is simply blanked along with the rest; we only need to not match inside it.
    if (c === "`") {
      out.push(c);
      i += 1;
      while (i < n && text[i] !== "`") {
        if (text[i] === "\\" && i + 1 < n) {
          out.push(BLANK, text[i + 1] === "\n" ? "\n" : BLANK);
          i += 2;
          continue;
        }
        out.push(text[i] === "\n" ? "\n" : BLANK);
        i += 1;
      }
      if (i < n && text[i] === "`") {
        out.push("`");
        i += 1;
      }
      continue;
    }

    // single/double quoted strings.
    //
    // Only enter string mode when a matching delimiter actually closes on THIS line.
    // A quote character with no partner is far more likely to be a quote inside a regex
    // literal (`/['"]/` — an ordinary pattern for splitting or validating quoted text)
    // than the start of a string. Blanking to end-of-line in that case erased real code:
    // `const ok = /['"]/.test(x); expect(ok).toBe(true);` lost its assertion and the test
    // was reported as assertion-free. Treating the orphan quote as an ordinary character
    // keeps the line intact, and costs nothing when it really was an unterminated string
    // (which is a syntax error the test runner will report anyway).
    if (c !== undefined && "\"'".includes(c)) {
      if (!closesOnSameLine(text, i, c)) {
        out.push(c);
        i += 1;
        continue;
      }
      const delim = c;
      out.push(delim);
      i += 1;
      while (i < n && text[i] !== delim) {
        if (text[i] === "\\" && i + 1 < n) {
          out.push(BLANK.repeat(2));
          i += 2;
          continue;
        }
        out.push(BLANK);
        i += 1;
      }
      if (i < n && text[i] === delim) {
        out.push(delim);
        i += 1;
      }
      continue;
    }

    out.push(c ?? "");
    i += 1;
  }
  return out.join("");
}

// Exception types used to probe for optional capabilities; `except ImportError: pass` is
// a compatibility idiom, not a swallowed test failure.
const COMPAT_EXCEPTIONS = /\b(AttributeError|ImportError|ModuleNotFoundError|NotImplementedError|SkipTest)\b/;

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

function scanFile(filePath: string): Finding[] {
  const findings: Finding[] = [];
  let raw: string;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
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
  const assertingLocals = new Set<string>();
  {
    const defs: Array<{ name: string; line: number; indent: number }> = [];
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i] ?? "";
      const m = LOCAL_DEF.exec(line);
      if (!m) continue;
      const name = m.slice(1).find((g) => g !== undefined);
      if (name) defs.push({ name, line: i, indent: line.length - line.trimStart().length });
    }
    const allLocals = new Set(defs.map((d) => d.name));
    // name -> { assertsDirectly, callee names defined in this file }
    const info = new Map<string, { direct: boolean; calls: Set<string> }>();
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
      for (let j = def.line; j < end; j += 1) {
        const line = lines[j] ?? "";
        if (assertsOnLine(line)) entry.direct = true;
        for (const m of line.matchAll(CALL_NAME)) {
          const callee = m[1];
          if (callee !== undefined && callee !== def.name && allLocals.has(callee)) {
            entry.calls.add(callee);
          }
        }
      }
      info.set(def.name, entry);
    }
    for (const [name, entry] of info) if (entry.direct) assertingLocals.add(name);
    let grew = true;
    while (grew) {
      grew = false;
      for (const [name, entry] of info) {
        if (assertingLocals.has(name)) continue;
        for (const callee of entry.calls) {
          if (assertingLocals.has(callee)) {
            assertingLocals.add(name);
            grew = true;
            break;
          }
        }
      }
    }
  }
  function delegatesToLocal(text: string): boolean {
    for (const m of text.matchAll(CALL_NAME)) {
      if (m[1] !== undefined && assertingLocals.has(m[1])) return true;
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
    if (swallowed && inTest.has(i) && !COMPAT_EXCEPTIONS.test(text)) {
      findings.push(
        makeFinding(
          filePath,
          i + 1,
          "swallowed-error",
          "exception discarded — the failure this test exists to catch is thrown away",
          "P1",
        ),
      );
    }
    if (SLEEP_WAIT_HARD.test(text) || (SLEEP_WAIT_SOFT.test(text) && !hasFakeTimers)) {
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
    if (asserts.length === 0 && !delegates && real.length > NO_ASSERTION_MIN_BODY) {
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
