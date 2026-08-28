/**
 * Contract tests for the hook-wiring auditor.
 *
 * Run: node --experimental-strip-types --test tests/check-hooks.test.ts
 *
 * Both halves, per house style: what must be flagged, and — just as load-bearing — what must
 * NOT be. The two real opt-in scripts (post-tool-use-format.ts, statusline.ts) are the
 * canonical must-NOT case: an earlier analysis wrongly called them a defect, and this file
 * pins that they are clean by design.
 */
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  parseHooksJson,
  checkShape,
  extractCommands,
  pathTokens,
  checkPathUsage,
  dryRunHooks,
  documentedAsOptIn,
  checkOrphanScripts,
  checkHooks,
} from "../plugins/viby-toolkit/skills/extend/scripts/check-hooks.ts";

const REPO_ROOT = path.join(import.meta.dirname, "..");
const REAL_PLUGIN_ROOT = path.join(REPO_ROOT, "plugins", "viby-toolkit");
const REAL_README = path.join(REPO_ROOT, "README.md");

function checks(findings: { check: string }[]): string[] {
  return findings.map((f) => f.check);
}

/** A minimal plugin skeleton: hooks/ dir with the given hooks.json + extra files. */
function fixture(hooksJson: unknown, extraFiles: Record<string, string> = {}): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "check-hooks-"));
  const hooksDir = path.join(root, "hooks");
  fs.mkdirSync(hooksDir, { recursive: true });
  fs.writeFileSync(path.join(hooksDir, "hooks.json"), typeof hooksJson === "string" ? hooksJson : JSON.stringify(hooksJson, null, 2));
  for (const [rel, body] of Object.entries(extraFiles)) {
    const full = path.join(hooksDir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, body);
    if (rel.endsWith(".sh")) fs.chmodSync(full, 0o755);
  }
  return root;
}

function cleanup(root: string): void {
  fs.rmSync(root, { recursive: true, force: true });
}

// ---------------------------------------------------------------------------------
// The real shipped artifact must pass — dogfooding.
// ---------------------------------------------------------------------------------

test("the real shipped hooks.json has zero findings", () => {
  const findings = checkHooks(REAL_PLUGIN_ROOT, REAL_README);
  assert.deepEqual(findings, [], JSON.stringify(findings, null, 2));
});

test("the real opt-in scripts (post-tool-use-format.ts, statusline.ts) do not trip the orphan check", () => {
  const hooksJson = JSON.parse(fs.readFileSync(path.join(REAL_PLUGIN_ROOT, "hooks", "hooks.json"), "utf8"));
  const readme = fs.readFileSync(REAL_README, "utf8");
  const findings = checkOrphanScripts(path.join(REAL_PLUGIN_ROOT, "hooks"), hooksJson, readme);
  assert.deepEqual(
    findings.filter((f) => f.location === "post-tool-use-format.ts" || f.location === "statusline.ts"),
    [],
  );
});

// ---------------------------------------------------------------------------------
// hooks.json parsing / shape
// ---------------------------------------------------------------------------------

test("invalid JSON is a P1", () => {
  const { config, findings } = parseHooksJson("{ not json");
  assert.equal(config, null);
  assert.equal(findings[0]?.severity, "P1");
  assert.equal(findings[0]?.check, "invalid-json");
});

test("valid JSON that is an array (not an object) is a P1", () => {
  const { config, findings } = parseHooksJson("[]");
  assert.equal(config, null);
  assert.equal(findings[0]?.check, "invalid-shape");
});

test("missing top-level hooks key is a P1", () => {
  const findings = checkShape({});
  assert.ok(findings.some((f) => f.check === "missing-hooks-key" && f.severity === "P1"));
});

test("an unrecognised event name is a P1 (the typo case)", () => {
  const findings = checkShape({ hooks: { SessionStrat: [] } });
  assert.ok(findings.some((f) => f.check === "unknown-event" && f.severity === "P1"));
});

test("a real event name is not flagged as unknown", () => {
  const findings = checkShape({ hooks: { PreToolUse: [], SessionEnd: [], SubagentStop: [] } });
  assert.equal(findings.filter((f) => f.check === "unknown-event").length, 0);
});

test("a matcher block missing the hooks array is a P1", () => {
  const findings = checkShape({ hooks: { Stop: [{ matcher: "" }] } });
  assert.ok(findings.some((f) => f.check === "missing-hooks-array"));
});

test("a hook entry with the wrong type is a P1", () => {
  const findings = checkShape({ hooks: { Stop: [{ hooks: [{ type: "prompt", command: "echo hi" }] }] } });
  assert.ok(findings.some((f) => f.check === "bad-hook-type"));
});

test("a hook entry with no command string is a P1", () => {
  const findings = checkShape({ hooks: { Stop: [{ hooks: [{ type: "command" }] }] } });
  assert.ok(findings.some((f) => f.check === "missing-command"));
});

