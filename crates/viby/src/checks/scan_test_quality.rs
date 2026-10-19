//! viby-toolkit test-quality scanner — the executable half of /viby-toolkit:test.
//!
//! Scans test files for the defect classes that empirical studies find in
//! agent-written tests, and that a passing suite cannot reveal:
//!
//!   no-assertion        a test that asserts nothing — it passes if the code does anything
//!   tautology           an assertion that cannot fail (expect(true).toBe(true))
//!   over-mocking        mock density so high the test exercises mocks, not code
//!   focused-or-skipped  .only / .skip / @Ignore left in, silently shrinking the suite
//!   sleep-wait          waiting on a timer instead of an observable condition
//!   swallowed-error     except/catch that discards the failure it was meant to catch
//!
//! Usage:
//!   viby scan-test-quality                 # test files changed vs HEAD
//!   viby scan-test-quality <path> [...]    # explicit files or directories
//!   viby scan-test-quality --all           # every test file in the repo
//!   viby scan-test-quality --json          # machine-readable findings
//!   viby scan-test-quality --quiet         # findings only, no summary
//!
//! Exit: 0 = clean, 1 = findings, 2 = nothing to scan (not a failure).
//!
//! PRECISION IS THE CONTRACT. Every check here is one an agent can act on without
//! arguing. Noisy-but-classic smells were deliberately left out — magic numbers and
//! conditional test logic fire constantly on legitimate table-driven tests, and a
//! scanner that cries wolf gets ignored, taking the real findings with it. Each
//! finding is a heuristic over text, not a proof: confirm before "fixing".
//!
//! Port of `skills/test/scripts/scan-test-quality.ts`. The patterns below are the TypeScript
//! regex sources verbatim; `js()` translates JavaScript regex semantics (ASCII `\w`, `\d`, `\b`,
//! JavaScript's `\s` and `.`, and `/m` line anchors) so the matches stay the same.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{json, Value};

use crate::core::args::{flag, parse_or_exit};
use crate::core::git::git_within;
use crate::core::strip::strip_noncode;

// ---------------------------------------------------------------- JavaScript regex semantics

/// JavaScript's `\s` (no U+0085, which Rust's white space includes; plus the byte-order mark).
const JS_SPACE: &str = "\t\n\u{b}\u{c}\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
/// JavaScript's `.`: anything but a line terminator.
const JS_DOT: &str = "[^\n\r\u{2028}\u{2029}]";
const JS_LINE_END: &str = "[\n\r\u{2028}\u{2029}]";

fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// ASCII `\b` for fancy-regex, which has no `(?-u:…)`.
const ASCII_BOUNDARY: &str =
    "(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))";

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// For `regex`: `\b` as `(?-u:\b)`.
    Plain,
    /// For `fancy-regex`: `\b` with lookarounds.
    Fancy,
    /// For `regex`, matching a superset of the fancy pattern: lookaheads and `/m` anchors
    /// dropped, backreferences as any text. A line it rejects needs no backtracking engine.
    Prefilter,
}

/// Translate a JavaScript regex source (as `.source` printed it) to the same pattern in Rust.
/// `multiline` gives `^` and `$` JavaScript's `/m` meaning.
fn js(src: &str, multiline: bool, mode: Mode) -> String {
    let mut out = String::new();
    let mut in_class = false;
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        match c {
            '\\' => {
                let e = chars.get(i).copied();
                i += 1;
                match (e, in_class) {
                    (Some('s'), true) => out.push_str(JS_SPACE),
                    (Some('s'), false) => out.push_str(&format!("[{JS_SPACE}]")),
                    (Some('w'), true) => out.push_str("A-Za-z0-9_"),
                    (Some('w'), false) => out.push_str("[A-Za-z0-9_]"),
                    (Some('d'), true) => out.push_str("0-9"),
                    (Some('d'), false) => out.push_str("[0-9]"),
                    (Some('b'), false) if mode == Mode::Fancy => out.push_str(ASCII_BOUNDARY),
                    (Some('b'), false) => out.push_str(r"(?-u:\b)"),
                    (Some('k'), false) if mode == Mode::Prefilter => {
                        while i < chars.len() && chars[i] != '>' {
                            i += 1;
                        }
                        i += 1;
                        out.push_str("(?s:.*?)");
                    }
                    (Some('/'), _) => out.push('/'),
                    (Some(e), _) => {
                        out.push('\\');
                        out.push(e);
                    }
                    (None, _) => out.push_str(r"\\"),
                }
            }
            '(' if mode == Mode::Prefilter
                && !in_class
                && chars.get(i) == Some(&'?')
                && chars.get(i + 1) == Some(&'!') =>
            {
                // Skip the negative lookahead, to its balancing paren.
                let mut depth = 1;
                while i < chars.len() && depth > 0 {
                    match chars[i] {
                        '\\' => i += 1,
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                if chars.get(i) == Some(&'^') {
                    out.push('^');
                    i += 1;
                }
            }
            '[' => out.push_str(r"\["),
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '.' if !in_class => out.push_str(JS_DOT),
            '^' | '$' if multiline && !in_class && mode == Mode::Prefilter => {}
            '^' if multiline && !in_class => out.push_str(&format!("(?:^|(?<={JS_LINE_END}))")),
            '$' if multiline && !in_class => out.push_str(&format!("(?:$|(?={JS_LINE_END}))")),
            c => out.push(c),
        }
    }
    out
}

/// A JavaScript regex, compiled once: by `regex`, or by `fancy-regex` when it needs lookaround
/// or a backreference. A fancy one carries a `regex` prefilter, because the backtracking engine
/// on every line of a large suite was several times slower than the TypeScript.
enum Re {
    Plain(regex::Regex),
    Fancy(regex::Regex, fancy_regex::Regex),
}

/// A match's capture groups; `None` for a group that did not take part.
type Groups<'t> = Vec<Option<&'t str>>;

impl Re {
    fn new(parts: &[&str], multiline: bool) -> Re {
        let fancy = multiline
            || parts
                .iter()
                .any(|p| p.contains("(?!") || p.contains(r"\k<"));
        let build = |mode| {
            let src: Vec<String> = parts.iter().map(|p| js(p, multiline, mode)).collect();
            src.join("|")
        };
        // The patterns are constants of this file, so a failure here is a bug in this file.
        let plain = |src: String| regex::Regex::new(&src).expect("scan-test-quality pattern");
        if fancy {
            Re::Fancy(
                plain(build(Mode::Prefilter)),
                fancy_regex::Regex::new(&build(Mode::Fancy)).expect("scan-test-quality pattern"),
            )
        } else {
            Re::Plain(plain(build(Mode::Plain)))
        }
    }

    fn test(&self, text: &str) -> bool {
        match self {
            Re::Plain(r) => r.is_match(text),
            Re::Fancy(pre, r) => pre.is_match(text) && r.is_match(text).unwrap_or(false),
        }
    }

    /// The first match, as `exec` returns it.
    fn exec<'t>(&self, text: &'t str) -> Option<Groups<'t>> {
        match self {
            Re::Plain(r) => r
                .captures(text)
                .map(|c| c.iter().map(|g| g.map(|g| g.as_str())).collect()),
            Re::Fancy(pre, _) if !pre.is_match(text) => None,
            Re::Fancy(_, r) => r
                .captures(text)
                .ok()
                .flatten()
                .map(|c| c.iter().map(|g| g.map(|g| g.as_str())).collect()),
        }
    }

    /// Every match, as `matchAll` returns them.
    fn all<'t>(&self, text: &'t str) -> Vec<Groups<'t>> {
        match self {
            Re::Plain(r) => r
                .captures_iter(text)
                .map(|c| c.iter().map(|g| g.map(|g| g.as_str())).collect())
                .collect(),
            Re::Fancy(pre, _) if !pre.is_match(text) => Vec::new(),
            Re::Fancy(_, r) => r
                .captures_iter(text)
                .filter_map(Result::ok)
                .map(|c| c.iter().map(|g| g.map(|g| g.as_str())).collect())
                .collect(),
        }
    }

    /// `text.replace(re, "")`: the first match removed.
    fn remove_first(&self, text: &str) -> String {
        let found = match self {
            Re::Plain(r) => r.find(text).map(|m| m.range()),
            Re::Fancy(_, r) => r.find(text).ok().flatten().map(|m| m.range()),
        };
        match found {
            Some(range) => format!("{}{}", &text[..range.start], &text[range.end..]),
            None => text.to_string(),
        }
    }
}

