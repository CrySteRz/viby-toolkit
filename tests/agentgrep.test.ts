/**
 * Contract tests for the agentgrep MCP server: how calls become command lines, what the
 * session remembers from results, and the context it hands agentgrep. A fake binary stands in
 * for agentgrep (CI runners have no Rust); the real one was run live, see the 2.27.0 changelog.
 */
import test, { after } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const DIR = fs.mkdtempSync(path.join(os.tmpdir(), "viby-agentgrep-test-"));
const REPO = path.join(DIR, "repo");
fs.mkdirSync(REPO);
fs.writeFileSync(path.join(REPO, "actions.ts"), "export function bridgeRequest() {}\n");
fs.writeFileSync(path.join(REPO, "server.ts"), "bridgeRequest();\n");
const FAKE = path.join(DIR, "agentgrep");
fs.writeFileSync(
  FAKE,
  `#!/bin/sh
printf '%s\\n' "$@" > "${DIR}/args"
prev=""
for a in "$@"; do
  if [ "$prev" = "--context-json" ]; then cp "$a" "${DIR}/context"; fi
  prev="$a"
done
cat "${DIR}/out"
cat "${DIR}/err" >&2 2>/dev/null
exit "$(cat "${DIR}/code" 2>/dev/null || echo 0)"
`,
  { mode: 0o755 },
);
process.env.VIBY_AGENTGREP_BIN = FAKE;

const { INSTALL_GUIDE, callTool, cliArgs, findBinary, harnessContext, remember, seen, shownIn } = await import("../plugins/viby-toolkit/agentgrep/server.ts");

after(() => {
  fs.rmSync(DIR, { recursive: true, force: true });
});

/** agentgrep 0.1.7 trace output, trimmed from a real run (`trace subject:bridgeRequest relation:called_from`). */
const TRACE = `query parameters:
  subject: bridgeRequest
  relation: called_from

top results: 2 files, 2 regions
best answer likely in actions.ts

1. actions.ts
   role: generic
   why:
     - exact subject match or symbol hit
   structure:
     - function bridgeRequest @ 169-320 (152 lines)
     - function stepCommands @ 142-155 (14 lines)
   regions:
     - bridgeRequest @ 169-320 (152 lines)
       kind: callsite
       snippet:
         export function bridgeRequest(action: Action, input: Json, tabId: number | undefined): [string, Json] {
       why:
         - exact subject match
     - stepCommands @ 142-155 (14 lines)
       kind: callsite
       full region:
         function stepCommands(steps: unknown) {
         2. not a header: indented body text
         }
       why:
         - exact subject match

2. server.ts
   role: generic
   structure:
     - function callTool @ 348-392 (45 lines)
   regions:
     - callTool @ 360-361 (2 lines)
       kind: callsite
       full region:
         const [bridgeAction, params] = bridgeRequest(action, input, tabId);
`;

function fake(out: string, code = 0, err = ""): void {
  fs.writeFileSync(path.join(DIR, "out"), out);
  fs.writeFileSync(path.join(DIR, "code"), String(code));
  fs.writeFileSync(path.join(DIR, "err"), err);
  fs.rmSync(path.join(DIR, "context"), { force: true });
}
const args = () => fs.readFileSync(path.join(DIR, "args"), "utf8").trimEnd().split("\n");
const sentContext = () => (fs.existsSync(path.join(DIR, "context")) ? JSON.parse(fs.readFileSync(path.join(DIR, "context"), "utf8")) : null);

test("cli: options first, then --, so a query that starts with - stays a query", () => {
  assert.deepEqual(cliArgs({ mode: "grep", query: "--force", regex: true, glob: "*.ts" }, "/r"), ["grep", "--path", "/r", "--glob", "*.ts", "--regex", "--", "--force"]);
  assert.deepEqual(cliArgs({ mode: "find", query: "native  host", max_files: 3 }, "/r"), ["find", "--path", "/r", "--max-files", "3", "--", "native", "host"]);
  assert.deepEqual(cliArgs({ mode: "outline", file: "a.ts", glob: "*.ts", paths_only: true }, "/r"), ["outline", "--path", "/r", "--", "a.ts"], "outline takes none of the search filters");
  assert.deepEqual(cliArgs({ mode: "trace", terms: ["subject:x", "relation:defined"], max_regions: 2 }, "/r", "/c.json"), [
    "trace", "--path", "/r", "--max-regions", "2", "--context-json", "/c.json", "--", "subject:x", "relation:defined",
  ]);
  assert.deepEqual(cliArgs({ mode: "grep", query: "x" }, "/r", "/c.json").includes("--context-json"), false, "only trace takes a context");
  assert.throws(() => cliArgs({ mode: "grep" }, "/r"), /query is required/);
  assert.throws(() => cliArgs({ mode: "outline" }, "/r"), /file is required/);
  assert.throws(() => cliArgs({ mode: "trace", terms: [] }, "/r"), /terms are required/);
});

