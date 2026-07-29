/**
 * Contract tests for the diff-hygiene checker.
 *
 * Run: node --experimental-strip-types --test tests/check-diff-hygiene.test.ts
 *
 * Both halves. The must-NOT half is the whole ballgame here: this runs on every change, so a rule
 * that fires on an ordinary small commit would be switched off within a day.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkDiffHygiene, parseDiff } from "../plugins/viby-toolkit/skills/orchestrate/scripts/check-diff-hygiene.ts";

/** Build a unified diff the way `git diff --unified=0` emits it. */
function diffOf(file: string, added: string[], removed: string[] = []): string {
  return [
    `diff --git a/${file} b/${file}`,
    `--- a/${file}`,
    `+++ b/${file}`,
    `@@ -1,${removed.length} +1,${added.length} @@`,
    ...removed.map((l) => `-${l}`),
    ...added.map((l) => `+${l}`),
    "",
  ].join("\n");
}

function checks(diff: string): string[] {
  return checkDiffHygiene(diff).findings.map((f) => f.check);
}

test("the parser reports the added lines with their new line numbers", () => {
  const p = parseDiff(diffOf("src/a.ts", ["const a = 1;", "const b = 2;"]));
  assert.equal(p.length, 1);
  assert.equal(p[0]?.file, "src/a.ts");
  assert.deepEqual(p[0]?.added.map((a) => a.line), [1, 2]);
});

test("a merge conflict marker is P1", () => {
  const f = checkDiffHygiene(diffOf("src/a.ts", ["<<<<<<< HEAD", "const a = 1;", ">>>>>>> other"])).findings;
  const hit = f.find((x) => x.check === "conflict-marker");
  assert.ok(hit);
  assert.equal(hit.severity, "P1");
});

test("a credential-shaped line is P1", () => {
  for (const line of [
    "const key = 'AKIAIOSFODNN7EXAMPLE';",
    "token = 'ghp_abcdefghijklmnopqrstuvwxyz0123456789';",
    "-----BEGIN RSA PRIVATE KEY-----",
  ]) {
    assert.ok(checks(diffOf("src/a.ts", [line])).includes("secret-shaped"), `should flag: ${line}`);
  }
});

test("an ordinary variable named key or token is NOT flagged as a secret", () => {
  const f = checks(diffOf("src/a.ts", ["const apiKey = process.env.API_KEY;", "const token = await getToken();"]));
  assert.ok(!f.includes("secret-shaped"), `must not fire on env reads: ${f.join()}`);
});

test("added debug output is flagged, per language", () => {
  assert.ok(checks(diffOf("src/a.ts", ["  console.log('here', x);"])).includes("debug-added"));
  assert.ok(checks(diffOf("src/a.ts", ["  debugger;"])).includes("debug-added"));
  assert.ok(checks(diffOf("app/m.py", ["    print(x)"])).includes("debug-added"));
  assert.ok(checks(diffOf("m.go", ["\tfmt.Println(x)"])).includes("debug-added"));
  assert.ok(checks(diffOf("m.rs", ["    dbg!(x);"])).includes("debug-added"));
});

test("real logging and a commented-out print are NOT 'debug-added'", () => {
  const f = checks(diffOf("src/a.ts", ["  logger.info({ orderId }, 'order settled');", "  // console.log('old')"]));
  assert.ok(!f.includes("debug-added"), `structured logging is not debug output: ${f.join()}`);
});

test("a new TODO is P3, not a blocker", () => {
  const hit = checkDiffHygiene(diffOf("src/a.ts", ["// TODO: handle the retry case"])).findings.find((x) => x.check === "todo-added");
  assert.ok(hit);
  assert.equal(hit.severity, "P3");
});

test("commented-out code is flagged; a prose comment is not", () => {
  assert.ok(checks(diffOf("src/a.ts", ["// doThing(arg);"])).includes("commented-out-code"));
  assert.ok(
    !checks(diffOf("src/a.ts", ["// this exists because the upstream API returns 200 on failure"])).includes("commented-out-code"),
    "explanatory comments are the good kind",
  );
});