macro_rules! re {
    ($name:ident, [$($p:expr),+ $(,)?]) => {
        static $name: LazyLock<Re> = LazyLock::new(|| Re::new(&[$($p),+], false));
    };
    ($name:ident, m, [$($p:expr),+ $(,)?]) => {
        static $name: LazyLock<Re> = LazyLock::new(|| Re::new(&[$($p),+], true));
    };
}

/// Capture group `i` of a match.
fn group<'t>(m: &Groups<'t>, i: usize) -> Option<&'t str> {
    m.get(i).copied().flatten()
}

// ---------------------------------------------------------------- file detection

// Filename conventions — conclusive on their own, wherever the file lives.
re!(
    NAME_PATTERNS,
    [
        r"(^|\/)test_[^/]+\.py$",
        r"(^|\/)[^/]+_test\.py$",
        r"\.(test|spec)\.[jt]sx?$",
        // Cypress's own convention. Without it, a repo using idiomatic Cypress layout produced
        // zero findings — not because it was clean, but because every file was invisible.
        r"\.cy\.[jt]sx?$",
        r"(^|\/)cypress\/(e2e|integration|component)\/.*\.[jt]sx?$",
        r"(^|\/)[^/]+_test\.go$",
        r"(^|\/)[^/]*Tests?\.(java|kt|cs)$",
        r"(^|\/)[^/]+_spec\.rb$",
        r"(^|\/)[^/]+_test\.(rs|php|swift)$",
    ]
);
// A test directory — suggestive, but only when the file isn't support code.
re!(
    DIR_PATTERNS,
    [r"(^|\/)(tests?|specs?|__tests__)\/.*\.(py|[jt]sx?|go|rb|rs|java|kt)$"]
);
// Support code that lives under a test directory but contains no tests. Applied ONLY
// to the directory rule, so `tests/utils/parse_test.go` still counts on its filename.
re!(
    DIR_DISQUALIFY,
    [
        r"(^|\/)(scripts?|fixtures?|helpers?|utils?|mocks?|__mocks__|__snapshots__|testdata|factories)\/"
    ]
);
// Support FILES that live in a test directory but contain no tests of their own — shared
// browser/DOM helpers, setup files, and config. Measured against real suites, these were a
// steady false-positive source: a polling utility's timeout guard is not a flaky test.
re!(
    SUPPORT_FILE,
    [
        r"(^|\/)([\w.-]*[Uu]tils?|[\w.-]*[Hh]elpers?|setup(Tests?)?|test-setup|globalSetup|\w+\.config|conftest|serve|commonTests|fixtures?|testServer)\.[jt]sx?$"
    ]
);

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "venv",
    ".venv",
    "dist",
    "build",
    "target",
    "__pycache__",
    ".pytest_cache",
    ".next",
    "vendor",
    "coverage",
    ".tox",
];

pub fn is_test_file(p0: &str) -> bool {
    let p = p0.replace(std::path::MAIN_SEPARATOR, "/");
    if SUPPORT_FILE.test(&p) {
        return false;
    }
    if NAME_PATTERNS.test(&p) {
        return true;
    }
    DIR_PATTERNS.test(&p) && !DIR_DISQUALIFY.test(&p)
}

// ---------------------------------------------------------------- paths, as node:path (posix)

/// `path.normalize`.
fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let absolute = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            s => parts.push(s),
        }
    }
    let mut out = parts.join("/");
    if out.is_empty() {
        if absolute {
            return "/".to_string();
        }
        return if trailing { "./" } else { "." }.to_string();
    }
    if trailing {
        out.push('/');
    }
    if absolute {
        format!("/{out}")
    } else {
        out
    }
}

/// `path.join`.
fn join(parts: &[&str]) -> String {
    let parts: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return ".".to_string();
    }
    normalize(&parts.join("/"))
}

/// `path.resolve(dir, spec)`.
fn resolve(dir: &str, spec: &str) -> String {
    let mut combined = String::new();
    for p in [spec, dir] {
        if p.is_empty() {
            continue;
        }
        combined = if combined.is_empty() {
            p.to_string()
        } else {
            format!("{p}/{combined}")
        };
        if p.starts_with('/') {
            break;
        }
    }
    if !combined.starts_with('/') {
        let cwd = std::env::current_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        combined = format!("{cwd}/{combined}");
    }
    let out = normalize(&combined);
    match out.strip_suffix('/') {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => out,
    }
}

