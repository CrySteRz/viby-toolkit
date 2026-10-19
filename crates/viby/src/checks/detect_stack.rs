//! viby-toolkit stack detector — makes "language-agnostic" executable instead of aspirational.
//!
//! Every skill here says some version of "find the project's real commands." This finds
//! them, for any language, and reports where each answer came from so the caller can judge
//! how much to trust it.
//!
//! Usage:
//!   viby detect-stack [dir]          # human-readable report
//!   viby detect-stack [dir] --json   # machine-readable
//!
//! Exit: 0 always (a detector that fails a build is worse than one that says "unknown").
//!
//! DESIGN: authority is ranked, and every command carries its source.
//!
//! ```text
//!   1. CI config      — what the project itself considers "green". Authoritative.
//!   2. Task runner    — package.json scripts, Makefile, justfile, pyproject, Cargo…
//!   3. Convention     — inferred from a lockfile or manifest.
//! ```
//!
//! It NEVER invents a command. An honest `unknown` beats a plausible guess that wastes a
//! cycle and produces a misleading failure — the same reason the test scanner drops checks
//! it cannot make precise.
//!
//! Port notes: directory entries are visited in byte order, as libuv's `scandir` returns them
//! (the TypeScript relied on that, so the language ranking on ties and the order of `infra`
//! match it). Regexes are written to the JavaScript semantics: ASCII `\w`, `\b` and `/i`, JS
//! `\s`, and `^` after any JS line terminator.

use crate::core::args::{flag, parse_or_exit};
use fancy_regex::Regex as Fancy;
use regex::Regex;
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Ci,
    TaskRunner,
    Convention,
}

#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct Command {
    pub command: String,
    pub source: Source,
    pub from: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Install,
    Build,
    Test,
    Lint,
    Typecheck,
    Format,
    Dev,
    Bench,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Install => "install",
            Role::Build => "build",
            Role::Test => "test",
            Role::Lint => "lint",
            Role::Typecheck => "typecheck",
            Role::Format => "format",
            Role::Dev => "dev",
            Role::Bench => "bench",
        }
    }
}

/// Commands by role, in the order each role first received one (a JS object's key order, which
/// the JSON output shows).
#[derive(Clone, Default, Debug)]
pub struct Commands(Vec<(Role, Vec<Command>)>);

impl Commands {
    pub fn get(&self, role: &str) -> Option<&Vec<Command>> {
        self.0
            .iter()
            .find(|(r, _)| r.as_str() == role)
            .map(|(_, v)| v)
    }

    fn of(&self, role: Role) -> Option<&Vec<Command>> {
        self.get(role.as_str())
    }

    fn add(&mut self, role: Role, entry: Command) {
        let at = match self.0.iter().position(|(r, _)| *r == role) {
            Some(i) => i,
            None => {
                self.0.push((role, Vec::new()));
                self.0.len() - 1
            }
        };
        let list = &mut self.0[at].1;
        if !list.iter().any(|c| c.command == entry.command) {
            list.push(entry);
        }
    }
}

