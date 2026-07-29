/**
 * viby-toolkit stack detector — makes "language-agnostic" executable instead of aspirational.
 *
 * Every skill here says some version of "find the project's real commands." This finds
 * them, for any language, and reports where each answer came from so the caller can judge
 * how much to trust it.
 *
 * Usage:
 *   node detect-stack.ts [dir]          # human-readable report
 *   node detect-stack.ts [dir] --json   # machine-readable
 *
 * Exit: 0 always (a detector that fails a build is worse than one that says "unknown").
 *
 * DESIGN: authority is ranked, and every command carries its source.
 *   1. CI config      — what the project itself considers "green". Authoritative.
 *   2. Task runner    — package.json scripts, Makefile, justfile, pyproject, Cargo…
 *   3. Convention     — inferred from a lockfile or manifest.
 * It NEVER invents a command. An honest `unknown` beats a plausible guess that wastes a
 * cycle and produces a misleading failure — the same reason the test scanner drops checks
 * it cannot make precise.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

type Source = "ci" | "task-runner" | "convention";
type Command = { command: string; source: Source; from: string };
type Commands = Partial<
  Record<"install" | "build" | "test" | "lint" | "typecheck" | "format" | "dev" | "bench", Command[]>
>;

export type Stack = {
  root: string;
  languages: Array<{ name: string; files: number; share: number }>;
  packageManagers: string[];
  monorepo: { tool: string; from: string } | null;
  testFrameworks: string[];
  ciFiles: string[];
  profilers: string[];
  commands: Commands;
  unknowns: string[];
  shape: Shape;
};

const SKIP_DIRS = new Set([
  ".git", "node_modules", "venv", ".venv", "dist", "build", "target", "out",
  "__pycache__", ".pytest_cache", ".next", ".nuxt", "vendor", "coverage", ".tox",
  ".gradle", ".idea", ".vscode", "Pods", "bin", "obj", ".terraform", ".mypy_cache",
]);

// Extension -> language. Deliberately broad: the census is for orientation, so a missing
// exotic extension degrades the report rather than breaking it.
const LANGS: Record<string, string> = {
  ".ts": "TypeScript", ".tsx": "TypeScript", ".mts": "TypeScript", ".cts": "TypeScript",
  ".js": "JavaScript", ".jsx": "JavaScript", ".mjs": "JavaScript", ".cjs": "JavaScript",
  ".py": "Python", ".pyi": "Python",
  ".go": "Go", ".rs": "Rust", ".rb": "Ruby", ".php": "PHP",
  ".java": "Java", ".kt": "Kotlin", ".kts": "Kotlin", ".scala": "Scala", ".groovy": "Groovy",
  ".cs": "C#", ".fs": "F#", ".vb": "VB.NET",
  ".c": "C", ".h": "C/C++ header", ".cc": "C++", ".cpp": "C++", ".cxx": "C++", ".hpp": "C++",
  ".swift": "Swift", ".m": "Objective-C", ".mm": "Objective-C++",
  ".ex": "Elixir", ".exs": "Elixir", ".erl": "Erlang", ".hs": "Haskell", ".ml": "OCaml",
  ".clj": "Clojure", ".cljs": "ClojureScript", ".dart": "Dart", ".lua": "Lua",
  ".sh": "Shell", ".bash": "Shell", ".zsh": "Shell", ".ps1": "PowerShell",
  ".sql": "SQL", ".tf": "Terraform", ".hcl": "HCL",
  ".vue": "Vue", ".svelte": "Svelte", ".astro": "Astro",
  ".zig": "Zig", ".nim": "Nim", ".jl": "Julia", ".r": "R", ".pl": "Perl",
  ".proto": "Protobuf", ".graphql": "GraphQL", ".gql": "GraphQL",
};

// Lockfile / manifest -> package manager, and the install command it implies.
const PM_MARKERS: Array<{ file: string; pm: string; install: string }> = [
  { file: "pnpm-lock.yaml", pm: "pnpm", install: "pnpm install --frozen-lockfile" },
  { file: "bun.lockb", pm: "bun", install: "bun install --frozen-lockfile" },
  { file: "bun.lock", pm: "bun", install: "bun install --frozen-lockfile" },
  { file: "yarn.lock", pm: "yarn", install: "yarn install --immutable" },
  { file: "package-lock.json", pm: "npm", install: "npm ci" },
  { file: "uv.lock", pm: "uv", install: "uv sync" },
  { file: "poetry.lock", pm: "poetry", install: "poetry install" },
  { file: "Pipfile.lock", pm: "pipenv", install: "pipenv install --dev" },
  { file: "requirements.txt", pm: "pip", install: "pip install -r requirements.txt" },
  { file: "Cargo.lock", pm: "cargo", install: "cargo fetch" },
  { file: "go.sum", pm: "go modules", install: "go mod download" },
  { file: "Gemfile.lock", pm: "bundler", install: "bundle install" },
  { file: "composer.lock", pm: "composer", install: "composer install" },
  { file: "mix.lock", pm: "mix", install: "mix deps.get" },
  { file: "pubspec.lock", pm: "pub", install: "dart pub get" },
  { file: "gradle.lockfile", pm: "gradle", install: "./gradlew dependencies" },
];

const MONOREPO_MARKERS: Array<{ file: string; tool: string }> = [
  { file: "nx.json", tool: "Nx" },
  { file: "turbo.json", tool: "Turborepo" },
  { file: "lerna.json", tool: "Lerna" },
  { file: "pnpm-workspace.yaml", tool: "pnpm workspaces" },
  { file: "rush.json", tool: "Rush" },
  { file: "WORKSPACE", tool: "Bazel" },
  { file: "MODULE.bazel", tool: "Bazel" },
  { file: "Cargo.toml", tool: "Cargo workspace" }, // confirmed below by [workspace]
  { file: "go.work", tool: "Go workspace" },
];

const TEST_FRAMEWORK_HINTS: Array<{ pattern: RegExp; name: string }> = [
  { pattern: /"vitest"/, name: "vitest" },
  { pattern: /"jest"/, name: "jest" },
  { pattern: /"mocha"/, name: "mocha" },
  { pattern: /"@playwright\/test"/, name: "playwright" },
  { pattern: /"cypress"/, name: "cypress" },
  { pattern: /"ava"/, name: "ava" },
  { pattern: /"jasmine"/, name: "jasmine" },
  { pattern: /\bpytest\b/, name: "pytest" },
  { pattern: /\bunittest\b/, name: "unittest" },
  { pattern: /\bnose2?\b/, name: "nose" },
  { pattern: /\brspec\b/, name: "rspec" },
  { pattern: /\bminitest\b/, name: "minitest" },
  { pattern: /\bphpunit\b/, name: "phpunit" },
  { pattern: /\bjunit\b/, name: "junit" },
  { pattern: /\bgotestsum\b/, name: "gotestsum" },
];

/** Which script names map to which role. Ordered: earlier names win. */
const SCRIPT_ROLES: Array<{ role: keyof Commands; names: string[] }> = [
  { role: "test", names: ["test", "tests", "test:unit", "unit", "spec", "check:test"] },
  { role: "build", names: ["build", "compile", "bundle", "dist"] },
  { role: "lint", names: ["lint", "eslint", "check:lint", "ruff", "clippy"] },
  { role: "typecheck", names: ["typecheck", "type-check", "tsc", "types", "mypy"] },
  { role: "format", names: ["format", "fmt", "prettier", "format:check"] },
  { role: "bench", names: ["bench", "benchmark", "benchmarks", "perf"] },
  { role: "dev", names: ["dev", "start", "serve", "watch"] },
];