test("a well-formed config produces no shape findings", () => {
  const findings = checkShape({
    hooks: { SessionStart: [{ matcher: "startup", hooks: [{ type: "command", command: "sh \"${CLAUDE_PLUGIN_ROOT}/hooks/x.sh\"" }] }] },
  });
  assert.deepEqual(findings, []);
});

// ---------------------------------------------------------------------------------
// path extraction / plugin-root usage
// ---------------------------------------------------------------------------------

test("pathTokens finds a CLAUDE_PLUGIN_ROOT-relative path inside quotes", () => {
  const tokens = pathTokens('sh "${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh"');
  assert.deepEqual(tokens, ["${CLAUDE_PLUGIN_ROOT}/hooks/session-start.sh"]);
});

test("pathTokens does not treat the bare command name as a path", () => {
  const tokens = pathTokens('sh "${CLAUDE_PLUGIN_ROOT}/hooks/x.sh"');
  assert.ok(!tokens.includes("sh"));
});

test("a relative path is a P1, not a silent pass", () => {
  const root = fixture({});
  try {
    fs.writeFileSync(path.join(root, "hooks", "x.sh"), "#!/bin/sh\nexit 0\n");
    const findings = checkPathUsage([{ event: "Stop", command: 'sh "hooks/x.sh"' }], root);
    assert.ok(findings.some((f) => f.check === "not-plugin-root-relative" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("an absolute path is a P1", () => {
  const findings = checkPathUsage([{ event: "Stop", command: 'sh "/usr/local/bin/x.sh"' }], "/nonexistent");
  assert.ok(findings.some((f) => f.check === "not-plugin-root-relative"));
});

test("a ${CLAUDE_PLUGIN_ROOT} path that does not exist on disk is a P1", () => {
  const root = fixture({});
  try {
    const findings = checkPathUsage([{ event: "Stop", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/missing.sh"' }], root);
    assert.ok(findings.some((f) => f.check === "missing-script" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("a ${CLAUDE_PLUGIN_ROOT} path that exists is not flagged", () => {
  const root = fixture({}, { "x.sh": "#!/bin/sh\nexit 0\n" });
  try {
    const findings = checkPathUsage([{ event: "Stop", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/x.sh"' }], root);
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

test("a .ts hook invoked with bare node (bypassing run.sh) is a P1", () => {
  const root = fixture({}, { "foo.ts": "console.log('x');\n" });
  try {
    const findings = checkPathUsage([{ event: "Stop", command: 'node "${CLAUDE_PLUGIN_ROOT}/hooks/foo.ts"' }], root);
    assert.ok(findings.some((f) => f.check === "ts-bypasses-run-sh" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("a .ts hook invoked through run.sh is not flagged for bypassing it", () => {
  const root = fixture({}, { "foo.ts": "console.log('x');\n", "run.sh": "#!/bin/sh\nexec node \"$1\"\n" });
  try {
    const findings = checkPathUsage(
      [{ event: "Stop", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/run.sh" "${CLAUDE_PLUGIN_ROOT}/hooks/foo.ts"' }],
      root,
    );
    assert.equal(findings.filter((f) => f.check === "ts-bypasses-run-sh").length, 0);
  } finally {
    cleanup(root);
  }
});

// ---------------------------------------------------------------------------------
// dry-run execution
// ---------------------------------------------------------------------------------

test("a hook that exits non-zero on its representative event is a P1", () => {
  const root = fixture({}, { "bad.sh": "#!/bin/sh\nexit 3\n" });
  try {
    const findings = dryRunHooks([{ event: "Stop", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/bad.sh"' }], root);
    assert.ok(findings.some((f) => f.check === "dry-run-nonzero-exit" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("a hook that exits 0 is not flagged by the dry run", () => {
  const root = fixture({}, { "ok.sh": "#!/bin/sh\ncat >/dev/null\nexit 0\n" });
  try {
    const findings = dryRunHooks([{ event: "Stop", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/ok.sh"' }], root);
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

test("a SessionStart hook that emits malformed JSON is a P1 (it wedges the session)", () => {
  const root = fixture({}, { "bad-start.sh": "#!/bin/sh\ncat >/dev/null\necho 'not json'\nexit 0\n" });
  try {
    const findings = dryRunHooks([{ event: "SessionStart", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/bad-start.sh"' }], root);
    assert.ok(findings.some((f) => f.check === "sessionstart-malformed-json" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("a SessionStart hook that emits valid JSON is not flagged", () => {
  const root = fixture({}, { "good-start.sh": '#!/bin/sh\ncat >/dev/null\necho \'{"hookSpecificOutput":{"hookEventName":"SessionStart"}}\'\n' });
  try {
    const findings = dryRunHooks([{ event: "SessionStart", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/good-start.sh"' }], root);
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

test("a SessionStart hook that emits no stdout is a P1", () => {
  const root = fixture({}, { "silent-start.sh": "#!/bin/sh\ncat >/dev/null\nexit 0\n" });
  try {
    const findings = dryRunHooks([{ event: "SessionStart", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/silent-start.sh"' }], root);
    assert.ok(findings.some((f) => f.check === "sessionstart-empty-stdout"));
  } finally {
    cleanup(root);
  }
});

// ---------------------------------------------------------------------------------
// orphan / opt-in scripts
// ---------------------------------------------------------------------------------

test("documentedAsOptIn requires opt-in and the basename on the same line", () => {
  assert.ok(documentedAsOptIn("- **Opt-in** (shipped, not enabled): `hooks/post-tool-use-format.ts` does X.", "post-tool-use-format.ts"));
  assert.ok(!documentedAsOptIn("Some unrelated line about opt-in features.\nhooks/post-tool-use-format.ts is nice.", "post-tool-use-format.ts"));
});

test("a hook script registered nowhere and undocumented is a P1 orphan", () => {
  const root = fixture(
    { hooks: { Stop: [{ hooks: [{ type: "command", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/wired.sh"' }] }] } },
    { "wired.sh": "#!/bin/sh\nexit 0\n", "stray.ts": "console.log('x');\n" },
  );
  try {
    const config = JSON.parse(fs.readFileSync(path.join(root, "hooks", "hooks.json"), "utf8"));
    const findings = checkOrphanScripts(path.join(root, "hooks"), config, "no mention of it here\n");
    assert.ok(findings.some((f) => f.check === "orphaned-hook-script" && f.location === "stray.ts" && f.severity === "P1"));
    assert.ok(!findings.some((f) => f.location === "wired.sh"));
  } finally {
    cleanup(root);
  }
});

test("a script registered in hooks.json is not an orphan", () => {
  const root = fixture(
    { hooks: { Stop: [{ hooks: [{ type: "command", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/wired.sh"' }] }] } },
    { "wired.sh": "#!/bin/sh\nexit 0\n" },
  );
  try {
    const config = JSON.parse(fs.readFileSync(path.join(root, "hooks", "hooks.json"), "utf8"));
    const findings = checkOrphanScripts(path.join(root, "hooks"), config, "");
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

test("a script documented as opt-in but unregistered is not an orphan", () => {
  const root = fixture({ hooks: {} }, { "optional.ts": "console.log('x');\n" });
  try {
    const config = JSON.parse(fs.readFileSync(path.join(root, "hooks", "hooks.json"), "utf8"));
    const readme = "- **Opt-in** (shipped, not enabled): `hooks/optional.ts` does a thing.\n";
    const findings = checkOrphanScripts(path.join(root, "hooks"), config, readme);
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

test("run.sh itself is never flagged as an orphan", () => {
  const root = fixture({ hooks: {} }, { "run.sh": "#!/bin/sh\nexit 0\n" });
  try {
    const config = JSON.parse(fs.readFileSync(path.join(root, "hooks", "hooks.json"), "utf8"));
    const findings = checkOrphanScripts(path.join(root, "hooks"), config, "");
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});

// ---------------------------------------------------------------------------------
// extractCommands
// ---------------------------------------------------------------------------------

test("extractCommands pulls every command across every event and matcher block", () => {
  const refs = extractCommands({
    hooks: {
      SessionStart: [{ matcher: "startup", hooks: [{ type: "command", command: "a" }] }],
      Stop: [{ hooks: [{ type: "command", command: "b" }, { type: "command", command: "c" }] }],
    },
  });
  assert.deepEqual(
    refs.map((r) => `${r.event}:${r.command}`),
    ["SessionStart:a", "Stop:b", "Stop:c"],
  );
});

// ---------------------------------------------------------------------------------
// end-to-end checkHooks
// ---------------------------------------------------------------------------------

test("a missing hooks.json is a P1, not a silent pass", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "check-hooks-empty-"));
  try {
    const findings = checkHooks(root, path.join(root, "README.md"));
    assert.ok(findings.some((f) => f.check === "missing-hooks-json" && f.severity === "P1"));
  } finally {
    cleanup(root);
  }
});

test("a fully wired, existing, well-behaved hook produces zero findings end to end", () => {
  const root = fixture(
    { hooks: { Stop: [{ hooks: [{ type: "command", command: 'sh "${CLAUDE_PLUGIN_ROOT}/hooks/ok.sh"' }] }] } },
    { "ok.sh": "#!/bin/sh\ncat >/dev/null\nexit 0\n" },
  );
  try {
    const findings = checkHooks(root, path.join(root, "README.md"));
    assert.deepEqual(findings, []);
  } finally {
    cleanup(root);
  }
});