test("a diff over 1,000 changed lines is P1 unreviewable", () => {
  const many = Array.from({ length: 1100 }, (_, i) => `const x${i} = ${i};`);
  const hit = checkDiffHygiene(diffOf("src/big.ts", many)).findings.find((x) => x.check === "unreviewable-size");
  assert.ok(hit, "expected unreviewable-size");
  assert.equal(hit.severity, "P1");
  assert.match(hit.problem, /28%/, "the finding should carry the measured detection rate");
});

test("a diff between 400 and 1,000 lines is P2 oversized, not P1", () => {
  const many = Array.from({ length: 500 }, (_, i) => `const x${i} = ${i};`);
  const f = checkDiffHygiene(diffOf("src/big.ts", many)).findings;
  assert.ok(f.some((x) => x.check === "oversized-diff"));
  assert.ok(!f.some((x) => x.check === "unreviewable-size"));
});

test("an ordinary small diff with a test is completely silent", () => {
  // The single most important test in this file: the normal case must produce nothing.
  const diff =
    diffOf("src/pricing.ts", ["export function total(items: Item[]) {", "  return items.reduce((s, i) => s + i.price, 0);", "}"]) +
    diffOf("src/pricing.test.ts", ['test("total sums prices", () => {', "  expect(total([{ price: 2 }])).toBe(2);", "});"]);
  assert.deepEqual(checks(diff), [], `a good small change must be silent, got ${checks(diff).join()}`);
});

test("source changed with no test touched is P2 — above the tiny-change threshold only", () => {
  const big = Array.from({ length: 30 }, (_, i) => `  const step${i} = ${i};`);
  assert.ok(checks(diffOf("src/a.ts", big)).includes("code-without-test"));
  // A three-line change does not need its own test to be defensible.
  assert.ok(!checks(diffOf("src/a.ts", ["const a = 1;"])).includes("code-without-test"));
});

test("a docs-only or config-only change does not demand a test", () => {
  const f = checks(diffOf("README.md", Array.from({ length: 40 }, (_, i) => `line ${i}`)));
  assert.ok(!f.includes("code-without-test"), `prose needs no unit test: ${f.join()}`);
});

test("formatting churn mixed with real changes is flagged", () => {
  // The churn file's added lines match its removed lines except for whitespace.
  const body = Array.from({ length: 10 }, (_, i) => `const v${i} = ${i};`);
  const churn = diffOf("src/formatted.ts", body.map((l) => `    ${l}`), body);
  const real = diffOf("src/logic.ts", ["export const rate = compute(base) * 1.2;"]);
  assert.ok(checks(churn + real).includes("mixed-concerns"), "reformatting hides the real change");
});

test("a formatting-ONLY diff is not 'mixed' — there is nothing it hides", () => {
  const body = Array.from({ length: 10 }, (_, i) => `const v${i} = ${i};`);
  const f = checks(diffOf("src/formatted.ts", body.map((l) => `    ${l}`), body));
  assert.ok(!f.includes("mixed-concerns"), `a pure reformat is a legitimate commit: ${f.join()}`);
});

test("a lockfile moving alone is flagged; with its manifest it is not", () => {
  assert.ok(checks(diffOf("package-lock.json", ['    "resolved": "https://registry.npmjs.org/x/-/x-1.2.4.tgz",'])).includes("lockfile-without-manifest"));
  const withManifest =
    diffOf("package-lock.json", ['    "version": "1.2.4",']) + diffOf("package.json", ['    "x": "^1.2.4",']);
  assert.ok(!checks(withManifest).includes("lockfile-without-manifest"));
});