/**
 * Profilers worth reaching for per language, reported only when the binary is actually on
 * PATH. Agent-authored performance changes validate by static reasoning 67.2% of the time
 * (arXiv 2512.21757) — naming the tool that is already installed removes one excuse.
 */
const PROFILERS: Array<{ lang: string; bin: string; note: string }> = [
  { lang: "Python", bin: "py-spy", note: "py-spy top -- python x.py (sampling, no code change)" },
  { lang: "Python", bin: "python3", note: "python3 -m cProfile -s cumtime x.py" },
  { lang: "Python", bin: "scalene", note: "scalene x.py (CPU + memory)" },
  { lang: "Go", bin: "go", note: "go test -cpuprofile cpu.out -bench . && go tool pprof cpu.out" },
  { lang: "Rust", bin: "cargo", note: "cargo flamegraph / cargo bench (criterion)" },
  { lang: "JavaScript", bin: "node", note: "node --cpu-prof / --heap-prof, or 0x" },
  { lang: "TypeScript", bin: "node", note: "node --cpu-prof / --heap-prof, or 0x" },
  { lang: "Ruby", bin: "ruby", note: "stackprof / rbspy" },
  { lang: "Java", bin: "java", note: "async-profiler / JFR" },
  { lang: "C", bin: "perf", note: "perf record -g ./bin && perf report" },
  { lang: "C++", bin: "perf", note: "perf record -g ./bin && perf report" },
];