/// `path.dirname`.
fn dirname(p: &str) -> String {
    let b = p.as_bytes();
    if b.is_empty() {
        return ".".to_string();
    }
    let has_root = b[0] == b'/';
    let mut end = None;
    let mut matched_slash = true;
    for i in (1..b.len()).rev() {
        if b[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None => if has_root { "/" } else { "." }.to_string(),
        Some(1) if has_root => "//".to_string(),
        Some(e) => p[..e].to_string(),
    }
}

/// `path.extname`.
fn extname(p: &str) -> &str {
    let b = p.as_bytes();
    let (mut start_dot, mut start_part, mut end) = (None, 0, None);
    let mut matched_slash = true;
    let mut pre_dot_state = 0;
    for i in (0..b.len()).rev() {
        let c = b[i];
        if c == b'/' {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            continue;
        }
        if end.is_none() {
            matched_slash = false;
            end = Some(i + 1);
        }
        if c == b'.' {
            if start_dot.is_none() {
                start_dot = Some(i);
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot.is_some() {
            pre_dot_state = -1;
        }
    }
    match (start_dot, end) {
        (Some(d), Some(e))
            if pre_dot_state != 0 && !(pre_dot_state == 1 && d + 1 == e && d == start_part + 1) =>
        {
            &p[d..e]
        }
        _ => "",
    }
}

/// JavaScript's `<` on strings: UTF-16 code unit order.
fn js_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn walk_repo(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else {
            continue;
        };
        // readdirSync returns names in byte order; every caller sorts the result anyway.
        let mut entries: Vec<(String, bool)> = rd
            .filter_map(Result::ok)
            .map(|e| {
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                (e.file_name().to_string_lossy().into_owned(), is_dir)
            })
            .collect();
        entries.sort();
        for (name, is_dir) in entries {
            if is_dir {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(join(&[&dir, &name]));
                }
            } else {
                let full = join(&[&dir, &name]);
                if is_test_file(&full) {
                    out.push(normalize(&full));
                }
            }
        }
    }
    out
}

fn is_file(p: &str) -> bool {
    fs::metadata(p).map(|m| m.is_file()).unwrap_or(false)
}

/// Test files touched vs HEAD, including untracked. Empty if not a git repo.
fn changed_files() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let cmds: [&[&str]; 3] = [
        &["diff", "--name-only", "HEAD"],
        &["diff", "--name-only", "--staged"],
        &["ls-files", "--others", "--exclude-standard"],
    ];
    for cmd in cmds {
        let r = git_within(Path::new("."), cmd, Duration::from_secs(10));
        if r.ok && !r.stdout.is_empty() {
            for l in r.stdout.split('\n') {
                if !l.chars().all(js_space) && !out.iter().any(|o| o == l) {
                    out.push(l.to_string());
                }
            }
        }
    }
    let mut out: Vec<String> = out
        .into_iter()
        .filter(|f| is_test_file(f) && is_file(f))
        .collect();
    out.sort_by(|a, b| js_cmp(a, b));
    out
}

// ---------------------------------------------------------------- patterns

// A line that starts a new test case. Containers (describe/class) deliberately excluded.
re!(
    TEST_START,
    [
        r"^\s*(?:async\s+)?def\s+test\w*\s*\(",         // python
        r"^\s*(?:it|test|specify)\s*(?:\.\w+)*\s*[(<]", // js/ts jest/mocha/vitest
        r#"^\s*(?:it|specify)\s+['"]"#,                 // ruby rspec
        r"^\s*func\s+(?:Test|Benchmark|Fuzz)\w*\s*\(",  // go
        r"^\s*#\[test\]",                               // rust
        r"^\s*@Test\b",                                 // java/kotlin junit
        r"^\s*\[(?:Test|Fact|Theory)\]",                // c#
    ]
);

re!(
    ASSERT,
    [
        r"\bassert\w*\s*[(\s]",
        // `assert.ok(...)` / `assert.deepStrictEqual(...)` — the node:assert and chai style.
        // The pattern above cannot match it (it needs `(` or space right after `assert`),
        // which made the scanner miss the assertion style of its own test framework.
        r"\bassert\.\w+\s*\(",
        r"\bexpect\s*\(",
        r"\bassertThat\s*\(",
        r"\bverify\s*\(",
        r"\bshould\b",
        r"\brequire\.\w+\s*\(",
        r"\bt\.(?:Error|Fatal|Errorf|Fatalf)\s*\(",
        // ava: `t.is(...)`, `t.deepEqual(...)`, `t.throws(...)` — an entire framework's
        // assertion family that matched nothing at all.
        r"\bt\.(?:is|not|true|false|deepEqual|notDeepEqual|like|throws\w*|notThrows\w*|regex|notRegex|pass|fail|snapshot)\s*\(",
        // Any `.toSomething(` matcher, plus the resolves/rejects modifiers. The old fixed list
        // (toBe|toEqual|toMatch|toHave*|toThrow|toContain|toBeCalled*) missed toStrictEqual,
        // toBeTruthy, toBeVisible, toBeCloseTo, toMatchObject, toMatchSnapshot and more. That
        // list is the ONLY thing that can recognise an assertion chain split across lines, so
        // every gap in it became a false `no-assertion` — 9 of vite's 14 came from exactly this.
        r"\.to[A-Z]\w*\s*\(",
        r"\.(?:resolves|rejects)\b",
        // `expect.poll(...)`, `expect.soft(...)`, `expect.unreachable()` — `expect` not
        // immediately followed by `(`, so the plain `expect\s*\(` alternative cannot see it.
        r"\bexpect\s*\.\s*\w+",
        r"\bassert_\w+",
        r"\bXCTAssert\w*\s*\(",
        r"\bpytest\.raises\s*\(", // expecting a raise IS an assertion
        r"\bassertRaises\w*\s*\(",
        r"\bassertWarns\w*\s*\(",
        r"\bpytest\.warns\s*\(",
        r"\bshould_panic\b",
        r"\bassert!\s*\(", // rust
        r"\bassert_eq!\s*\(",
        r"\bexpect_err\s*\(",
        r"\bhypothesis\b",
        // `self.fail("...")` / `pytest.fail(...)` — an unconditional failure IS an assertion
        // mechanism, and is the standard way to assert "we should not have reached here".
        r"\bfail\s*\(",
    ]
);

// An assertion function bound to a short local alias, e.g. `eq = self.assertEqual`, then
// called as `eq(a, b)`. Extremely common in large suites (CPython uses it throughout), and
// without it every such test looks assertion-free. Matches the BINDING (no call parens).
re!(
    ASSERT_ALIAS_BINDING,
    [r"^\s*(\w+)\s*=\s*(?:self\.|cls\.|this\.)?(\w*(?:[aA]ssert|[eE]xpect)\w*|fail)\s*$"]
);

// A name bound to ANY method reference or partial application, e.g. `check = self.check_match`
// or `raises = partial(self.assertRaises, ValueError)`. Only counted as an assertion alias
// when the target is independently known to assert, so this cannot excuse an arbitrary call.
// Measured on CPython, custom-named aliases like `check = self.check_match` were a distinct
// false-positive class the assert/expect-named pattern above could never catch.
re!(
    GENERIC_ALIAS_BINDING,
    [r"^\s*(\w+)\s*=\s*(?:partial\s*\(\s*)?(?:self\.|cls\.|this\.)?(\w+)"]
);
re!(ASSERTISH_NAME, [r"(?:[aA]ssert|[eE]xpect)"]);

