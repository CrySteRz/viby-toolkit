/**
 * Contract tests for the release pre-flight.
 *
 * Run: node --experimental-strip-types --test tests/check-release.test.ts
 *
 * Each case builds a throwaway git repo. Both halves pinned as always: what must be flagged,
 * and — the half that keeps a checker trustworthy — what must NOT be.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkRelease } from "../plugins/viby-toolkit/skills/release/scripts/check-release.ts";

function repo(files: Record<string, string>, opts: { commit?: boolean; tags?: string[] } = {}): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rel-"));
  const run = (...args: string[]): void => {
    spawnSync("git", args, { cwd: dir, encoding: "utf8" });
  };
  run("init", "-q");
  run("config", "user.email", "t@example.com");
  run("config", "user.name", "T");
  for (const [rel, body] of Object.entries(files)) {
    const full = path.join(dir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  if (opts.commit !== false) {
    run("add", "-A");
    run("commit", "-qm", "init");
  }
  for (const t of opts.tags ?? []) run("tag", t);
  return dir;
}

function checks(dir: string): string[] {
  return checkRelease(dir).findings.map((f) => f.check);
}

function cleanup(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

test("version drift across manifests is flagged", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.2.0" }),
    "pyproject.toml": "[project]\nname='x'\nversion = '1.1.0'\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    const found = checks(dir);
    assert.ok(found.includes("version-drift"), `expected version-drift, got ${found.join() || "none"}`);
  } finally {
    cleanup(dir);
  }
});

test("consistent versions across manifests are NOT flagged", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.2.0" }),
    "pyproject.toml": "[project]\nname='x'\nversion = '1.2.0'\n",
    "VERSION": "1.2.0\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    const found = checks(dir);
    assert.ok(!found.includes("version-drift"), `must not flag matching versions, got ${found.join()}`);
  } finally {
    cleanup(dir);
  }
});

test("a nested plugin/marketplace manifest is compared with the root", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "0.9.0" }),
    ".claude-plugin/marketplace.json": JSON.stringify({ plugins: [{ name: "p", version: "0.8.0" }] }),
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("version-drift"), "a stale marketplace version must be caught");
  } finally {
    cleanup(dir);
  }
});

test("monorepo per-package versions are expected, not drift", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.0.0" }),
    "pnpm-workspace.yaml": "packages:\n  - packages/*\n",
    "packages/a/package.json": JSON.stringify({ version: "2.3.1" }),
    "packages/b/package.json": JSON.stringify({ version: "0.4.0" }),
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    const found = checks(dir);
    assert.ok(
      !found.includes("version-drift"),
      `independent package versions are normal in a monorepo, got ${found.join()}`,
    );
  } finally {
    cleanup(dir);
  }
});

test("an uncommitted change is flagged, because it would not ship", () => {
  const dir = repo({ "package.json": JSON.stringify({ version: "1.0.0" }), ".github/workflows/ci.yml": "jobs: {}\n" });
  try {
    fs.writeFileSync(path.join(dir, "src.ts"), "export const x = 1;\n");
    assert.ok(checks(dir).includes("dirty-tree"));
  } finally {
    cleanup(dir);
  }
});

test("a clean committed tree is not flagged as dirty", () => {
  const dir = repo({ "package.json": JSON.stringify({ version: "1.0.0" }), ".github/workflows/ci.yml": "jobs: {}\n" });
  try {
    assert.ok(!checks(dir).includes("dirty-tree"));
  } finally {
    cleanup(dir);
  }
});

test("an existing tag for this version is flagged, with or without a v prefix", () => {
  for (const tag of ["1.0.0", "v1.0.0"]) {
    const dir = repo(
      { "package.json": JSON.stringify({ version: "1.0.0" }), ".github/workflows/ci.yml": "jobs: {}\n" },
      { tags: [tag] },
    );
    try {
      assert.ok(checks(dir).includes("tag-exists"), `tag ${tag} should be detected`);
    } finally {
      cleanup(dir);
    }
  }
});

test("an unrelated existing tag is not mistaken for this version", () => {
  const dir = repo(
    { "package.json": JSON.stringify({ version: "2.0.0" }), ".github/workflows/ci.yml": "jobs: {}\n" },
    { tags: ["v1.0.0"] },
  );
  try {
    assert.ok(!checks(dir).includes("tag-exists"));
  } finally {
    cleanup(dir);
  }
});

test("a changelog that omits this version is flagged", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "2.0.0" }),
    "CHANGELOG.md": "# Changelog\n\n## 1.9.0\n- old stuff\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("changelog-stale"));
  } finally {
    cleanup(dir);
  }
});

test("a changelog mentioning this version is clean", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "2.0.0" }),
    "CHANGELOG.md": "# Changelog\n\n## 2.0.0\n- the new thing\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(!checks(dir).includes("changelog-stale"));
  } finally {
    cleanup(dir);
  }
});

test("a real focused test is flagged as a debug artifact", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.0.0" }),
    "a.test.ts": 'describe.only("payments", () => {\n  it("charges", () => {});\n});\n',
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("debug-artifact"));
  } finally {
    cleanup(dir);
  }
});

test("a focused test inside a string fixture is NOT a debug artifact", () => {
  // The exact false positive this checker produced on its own repo before the shared
  // code-blanking pass was wired in.
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.0.0" }),
    "meta.test.ts":
      'const FIXTURE = `\ndescribe.only("payments", () => {});\n`;\n' +
      'test("scanner flags focused tests", () => {\n  expect(scan(FIXTURE)).toContain("focused");\n});\n',
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    const found = checks(dir);
    assert.ok(
      !found.includes("debug-artifact"),
      `a fixture mentioning describe.only is not a focused test, got ${found.join()}`,
    );
  } finally {
    cleanup(dir);
  }
});

test("a debugger statement is flagged", () => {
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.0.0" }),
    "src.ts": "export function f() {\n  debugger;\n  return 1;\n}\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("debug-artifact"));
  } finally {
    cleanup(dir);
  }
});

test("a missing CI config is reported, at low severity", () => {
  const dir = repo({ "package.json": JSON.stringify({ version: "1.0.0" }) });
  try {
    const { findings } = checkRelease(dir);
    const noCi = findings.find((f) => f.check === "no-ci");
    assert.ok(noCi, "expected a no-ci finding");
    assert.equal(noCi.severity, "P3", "missing CI is informational, not a blocker");
  } finally {
    cleanup(dir);
  }
});

test("a non-git directory reports isRepo=false rather than crashing", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "norepo-"));
  try {
    fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ version: "1.0.0" }));
    const r = checkRelease(dir);
    assert.equal(r.isRepo, false);
    assert.ok(!r.findings.some((f) => f.check === "dirty-tree"), "git checks must be skipped, not guessed");
  } finally {
    cleanup(dir);
  }
});

test("a version is matched as a token, not a substring", () => {
  // Regression: "12.0.1".includes("2.0") is true, so an unrelated older release silently
  // satisfied the changelog check.
  const dir = repo({
    "package.json": JSON.stringify({ version: "2.0" }),
    "CHANGELOG.md": "# Changelog\n\n## 12.0.1\n- unrelated\n",
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("changelog-stale"), "2.0 is not documented by 12.0.1");
  } finally {
    cleanup(dir);
  }
});

test("every debug artifact line is reported, not only the first", () => {
  // Regression: findIndex reported one hit per pattern per file, so a second focused test
  // shipped silently even after the first was fixed.
  const dir = repo({
    "package.json": JSON.stringify({ version: "1.0.0" }),
    "a.test.js": 'describe.only("a", () => {});\nconst x = 1;\nit.only("b", () => {});\n',
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    const f = checkRelease(dir).findings.find((x) => x.check === "debug-artifact");
    assert.ok(f, "expected a debug-artifact finding");
    assert.match(f.detail ?? "", /a\.test\.js:1/, "first artifact reported");
    assert.match(f.detail ?? "", /a\.test\.js:3/, "the second artifact on line 3 must also be reported");
  } finally {
    cleanup(dir);
  }
});

test("a manifest with a UTF-8 BOM is still parsed, so drift is still caught", () => {
  // Regression: Node keeps the BOM under "utf8", JSON.parse threw, the catch swallowed it,
  // and the manifest silently dropped out of version detection entirely.
  const dir = repo({
    "package.json": "\uFEFF" + JSON.stringify({ version: "1.0.0" }),
    "other/package.json": JSON.stringify({ version: "2.0.0" }),
    ".github/workflows/ci.yml": "jobs: {}\n",
  });
  try {
    assert.ok(checks(dir).includes("version-drift"), "a BOM must not hide a real version drift");
  } finally {
    cleanup(dir);
  }
});