impl Serialize for Commands {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (role, list) in &self.0 {
            map.serialize_entry(role.as_str(), list)?;
        }
        map.end()
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Language {
    pub name: String,
    pub files: u64,
    pub share: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Monorepo {
    pub tool: String,
    pub from: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub code_files: u64,
    pub config_files: u64,
    pub doc_files: u64,
    pub infra: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stack {
    pub root: String,
    pub languages: Vec<Language>,
    pub package_managers: Vec<String>,
    pub monorepo: Option<Monorepo>,
    pub test_frameworks: Vec<String>,
    pub ci_files: Vec<String>,
    pub profilers: Vec<String>,
    pub commands: Commands,
    pub unknowns: Vec<String>,
    pub shape: Shape,
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "venv",
    ".venv",
    "dist",
    "build",
    "target",
    "out",
    "__pycache__",
    ".pytest_cache",
    ".next",
    ".nuxt",
    "vendor",
    "coverage",
    ".tox",
    ".gradle",
    ".idea",
    ".vscode",
    "Pods",
    "bin",
    "obj",
    ".terraform",
    ".mypy_cache",
];

/// Extension -> language. Deliberately broad: the census is for orientation, so a missing
/// exotic extension degrades the report rather than breaking it.
fn lang_of(ext: &str) -> Option<&'static str> {
    Some(match ext {
        ".ts" | ".tsx" | ".mts" | ".cts" => "TypeScript",
        ".js" | ".jsx" | ".mjs" | ".cjs" => "JavaScript",
        ".py" | ".pyi" => "Python",
        ".go" => "Go",
        ".rs" => "Rust",
        ".rb" => "Ruby",
        ".php" => "PHP",
        ".java" => "Java",
        ".kt" | ".kts" => "Kotlin",
        ".scala" => "Scala",
        ".groovy" => "Groovy",
        ".cs" => "C#",
        ".fs" => "F#",
        ".vb" => "VB.NET",
        ".c" => "C",
        ".h" => "C/C++ header",
        ".cc" | ".cpp" | ".cxx" | ".hpp" => "C++",
        ".swift" => "Swift",
        ".m" => "Objective-C",
        ".mm" => "Objective-C++",
        ".ex" | ".exs" => "Elixir",
        ".erl" => "Erlang",
        ".hs" => "Haskell",
        ".ml" => "OCaml",
        ".clj" => "Clojure",
        ".cljs" => "ClojureScript",
        ".dart" => "Dart",
        ".lua" => "Lua",
        ".sh" | ".bash" | ".zsh" => "Shell",
        ".ps1" => "PowerShell",
        ".sql" => "SQL",
        ".tf" => "Terraform",
        ".hcl" => "HCL",
        ".vue" => "Vue",
        ".svelte" => "Svelte",
        ".astro" => "Astro",
        ".zig" => "Zig",
        ".nim" => "Nim",
        ".jl" => "Julia",
        ".r" => "R",
        ".pl" => "Perl",
        ".proto" => "Protobuf",
        ".graphql" | ".gql" => "GraphQL",
        _ => return None,
    })
}

/// Lockfile / manifest -> package manager, and the install command it implies.
const PM_MARKERS: &[(&str, &str, &str)] = &[
    ("pnpm-lock.yaml", "pnpm", "pnpm install --frozen-lockfile"),
    ("bun.lockb", "bun", "bun install --frozen-lockfile"),
    ("bun.lock", "bun", "bun install --frozen-lockfile"),
    ("yarn.lock", "yarn", "yarn install --immutable"),
    ("package-lock.json", "npm", "npm ci"),
    ("uv.lock", "uv", "uv sync"),
    ("poetry.lock", "poetry", "poetry install"),
    ("Pipfile.lock", "pipenv", "pipenv install --dev"),
    ("requirements.txt", "pip", "pip install -r requirements.txt"),
    ("Cargo.lock", "cargo", "cargo fetch"),
    ("go.sum", "go modules", "go mod download"),
    ("Gemfile.lock", "bundler", "bundle install"),
    ("composer.lock", "composer", "composer install"),
    ("mix.lock", "mix", "mix deps.get"),
    ("pubspec.lock", "pub", "dart pub get"),
    ("gradle.lockfile", "gradle", "./gradlew dependencies"),
];

const MONOREPO_MARKERS: &[(&str, &str)] = &[
    ("nx.json", "Nx"),
    ("turbo.json", "Turborepo"),
    ("lerna.json", "Lerna"),
    ("pnpm-workspace.yaml", "pnpm workspaces"),
    ("rush.json", "Rush"),
    ("WORKSPACE", "Bazel"),
    ("MODULE.bazel", "Bazel"),
    ("Cargo.toml", "Cargo workspace"), // confirmed below by [workspace]
    ("go.work", "Go workspace"),
];

const TEST_FRAMEWORK_HINTS: &[(&str, &str)] = &[
    (r#""vitest""#, "vitest"),
    (r#""jest""#, "jest"),
    (r#""mocha""#, "mocha"),
    (r#""@playwright/test""#, "playwright"),
    (r#""cypress""#, "cypress"),
    (r#""ava""#, "ava"),
    (r#""jasmine""#, "jasmine"),
    (r"(?-u:\b)pytest(?-u:\b)", "pytest"),
    (r"(?-u:\b)unittest(?-u:\b)", "unittest"),
    (r"(?-u:\b)nose2?(?-u:\b)", "nose"),
    (r"(?-u:\b)rspec(?-u:\b)", "rspec"),
    (r"(?-u:\b)minitest(?-u:\b)", "minitest"),
    (r"(?-u:\b)phpunit(?-u:\b)", "phpunit"),
    (r"(?-u:\b)junit(?-u:\b)", "junit"),
    (r"(?-u:\b)gotestsum(?-u:\b)", "gotestsum"),
];

/// Which script names map to which role. Ordered: earlier names win.
const SCRIPT_ROLES: &[(Role, &[&str])] = &[
    (
        Role::Test,
        &["test", "tests", "test:unit", "unit", "spec", "check:test"],
    ),
    (Role::Build, &["build", "compile", "bundle", "dist"]),
    (
        Role::Lint,
        &["lint", "eslint", "check:lint", "ruff", "clippy"],
    ),
    (
        Role::Typecheck,
        &["typecheck", "type-check", "tsc", "types", "mypy"],
    ),
    (Role::Format, &["format", "fmt", "prettier", "format:check"]),
    (Role::Bench, &["bench", "benchmark", "benchmarks", "perf"]),
    (Role::Dev, &["dev", "start", "serve", "watch"]),
];

/// Profilers worth reaching for per language, reported only when the binary is actually on
/// PATH. Agent-authored performance changes validate by static reasoning 67.2% of the time
/// (arXiv 2512.21757) — naming the tool that is already installed removes one excuse.
const PROFILERS: &[(&str, &str, &str)] = &[
    (
        "Python",
        "py-spy",
        "py-spy top -- python x.py (sampling, no code change)",
    ),
    ("Python", "python3", "python3 -m cProfile -s cumtime x.py"),
    ("Python", "scalene", "scalene x.py (CPU + memory)"),
    (
        "Go",
        "go",
        "go test -cpuprofile cpu.out -bench . && go tool pprof cpu.out",
    ),
    (
        "Rust",
        "cargo",
        "cargo flamegraph / cargo bench (criterion)",
    ),
    ("JavaScript", "node", "node --cpu-prof / --heap-prof, or 0x"),
    ("TypeScript", "node", "node --cpu-prof / --heap-prof, or 0x"),
    ("Ruby", "ruby", "stackprof / rbspy"),
    ("Java", "java", "async-profiler / JFR"),
    ("C", "perf", "perf record -g ./bin && perf report"),
    ("C++", "perf", "perf record -g ./bin && perf report"),
];

/// JS `\s`: not Rust's, which has U+0085 and lacks U+FEFF.
const JS_WS: &str = "\t\n\u{b}\u{c}\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
/// JS `^` under `/m`: the start of the text or just after any JS line terminator.
const BOL: &str = "(?:^|(?<=[\n\r\u{2028}\u{2029}]))";
/// JS `.`: anything but a line terminator.
const DOT: &str = "[^\n\r\u{2028}\u{2029}]";

fn is_js_ws(c: char) -> bool {
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

fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_ws)
}

/// All matches of a backtracking regex; a regex error (backtrack limit) ends the scan quietly.
fn captures_all<'t>(re: &Fancy, text: &'t str) -> Vec<fancy_regex::Captures<'t>> {
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        match caps {
            Ok(c) => out.push(c),
            Err(_) => break,
        }
    }
    out
}

fn on_path(bin: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths)
        .any(|d| !d.as_os_str().is_empty() && fs::metadata(d.join(bin)).is_ok_and(|m| m.is_file()))
}

fn exists(p: &Path) -> bool {
    fs::metadata(p).is_ok()
}

fn read(p: &Path) -> String {
    // Strip a UTF-8 BOM: Node's "utf8" keeps it, and JSON.parse then throws on an
    // otherwise-valid manifest. That silently dropped the file from version detection, so a
    // real version drift went unreported because of an encoding artifact.
    match fs::read(p) {
        Ok(b) => {
            let s = String::from_utf8_lossy(&b);
            s.strip_prefix('\u{feff}').unwrap_or(&s).to_string()
        }
        Err(_) => String::new(),
    }
}

/// `path.extname`, on a file name.
fn extname(name: &str) -> &str {
    match name.rfind('.') {
        None | Some(0) => "",
        Some(_) if name == ".." => "",
        Some(i) => &name[i..],
    }
}

/// Config / infrastructure file extensions. Not programming languages, and deliberately NOT
/// folded into the language census — but counted, because a repo can be 97% YAML and the census
/// will happily report "Python 100%" off a single stray script. That happened on a real
/// Kubernetes GitOps repo: 183 YAML files, 1 Python file, headline "Python 100%". A fast
/// confident wrong answer about what a repo even IS is the worst output this tool can produce.
const CONFIG_EXT: &[&str] = &[
    ".yaml",
    ".yml",
    ".tf",
    ".tfvars",
    ".json",
    ".toml",
    ".ini",
    ".conf",
    ".hcl",
    ".env",
    ".properties",
];

static INFRA_MARKERS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"(?i-u)^kustomization\.ya?ml$", "Kubernetes (kustomize)"),
        (r"(?i-u)^Chart\.ya?ml$", "Helm chart"),
        (r"(?i-u)^values(-[A-Za-z0-9_]+)?\.ya?ml$", "Helm values"),
        (r"(?i-u)\.tf$", "Terraform"),
        (
            r"(?i-u)^docker-compose(\.[A-Za-z0-9_]+)?\.ya?ml$",
            "Docker Compose",
        ),
        (r"(?i-u)^Dockerfile", "Docker image"),
        (r"(?i-u)^(playbook|site)\.ya?ml$", "Ansible"),
        (r"(?i-u)^serverless\.ya?ml$", "Serverless Framework"),
        (r"(?i-u)^skaffold\.ya?ml$", "Skaffold"),
    ]
    .into_iter()
    .filter_map(|(p, what)| Regex::new(p).ok().map(|r| (r, what)))
    .collect()
});