// `except X: pass` paired with an `else:` branch that fails — the manual raise-assertion
// idiom (`try: bad() / except ValueError: pass / else: self.fail("no raise")`). The `pass`
// is the SUCCESS path and the `else` is the assertion, so this is the opposite of a
// swallowed failure. On CPython this single pattern accounted for a third of all
// swallowed-error findings, and a 12-sample audit found zero true positives overall.
re!(RAISE_ASSERTION_ELSE, [r"^\s*else\s*:"]);
re!(FAIL_CALL, [r"\b(?:self\.fail|pytest\.fail|fail)\s*\("]);

// The JS equivalent: `try { await x(); expect.unreachable() } catch {}` asserts that the
// call throws, then checks the error's effects after the block. The empty catch is the
// success path, not a discarded failure.
re!(
    UNREACHABLE_ASSERTION,
    [r"\b(?:expect\s*\.\s*unreachable|assert\s*\.\s*fail|fail)\s*\("]
);

re!(
    MOCK,
    [
        r"\bMock\w*\s*\(",
        r"\bMagicMock\b",
        r"\bAsyncMock\b",
        // `mock.patch(...)` etc. — but NOT `.mock.calls` / `.mock.results`, which INSPECT a
        // mock inside an assertion. Counting those as mocking inflated the mock density of
        // tests that were, in fact, asserting carefully on a single spy.
        r"\bmock\.(?!calls\b|results\b|instances\b|lastCall\b|invocationCallOrder\b)\w+",
        r"\bpatch\s*\(",
        r"@patch\b",
        r"\bjest\.(?:mock|fn|spyOn)\b",
        r"\bvi\.(?:mock|fn|spyOn)\b",
        r"\bsinon\.\w+",
        r"\bmockito\b",
        r"\bwhen\s*\(",
        r"\bgomock\b",
        r"\bcreateMock\w*\s*\(",
        r"\bstub\w*\s*\(",
        r"\bspyOn\s*\(",
        r"\bmockReturnValue\b",
        r"\bmockResolvedValue\b",
        r"\bmockImplementation\b",
    ]
);

re!(
    TAUTOLOGY,
    m,
    [
        r"\bassert(?:True|_true)\s*\(\s*(?:True|true|1)\s*[,)]",
        r"\bassert\s+(?:True|true)\s*(?:$|#)",
        r"\bassertFalse\s*\(\s*(?:False|false|0)\s*[,)]",
        r"\bexpect\s*\(\s*(?:true|1)\s*\)\s*\.\s*(?:toBe|toEqual)\s*\(\s*(?:true|1)\s*\)",
        r"\bassert\s+1\s*==\s*1\b",
        r"\bassertEquals?\s*\(\s*(?<ae>\w+)\s*,\s*\k<ae>\s*\)",
        r"\bexpect\s*\(\s*(?<ex>\w+)\s*\)\s*\.\s*(?:toBe|toEqual)\s*\(\s*\k<ex>\s*\)",
        r"\bassert\s+(?<eq>\w+)\s*==\s*\k<eq>\s*(?:$|#)",
    ]
);

re!(
    FOCUSED_OR_SKIPPED,
    [
        r"\b(?:describe|it|test|context)\s*\.\s*(?:only|skip|todo|failing)\b",
        r"\bf(?:describe|it|context)\s*\(",
        r"\bx(?:describe|it|test|context)\s*\(",
        r"@pytest\.mark\.(?:skip|xfail)\b",
        r"@unittest\.(?:skip|expectedFailure)\b",
        r"\bt\.Skip\s*\(",
        r"#\[ignore\]",
        r"@(?:Ignore|Disabled)\b",
        r"\.pending\b",
        r"\bpytest\.skip\s*\(",
    ]
);

// A real, blocking wait. Fake timers NEVER excuse these: a fake-timer clock does not make
// `time.sleep` non-blocking, and an *awaited* real-timer promise would simply hang under
// fake timers — so its presence means the wait is genuine. Keeping these unconditional
// fixes a silent false negative: one suite calling `useFakeTimers()` used to suppress the
// flaky real waits in every other, unrelated suite in the same file.
re!(
    SLEEP_WAIT_HARD,
    [
        r"\btime\.sleep\s*\(",
        r"\bThread\.sleep\s*\(",
        // Bounded `.` rather than `[^)]*`: the idiomatic form is
        // `await new Promise((r) => setTimeout(r, 2000))`, whose arrow-function parameter list
        // closes a paren before `setTimeout` is reached.
        r"\bawait\s+new\s+Promise\s*\(.{0,120}?setTimeout",
        r"\bawait\s+(?:delay|sleep|wait)\s*\(\s*\d",
        r"\btime\.Sleep\s*\(",
        r"\busleep\s*\(",
    ]
);
// A bare timer call, which fake timers DO make deterministic — so it is excused when the
// file installs them.
re!(SLEEP_WAIT_SOFT, [r"\b(?:setTimeout|setInterval)\s*\("]);

// Timer uses that are NOT arbitrary waits, measured against real TypeScript suites where
// they accounted for most of the `sleep-wait` findings:
//
//  - `await new Promise(r => setTimeout(r))` / `setTimeout(r, 0)` — a macrotask "tick
//    flush". Deterministic, idiomatic, and the opposite of a flaky sleep: there is no
//    duration to race against.
//  - a `setTimeout` whose callback rejects or throws — that is a timeout *guard* wrapping a
//    poll loop, i.e. exactly the pattern this check is supposed to recommend.
re!(
    TICK_FLUSH,
    [r"\b(?:setTimeout|setImmediate)\s*\(\s*[^,()]*(?:\([^)]*\)\s*=>\s*[^,]*)?\s*(?:,\s*0\s*)?\)"]
);
re!(TIMEOUT_GUARD, [r"\b(?:reject|throw)\b"]);
re!(
    FAKE_TIMERS,
    [r"\b(useFakeTimers|sinon\.useFakeTimers|vi\.useFakeTimers|freeze_time|freezegun)\b"]
);

// Single-line forms: `except ValueError: pass`, `catch (e) {}`.
re!(
    SWALLOWED,
    m,
    [
        r"\bexcept\b[^:\n]*:\s*pass\s*(?:$|#)",
        r"\bcatch\s*\([^)]*\)\s*\{\s*\}",
        r"\bcatch\s*\{\s*\}",
        r"\bexcept\b[^:\n]*:\s*\.\.\.\s*$",
    ]
);
// Multi-line forms, far more common than the single-line ones:
//     except ValueError:        }  catch (e) {
//         pass                  }
re!(HANDLER_OPEN_PY, [r"^\s*except\b[^:]*:\s*(?:#.*)?$"]);
re!(
    HANDLER_OPEN_JS,
    [r"^\s*\}?\s*catch\s*(?:\([^)]*\))?\s*\{\s*(?:\/\/.*)?$"]
);
re!(HANDLER_NOOP_PY, [r"^\s*(?:pass|\.\.\.)\s*(?:#.*)?$"]);
re!(HANDLER_NOOP_JS, [r"^\s*\}\s*(?:\/\/.*)?$"]);