function onPath(bin: string): boolean {
  const dirs = (process.env.PATH ?? "").split(path.delimiter);
  for (const d of dirs) {
    if (!d) continue;
    try {
      if (fs.statSync(path.join(d, bin)).isFile()) return true;
    } catch {
      /* keep looking */
    }
  }
  return false;
}

function exists(p: string): boolean {
  try {
    fs.statSync(p);
    return true;
  } catch {
    return false;
  }
}

function read(p: string): string {
  try {
    // Strip a UTF-8 BOM: Node's "utf8" keeps it, and JSON.parse then throws on an
    // otherwise-valid manifest. That silently dropped the file from version detection, so a
    // real version drift went unreported because of an encoding artifact.
    return fs.readFileSync(p, "utf8").replace(/^\uFEFF/, "");
  } catch {
    return "";
  }
}

/**
 * Config / infrastructure file extensions. Not programming languages, and deliberately NOT
 * folded into the language census — but counted, because a repo can be 97% YAML and the census
 * will happily report "Python 100%" off a single stray script. That happened on a real
 * Kubernetes GitOps repo: 183 YAML files, 1 Python file, headline "Python 100%". A fast
 * confident wrong answer about what a repo even IS is the worst output this tool can produce.
 */
const CONFIG_EXT = new Set([".yaml", ".yml", ".tf", ".tfvars", ".json", ".toml", ".ini", ".conf", ".hcl", ".env", ".properties"]);
const INFRA_MARKERS: Array<{ file: RegExp; what: string }> = [
  { file: /^kustomization\.ya?ml$/i, what: "Kubernetes (kustomize)" },
  { file: /^Chart\.ya?ml$/i, what: "Helm chart" },
  { file: /^values(-\w+)?\.ya?ml$/i, what: "Helm values" },
  { file: /\.tf$/i, what: "Terraform" },
  { file: /^docker-compose(\.\w+)?\.ya?ml$/i, what: "Docker Compose" },
  { file: /^Dockerfile/i, what: "Docker image" },
  { file: /^(playbook|site)\.ya?ml$/i, what: "Ansible" },
  { file: /^serverless\.ya?ml$/i, what: "Serverless Framework" },
  { file: /^skaffold\.ya?ml$/i, what: "Skaffold" },
];

export type Shape = { codeFiles: number; configFiles: number; docFiles: number; infra: string[] };

/** Whether this repo is mostly configuration rather than code, and of what kind. */
function censusShape(root: string): Shape {
  const shape: Shape = { codeFiles: 0, configFiles: 0, docFiles: 0, infra: [] };
  const seen = new Set<string>();
  const stack = [root];
  let visited = 0;
  while (stack.length > 0 && visited < 20_000) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name) && !e.name.startsWith(".")) stack.push(path.join(dir, e.name));
        continue;
      }
      visited += 1;
      const ext = path.extname(e.name).toLowerCase();
      if (LANGS[ext] !== undefined) shape.codeFiles += 1;
      else if (CONFIG_EXT.has(ext)) shape.configFiles += 1;
      else if (ext === ".md" || ext === ".rst" || ext === ".txt") shape.docFiles += 1;
      for (const m of INFRA_MARKERS) {
        if (m.file.test(e.name) && !seen.has(m.what)) {
          seen.add(m.what);
          shape.infra.push(m.what);
        }
      }
    }
  }
  return shape;
}

function censusLanguages(root: string): Array<{ name: string; files: number; share: number }> {
  const counts = new Map<string, number>();
  let total = 0;
  const stack = [root];
  let visited = 0;
  while (stack.length > 0 && visited < 20_000) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name) && !e.name.startsWith(".")) stack.push(path.join(dir, e.name));
        continue;
      }
      visited += 1;
      const lang = LANGS[path.extname(e.name).toLowerCase()];
      if (lang === undefined) continue;
      counts.set(lang, (counts.get(lang) ?? 0) + 1);
      total += 1;
    }
  }
  return [...counts.entries()]
    .map(([name, files]) => ({ name, files, share: total ? Math.round((files / total) * 100) : 0 }))
    .sort((a, b) => b.files - a.files);
}