/// The directory walk both censuses share: visit files (at most ~20,000, checked per directory),
/// skipping vendor and dot directories. `f` gets each non-directory entry's name.
fn walk_files(root: &Path, mut f: impl FnMut(&str)) {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0u32;
    while visited < 20_000 {
        let Some(dir) = stack.pop() else { break };
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<(String, bool)> = rd
            .filter_map(|e| e.ok())
            .map(|e| {
                let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
                (e.file_name().to_string_lossy().into_owned(), is_dir)
            })
            .collect();
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        for (name, is_dir) in entries {
            if is_dir {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(dir.join(&name));
                }
                continue;
            }
            visited += 1;
            f(&name);
        }
    }
}

/// Whether this repo is mostly configuration rather than code, and of what kind.
fn census_shape(root: &Path) -> Shape {
    let mut shape = Shape {
        code_files: 0,
        config_files: 0,
        doc_files: 0,
        infra: Vec::new(),
    };
    let mut seen: HashSet<&'static str> = HashSet::new();
    walk_files(root, |name| {
        let ext = extname(name).to_lowercase();
        if lang_of(&ext).is_some() {
            shape.code_files += 1;
        } else if CONFIG_EXT.contains(&ext.as_str()) {
            shape.config_files += 1;
        } else if matches!(ext.as_str(), ".md" | ".rst" | ".txt") {
            shape.doc_files += 1;
        }
        for (re, what) in INFRA_MARKERS.iter() {
            if re.is_match(name) && seen.insert(what) {
                shape.infra.push((*what).to_string());
            }
        }
    });
    shape
}