// Lines that are comments or strings-only are poor evidence; skip the obvious ones.
re!(COMMENT_ONLY, [r#"^\s*(#|\/\/|\*|\/\*|"""|'''|--)"#]);

// Callables defined in the file under scan, so delegated assertions can be recognised.
re!(
    LOCAL_DEF,
    [
        r"^\s*(?:async\s+)?(?:def|function|fn|sub)\s+(\w+)",
        r"^\s*(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s*)?(?:\(|function)",
        r"^\s*(?:public|private|protected|static|\s)*[\w<>[\],\s]+?\s+(\w+)\s*\([^)]*\)\s*\{",
    ]
);

re!(CALL_NAME, [r"\b(\w+)\s*\("]);
// A method call through a receiver: `self.checkParam(...)`, `this.expectOk(...)`.
re!(RECEIVER_CALL, [r"\b(?:self|this|cls)\s*\.\s*(\w+)\s*\("]);
re!(WORD, [r"\b(\w+)\b"]);
// A line with nothing but white space, blanked filler and quotes.
re!(FILLER_ONLY, [r#"^[\s\x00"'`]*$"#]);

// A declaration whose body does nothing — `pass`, `...`, `raise NotImplementedError`, or
// only a docstring. It is an abstract or stub method, so the real implementation (and its
// assertions) live somewhere this scanner cannot see. Calling one is not evidence that a
// test asserts nothing.
re!(
    STUB_BODY,
    [
        r"^\s*(?:pass|\.\.\.|raise\s+NotImplementedError|return\s+NotImplemented|throw\s+new\s+Error)\b"
    ]
);

// Modules that hold shared test infrastructure, indexed even without a resolvable import.
re!(
    SHARED_INFRA,
    [
        r"(^|\/)(\w*_tests?|util|utils|support|helpers?|base|common\w*|testcase|mixins?|shared|fixtures?)\.(py|[jt]sx?)$"
    ]
);

// Import specifiers, read from RAW text — a stripped file has its specifiers blanked.
re!(
    JS_IMPORT,
    [r#"(?:from|import)\s*\(?\s*["']([^"']+)["']|require\(\s*["']([^"']+)["']\s*\)"#]
);
re!(
    PY_IMPORT,
    m,
    [r"^[ \t]*(?:from[ \t]+([\w.]+)[ \t]+import|import[ \t]+([\w.]+))"]
);
re!(JS_EXT, [r"\.[jt]sx?$"]);

const MAX_RELATED_FILES: usize = 12; // per scanned file, so a wide import graph can't blow up runtime

// Capability probes AND loop-termination sentinels. `except ImportError: pass` is a
// compatibility idiom; `except StopIteration: pass` after draining a generator, or
// `except Empty: pass` draining a queue, is how those loops are DOCUMENTED to end. Measured
// on CPython, "drain until exhausted" was one of the largest remaining false-positive
// classes for this check.
re!(
    COMPAT_EXCEPTIONS,
    [
        r"\b(AttributeError|ImportError|ModuleNotFoundError|NotImplementedError|SkipTest|StopIteration|StopAsyncIteration|Empty|EOFError|BlockingIOError|BrokenPipeError|ConnectionResetError)\b"
    ]
);

const MOCK_DENSITY_MAX: usize = 4; // mock/stub calls in one test before it tests mocks
const NO_ASSERTION_MIN_BODY: usize = 2; // code lines a test needs before "no assertion" is meaningful

// Deliberately NOT checked mechanically, though /viby-toolkit:test still teaches both:
//   assertion-roulette  — 4+ unexplained assertions. Measured against CPython's own suite
//                         this fired ~6.2 times per file on idiomatic code (four
//                         consecutive assertRaises calls is normal, readable style).
//   magic-number-test   — fires constantly on legitimate table-driven tests.
// Both are real authoring smells and useless as gates. A scanner that cries wolf gets
// ignored, which costs more than the findings it would have surfaced.

#[derive(Clone, Debug)]
struct Finding {
    file: String,
    line: usize,
    check: &'static str,
    message: String,
    severity: &'static str,
}

fn finding(
    file: &str,
    line: usize,
    check: &'static str,
    message: &str,
    severity: &'static str,
) -> Finding {
    Finding {
        file: file.to_string(),
        line,
        check,
        message: message.to_string(),
        severity,
    }
}

type LineEntry<'a> = (usize, &'a str);

/// `line.length - line.trimStart().length`, in UTF-16 units as JavaScript counted it.
fn indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| js_space(*c))
        .map(char::len_utf16)
        .sum()
}

/// `!line.trim()`.
fn is_blank(line: &str) -> bool {
    line.chars().all(js_space)
}

/// [nameLineIndex, body] for each test case in the file.
///
/// Nested declarations are NOT new tests: a table-driven test commonly defines a local
/// helper called `test(...)`, and treating that as a separate test case truncates the
/// real one — which then looks assertion-free because its assertions live in the helper.
/// Indentation decides: a declaration indented deeper than the test it sits inside is
/// part of that test.
fn split_tests<'a>(lines: &[&'a str]) -> Vec<(usize, Vec<LineEntry<'a>>)> {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !TEST_START.test(line) {
            continue;
        }
        let ind = indent(line);
        if starts.last().is_some_and(|last| ind > last.1) {
            continue;
        }
        starts.push((i, ind));
    }

    let mut out = Vec::new();
    for (n, &(start, ind)) in starts.iter().enumerate() {
        let hard_end = starts.get(n + 1).map_or(lines.len(), |next| next.0);
        // A test body also ends at the next SIBLING definition — a helper method declared
        // between two tests belongs to neither. Without this, a helper's `except: pass`
        // and its assertions both get attributed to the preceding test.
        let mut end = hard_end;
        for (j, line) in lines.iter().enumerate().take(hard_end).skip(start + 1) {
            if is_blank(line) {
                continue;
            }
            if LOCAL_DEF.test(line) && indent(line) <= ind {
                end = j;
                break;
            }
        }
        let body: Vec<LineEntry> = (start..end).map(|i| (i, lines[i])).collect();
        out.push((start, body));
    }
    out
}

#[derive(Default, Clone)]
struct Helpers {
    asserting: HashSet<String>,
    stubs: HashSet<String>,
}

fn alias_bindings(lines: &[&str]) -> HashSet<String> {
    lines
        .iter()
        .filter_map(|l| {
            ASSERT_ALIAS_BINDING
                .exec(l)
                .and_then(|m| group(&m, 1).map(str::to_string))
        })
        .collect()
}

