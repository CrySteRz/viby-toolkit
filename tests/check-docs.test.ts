/**
 * Contract tests for the docs auditor.
 *
 * Run: node --experimental-strip-types --test tests/check-docs.test.ts
 *
 * Both halves. Three must-NOT cases are real false positives measured on this repo's own docs, where
 * the first version produced 76 findings and every one was wrong.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { auditDoc } from "../plugins/viby-toolkit/skills/docs/scripts/check-docs.ts";

function repo(files: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "docs-"));
  for (const [rel, body] of Object.entries(files)) {
    const full = path.join(dir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
  }
  return dir;
}
function index(root: string): Set<string> {
  const out = new Set<string>();
  const walk = (d: string): void => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const f = path.join(d, e.name);
      if (e.isDirectory()) walk(f);
      else out.add(f);
    }
  };
  walk(root);
  return out;
}
function rules(root: string, doc: string, scripts: string[] = []): string[] {
  return auditDoc(doc, fs.readFileSync(path.join(root, doc), "utf8"), root, new Set(scripts), index(root)).map((f) => f.rule);
}

test("a documented path that does not exist is P1", () => {
  // Needs one resolving reference so the root is trusted — otherwise the self-calibration correctly
  // reports root-unknown instead, which is the behaviour pinned in its own test below.
  const root = repo({ "README.md": "See `src/real.ts` and `src/gone.ts` for details.\n", "src/real.ts": "x" });
  try {
    assert.ok(rules(root, "README.md").includes("stale-path"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a path that exists one directory deeper still resolves", () => {
  // 28 false findings on this repo came from guessing the base directory: a doc legitimately cites a
  // path relative to a package subdirectory. Resolution is by suffix against the real file index.
  const root = repo({ "README.md": "See `skills/brain/methods.md`.\n", "plugins/tk/skills/brain/methods.md": "x" });
  try {
    assert.deepEqual(rules(root, "README.md"), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("English prose containing 'make' is not a script invocation", () => {
  // The single worst false positive: `make` matched "make it readable", "make a decision" — 48 times.
  const root = repo({ "README.md": "We make it readable, then make a decision about the layout.\n" });
  try {
    assert.deepEqual(rules(root, "README.md", ["check"]), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a regex flag in backticks is not a path", () => {
  const root = repo({ "README.md": "The pattern uses `/g` and `/i` flags.\n" });
  try {
    assert.deepEqual(rules(root, "README.md"), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a documented npm script that does not exist is P1", () => {
  const root = repo({ "README.md": "Run `npm run buidl` to build.\n" });
  try {
    assert.ok(rules(root, "README.md", ["build", "check"]).includes("unknown-script"));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a documented script that DOES exist is silent", () => {
  const root = repo({ "README.md": "Run `npm run check` before pushing.\n" });
  try {
    assert.deepEqual(rules(root, "README.md", ["check"]), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("npm install / npm test are not project scripts", () => {
  const root = repo({ "README.md": "Run `npm ci` then `npm test`.\n" });
  try {
    assert.deepEqual(rules(root, "README.md", ["check"]), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a dead relative link is P1; a live one is silent", () => {
  const root = repo({ "README.md": "See [guide](docs/guide.md) and [gone](docs/gone.md).\n", "docs/guide.md": "# Guide\n" });
  try {
    const f = rules(root, "README.md");
    assert.equal(f.filter((r) => r === "dead-link").length, 1, f.join());
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("a dead anchor is P2, and a live anchor is silent", () => {
  const root = repo({
    "README.md": "See [x](docs/g.md#the-real-heading) and [y](docs/g.md#renamed-away).\n",
    "docs/g.md": "# Doc\n\n## The Real Heading\n\ntext\n",
  });
  try {
    const f = rules(root, "README.md");
    assert.equal(f.filter((r) => r === "dead-anchor").length, 1, f.join());
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("placeholders in templates are not references", () => {
  const root = repo({ "README.md": "Add `- [Title](file.md) — hook` to the index, in `path/to/your-file.ts`.\n" });
  try {
    assert.deepEqual(rules(root, "README.md"), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("when NOTHING resolves, the root is reported as wrong rather than the doc as stale", () => {
  const root = repo({ "README.md": "See `src/a.ts`, `src/b.ts` and `lib/c.ts`.\n" });
  try {
    const f = rules(root, "README.md");
    assert.ok(f.includes("root-unknown"), f.join());
    assert.ok(!f.includes("stale-path"), "must not claim staleness it could not check");
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test("an external URL is never treated as a path or a link to resolve", () => {
  const root = repo({ "README.md": "See [the paper](https://arxiv.org/abs/1234.5678) and `https://x.dev/a.ts`.\n", "a.ts": "x" });
  try {
    assert.deepEqual(rules(root, "README.md"), []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
