#!/usr/bin/env -S node --experimental-strip-types
// viby-code statusline — makes the two resources that actually bind visible.
//
// Prints one line:
//   <model> · ctx NN% · cache NN% · 5h NN% · 7d NN% · $C.CC
//
//   ctx    percentage of the context window in use (context_window.used_percentage,
//          which is input-only: input + cache_creation + cache_read). Colour-banded
//          green <60 / yellow <80 / red, matching the 40-60% target the toolkit aims for.
//   cache  cache-read share of input tokens — confirms the prompt cache is being reused.
//          On a subscription, cache hits are the cheapest tokens you have.
//   5h/7d  rate-limit consumption (rate_limits.five_hour / .seven_day). On a Max plan the
//          scarce resources are context and rate limit, not dollars — so these matter more
//          than the cost figure. Only present for Pro/Max, after the first API response.
//   $      client-side session cost estimate; omitted when not reported.
//
// Fully generic; never blocks. On any error it prints a minimal line so the statusline
// never breaks. Fields that are absent or null (early in a session, or right after
// /compact) are simply skipped rather than shown as 0.
//
// Wire it up (statuslines live in settings.json, not in plugin hooks.json). This form
// survives plugin version bumps by globbing the cache directory:
//
//   "statusLine": {
//     "type": "command",
//     "command": "sh \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-code/*/hooks/run.sh | tail -1)\" \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-code/*/hooks/statusline.ts | tail -1)\""
//   }
//
// Or point it straight at your checkout: "sh $HOME/Projects/Personal/viby-toolkit/plugins/viby-code/hooks/run.sh $HOME/Projects/Personal/viby-toolkit/plugins/viby-code/hooks/statusline.ts"

const DIM = "\x1b[2m";
const RESET = "\x1b[0m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const RED = "\x1b[31m";

type UsageNode = {
  input_tokens?: unknown;
  cache_creation_input_tokens?: unknown;
  cache_read_input_tokens?: unknown;
  output_tokens?: unknown;
};

type ContextWindow = {
  used_percentage?: unknown;
  context_window_size?: unknown;
  current_usage?: unknown;
};

type RateLimits = {
  five_hour?: unknown;
  seven_day?: unknown;
};

type StatusPayload = {
  model?: unknown;
  context_window?: unknown;
  rate_limits?: unknown;
  cost?: unknown;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Colour a percentage: green below `low`, yellow below `high`, red beyond. */
function band(pct: number, low = 60, high = 80): [string, string] {
  const colour = pct < low ? GREEN : pct < high ? YELLOW : RED;
  return [colour, RESET];
}

/** Read a percentage field, returning null when absent or null. */
function pctOf(node: unknown, key = "used_percentage"): number | null {
  if (!isRecord(node)) return null;
  const v = node[key];
  if (v === null || v === undefined) return null;
  const n = typeof v === "number" ? v : typeof v === "string" ? Number(v) : NaN;
  if (Number.isNaN(n)) return null;
  return Math.round(n);
}

function numOr0(value: unknown): number {
  return typeof value === "number" ? value : 0;
}

function main(input: string): void {
  const parsed: unknown = JSON.parse(input);
  const d: StatusPayload = isRecord(parsed) ? (parsed as StatusPayload) : {};

  const modelNode = isRecord(d.model) ? d.model : {};
  const model =
    (typeof modelNode.display_name === "string" ? modelNode.display_name : null) ||
    (typeof modelNode.id === "string" ? modelNode.id : null) ||
    "claude";
  const parts: string[] = [`${DIM}${model}${RESET}`];

  const cw: ContextWindow = isRecord(d.context_window) ? d.context_window : {};

  // --- context window
  let pct = pctOf(cw);
  if (pct === null) {
    const usage: UsageNode = isRecord(cw.current_usage) ? cw.current_usage : {};
    const used =
      numOr0(usage.input_tokens) +
      numOr0(usage.cache_creation_input_tokens) +
      numOr0(usage.cache_read_input_tokens);
    const cap = numOr0(cw.context_window_size);
    pct = cap && used ? Math.round((used * 100.0) / cap) : null;
  }
  if (pct !== null) {
    const [c, r] = band(pct);
    parts.push(`${c}ctx ${pct}%${r}`);
  }

  // --- prompt-cache reuse share
  const usage: UsageNode = isRecord(cw.current_usage) ? cw.current_usage : {};
  const inp = numOr0(usage.input_tokens);
  const cc = numOr0(usage.cache_creation_input_tokens);
  const cr = numOr0(usage.cache_read_input_tokens);
  const totalIn = inp + cc + cr;
  if (totalIn) {
    parts.push(`${DIM}cache ${Math.round((cr * 100.0) / totalIn)}%${RESET}`);
  }

  // --- rate limits: the real ceiling on a subscription
  const rl: RateLimits = isRecord(d.rate_limits) ? d.rate_limits : {};
  const rateLimitKeys: Array<["5h" | "7d", "five_hour" | "seven_day"]> = [
    ["5h", "five_hour"],
    ["7d", "seven_day"],
  ];
  for (const [label, key] of rateLimitKeys) {
    const rpct = pctOf(rl[key]);
    if (rpct !== null) {
      const [c, r] = band(rpct, 50, 75);
      parts.push(`${c}${label} ${rpct}%${r}`);
    }
  }

  // --- cost, last: informational on a subscription
  const costNode = isRecord(d.cost) ? d.cost : {};
  const cost = costNode.total_cost_usd;
  if (typeof cost === "number" && !Number.isNaN(cost) && cost > 0) {
    parts.push(`${DIM}$${cost.toFixed(2)}${RESET}`);
  }

  console.log(parts.join(" · "));
}

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      data += chunk;
    });
    process.stdin.on("end", () => resolve(data));
    process.stdin.on("error", reject);
  });
}

readStdin()
  .then((input) => {
    try {
      main(input);
    } catch {
      console.log("viby-code");
    }
  })
  .catch(() => {
    console.log("viby-code");
  });