fn asserts_with(aliases: &HashSet<String>, text: &str) -> bool {
    if ASSERT.test(text) {
        return true;
    }
    if aliases.is_empty() {
        return false;
    }
    CALL_NAME
        .all(text)
        .iter()
        .any(|m| group(m, 1).is_some_and(|n| aliases.contains(n)))
}

/// Names declared in `lines` that provably assert, plus names that are abstract stubs.
///
/// Extracted from scan_file so the same analysis can run over a RELATED file — a base class
/// or mixin in another module. Resolved to a fixpoint, because helper chains are normal.
fn compute_helpers(lines: &[&str]) -> Helpers {
    let aliases = alias_bindings(lines);

    struct Def<'a> {
        name: &'a str,
        line: usize,
        indent: usize,
    }
    let mut defs: Vec<Def> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(m) = LOCAL_DEF.exec(line) else {
            continue;
        };
        if let Some(name) = (1..m.len()).find_map(|g| group(&m, g)) {
            defs.push(Def {
                name,
                line: i,
                indent: indent(line),
            });
        }
    }
    let all_locals: HashSet<&str> = defs.iter().map(|d| d.name).collect();
    let mut info: HashMap<&str, (bool, HashSet<&str>)> = HashMap::new();
    let mut stubs = HashSet::new();

    for (d, def) in defs.iter().enumerate() {
        let end = defs[d + 1..]
            .iter()
            .find(|next| next.indent <= def.indent)
            .map_or(lines.len(), |next| next.line);
        let mut entry = info.remove(def.name).unwrap_or_default();
        let mut body_code: Vec<&str> = Vec::new();
        for (j, line) in lines.iter().enumerate().take(end).skip(def.line) {
            if j > def.line && !is_blank(line) && !COMMENT_ONLY.test(line) {
                body_code.push(line);
            }
            if asserts_with(&aliases, line) {
                entry.0 = true;
            }
            for m in CALL_NAME.all(line) {
                if let Some(callee) = group(&m, 1) {
                    if callee != def.name && all_locals.contains(callee) {
                        entry.1.insert(callee);
                    }
                }
            }
        }
        // Abstract/stub: nothing but a docstring (blanked to filler), `pass`, `...`, or a raise.
        let meaningful: Vec<&str> = body_code
            .into_iter()
            .filter(|l| !FILLER_ONLY.test(l))
            .collect();
        if meaningful.is_empty() || (meaningful.len() == 1 && STUB_BODY.test(meaningful[0])) {
            stubs.insert(def.name.to_string());
        }
        info.insert(def.name, entry);
    }

    let mut asserting: HashSet<String> = info
        .iter()
        .filter(|(_, e)| e.0)
        .map(|(n, _)| n.to_string())
        .collect();
    let mut grew = true;
    while grew {
        grew = false;
        for (name, entry) in &info {
            if asserting.contains(*name) {
                continue;
            }
            if entry.1.iter().any(|c| asserting.contains(*c)) {
                asserting.insert(name.to_string());
                grew = true;
            }
        }
    }
    Helpers { asserting, stubs }
}