fn census_languages(root: &Path) -> Vec<Language> {
    let mut counts: Vec<(&'static str, u64)> = Vec::new();
    let mut total = 0u64;
    walk_files(root, |name| {
        let Some(lang) = lang_of(&extname(name).to_lowercase()) else {
            return;
        };
        match counts.iter_mut().find(|(n, _)| *n == lang) {
            Some(c) => c.1 += 1,
            None => counts.push((lang, 1)),
        }
        total += 1;
    });
    let mut out: Vec<Language> = counts
        .into_iter()
        .map(|(name, files)| Language {
            name: name.to_string(),
            files,
            share: if total > 0 {
                ((files as f64 / total as f64) * 100.0).round() as u64
            } else {
                0
            },
        })
        .collect();
    out.sort_by_key(|l| std::cmp::Reverse(l.files));
    out
}

static RUN_STEP: LazyLock<Option<Fancy>> = LazyLock::new(|| {
    // `run:` steps, both scalar and block forms.
    Fancy::new(&format!(
        "{BOL}[{JS_WS}]*(?:-[{JS_WS}]*)?run:[{JS_WS}]*(?:\\|[-+]?[{JS_WS}]*)?({DOT}*)"
    ))
    .ok()
});

/// Commands a CI workflow actually runs. The project's own definition of "green".
fn ci_commands(root: &Path) -> (Vec<String>, Vec<Command>) {
    let mut files: Vec<String> = Vec::new();
    let mut commands: Vec<Command> = Vec::new();
    const CANDIDATES: &[&str] = &[
        ".github/workflows",
        ".gitlab-ci.yml",
        ".circleci/config.yml",
        "azure-pipelines.yml",
        "Jenkinsfile",
        ".travis.yml",
        "bitbucket-pipelines.yml",
        ".drone.yml",
    ];
    let yaml_name = Regex::new(r"\.ya?ml$").ok();
    for c in CANDIDATES {
        let full = root.join(c);
        if !exists(&full) {
            continue;
        }
        let mut list: Vec<(String, std::path::PathBuf)> = vec![((*c).to_string(), full.clone())];
        match fs::metadata(&full) {
            Ok(m) if m.is_dir() => {
                let Ok(rd) = fs::read_dir(&full) else {
                    continue;
                };
                let mut names: Vec<String> = rd
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| yaml_name.as_ref().is_some_and(|r| r.is_match(n)))
                    .collect();
                names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
                list = names
                    .into_iter()
                    .map(|n| (format!("{c}/{n}"), full.join(&n)))
                    .collect();
            }
            Ok(_) => {}
            Err(_) => continue,
        }
        for (rel, f) in list {
            files.push(rel.clone());
            let text = read(&f);
            let Some(re) = RUN_STEP.as_ref() else {
                continue;
            };
            for m in captures_all(re, &text) {
                let line = js_trim(m.get(1).map_or("", |g| g.as_str()));
                if line.is_empty() {
                    continue;
                }
                for part in line.split(';').flat_map(|p| p.split("&&")) {
                    let cmd = js_trim(part);
                    let len = cmd.encode_utf16().count();
                    if !(3..=160).contains(&len) {
                        continue;
                    }
                    if skip_cmd(cmd) {
                        continue;
                    }
                    commands.push(Command {
                        command: cmd.to_string(),
                        source: Source::Ci,
                        from: rel.clone(),
                    });
                }
            }
        }
    }
    commands.truncate(60);
    (files, commands)
}

