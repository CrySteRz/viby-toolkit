/**
 * Contract tests for the public-surface differ.
 *
 * Run: node --experimental-strip-types --test tests/check-api-surface.test.ts
 *
 * Both halves pinned. The must-NOT half carries most of the weight here: a differ that calls
 * every change breaking is exactly as useless as one that calls none, because the whole point
 * is to tell major from minor. So a renamed parameter must NOT be reported as a signature
 * break, and an export inside a string fixture must NOT be reported at all.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { diffSurface, extractSurface } from "../plugins/viby-toolkit/skills/api/scripts/check-api-surface.ts";

function repo(before: Record<string, string>, after: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "surface-"));
  const run = (...args: string[]): void => {
    spawnSync("git", args, { cwd: dir, encoding: "utf8" });
  };
  run("init", "-q");
  run("config", "user.email", "t@example.com");
  run("config", "user.name", "T");
  const write = (files: Record<string, string>): void => {
    for (const [rel, body] of Object.entries(files)) {
      const full = path.join(dir, rel);
      fs.mkdirSync(path.dirname(full), { recursive: true });
      fs.writeFileSync(full, body);
    }
  };
  write(before);
  run("add", "-A");
  run("commit", "-qm", "base");
  write(after);
  return dir;
}

function cleanup(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

function verdictOf(before: Record<string, string>, after: Record<string, string>): { verdict: string; kinds: string[] } {
  const dir = repo(before, after);
  try {
    const d = diffSurface(dir, "HEAD", [dir]);
    return { verdict: d.verdict, kinds: d.changes.map((c) => c.kind) };
  } finally {
    cleanup(dir);
  }
}

test("removing an exported function is MAJOR", () => {
  const r = verdictOf(
    { "src/a.ts": "export function alpha(x: number) {\n  return x;\n}\nexport function beta() {}\n" },
    { "src/a.ts": "export function alpha(x: number) {\n  return x;\n}\n" },
  );
  assert.equal(r.verdict, "major", JSON.stringify(r));
  assert.ok(r.kinds.includes("removed"));
});

test("adding an exported function is MINOR, not major", () => {
  const r = verdictOf(
    { "src/a.ts": "export function alpha() {}\n" },
    { "src/a.ts": "export function alpha() {}\nexport function beta(y: string) {}\n" },
  );
  assert.equal(r.verdict, "minor", JSON.stringify(r));
});

test("an internal-only change with no surface movement is PATCH", () => {
  const r = verdictOf(
    { "src/a.ts": "export function alpha() {\n  return 1;\n}\n" },
    { "src/a.ts": "export function alpha() {\n  const x = 2;\n  return x - 1;\n}\n" },
  );
  assert.equal(r.verdict, "patch", JSON.stringify(r));
});

test("changing the parameter count is a signature break", () => {
  const r = verdictOf(
    { "src/a.ts": "export function alpha(a: number) {}\n" },
    { "src/a.ts": "export function alpha(a: number, b: number) {}\n" },
  );
  assert.equal(r.verdict, "major");
  assert.ok(r.kinds.includes("signature"), JSON.stringify(r));
});

test("renaming a parameter is P2, NOT a major break", () => {
  // Positionally compatible. Calling it major would make the tool cry wolf on a rename, and a
  // differ nobody believes gets ignored on the one release where it was right.
  const r = verdictOf(
    { "src/a.ts": "export function alpha(oldName: number) {}\n" },
    { "src/a.ts": "export function alpha(newName: number) {}\n" },
  );
  assert.ok(r.kinds.includes("param-name"), JSON.stringify(r));
  assert.notEqual(r.verdict, "major", "a positional rename must not be reported as MAJOR");
});

test("an `export function` inside a string fixture is NOT part of the surface", () => {
  const { symbols } = extractSurface(
    'const FIXTURE = `\nexport function ghost() {}\n`;\nexport function real() {}\n',
    "a.ts",
  );
  assert.deepEqual(symbols.map((s) => s.name), ["real"], JSON.stringify(symbols));
});

test("an `export function` in a comment is NOT part of the surface", () => {
  const { symbols } = extractSurface("// export function ghost() {}\nexport function real() {}\n", "a.ts");
  assert.deepEqual(symbols.map((s) => s.name), ["real"]);
});

test("types, interfaces, classes and consts are all surface", () => {
  const { symbols } = extractSurface(
    "export type A = string;\nexport interface B { x: number }\nexport class C {}\nexport const d = 1;\n",
    "a.ts",
  );
  assert.deepEqual(symbols.map((s) => s.kind).sort(), ["class", "const", "interface", "type"]);
});

test("a widened exported type is a signature change, because callers depend on it", () => {
  const r = verdictOf(
    { "src/a.ts": "export type Mode = 'on' | 'off';\n" },
    { "src/a.ts": "export type Mode = 'on';\n" },
  );
  assert.ok(r.kinds.includes("signature"), JSON.stringify(r));
});

test("`export { a as b }` exports the ALIAS, which is what a caller imports", () => {
  const { symbols } = extractSurface("function a() {}\nexport { a as publicName };\n", "a.ts");
  assert.ok(symbols.some((s) => s.name === "publicName"), JSON.stringify(symbols));
});

test("an unresolved `export * from` barrel is REPORTED, never silently excluded", () => {
  // The tool cannot follow it without module resolution. Staying quiet would make the report
  // claim a completeness it does not have — the exact failure the source spike documented.
  const { barrels } = extractSurface('export * from "./inner.ts";\n', "a.ts");
  assert.deepEqual(barrels, ["./inner.ts"]);
  const r = verdictOf({ "src/a.ts": "export const x = 1;\n" }, { "src/a.ts": 'export const x = 1;\nexport * from "./inner.ts";\n' });
  assert.ok(r.kinds.includes("unresolved-reexport"), JSON.stringify(r));
});

test("Python: a leading-underscore def is private and NOT surface", () => {
  const { symbols } = extractSurface("def public(a):\n    pass\n\ndef _private(b):\n    pass\n", "m.py");
  assert.deepEqual(symbols.map((s) => s.name), ["public"]);
});

test("Python: an explicit __all__ IS the surface, overriding convention", () => {
  const { symbols } = extractSurface('__all__ = ["only_this"]\n\ndef only_this():\n    pass\n\ndef also_public():\n    pass\n', "m.py");
  assert.deepEqual(symbols.map((s) => s.name), ["only_this"]);
});

test("Go: a lowercase func is unexported and NOT surface", () => {
  const { symbols } = extractSurface("func Exported(a int) {}\nfunc unexported(b int) {}\n", "m.go");
  assert.deepEqual(symbols.map((s) => s.name), ["Exported"]);
});

test("Rust: only `pub` items are surface", () => {
  const { symbols } = extractSurface("pub fn open(path: String) {}\nfn hidden() {}\npub struct Handle;\n", "m.rs");
  assert.deepEqual(symbols.map((s) => s.name).sort(), ["Handle", "open"]);
});

test("a language it does not understand yields no symbols rather than a guess", () => {
  const { symbols } = extractSurface("PROCEDURE DIVISION.\nDISPLAY 'HELLO'.\n", "legacy.cbl");
  assert.deepEqual(symbols, []);
});

test("a non-git directory reports unknown instead of inventing a verdict", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nogit-"));
  try {
    fs.writeFileSync(path.join(dir, "a.ts"), "export const x = 1;\n");
    const d = diffSurface(dir, "HEAD", [dir]);
    assert.equal(d.verdict, "unknown");
    assert.equal(d.changes.length, 0, "no repo means nothing to compare, not 'no changes'");
  } finally {
    cleanup(dir);
  }
});

test("a brand-new file's exports are additions, not a false 'removed' on the old side", () => {
  const r = verdictOf(
    { "src/a.ts": "export const x = 1;\n" },
    { "src/a.ts": "export const x = 1;\n", "src/b.ts": "export const y = 2;\n" },
  );
  assert.equal(r.verdict, "minor", JSON.stringify(r));
  assert.ok(!r.kinds.includes("removed"));
});

test("CLI: a breaking change exits 1, an additive one exits 0", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "api", "scripts", "check-api-surface.ts",
  );
  const run = (dir: string): { status: number | null; stdout: string } => {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, ".", "--quiet"],
      { cwd: dir, encoding: "utf8" },
    );
    return { status: p.status, stdout: p.stdout ?? "" };
  };

  const broken = repo({ "a.ts": "export function alpha() {}\n" }, { "a.ts": "export function renamed() {}\n" });
  try {
    const r = run(broken);
    assert.equal(r.status, 1, `a removal must exit 1: ${r.stdout}`);
  } finally {
    cleanup(broken);
  }

  const additive = repo({ "a.ts": "export function alpha() {}\n" }, { "a.ts": "export function alpha() {}\nexport function beta() {}\n" });
  try {
    const r = run(additive);
    assert.equal(r.status, 0, `an addition must exit 0: ${r.stdout}`);
  } finally {
    cleanup(additive);
  }
});

test("the report always states that behavioural breaks are invisible to it", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "api", "scripts", "check-api-surface.ts",
  );
  const dir = repo({ "a.ts": "export function alpha() {}\n" }, { "a.ts": "export function alpha() {}\nexport const b = 2;\n" });
  try {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, "."],
      { cwd: dir, encoding: "utf8" },
    );
    assert.match(p.stdout ?? "", /Syntactic surface only/);
    assert.match(p.stdout ?? "", /MEANING changed/);
  } finally {
    cleanup(dir);
  }
});
