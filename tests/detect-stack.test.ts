/**
 * Contract tests for the stack detector.
 *
 * Run: node --experimental-strip-types --test tests/detect-stack.test.ts
 *
 * Each case builds a synthetic repo in a temp dir. The contract has three halves:
 *  1. it finds the right commands for each ecosystem,
 *  2. it ranks CI above task-runner above convention, and
 *  3. it says "unknown" instead of inventing a command — the property that makes it safe
 *     to trust, and the one a plausible-sounding guess would quietly destroy.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { detectStack } from "../plugins/viby-code/skills/verify/scripts/detect-stack.ts";

function build(files: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "stack-"));
  for (const [rel, body] of Object.entries(files)) {
    const full = path.join(dir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  return dir;
}

function commandsFor(files: Record<string, string>, role: string): string[] {
  const dir = build(files);
  try {
    const s = detectStack(dir);
    return (s.commands[role as keyof typeof s.commands] ?? []).map((c) => c.command);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test("node: pnpm lockfile picks pnpm, and scripts map to roles", () => {
  const files = {
    "pnpm-lock.yaml": "lockfileVersion: 6.0\n",
    "package.json": JSON.stringify({
      scripts: { test: "vitest run", build: "tsc -p .", lint: "eslint .", typecheck: "tsc --noEmit" },
      devDependencies: { vitest: "^1.0.0" },
    }),
    "src/index.ts": "export const x = 1;\n",
  };
  const dir = build(files);
  try {
    const s = detectStack(dir);
    assert.ok(s.packageManagers.includes("pnpm"), `expected pnpm, got ${s.packageManagers.join()}`);
    assert.deepEqual(s.commands.test?.map((c) => c.command), ["pnpm test"]);
    assert.deepEqual(s.commands.typecheck?.map((c) => c.command), ["pnpm typecheck"]);
    assert.ok(s.testFrameworks.includes("vitest"));
    assert.equal(s.languages[0]?.name, "TypeScript");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("rust: cargo conventions are found from Cargo.toml alone", () => {
  const cmds = commandsFor({ "Cargo.toml": "[package]\nname='x'\n", "src/main.rs": "fn main() {}\n" }, "test");
  assert.deepEqual(cmds, ["cargo test"]);
});

test("go: go.mod yields go test ./...", () => {
  const cmds = commandsFor({ "go.mod": "module x\n\ngo 1.22\n", "main.go": "package main\n" }, "test");
  assert.deepEqual(cmds, ["go test ./..."]);
});

test("python: pyproject yields pytest and uv is detected from uv.lock", () => {
  const dir = build({
    "pyproject.toml": "[project]\nname='x'\ndependencies=['pytest']\n",
    "uv.lock": "version = 1\n",
    "src/app.py": "x = 1\n",
  });
  try {
    const s = detectStack(dir);
    assert.deepEqual(s.commands.test?.map((c) => c.command), ["pytest"]);
    assert.ok(s.packageManagers.includes("uv"));
    assert.ok(s.testFrameworks.includes("pytest"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("ruby, php, elixir, java, swift, gradle conventions each resolve", () => {
  const cases: Array<[Record<string, string>, string]> = [
    [{ Gemfile: "source 'https://rubygems.org'\n" }, "bundle exec rspec"],
    [{ "composer.json": "{}" }, "./vendor/bin/phpunit"],
    [{ "mix.exs": "defmodule X do\nend\n" }, "mix test"],
    [{ "pom.xml": "<project/>" }, "mvn test"],
    [{ "Package.swift": "// swift-tools-version:5.9\n" }, "swift test"],
    [{ "build.gradle": "plugins {}\n" }, "./gradlew test"],
  ];
  for (const [files, expected] of cases) {
    const cmds = commandsFor(files, "test");
    assert.ok(cmds.includes(expected), `${Object.keys(files)[0]}: expected ${expected}, got ${cmds.join() || "none"}`);
  }
});

test("CI outranks the task runner and is labelled as authoritative", () => {
  const dir = build({
    "package.json": JSON.stringify({ scripts: { test: "vitest run" } }),
    "package-lock.json": "{}",
    ".github/workflows/ci.yml": `
name: ci
jobs:
  build:
    steps:
      - run: npm ci
      - run: npm run test:integration
`,
    "src/a.ts": "export {};\n",
  });
  try {
    const s = detectStack(dir);
    const tests = s.commands.test ?? [];
    assert.ok(tests.length >= 2, `expected CI and task-runner commands, got ${tests.length}`);
    assert.equal(tests[0]?.source, "ci", "the CI command must rank first");
    assert.equal(tests[0]?.command, "npm run test:integration");
    assert.ok(
      tests.some((c) => c.source === "task-runner"),
      "the package.json script must still be reported as a lower-authority option",
    );
    assert.ok(s.ciFiles.includes(".github/workflows/ci.yml"));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a repo with no test tooling reports unknown rather than inventing a command", () => {
  const dir = build({ "README.md": "# docs only\n", "notes.txt": "hello\n" });
  try {
    const s = detectStack(dir);
    assert.equal(s.commands.test, undefined, "must not invent a test command");
    assert.ok(
      s.unknowns.some((u) => u.includes("no test command")),
      `expected an explicit unknown, got ${JSON.stringify(s.unknowns)}`,
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("polyglot repos are flagged, because one test command will not cover them", () => {
  const dir = build({
    "a.ts": "export {};\n",
    "b.py": "x = 1\n",
    "c.go": "package main\n",
    "d.rs": "fn main() {}\n",
    "e.rb": "puts 1\n",
  });
  try {
    const s = detectStack(dir);
    assert.ok(s.languages.length >= 4);
    assert.ok(
      s.unknowns.some((u) => u.includes("polyglot")),
      `expected a polyglot warning, got ${JSON.stringify(s.unknowns)}`,
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("monorepo tooling is identified, and a plain Cargo.toml is not a workspace", () => {
  const nx = build({ "nx.json": "{}", "package.json": "{}" });
  const plain = build({ "Cargo.toml": "[package]\nname='x'\n" });
  const ws = build({ "Cargo.toml": "[workspace]\nmembers=['a']\n" });
  try {
    assert.equal(detectStack(nx).monorepo?.tool, "Nx");
    assert.equal(detectStack(plain).monorepo, null, "a non-workspace Cargo.toml is not a monorepo");
    assert.equal(detectStack(ws).monorepo?.tool, "Cargo workspace");
  } finally {
    for (const d of [nx, plain, ws]) fs.rmSync(d, { recursive: true, force: true });
  }
});

test("Makefile targets are picked up", () => {
  const cmds = commandsFor({ Makefile: "build:\n\tcc -o a a.c\ntest:\n\t./a\n", "a.c": "int main(){}\n" }, "test");
  assert.ok(cmds.includes("make test"), `got ${cmds.join() || "none"}`);
});

test("a CI publish script is not mistaken for an install step", () => {
  const dir = build({
    "package.json": "{}",
    ".github/workflows/publish.yml": "jobs:\n  p:\n    steps:\n      - run: pnpm run ci-publish\n",
  });
  try {
    const s = detectStack(dir);
    const installs = (s.commands.install ?? []).map((c) => c.command);
    assert.ok(
      !installs.some((c) => c.includes("ci-publish")),
      `\`ci-publish\` must not classify as install, got ${installs.join()}`,
    );
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("skips vendor directories when taking the language census", () => {
  const dir = build({
    "src/a.ts": "export {};\n",
    "node_modules/pkg/index.js": "module.exports = 1;\n",
    "node_modules/pkg/b.js": "module.exports = 2;\n",
    "dist/bundle.js": "console.log(1);\n",
  });
  try {
    const s = detectStack(dir);
    assert.equal(s.languages.length, 1, `only src should count, got ${JSON.stringify(s.languages)}`);
    assert.equal(s.languages[0]?.name, "TypeScript");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