fn skip_cmd(cmd: &str) -> bool {
    ["cd ", "echo ", "export ", "if ", "#"]
        .iter()
        .any(|p| cmd.starts_with(p))
        || cmd == "fi"
        || cmd == "then"
}

macro_rules! rx {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| match Regex::new($pat) {
            Ok(r) => r,
            Err(e) => panic!("bad built-in regex {}: {e}", $pat),
        });
    };
}

// Benchmarks first: `go test -bench=.` contains "test" but is a measurement, not a check.
rx!(
    RE_BENCH,
    r"(?i-u)(-bench|\bbench\b|\bbenchmark|criterion|hyperfine|pytest-benchmark|\basv\b)"
);
rx!(
    RE_TEST,
    r"(?i-u)\b(test|pytest|vitest|jest|mocha|go test|cargo test|rspec|phpunit|gradlew test|mix test)\b"
);
rx!(
    RE_TYPECHECK,
    r"(?i-u)\b(tsc|mypy|pyright|typecheck|type-check)\b"
);
rx!(
    RE_LINT,
    r"(?i-u)\b(lint|eslint|ruff|clippy|flake8|golangci-lint|rubocop|shellcheck)\b"
);
rx!(
    RE_FORMAT,
    r"(?i-u)\b(fmt|format|prettier|black|gofmt|rustfmt)\b"
);
rx!(
    RE_BUILD,
    r"(?i-u)\b(build|compile|bundle|make\b|cargo build|go build|gradlew (assemble|build)|mvn package)\b"
);
// `npm ci` specifically, never a bare `ci` — that matched `pnpm run ci-publish`, since a
// word boundary sits between "ci" and "-".
rx!(
    RE_INSTALL,
    r"(?i-u)(\bnpm ci\b|\binstall\b|\bsync\b|deps\.get|mod download|bundle install|\brestore\b)"
);

fn classify(cmd: &str) -> Option<Role> {
    if RE_BENCH.is_match(cmd) {
        return Some(Role::Bench);
    }
    if RE_TEST.is_match(cmd) {
        return Some(Role::Test);
    }
    if RE_TYPECHECK.is_match(cmd) {
        return Some(Role::Typecheck);
    }
    if RE_LINT.is_match(cmd) {
        return Some(Role::Lint);
    }
    if RE_FORMAT.is_match(cmd) {
        return Some(Role::Format);
    }
    if RE_BUILD.is_match(cmd) {
        return Some(Role::Build);
    }
    if RE_INSTALL.is_match(cmd) {
        return Some(Role::Install);
    }
    None
}

