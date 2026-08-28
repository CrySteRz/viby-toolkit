/**
 * Contract tests for the TypeScript runner shim.
 *
 * Run: node --experimental-strip-types --test tests/run-shim.test.ts
 *
 * The shim's job is to pick a runtime that can actually strip types, and to degrade to a
 * silent no-op when none exists. Both halves are pinned here, with fake runtimes on PATH so
 * the outcome does not depend on what this machine happens to have installed.
 *
 * The regression these exist for: the shim used to gate on Node's VERSION and then `exec`
 * it. A distro Node built without amaro clears any version check and dies with
 * ERR_NO_TYPESCRIPT — and because `exec` had already replaced the shell, bun and tsx were
 * unreachable. Every hook and checker was dead on such a machine, loudly.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const SHIM = path.join(
  path.dirname(import.meta.dirname),
  "plugins", "viby-toolkit", "hooks", "run.sh",
);

/**
 * A fake `node` standing in for a real distro build.
 *
 * It must be faithful to the bug, not merely to the fix: it reports a version well past
 * 22.6 (so any version-only gate lets it through), answers the capability probe honestly,
 * and — when it cannot strip types — dies the way the real binary does. A fake that simply
 * failed the version probe would let a version-gated shim look correct.
 */
function fakeNode(canStripTypes: boolean): string {
  return [
    "#!/bin/sh",
    'if [ "$1" = "-p" ]; then',
    '  case "$2" in',
    '    *"[0]"*) echo 22 ;;',
    '    *"[1]"*) echo 22 ;;',
    "    *) echo 22 ;;",
    "  esac",
    "  exit 0",
    "fi",
    'if [ "$1" = "-e" ]; then',
    `  exit ${canStripTypes ? 0 : 1}`,
    "fi",
    'if [ "$1" = "--experimental-strip-types" ]; then',
    canStripTypes
      ? "  echo NODE_RAN; exit 0"
      : '  echo "Error [ERR_NO_TYPESCRIPT]: Node.js is not compiled with TypeScript support" >&2; exit 1',
    "fi",
    "echo NODE_RAN",
    "",
  ].join("\n");
}

function bin(dir: string, name: string, body: string): void {
  const p = path.join(dir, name);
  fs.writeFileSync(p, body);
  fs.chmodSync(p, 0o755);
}

/** Runs the shim with PATH containing only `dir`, so nothing real leaks in. */
function runShim(dir: string): { status: number | null; out: string } {
  const script = path.join(dir, "sentinel.ts");
  fs.writeFileSync(script, "const ok: string = 'x';\nconsole.log(ok);\n");
  // Absolute interpreter on purpose: PATH is stripped to `dir` so no real runtime can leak
  // into the fake, and `sh` itself would no longer resolve through it.
  const p = spawnSync("/bin/sh", [SHIM, script], {
    encoding: "utf8",
    env: { PATH: dir },
  });
  return { status: p.status, out: ((p.stdout || "") + (p.stderr || "")).trim() };
}

function sandbox(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "shim-"));
}

test("a node that CAN strip types is used", () => {
  const dir = sandbox();
  try {
    bin(dir, "node", fakeNode(true));
    bin(dir, "bun", "#!/bin/sh\necho BUN_RAN\n");
    const r = runShim(dir);
    assert.equal(r.status, 0);
    assert.match(r.out, /NODE_RAN/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("a node that CANNOT strip types falls through to bun instead of dying", () => {
  const dir = sandbox();
  try {
    bin(dir, "node", fakeNode(false));
    bin(dir, "bun", "#!/bin/sh\necho BUN_RAN\n");
    const r = runShim(dir);
    assert.equal(r.status, 0, "shim must not propagate the unusable runtime's failure");
    assert.match(r.out, /BUN_RAN/);
    assert.doesNotMatch(r.out, /NODE_RAN/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("no usable runtime at all is a SILENT no-op, exit 0", () => {
  const dir = sandbox();
  try {
    const r = runShim(dir);
    assert.equal(r.status, 0, "a machine with no TS runtime must degrade to 'no hook', not a wedged session");
    assert.equal(r.out, "", "a no-op must not print anything a hook would treat as output");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("an unusable node with no fallback is still a silent no-op", () => {
  const dir = sandbox();
  try {
    bin(dir, "node", fakeNode(false));
    const r = runShim(dir);
    assert.equal(r.status, 0);
    assert.equal(r.out, "");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("no script argument is a no-op rather than an error", () => {
  const p = spawnSync("sh", [SHIM], { encoding: "utf8" });
  assert.equal(p.status, 0);
  assert.equal(((p.stdout || "") + (p.stderr || "")).trim(), "");
});

test("the real shim on this machine resolves a runtime that strips types", () => {
  const dir = sandbox();
  try {
    const script = path.join(dir, "typed.ts");
    fs.writeFileSync(script, "const n: number = 41;\nconsole.log(n + 1);\n");
    const p = spawnSync("sh", [SHIM, script], { encoding: "utf8" });
    assert.equal(p.status, 0, `shim failed: ${(p.stderr || "").slice(0, 400)}`);
    assert.match((p.stdout || "").trim(), /^42$/m);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