test("CLI: an unresolvable base ref is exit 2, not a pass", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "orchestrate", "scripts", "check-diff-hygiene.ts",
  );
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "hyg-"));
  try {
    spawnSync("git", ["init", "-q"], { cwd: dir });
    spawnSync("git", ["config", "user.email", "t@e.com"], { cwd: dir });
    spawnSync("git", ["config", "user.name", "T"], { cwd: dir });
    fs.writeFileSync(path.join(dir, "a.ts"), "const a = 1;\n");
    spawnSync("git", ["add", "-A"], { cwd: dir });
    spawnSync("git", ["commit", "-qm", "init"], { cwd: dir });
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, "--base", "v99-nope"],
      { cwd: dir, encoding: "utf8" },
    );
    assert.equal(p.status, 2);
    assert.match(p.stdout ?? "", /NOTHING was compared/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("CLI: a real dirty working tree is audited, and the report says what it does not judge", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "orchestrate", "scripts", "check-diff-hygiene.ts",
  );
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "hyg-"));
  try {
    spawnSync("git", ["init", "-q"], { cwd: dir });
    spawnSync("git", ["config", "user.email", "t@e.com"], { cwd: dir });
    spawnSync("git", ["config", "user.name", "T"], { cwd: dir });
    fs.writeFileSync(path.join(dir, "a.ts"), "const a = 1;\n");
    spawnSync("git", ["add", "-A"], { cwd: dir });
    spawnSync("git", ["commit", "-qm", "init"], { cwd: dir });
    fs.writeFileSync(path.join(dir, "a.ts"), "const a = 1;\nconsole.log('debugging');\n");
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script],
      { cwd: dir, encoding: "utf8" },
    );
    assert.equal(p.status, 1, p.stdout);
    assert.match(p.stdout ?? "", /debug-added/);
    assert.match(p.stdout ?? "", /not whether it is correct/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("console.log in a script or seed file is OUTPUT, not debug", () => {
  // Measured on four real repositories: every one of the 20 `debug-added` findings was progress
  // output in a CLI, a migration script or a seed file. A script's stdout is its interface.
  for (const f of ["scripts/migrate-funds.mjs", "src/db/seed.ts", "bin/deploy.ts", "tools/codemod.js", "tasks/backfill.ts"]) {
    assert.ok(
      !checks(diffOf(f, ['  console.log("Renaming existing funds...");'])).includes("debug-added"),
      `${f} is a script — its output is not debug`,
    );
  }
});

test("console.log in application code is still debug", () => {
  assert.ok(
    checks(diffOf("src/app/checkout/page.tsx", ["  console.log('here', order);"])).includes("debug-added"),
    "the must-flag half of the same rule",
  );
});

test("comment-shaped rules do NOT run on prose files", () => {
  // Found dogfooding this on its own repo: `#` is a heading in Markdown, so `commented-out-code`
  // fired on six SKILL.md files at once. A TODO in a design doc is also normal writing.
  const md = diffOf("docs/design.md", ["## 3. Capture the behaviour", "# TODO: decide the grain", "// doThing(arg);"]);
  const f = checks(md);
  assert.ok(!f.includes("commented-out-code"), `markdown is prose: ${f.join()}`);
  assert.ok(!f.includes("todo-added"), `a TODO in a doc is normal: ${f.join()}`);
});

test("but a credential or a conflict marker matters in ANY file type", () => {
  assert.ok(checks(diffOf("README.md", ["export AWS_KEY=AKIAIOSFODNN7EXAMPLE"])).includes("secret-shaped"));
  assert.ok(checks(diffOf("config.yaml", ["<<<<<<< HEAD"])).includes("conflict-marker"));
});

test("P2 regression: one removed line cannot excuse two added copies of it", () => {
  // pendingRemovals was not consumed, so a single removal satisfied unlimited added lines and a file
  // that genuinely duplicated a line was classified as pure formatting churn.
  const p = parseDiff(diffOf("src/a.ts", ["const a = 1;", "const a = 1;"], ["const a  =  1;"]));
  assert.equal(p[0]?.whitespaceOnly, 1, "only the first added copy pairs with the one removal");
});