static MAKE_TARGET: LazyLock<Option<Fancy>> =
    LazyLock::new(|| Fancy::new(&format!("{BOL}([A-Za-z0-9_.-]+):(?!=)")).ok());
static TASK_TARGET: LazyLock<Option<Fancy>> =
    LazyLock::new(|| Fancy::new(&format!("{BOL}([A-Za-z0-9_-]+):")).ok());

fn task_runner_commands(
    root: &Path,
    commands: &mut Commands,
    unknowns: &mut Vec<String>,
) -> Vec<String> {
    let mut managers: Vec<String> = Vec::new();
    let mut runner = "npm run";
    for (file, pm, install) in PM_MARKERS {
        if !exists(&root.join(file)) {
            continue;
        }
        managers.push((*pm).to_string());
        commands.add(
            Role::Install,
            Command {
                command: (*install).to_string(),
                source: Source::Convention,
                from: (*file).to_string(),
            },
        );
        match *pm {
            "pnpm" => runner = "pnpm",
            "yarn" => runner = "yarn",
            "bun" => runner = "bun run",
            _ => {}
        }
    }

    // package.json scripts — the single richest source in the JS/TS world.
    let pkg_path = root.join("package.json");
    if exists(&pkg_path) {
        match serde_json::from_str::<Value>(&read(&pkg_path)) {
            Ok(pkg) => {
                if let Some(scripts) = pkg.get("scripts").and_then(Value::as_object) {
                    for (role, names) in SCRIPT_ROLES {
                        for n in *names {
                            if scripts.get(*n).is_some_and(Value::is_string) {
                                commands.add(
                                    *role,
                                    Command {
                                        command: format!("{runner} {n}"),
                                        source: Source::TaskRunner,
                                        from: "package.json".to_string(),
                                    },
                                );
                                break;
                            }
                        }
                    }
                }
            }
            Err(_) => unknowns.push("package.json is present but could not be parsed".to_string()),
        }
    }

    // Makefile / justfile / Taskfile targets.
    for (file, invoke) in [
        ("Makefile", "make"),
        ("justfile", "just"),
        ("Taskfile.yml", "task"),
    ] {
        let full = root.join(file);
        if !exists(&full) {
            continue;
        }
        let text = read(&full);
        let re = if file == "Makefile" {
            &*MAKE_TARGET
        } else {
            &*TASK_TARGET
        };
        let mut targets: HashSet<String> = HashSet::new();
        if let Some(re) = re {
            for m in captures_all(re, &text) {
                if let Some(g) = m.get(1) {
                    targets.insert(g.as_str().to_string());
                }
            }
        }
        for (role, names) in SCRIPT_ROLES {
            for n in *names {
                if targets.contains(*n) {
                    commands.add(
                        *role,
                        Command {
                            command: format!("{invoke} {n}"),
                            source: Source::TaskRunner,
                            from: file.to_string(),
                        },
                    );
                    break;
                }
            }
        }
    }

    // Language-native conventions, only when the manifest is actually present.
    const NATIVE: &[(&str, Role, &str)] = &[
        ("Cargo.toml", Role::Test, "cargo test"),
        ("Cargo.toml", Role::Build, "cargo build"),
        ("Cargo.toml", Role::Lint, "cargo clippy -- -D warnings"),
        ("go.mod", Role::Test, "go test ./..."),
        ("go.mod", Role::Build, "go build ./..."),
        ("go.mod", Role::Lint, "go vet ./..."),
        ("pyproject.toml", Role::Test, "pytest"),
        ("tox.ini", Role::Test, "tox"),
        ("Gemfile", Role::Test, "bundle exec rspec"),
        ("pom.xml", Role::Test, "mvn test"),
        ("pom.xml", Role::Build, "mvn package"),
        ("build.gradle", Role::Test, "./gradlew test"),
        ("build.gradle.kts", Role::Test, "./gradlew test"),
        ("mix.exs", Role::Test, "mix test"),
        ("composer.json", Role::Test, "./vendor/bin/phpunit"),
        ("pubspec.yaml", Role::Test, "dart test"),
        ("Package.swift", Role::Test, "swift test"),
        ("CMakeLists.txt", Role::Build, "cmake --build build"),
        // Benchmarks: how you MEASURE, which is the step agents skip most often.
        ("go.mod", Role::Bench, "go test -bench=. -benchmem ./..."),
        ("Cargo.toml", Role::Bench, "cargo bench"),
        ("mix.exs", Role::Bench, "mix run bench.exs"),
    ];
    for (marker, role, command) in NATIVE {
        if exists(&root.join(marker)) {
            commands.add(
                *role,
                Command {
                    command: (*command).to_string(),
                    source: Source::Convention,
                    from: (*marker).to_string(),
                },
            );
        }
    }
    managers
}

