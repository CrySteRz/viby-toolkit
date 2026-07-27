/**
 * Smoke tests for the viby-code statusline.
 *
 * Run: node --experimental-strip-types --test tests/statusline.test.ts
 *
 * Payload shapes follow the documented statusLine stdin contract, including the
 * documented null cases: `current_usage` is null before the first API call and again
 * after /compact, `used_percentage` may be null early, and `rate_limits` appears only
 * for Pro/Max subscribers after the first API response.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.dirname(__dirname);
const SCRIPT = path.join(ROOT, "plugins", "viby-code", "hooks", "statusline.ts");

type JsonRecord = Record<string, unknown>;

const FULL: JsonRecord = {
  model: { display_name: "Opus 5", id: "claude-opus-5" },
  cost: { total_cost_usd: 1.2345 },
  context_window: {
    context_window_size: 1000000,
    used_percentage: 43,
    current_usage: {
      input_tokens: 1200,
      cache_creation_input_tokens: 800,
      cache_read_input_tokens: 18000,
      output_tokens: 400,
    },
  },
  rate_limits: {
    five_hour: { used_percentage: 23.5 },
    seven_day: { used_percentage: 81.2 },
  },
  exceeds_200k_tokens: false,
};

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

/** Deep-clone `d` with the key at `path` removed. */
function drop(d: JsonRecord, ...pathKeys: string[]): JsonRecord {
  const out = deepClone(d);
  let node: JsonRecord = out;
  for (const k of pathKeys.slice(0, -1)) {
    node = node[k] as JsonRecord;
  }
  const lastKey = pathKeys[pathKeys.length - 1];
  if (lastKey !== undefined) delete node[lastKey];
  return out;
}

function nullify(d: JsonRecord, ...pathKeys: string[]): JsonRecord {
  const out = deepClone(d);
  let node: JsonRecord = out;
  for (const k of pathKeys.slice(0, -1)) {
    node = node[k] as JsonRecord;
  }
  const lastKey = pathKeys[pathKeys.length - 1];
  if (lastKey !== undefined) node[lastKey] = null;
  return out;
}

type Case = [string, JsonRecord, string[], string[]];

const CASES: Case[] = [
  // cache share = cache_read / (input + cache_creation + cache_read) = 18000/20000 = 90%
  ["full payload", FULL, ["Opus 5", "ctx 43%", "cache 90%", "5h 24%", "7d 81%", "$1.23"], []],
  ["no rate_limits (non-subscriber)", drop(FULL, "rate_limits"), ["ctx 43%"], ["5h ", "7d "]],
  ["five_hour only", drop(FULL, "rate_limits", "seven_day"), ["5h 24%"], ["7d "]],
  [
    "current_usage null (pre-first-call)",
    nullify(FULL, "context_window", "current_usage"),
    ["ctx 43%"],
    ["cache "],
  ],
  [
    "used_percentage null, usage present",
    nullify(FULL, "context_window", "used_percentage"),
    ["ctx 2%"],
    [],
  ],
  [
    "used_percentage null + no size",
    drop(nullify(FULL, "context_window", "used_percentage"), "context_window", "context_window_size"),
    [],
    ["ctx "],
  ],
  ["no context_window at all", drop(FULL, "context_window"), ["Opus 5"], ["ctx ", "cache "]],
  ["zero cost omitted", { ...FULL, cost: { total_cost_usd: 0 } }, ["ctx 43%"], ["$"]],
  ["no cost key", drop(FULL, "cost"), ["ctx 43%"], ["$"]],
  ["model id fallback", { model: { id: "claude-sonnet-5" } }, ["claude-sonnet-5"], ["ctx "]],
  ["empty object", {}, ["claude"], ["ctx ", "$"]],
  [
    "red band at high ctx",
    {
      ...FULL,
      context_window: {
        ...(FULL.context_window as JsonRecord),
        used_percentage: 91,
      },
    },
    ["\x1b[31mctx 91%"],
    [],
  ],
  [
    "green band at low ctx",
    {
      ...FULL,
      context_window: {
        ...(FULL.context_window as JsonRecord),
        used_percentage: 12,
      },
    },
    ["\x1b[32mctx 12%"],
    [],
  ],
];

function run(payload: JsonRecord): { status: number | null; stdout: string; stderr: string } {
  const result = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", SCRIPT],
    { input: JSON.stringify(payload), encoding: "utf8" },
  );
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

for (const [name, payload, must, mustNot] of CASES) {
  test(name, () => {
    const { status, stdout, stderr } = run(payload);
    assert.equal(status, 0, `expected exit 0, got ${status}: ${stderr.slice(0, 200)}`);
    const lineCount = (stdout.match(/\n/g) || []).length;
    assert.equal(lineCount, 1, `expected exactly one line, got ${lineCount}`);
    assert.notEqual(stdout.trim(), "", "empty output");
    for (const m of must) {
      assert.ok(stdout.includes(m), `missing ${JSON.stringify(m)} in ${JSON.stringify(stdout)}`);
    }
    for (const m of mustNot) {
      assert.ok(!stdout.includes(m), `should not contain ${JSON.stringify(m)} in ${JSON.stringify(stdout)}`);
    }
  });
}