/// The text the TypeScript matched: strip-noncode's output with each blanked character as one
/// NUL per UTF-16 unit, as JavaScript blanked it (the Rust strip keeps UTF-8 byte lengths).
fn code_text(raw: &str, ext: &str) -> String {
    let stripped = strip_noncode(raw, ext);
    let sb = stripped.as_bytes();
    let mut out = String::with_capacity(raw.len());
    for (at, c) in raw.char_indices() {
        let len = c.len_utf8();
        let blanked = c != '\0'
            && sb
                .get(at..at + len)
                .is_some_and(|b| b.iter().all(|&x| x == 0));
        if blanked {
            for _ in 0..c.len_utf16() {
                out.push('\0');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `fs.readFileSync(path, "utf8")`: invalid UTF-8 becomes U+FFFD, as Node decodes it.
fn read_text(path: &str) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Memoized per-file helper analysis, so a shared mixin is parsed once per run.
fn helpers_in_file(cache: &mut HashMap<String, Helpers>, file_path: &str) -> Helpers {
    if let Some(h) = cache.get(file_path) {
        return h.clone();
    }
    // unreadable — treat as contributing nothing
    let out = match read_text(file_path) {
        Some(raw) => {
            let text = code_text(&raw, &extname(file_path).to_lowercase());
            compute_helpers(&text.split('\n').collect::<Vec<_>>())
        }
        None => Helpers::default(),
    };
    cache.insert(file_path.to_string(), out.clone());
    out
}

/// Files whose declarations a test file could plausibly inherit or import: resolvable
/// relative/`from x import y` targets, plus shared test-infrastructure modules sitting in
/// the same directory. Import specifiers are read from RAW text, since the blanking pass
/// (correctly) erases string contents.
fn related_files(file_path: &str, raw: &str) -> Vec<String> {
    let dir = dirname(file_path);
    // Insertion-ordered set, as a JavaScript Set is.
    let mut found: Vec<String> = Vec::new();
    let add = |found: &mut Vec<String>, c: String| {
        if !found.contains(&c) {
            found.push(c);
        }
    };
    let try_add = |found: &mut Vec<String>, base: &str| {
        for e in [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py"] {
            let c = format!("{base}{e}");
            if is_file(&c) {
                add(found, c);
                return;
            }
        }
        for idx in ["/index.ts", "/index.tsx", "/index.js"] {
            let c = format!("{base}{idx}");
            if is_file(&c) {
                add(found, c);
                return;
            }
        }
    };

    for m in JS_IMPORT.all(raw) {
        let Some(spec) = group(&m, 1).or_else(|| group(&m, 2)) else {
            continue;
        };
        if !spec.starts_with('.') {
            continue; // relative only — skip packages
        }
        let spec = JS_EXT.remove_first(spec);
        try_add(&mut found, &resolve(&dir, &spec));
    }
    // Dotted Python modules: `from test.test_tkinter.widget_tests import AbstractWidgetTest`
    // resolves relative to whichever ancestor directory the package root sits in — often NOT
    // the importing file's own directory (test_ttk imports a mixin from test_tkinter). Try the
    // full dotted path against each ancestor, then progressively drop leading components.
    for m in PY_IMPORT.all(raw) {
        let module = group(&m, 1).or_else(|| group(&m, 2)).unwrap_or("");
        let parts: Vec<&str> = module.split('.').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            continue;
        }
        for up in 0..=4 {
            let mut segs: Vec<&str> = vec![&dir];
            segs.extend(std::iter::repeat_n("..", up));
            let ancestor = join(&segs);
            for drop in 0..parts.len() {
                let mut segs: Vec<&str> = vec![&ancestor];
                segs.extend(&parts[drop..]);
                try_add(&mut found, &join(&segs));
            }
        }
    }
    if let Ok(rd) = fs::read_dir(&dir) {
        let mut names: Vec<String> = rd
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort(); // readdirSync's order
        for entry in names {
            let full = join(&[&dir, &entry]);
            if full != file_path && SHARED_INFRA.test(&full.replace(std::path::MAIN_SEPARATOR, "/"))
            {
                add(&mut found, full);
            }
        }
    }

    found.retain(|f| f != file_path);
    found.truncate(MAX_RELATED_FILES);
    found
}

fn scan_file(cache: &mut HashMap<String, Helpers>, file_path: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(raw) = read_text(file_path) else {
        // A file we could not read must NOT be reported as clean. This checker's whole claim is
        // that "clean" means checked-and-clean; silently returning no findings turns an
        // unreadable file into a passing one, which is the exact silent-pass mode it exists to
        // catch elsewhere. Reproduced: chmod 000 on a file with a real defect printed
        // "clean: 1 test file(s), no quality defects found" and exited 0.
        findings.push(finding(
            file_path,
            1,
            "unreadable",
            "could not read this file, so it was NOT checked — do not treat this run as clean for it",
            "P2",
        ));
        return findings;
    };

    // Match against code only — string literals and comments are blanked first.
    let file_text = code_text(&raw, &extname(file_path).to_lowercase());
    let lines: Vec<&str> = file_text.split('\n').collect();
    let has_fake_timers = FAKE_TIMERS.test(&file_text);

    let code_lines = |body: &[LineEntry<'_>]| -> Vec<(usize, String)> {
        body.iter()
            .filter(|(_, t)| !is_blank(t) && !COMMENT_ONLY.test(t))
            .map(|(i, t)| (*i, t.to_string()))
            .collect()
    };

    // Local aliases bound to assertion functions, so calls through them count as assertions.
    let mut assert_aliases = alias_bindings(&lines);

    let tests = split_tests(&lines);

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
    let local = compute_helpers(&lines);
    let asserting_locals = local.asserting;
    // Locally-declared abstract stubs: `self.execute(...)` whose body here is only a
    // docstring, with the real implementation in a subclass elsewhere.
    let stub_names = local.stubs;

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
    let mut inherited_asserting = HashSet::new();
    let mut inherited_stubs = HashSet::new();
    for related in related_files(file_path, &raw) {
        let h = helpers_in_file(cache, &related);
        inherited_asserting.extend(h.asserting);
        inherited_stubs.extend(h.stubs);
    }

    // Second alias pass, now that we know which locals assert: a name bound to an asserting
    // method or a partial of one counts as an assertion alias even when its own name says
    // nothing (`check = self.check_match`, `raises = partial(self.assertRaises, ValueError)`).
    // Requiring the target to provably assert is what makes this safe.
    for line in &lines {
        let Some(m) = GENERIC_ALIAS_BINDING.exec(line) else {
            continue;
        };
        let (Some(alias), Some(target)) = (group(&m, 1), group(&m, 2)) else {
            continue;
        };
        if asserting_locals.contains(target) || ASSERTISH_NAME.test(target) || target == "fail" {
            assert_aliases.insert(alias.to_string());
        }
    }

    let delegates_to_local = |text: &str| -> bool {
        if CALL_NAME.all(text).iter().any(|m| {
            group(m, 1).is_some_and(|n| asserting_locals.contains(n) || stub_names.contains(n))
        }) {
            return true;
        }
        // A helper can also be PASSED rather than called — `test.each(cases)(name, assertThing)`
        // hands the asserting function to the runner, so it never appears with parentheses.
        if WORD
            .all(text)
            .iter()
            .any(|m| group(m, 1).is_some_and(|n| asserting_locals.contains(n)))
        {
            return true;
        }
        // Inherited from a base class or mixin in another file — receiver form only.
        RECEIVER_CALL.all(text).iter().any(|m| {
            group(m, 1)
                .is_some_and(|n| inherited_asserting.contains(n) || inherited_stubs.contains(n))
        })
    };
    // Line indices that sit inside a test body. `except: pass` in a shared helper or in
    // setUp is usually deliberate cleanup; inside a test it discards the failure.
    let in_test: HashSet<usize> = tests
        .iter()
        .flat_map(|(_, body)| body.iter().map(|(i, _)| *i))
        .collect();

    // ---- file-level checks (apply anywhere in the file)
    for (i, text) in lines.iter().enumerate() {
        let text = *text;
        if COMMENT_ONLY.test(text) {
            continue;
        }
        if FOCUSED_OR_SKIPPED.test(text) {
            findings.push(finding(
                file_path,
                i + 1,
                "focused-or-skipped",
                "focused or skipped test left in place — the suite silently shrinks and still reports green",
                "P1",
            ));
        }
        if TAUTOLOGY.test(text) {
            findings.push(finding(
                file_path,
                i + 1,
                "tautology",
                "assertion cannot fail — it passes regardless of the code under test",
                "P1",
            ));
        }
        let mut swallowed = SWALLOWED.test(text);
        if !swallowed && (HANDLER_OPEN_PY.test(text) || HANDLER_OPEN_JS.test(text)) {
            let noop: &Re = if HANDLER_OPEN_PY.test(text) {
                &HANDLER_NOOP_PY
            } else {
                &HANDLER_NOOP_JS
            };
            for nxt in &lines[i + 1..] {
                if is_blank(nxt) || COMMENT_ONLY.test(nxt) {
                    continue;
                }
                swallowed = noop.test(nxt);
                break;
            }
        }
        // The manual raise-assertion idiom, in both dialects:
        //   python: try/except X: pass  +  else: self.fail("did not raise")
        //   js:     try { x(); expect.unreachable() } catch {}
        // In both, the swallowing branch is the SUCCESS path and the assertion is elsewhere.
        let mut raise_assertion = false;
        for j in i + 1..lines.len().min(i + 10) {
            if RAISE_ASSERTION_ELSE.test(lines[j]) {
                raise_assertion = (j..lines.len().min(j + 4)).any(|k| FAIL_CALL.test(lines[k]));
                break;
            }
        }
        // Look BACKWARD for the js form: the assertion sits inside the try, above the catch.
        if !raise_assertion {
            raise_assertion =
                (i.saturating_sub(6)..i).any(|j| UNREACHABLE_ASSERTION.test(lines[j]));
        }
        if swallowed && in_test.contains(&i) && !COMPAT_EXCEPTIONS.test(text) && !raise_assertion {
            findings.push(finding(
                file_path,
                i + 1,
                "swallowed-error",
                "exception discarded — check whether the failure this test exists to catch is \
                 being thrown away (lowest-confidence check: many languages have legitimate \
                 swallow idioms, so confirm before changing anything)",
                // Deliberately P2, not P1: a 12-sample audit against CPython found zero true
                // positives before the raise-assertion and drain-sentinel exclusions were added.
                // The check earns its place on empty `catch {}` in JS, but it is the least
                // trustworthy finding this scanner emits and should not read as high-confidence.
                "P2",
            ));
        }
        let timer_is_benign = TICK_FLUSH.test(text) || TIMEOUT_GUARD.test(text);
        let real_wait = SLEEP_WAIT_HARD.test(text) && !TICK_FLUSH.test(text);
        let soft_wait = SLEEP_WAIT_SOFT.test(text) && !has_fake_timers && !timer_is_benign;
        if real_wait || soft_wait {
            findings.push(finding(
                file_path,
                i + 1,
                "sleep-wait",
                "waiting on a timer, not an observable condition — this is how flaky tests are \
                 born; poll for the state or await the event",
                "P2",
            ));
        }
    }

    // ---- per-test checks
    for (start, body) in &tests {
        let start = *start;
        let real = code_lines(body);
        let asserts = real
            .iter()
            .filter(|(_, t)| asserts_with(&assert_aliases, t))
            .count();
        let mocks = real.iter().filter(|(_, t)| MOCK.test(t)).count();

        // A test with a real body but nothing that can fail. A one- or two-line body is
        // often a deliberate "just don't raise" smoke test, so require some substance;
        // and a call into a local helper may be carrying the assertion.
        let delegates = real
            .iter()
            .any(|(i, t)| *i != start && delegates_to_local(t));
        // A body that is entirely a string literal is a doctest: the assertion mechanism is
        // doctest's own output comparison, and `strip_noncode` has (correctly) blanked it to
        // filler, so there is no code here to judge either way.
        let doctest_only = real.len() > 1
            && real[1..]
                .iter()
                .all(|(_, t)| is_blank(t) || FILLER_ONLY.test(t));
        if asserts == 0 && !delegates && !doctest_only && real.len() > NO_ASSERTION_MIN_BODY {
            findings.push(finding(
                file_path,
                start + 1,
                "no-assertion",
                "test asserts nothing — it passes as long as the code does not raise",
                "P1",
            ));
        }

        if mocks > MOCK_DENSITY_MAX && mocks > asserts {
            findings.push(finding(
                file_path,
                start + 1,
                "over-mocking",
                &format!(
                    "{mocks} mock/stub calls vs {asserts} assertions — this exercises the mocks, \
                     not the code; mocks only hold while they match the real implementation"
                ),
                "P2",
            ));
        }
    }

    findings.sort_by_key(|f| f.line);
    findings
}

/// Checks are test-specific, so non-test files are skipped unless --any says the
/// project uses unconventional naming.
fn collect_targets(all: bool, any: bool, paths: &[String]) -> (Vec<String>, Vec<String>) {
    if all {
        let mut targets = walk_repo(".");
        targets.sort_by(|a, b| js_cmp(a, b));
        return (targets, Vec::new());
    }
    if !paths.is_empty() {
        let mut out: Vec<String> = Vec::new();
        let mut skipped = Vec::new();
        for p in paths {
            let Ok(stat) = fs::metadata(p) else {
                continue;
            };
            if stat.is_dir() {
                out.extend(walk_repo(p));
            } else if stat.is_file() {
                if any || is_test_file(p) {
                    out.push(normalize(p));
                } else {
                    skipped.push(normalize(p));
                }
            }
        }
        let mut targets: Vec<String> = Vec::new();
        for t in out {
            if !targets.contains(&t) {
                targets.push(t);
            }
        }
        targets.sort_by(|a, b| js_cmp(a, b));
        return (targets, skipped);
    }
    (changed_files(), Vec::new())
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "P1" => 0,
        "P2" => 1,
        "P3" => 2,
        _ => 3,
    }
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(
        args,
        &[flag("all"), flag("any"), flag("json"), flag("quiet")],
    ) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let (json_out, quiet) = (parsed.flag("json"), parsed.flag("quiet"));
    let (targets, skipped) =
        collect_targets(parsed.flag("all"), parsed.flag("any"), &parsed.positionals);
    let mut out = String::new();
    let code = report(&mut out, &targets, &skipped, json_out, quiet);
    let _ = std::io::stdout().lock().write_all(out.as_bytes());
    code
}

fn report(
    out: &mut String,
    targets: &[String],
    skipped: &[String],
    json_out: bool,
    quiet: bool,
) -> i32 {
    let mut println = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    if targets.is_empty() {
        if json_out {
            println(
                &json!({ "scanned": 0, "findings": [], "skipped_non_test": skipped }).to_string(),
            );
        } else if !quiet {
            if !skipped.is_empty() {
                println(&format!(
                    "no test files among the given paths — skipped as non-test: {}",
                    skipped.join(", ")
                ));
                println("these checks only make sense on tests; pass --any to scan them anyway");
            } else {
                println("no test files to scan (pass paths explicitly, or --all)");
            }
        }
        return 2;
    }
    if !skipped.is_empty() && !json_out && !quiet {
        println(&format!(
            "note: skipped {} non-test file(s): {}\n",
            skipped.len(),
            skipped.join(", ")
        ));
    }

    let mut cache = HashMap::new();
    let findings: Vec<Finding> = targets
        .iter()
        .flat_map(|t| scan_file(&mut cache, t))
        .collect();
    let code = i32::from(!findings.is_empty());

    if json_out {
        let list: Vec<Value> = findings
            .iter()
            .map(|f| {
                json!({ "file": f.file, "line": f.line, "check": f.check, "message": f.message, "severity": f.severity })
            })
            .collect();
        let doc = json!({ "scanned": targets.len(), "findings": list });
        println(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        return code;
    }

    let mut sorted = findings.clone();
    sorted.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| js_cmp(&a.file, &b.file))
            .then(a.line.cmp(&b.line))
    });
    for f in &sorted {
        println(&format!(
            "{}:{}  [{} {}]  {}",
            f.file, f.line, f.severity, f.check, f.message
        ));
    }

    if !quiet {
        if !findings.is_empty() {
            let mut by: Vec<(&str, usize)> = Vec::new();
            for f in &findings {
                match by.iter_mut().find(|(k, _)| *k == f.check) {
                    Some(e) => e.1 += 1,
                    None => by.push((f.check, 1)),
                }
            }
            by.sort_by(|a, b| js_cmp(a.0, b.0));
            let breakdown: Vec<String> = by.iter().map(|(k, v)| format!("{k} {v}")).collect();
            println(&format!(
                "\n{} finding(s) across {} test file(s): {}",
                findings.len(),
                targets.len(),
                breakdown.join(", ")
            ));
            println(
                "Each is a text heuristic — confirm against the code before changing anything.",
            );
        } else {
            println(&format!(
                "clean: {} test file(s), no quality defects found",
                targets.len()
            ));
        }
    }
    code
}