static WORKSPACE_TABLE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    // `/^\s*\[workspace\]/m`; JS `\s` spans newlines, so a blank line before it is fine.
    Regex::new(&format!(
        "(?:^|[\n\r\u{2028}\u{2029}])[{JS_WS}]*\\[workspace\\]"
    ))
    .ok()
});

pub fn detect_stack(root: &Path) -> Stack {
    let mut unknowns: Vec<String> = Vec::new();
    let mut commands = Commands::default();

    let (ci_files, ci_cmds) = ci_commands(root);
    for c in ci_cmds {
        if let Some(role) = classify(&c.command) {
            commands.add(role, c);
        }
    }

    let package_managers = task_runner_commands(root, &mut commands, &mut unknowns);

    let mut monorepo: Option<Monorepo> = None;
    for (file, tool) in MONOREPO_MARKERS {
        let full = root.join(file);
        if !exists(&full) {
            continue;
        }
        if *file == "Cargo.toml"
            && !WORKSPACE_TABLE
                .as_ref()
                .is_some_and(|r| r.is_match(&read(&full)))
        {
            continue;
        }
        monorepo = Some(Monorepo {
            tool: (*tool).to_string(),
            from: (*file).to_string(),
        });
        break;
    }

    let manifest_text = [
        "package.json",
        "pyproject.toml",
        "requirements.txt",
        "Gemfile",
        "composer.json",
        "go.mod",
        "Cargo.toml",
        "setup.cfg",
        "tox.ini",
    ]
    .iter()
    .map(|f| read(&root.join(f)))
    .collect::<Vec<_>>()
    .join("\n");
    let test_frameworks: Vec<String> = TEST_FRAMEWORK_HINTS
        .iter()
        .filter(|(pat, _)| Regex::new(pat).is_ok_and(|r| r.is_match(&manifest_text)))
        .map(|(_, name)| (*name).to_string())
        .collect();

    let languages = census_languages(root);
    let shape = census_shape(root);

    // A config-dominated repo must say so BEFORE the language line, because the language line is
    // computed over a sliver of the files and reads as the headline fact when it isn't one.
    let total_counted = shape.code_files + shape.config_files + shape.doc_files;
    if total_counted > 10 && (shape.code_files as f64 / total_counted as f64) < 0.2 {
        let infra = if shape.infra.is_empty() {
            String::new()
        } else {
            format!(" — {}", shape.infra.join(", "))
        };
        unknowns.push(format!(
            "CONFIG/INFRA REPO: {} config file(s) vs {} code file(s){infra}. The language census \
             describes only the code sliver, so do not treat it as what this repo is. Changes here are \
             configuration: validate with the relevant tool (kubectl/helm/terraform plan), not a test suite.",
            shape.config_files, shape.code_files
        ));
    }

    if commands.of(Role::Test).is_none() {
        unknowns.push(
            "no test command found — say so rather than guessing one; the project may genuinely have none"
                .to_string(),
        );
    }
    if ci_files.is_empty() {
        unknowns.push(
            "no CI config found, so no authoritative definition of 'green' to check against"
                .to_string(),
        );
    }
    if languages.len() > 3 {
        unknowns.push(format!(
            "polyglot repo ({} languages) — a single test command probably does not cover all of it",
            languages.len()
        ));
    }

    let top_langs: HashSet<&str> = languages.iter().take(4).map(|l| l.name.as_str()).collect();
    let profilers: Vec<String> = PROFILERS
        .iter()
        .filter(|(lang, bin, _)| top_langs.contains(lang) && on_path(bin))
        .map(|(_, _, note)| (*note).to_string())
        .collect();

    if commands.of(Role::Bench).is_none() && !languages.is_empty() {
        unknowns.push(
            "no benchmark command found — a performance claim needs a measurement, so establish a \
             baseline first (see /viby-toolkit:perf) rather than reasoning about the change"
                .to_string(),
        );
    }

    Stack {
        root: root.to_string_lossy().into_owned(),
        languages,
        package_managers,
        monorepo,
        test_frameworks,
        ci_files,
        profilers,
        commands,
        unknowns,
        shape,
    }
}