/** Commands a CI workflow actually runs. The project's own definition of "green". */
function ciCommands(root: string): { files: string[]; commands: Command[] } {
  const files: string[] = [];
  const commands: Command[] = [];
  const candidates = [
    ".github/workflows",
    ".gitlab-ci.yml",
    ".circleci/config.yml",
    "azure-pipelines.yml",
    "Jenkinsfile",
    ".travis.yml",
    "bitbucket-pipelines.yml",
    ".drone.yml",
  ];
  for (const c of candidates) {
    const full = path.join(root, c);
    if (!exists(full)) continue;
    let list: string[] = [full];
    try {
      if (fs.statSync(full).isDirectory()) {
        list = fs
          .readdirSync(full)
          .filter((f) => /\.ya?ml$/.test(f))
          .map((f) => path.join(full, f));
      }
    } catch {
      continue;
    }
    for (const f of list) {
      files.push(path.relative(root, f));
      const text = read(f);
      // `run:` steps, both scalar and block forms.
      for (const m of text.matchAll(/^\s*(?:-\s*)?run:\s*(?:\|[-+]?\s*)?(.*)$/gm)) {
        const line = (m[1] ?? "").trim();
        if (line === "") continue;
        for (const part of line.split(/&&|;/)) {
          const cmd = part.trim();
          if (cmd.length < 3 || cmd.length > 160) continue;
          if (/^(cd |echo |export |if |fi$|then$|#)/.test(cmd)) continue;
          commands.push({ command: cmd, source: "ci", from: path.relative(root, f) });
        }
      }
    }
  }
  return { files, commands: commands.slice(0, 60) };
}

function classify(cmd: string): (keyof Commands) | null {
  // Benchmarks first: `go test -bench=.` contains "test" but is a measurement, not a check.
  if (/(-bench|\bbench\b|\bbenchmark|criterion|hyperfine|pytest-benchmark|\basv\b)/i.test(cmd)) return "bench";
  if (/\b(test|pytest|vitest|jest|mocha|go test|cargo test|rspec|phpunit|gradlew test|mix test)\b/i.test(cmd)) {
    return "test";
  }
  if (/\b(tsc|mypy|pyright|typecheck|type-check)\b/i.test(cmd)) return "typecheck";
  if (/\b(lint|eslint|ruff|clippy|flake8|golangci-lint|rubocop|shellcheck)\b/i.test(cmd)) return "lint";
  if (/\b(fmt|format|prettier|black|gofmt|rustfmt)\b/i.test(cmd)) return "format";
  if (/\b(build|compile|bundle|make\b|cargo build|go build|gradlew (assemble|build)|mvn package)\b/i.test(cmd)) {
    return "build";
  }
  // `npm ci` specifically, never a bare `ci` — that matched `pnpm run ci-publish`, since a
  // word boundary sits between "ci" and "-".
  if (/(\bnpm ci\b|\binstall\b|\bsync\b|deps\.get|mod download|bundle install|\brestore\b)/i.test(cmd)) {
    return "install";
  }
  return null;
}

function add(commands: Commands, role: keyof Commands, entry: Command): void {
  const list = commands[role] ?? [];
  if (!list.some((c) => c.command === entry.command)) list.push(entry);
  commands[role] = list;
}

function taskRunnerCommands(root: string, commands: Commands, unknowns: string[]): string[] {
  const managers: string[] = [];
  let runner = "npm run";
  for (const m of PM_MARKERS) {
    if (!exists(path.join(root, m.file))) continue;
    managers.push(m.pm);
    add(commands, "install", { command: m.install, source: "convention", from: m.file });
    if (m.pm === "pnpm") runner = "pnpm";
    else if (m.pm === "yarn") runner = "yarn";
    else if (m.pm === "bun") runner = "bun run";
  }

  // package.json scripts — the single richest source in the JS/TS world.
  const pkgPath = path.join(root, "package.json");
  if (exists(pkgPath)) {
    try {
      const pkg: unknown = JSON.parse(read(pkgPath));
      const scripts =
        typeof pkg === "object" && pkg !== null && "scripts" in pkg
          ? ((pkg as { scripts?: unknown }).scripts as Record<string, unknown> | undefined)
          : undefined;
      if (scripts) {
        for (const { role, names } of SCRIPT_ROLES) {
          for (const n of names) {
            if (typeof scripts[n] === "string") {
              add(commands, role, { command: `${runner} ${n}`, source: "task-runner", from: "package.json" });
              break;
            }
          }
        }
      }
    } catch {
      unknowns.push("package.json is present but could not be parsed");
    }
  }

  // Makefile / justfile / Taskfile targets.
  for (const [file, invoke] of [["Makefile", "make"], ["justfile", "just"], ["Taskfile.yml", "task"]] as const) {
    const full = path.join(root, file);
    if (!exists(full)) continue;
    const text = read(full);
    const targets = new Set<string>();
    const re = file === "Makefile" ? /^([A-Za-z0-9_.-]+):(?!=)/gm : /^([A-Za-z0-9_-]+):/gm;
    for (const m of text.matchAll(re)) if (m[1]) targets.add(m[1]);
    for (const { role, names } of SCRIPT_ROLES) {
      for (const n of names) {
        if (targets.has(n)) {
          add(commands, role, { command: `${invoke} ${n}`, source: "task-runner", from: file });
          break;
        }
      }
    }
  }

  // Language-native conventions, only when the manifest is actually present.
  const native: Array<{ marker: string; role: keyof Commands; command: string }> = [
    { marker: "Cargo.toml", role: "test", command: "cargo test" },
    { marker: "Cargo.toml", role: "build", command: "cargo build" },
    { marker: "Cargo.toml", role: "lint", command: "cargo clippy -- -D warnings" },
    { marker: "go.mod", role: "test", command: "go test ./..." },
    { marker: "go.mod", role: "build", command: "go build ./..." },
    { marker: "go.mod", role: "lint", command: "go vet ./..." },
    { marker: "pyproject.toml", role: "test", command: "pytest" },
    { marker: "tox.ini", role: "test", command: "tox" },
    { marker: "Gemfile", role: "test", command: "bundle exec rspec" },
    { marker: "pom.xml", role: "test", command: "mvn test" },
    { marker: "pom.xml", role: "build", command: "mvn package" },
    { marker: "build.gradle", role: "test", command: "./gradlew test" },
    { marker: "build.gradle.kts", role: "test", command: "./gradlew test" },
    { marker: "mix.exs", role: "test", command: "mix test" },
    { marker: "composer.json", role: "test", command: "./vendor/bin/phpunit" },
    { marker: "pubspec.yaml", role: "test", command: "dart test" },
    { marker: "Package.swift", role: "test", command: "swift test" },
    { marker: "CMakeLists.txt", role: "build", command: "cmake --build build" },
    // Benchmarks: how you MEASURE, which is the step agents skip most often.
    { marker: "go.mod", role: "bench", command: "go test -bench=. -benchmem ./..." },
    { marker: "Cargo.toml", role: "bench", command: "cargo bench" },
    { marker: "mix.exs", role: "bench", command: "mix run bench.exs" },
  ];
  for (const n of native) {
    if (exists(path.join(root, n.marker))) {
      add(commands, n.role, { command: n.command, source: "convention", from: n.marker });
    }
  }
  return managers;
}

export function detectStack(root: string): Stack {
  const unknowns: string[] = [];
  const commands: Commands = {};

  const ci = ciCommands(root);
  for (const c of ci.commands) {
    const role = classify(c.command);
    if (role) add(commands, role, c);
  }

  const packageManagers = taskRunnerCommands(root, commands, unknowns);

  let monorepo: Stack["monorepo"] = null;
  for (const m of MONOREPO_MARKERS) {
    const full = path.join(root, m.file);
    if (!exists(full)) continue;
    if (m.file === "Cargo.toml" && !/^\s*\[workspace\]/m.test(read(full))) continue;
    monorepo = { tool: m.tool, from: m.file };
    break;
  }

  const manifestText = [
    "package.json", "pyproject.toml", "requirements.txt", "Gemfile", "composer.json",
    "go.mod", "Cargo.toml", "setup.cfg", "tox.ini",
  ]
    .map((f) => read(path.join(root, f)))
    .join("\n");
  const testFrameworks = TEST_FRAMEWORK_HINTS.filter((h) => h.pattern.test(manifestText)).map((h) => h.name);

  const languages = censusLanguages(root);
  const shape = censusShape(root);

  // A config-dominated repo must say so BEFORE the language line, because the language line is
  // computed over a sliver of the files and reads as the headline fact when it isn't one.
  const totalCounted = shape.codeFiles + shape.configFiles + shape.docFiles;
  if (totalCounted > 10 && shape.codeFiles / totalCounted < 0.2) {
    unknowns.push(
      `CONFIG/INFRA REPO: ${shape.configFiles} config file(s) vs ${shape.codeFiles} code file(s)` +
        `${shape.infra.length > 0 ? ` — ${shape.infra.join(", ")}` : ""}. The language census ` +
        `describes only the code sliver, so do not treat it as what this repo is. Changes here are ` +
        `configuration: validate with the relevant tool (kubectl/helm/terraform plan), not a test suite.`,
    );
  }

  if (commands.test === undefined) {
    unknowns.push("no test command found — say so rather than guessing one; the project may genuinely have none");
  }
  if (ci.files.length === 0) {
    unknowns.push("no CI config found, so no authoritative definition of 'green' to check against");
  }
  if (languages.length > 3) {
    unknowns.push(
      `polyglot repo (${languages.length} languages) — a single test command probably does not cover all of it`,
    );
  }

  const topLangs = new Set(languages.slice(0, 4).map((l) => l.name));
  const profilers = PROFILERS.filter((p) => topLangs.has(p.lang) && onPath(p.bin)).map((p) => p.note);

  if (commands.bench === undefined && languages.length > 0) {
    unknowns.push(
      "no benchmark command found — a performance claim needs a measurement, so establish a " +
        "baseline first (see /viby-toolkit:perf) rather than reasoning about the change",
    );
  }

  return {
    root,
    languages,
    packageManagers,
    monorepo,
    testFrameworks,
    ciFiles: ci.files,
    profilers,
    commands,
    unknowns,
    shape,
  };
}

function report(s: Stack): string {
  const out: string[] = [];
  const langs = s.languages
    .filter((l) => l.share >= 1)
    .slice(0, 6)
    .map((l) => `${l.name} ${l.share}%`)
    .join(", ");
  // The shape line goes FIRST when config dominates. The language census is computed over code
  // files only, so on a 97%-YAML repo "Python 100%" is technically true and completely
  // misleading — and whichever line comes first is the one that gets believed.
  const counted = s.shape.codeFiles + s.shape.configFiles + s.shape.docFiles;
  const configDominated = counted > 10 && s.shape.codeFiles / counted < 0.2;
  if (configDominated) {
    out.push(
      `repo shape    CONFIG/INFRA — ${s.shape.configFiles} config vs ${s.shape.codeFiles} code file(s)` +
        `${s.shape.infra.length > 0 ? ` · ${s.shape.infra.join(", ")}` : ""}`,
    );
  }
  out.push(`languages     ${langs || "none detected"}${configDominated ? "   ← the code sliver only, not what this repo is" : ""}`);
  out.push(`package mgr   ${s.packageManagers.join(", ") || "unknown"}`);
  if (s.monorepo) out.push(`monorepo      ${s.monorepo.tool} (${s.monorepo.from})`);
  if (s.testFrameworks.length) out.push(`test frameworks ${s.testFrameworks.join(", ")}`);
  const ciShown = s.ciFiles.slice(0, 4).join(", ");
  const ciRest = s.ciFiles.length > 4 ? ` (+${s.ciFiles.length - 4} more)` : "";
  out.push(`ci config     ${ciShown || "none"}${ciRest}`);
  out.push("");
  const roles: Array<keyof Commands> = ["install", "build", "test", "bench", "typecheck", "lint", "format", "dev"];
  for (const role of roles) {
    const list = s.commands[role];
    if (!list || list.length === 0) continue;
    out.push(`${role}:`);
    for (const c of list.slice(0, 4)) out.push(`  ${c.command.padEnd(46)} [${c.source} · ${c.from}]`);
  }
  if (s.profilers.length) {
    out.push("");
    out.push("profilers available here:");
    for (const p of s.profilers) out.push(`  ${p}`);
  }
  if (s.unknowns.length) {
    out.push("");
    out.push("unknowns (do not guess past these):");
    for (const u of s.unknowns) out.push(`  · ${u}`);
  }
  out.push("");
  out.push("Prefer [ci] over [task-runner] over [convention]: CI is what the project itself");
  out.push("treats as green. Run the command, read the output — presence here is not a pass.");
  return out.join("\n");
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { json: { type: "boolean", default: false } },
  });
  const root = path.resolve(positionals[0] ?? ".");
  const stack = detectStack(root);
  console.log(values.json ? JSON.stringify(stack, null, 2) : report(stack));
  return 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