test("memory: reads which structures and full regions a trace showed, not snippets", () => {
  assert.deepEqual(shownIn("trace", TRACE), [
    { file: "actions.ts", structure: true, regions: [[142, 155]] },
    { file: "server.ts", structure: true, regions: [[360, 361]] },
  ]);
  assert.deepEqual(shownIn("outline", "file: src/a.ts\nlanguage: typescript\n"), [{ file: "src/a.ts", structure: true, regions: [] }]);
  assert.deepEqual(shownIn("grep", TRACE), [], "grep results are not remembered");
  assert.deepEqual(shownIn("trace", "something else entirely"), [], "unknown output yields nothing, never a guess");
});

test("memory: the context covers what was shown under this root, and forgets a file once it changes", () => {
  seen.clear();
  remember(REPO, shownIn("trace", TRACE));
  remember(REPO, [{ file: "gone.ts", structure: true, regions: [] }]);
  const context = harnessContext(REPO)!;
  assert.deepEqual(context.known_files.map((f: any) => f.path), ["actions.ts", "server.ts"], "a file that does not exist is not remembered");
  assert.deepEqual(context.known_regions.map((r: any) => [r.path, r.start_line, r.end_line]), [["actions.ts", 142, 155], ["server.ts", 360, 361]]);
  const region = context.known_regions[0];
  for (const key of ["body_confidence", "current_version_confidence", "prune_confidence"]) assert.ok(region[key] >= 0.7, `${key} clears agentgrep's prune threshold`);
  assert.equal(harnessContext(path.join(DIR, "elsewhere")), null, "nothing seen under another root");

  const later = new Date(Date.now() + 10_000);
  fs.utimesSync(path.join(REPO, "server.ts"), later, later);
  assert.deepEqual(harnessContext(REPO)!.known_files.map((f: any) => f.path), ["actions.ts"], "an edited file comes back in full");
  assert.equal(seen.has(path.join(REPO, "server.ts")), false);
  seen.clear();
});

test("server: a repeated trace sends agentgrep what this session already saw; force and outline never do", async () => {
  seen.clear();
  fake(TRACE);
  const first = await callTool({ mode: "trace", terms: ["subject:bridgeRequest", "relation:called_from"], path: REPO });
  assert.equal(first.isError, undefined);
  assert.equal(sentContext(), null, "nothing seen yet");
  assert.ok(args().includes("--") && args().at(-1) === "relation:called_from");

  await callTool({ mode: "trace", terms: ["subject:bridgeRequest", "relation:called_from"], path: REPO });
  assert.deepEqual(sentContext().known_regions.map((r: any) => r.start_line), [142, 360]);

  fake(TRACE);
  await callTool({ mode: "trace", terms: ["subject:bridgeRequest", "relation:called_from"], path: REPO, force: true });
  assert.equal(sentContext(), null, "force asks for everything in full");
  fake("file: actions.ts\nstructure:\n");
  await callTool({ mode: "outline", file: "actions.ts", path: REPO });
  assert.equal(args().includes("--context-json"), false, "an outline is an explicit ask");
  seen.clear();
});

test("server: agentgrep's own errors come back as tool errors; a missing folder never runs it", async () => {
  fake("", 2, "error: invalid smart DSL term: x (expected key:value or key=value)\n");
  const bad = await callTool({ mode: "trace", terms: ["x"], path: REPO });
  assert.equal(bad.isError, true);
  assert.match(bad.content[0]!.text, /expected key:value/);
  fs.rmSync(path.join(DIR, "args"));
  const missing = await callTool({ mode: "grep", query: "x", path: path.join(REPO, "nope") });
  assert.match(missing.content[0]!.text, /No such folder/);
  assert.equal(fs.existsSync(path.join(DIR, "args")), false);
  assert.match((await callTool({ mode: "read" })).content[0]!.text, /Unknown mode/);
});

test("install: found on PATH or in ~/.cargo/bin; missing, the guide pins the tested tag", async () => {
  const home = path.join(DIR, "home");
  const cargoBin = path.join(home, ".cargo", "bin");
  fs.mkdirSync(cargoBin, { recursive: true });
  assert.equal(findBinary({ PATH: "" }, home, "darwin"), null);
  fs.writeFileSync(path.join(cargoBin, "agentgrep"), "");
  assert.equal(findBinary({ PATH: "" }, home, "darwin"), path.join(cargoBin, "agentgrep"));
  assert.equal(findBinary({ PATH: "" }, home, "win32"), null, "Windows looks for agentgrep.exe");
  assert.match(INSTALL_GUIDE, /cargo install --git https:\/\/github\.com\/1jehuang\/agentgrep --tag v0\.1\.7 --locked/);
  process.env.VIBY_AGENTGREP_BIN = path.join(DIR, "missing");
  const result = await callTool({ mode: "grep", query: "x", path: REPO });
  process.env.VIBY_AGENTGREP_BIN = FAKE;
  assert.equal(result.isError, true);
  assert.match(result.content[0]!.text, /not installed[\s\S]*Ask the person before installing/);
});