/// JS `padEnd`: pads to a UTF-16 length.
fn pad_end(s: &str, width: usize) -> String {
    let len = s.encode_utf16().count();
    format!("{s}{}", " ".repeat(width.saturating_sub(len)))
}

fn report(s: &Stack) -> String {
    let mut out: Vec<String> = Vec::new();
    let langs = s
        .languages
        .iter()
        .filter(|l| l.share >= 1)
        .take(6)
        .map(|l| format!("{} {}%", l.name, l.share))
        .collect::<Vec<_>>()
        .join(", ");
    // The shape line goes FIRST when config dominates. The language census is computed over code
    // files only, so on a 97%-YAML repo "Python 100%" is technically true and completely
    // misleading — and whichever line comes first is the one that gets believed.
    let counted = s.shape.code_files + s.shape.config_files + s.shape.doc_files;
    let config_dominated = counted > 10 && (s.shape.code_files as f64 / counted as f64) < 0.2;
    if config_dominated {
        let infra = if s.shape.infra.is_empty() {
            String::new()
        } else {
            format!(" · {}", s.shape.infra.join(", "))
        };
        out.push(format!(
            "repo shape    CONFIG/INFRA — {} config vs {} code file(s){infra}",
            s.shape.config_files, s.shape.code_files
        ));
    }
    let langs = if langs.is_empty() {
        "none detected"
    } else {
        &langs
    };
    out.push(format!(
        "languages     {langs}{}",
        if config_dominated {
            "   ← the code sliver only, not what this repo is"
        } else {
            ""
        }
    ));
    let pms = if s.package_managers.is_empty() {
        "unknown".to_string()
    } else {
        s.package_managers.join(", ")
    };
    out.push(format!("package mgr   {pms}"));
    if let Some(m) = &s.monorepo {
        out.push(format!("monorepo      {} ({})", m.tool, m.from));
    }
    if !s.test_frameworks.is_empty() {
        out.push(format!("test frameworks {}", s.test_frameworks.join(", ")));
    }
    let ci_shown = s
        .ci_files
        .iter()
        .take(4)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let ci_rest = if s.ci_files.len() > 4 {
        format!(" (+{} more)", s.ci_files.len() - 4)
    } else {
        String::new()
    };
    out.push(format!(
        "ci config     {}{ci_rest}",
        if ci_shown.is_empty() {
            "none"
        } else {
            &ci_shown
        }
    ));
    out.push(String::new());
    for role in [
        Role::Install,
        Role::Build,
        Role::Test,
        Role::Bench,
        Role::Typecheck,
        Role::Lint,
        Role::Format,
        Role::Dev,
    ] {
        let Some(list) = s.commands.of(role) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        out.push(format!("{}:", role.as_str()));
        for c in list.iter().take(4) {
            out.push(format!(
                "  {} [{} · {}]",
                pad_end(&c.command, 46),
                match c.source {
                    Source::Ci => "ci",
                    Source::TaskRunner => "task-runner",
                    Source::Convention => "convention",
                },
                c.from
            ));
        }
    }
    if !s.profilers.is_empty() {
        out.push(String::new());
        out.push("profilers available here:".to_string());
        for p in &s.profilers {
            out.push(format!("  {p}"));
        }
    }
    if !s.unknowns.is_empty() {
        out.push(String::new());
        out.push("unknowns (do not guess past these):".to_string());
        for u in &s.unknowns {
            out.push(format!("  · {u}"));
        }
    }
    out.push(String::new());
    out.push(
        "Prefer [ci] over [task-runner] over [convention]: CI is what the project itself"
            .to_string(),
    );
    out.push(
        "treats as green. Run the command, read the output — presence here is not a pass."
            .to_string(),
    );
    out.join("\n")
}

/// `path.resolve(arg)`: absolute and lexically normalized, symlinks untouched, no trailing slash.
fn resolve(arg: &str) -> String {
    let joined = if arg.starts_with('/') {
        arg.to_string()
    } else {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{cwd}/{arg}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

pub fn main(args: &[String]) -> i32 {
    let parsed = match parse_or_exit(args, &[flag("json")]) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let root = resolve(parsed.positionals.first().map_or(".", String::as_str));
    let stack = detect_stack(Path::new(&root));
    if parsed.flag("json") {
        match serde_json::to_string_pretty(&stack) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("error: {e}");
                return 1;
            }
        }
    } else {
        println!("{}", report(&stack));
    }
    0
}
