/**
 * viby-code read-cost meter — the executable half of /viby-code:evaluate.
 *
 * Answers one question before you spend the context: **what would it cost to just read
 * this?** That number is the baseline every "this tool saves you tokens" claim has to beat,
 * and the denominator of every savings ratio. Without it, a savings claim is a vendor
 * adjective. It also answers the plainer daily question — "does this read set fit in the
 * budget I have left, or do I need a subagent?" (`/viby-code:principles` §2).
 *
 * Usage:
 *   node measure-read-cost.ts [paths...] [--repeat N] [--budget N] [--window N]
 *                             [--top N] [--json] [--quiet]
 * Exit: 0 = measured (and within budget if one was given), 1 = over budget,
 *       2 = nothing measurable found.
 *
 * ACCURACY, STATED HONESTLY — this is an ESTIMATE, not a tokenizer. It counts characters
 * and divides by a per-kind ratio (code is denser in punctuation than prose, data denser
 * still), and counts non-ASCII characters at ~1 token each. Against a real BPE tokenizer
 * expect roughly ±15% on ordinary source files, worse on minified or heavily non-Latin
 * content — both of which it flags. That precision is fit for the decisions it serves:
 * "is this read set 7k or 87k", "does this fit in the remaining budget", "is the graph
 * really 20× cheaper than reading the files". It is NOT fit for a precise published
 * number — if you need one, run a real tokenizer and say which. The tool prints its own
 * error bar for exactly this reason: a cheap estimate that hides its uncertainty is the
 * kind of fast-and-wrong answer that is worse than no measurement at all.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Kind = "code" | "prose" | "data" | "generated";

export type FileCost = {
  file: string;
  bytes: number;
  chars: number;
  tokens: number;
  kind: Kind;
  /** Set when the estimate for this file is less trustworthy than the headline error bar. */
  caveat?: string;
};

export type Skipped = { file: string; reason: string };

export type Measurement = {
  files: FileCost[];
  skipped: Skipped[];
  /** Estimated tokens for ONE pass over every measured file. */
  tokens: number;
  /** tokens × repeat — what a tool that re-sends its whole state each step actually costs. */
  totalTokens: number;
  repeat: number;
  /** Tokens attributable to generated/lock files, which are almost never worth reading. */
  generatedTokens: number;
};

/**
 * Characters per token, by content kind. Code carries more punctuation and more identifier
 * fragments than prose, so it tokenizes denser; JSON/YAML denser again (quotes, colons,
 * brackets). These are calibrated approximations for BPE tokenizers of the cl100k family,
 * not measured constants — see the accuracy note in the header.
 */
const CHARS_PER_TOKEN: Record<Kind, number> = {
  code: 3.6,
  prose: 4.0,
  data: 3.1,
  generated: 3.1,
};

const PROSE_EXT = new Set([".md", ".markdown", ".txt", ".rst", ".adoc", ".mdx"]);
const DATA_EXT = new Set([".json", ".yaml", ".yml", ".toml", ".ini", ".csv", ".tsv", ".xml", ".properties"]);

/**
 * Directories never worth counting: an agent does not read them, and including them would
 * inflate every baseline into meaninglessness. Being absent from the total is the point.
 */
const SKIP_DIRS = new Set([
  ".git",
  "node_modules",
  "vendor",
  "venv",
  ".venv",
  "__pycache__",
  ".mypy_cache",
  ".pytest_cache",
  ".ruff_cache",
  "dist",
  "build",
  "out",
  "target",
  "coverage",
  ".next",
  ".nuxt",
  ".turbo",
  ".gradle",
  ".terraform",
]);

/**
 * Files that exist but are not read by a human or an agent on purpose. They are MEASURED
 * (they really are in the directory you pointed at) but reported separately, because a
 * baseline dominated by a lockfile is measuring the wrong thing.
 */
const GENERATED_NAMES = new Set([
  "package-lock.json",
  "npm-shrinkwrap.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "bun.lockb",
  "cargo.lock",
  "poetry.lock",
  "pdm.lock",
  "uv.lock",
  "composer.lock",
  "gemfile.lock",
  "go.sum",
  "packages.lock.json",
  "flake.lock",
]);

function isGenerated(file: string): boolean {
  const base = path.basename(file).toLowerCase();
  if (GENERATED_NAMES.has(base)) return true;
  return /\.min\.(js|css|mjs|cjs)$/.test(base) || base.endsWith(".map") || base.endsWith(".snap");
}

export function kindOf(file: string): Kind {
  if (isGenerated(file)) return "generated";
  const ext = path.extname(file).toLowerCase();
  if (PROSE_EXT.has(ext)) return "prose";
  if (DATA_EXT.has(ext)) return "data";
  return "code";
}

/**
 * Estimate tokens for a string. ASCII goes through the per-kind ratio; non-ASCII is charged
 * at ~1 token per character, which is roughly right for CJK and deliberately pessimistic
 * for accented Latin — over-estimating a budget is the safe direction to be wrong in.
 */
export function estimateTokens(text: string, kind: Kind): number {
  let ascii = 0;
  for (let i = 0; i < text.length; i += 1) {
    if (text.charCodeAt(i) < 128) ascii += 1;
  }
  const nonAscii = text.length - ascii;
  return Math.ceil(ascii / CHARS_PER_TOKEN[kind] + nonAscii);
}

function looksBinary(buf: Buffer): boolean {
  const n = Math.min(buf.length, 4096);
  for (let i = 0; i < n; i += 1) {
    if (buf[i] === 0) return true;
  }
  return false;
}

function caveatFor(text: string, nonAsciiHeavy: boolean): string | undefined {
  let longest = 0;
  let current = 0;
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] === "\n") {
      if (current > longest) longest = current;
      current = 0;
    } else {
      current += 1;
    }
  }
  if (current > longest) longest = current;
  if (longest > 500) return `longest line is ${longest} chars (minified or generated?) — estimate less reliable`;
  if (nonAsciiHeavy) return "mostly non-ASCII — charged at ~1 token/char, real cost may differ substantially";
  return undefined;
}

function* walk(root: string): Generator<string> {
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      const full = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name)) stack.push(full);
      } else if (e.isFile()) {
        yield full;
      }
    }
  }
}

export function measureFile(file: string): FileCost | Skipped {
  let buf: Buffer;
  try {
    buf = fs.readFileSync(file);
  } catch {
    return { file, reason: "unreadable" };
  }
  if (looksBinary(buf)) return { file, reason: "binary" };
  const text = buf.toString("utf8");
  const kind = kindOf(file);
  let ascii = 0;
  for (let i = 0; i < text.length; i += 1) {
    if (text.charCodeAt(i) < 128) ascii += 1;
  }
  const nonAsciiHeavy = text.length > 0 && (text.length - ascii) / text.length > 0.05;
  const cost: FileCost = {
    file,
    bytes: buf.length,
    chars: text.length,
    tokens: estimateTokens(text, kind),
    kind,
  };
  const caveat = caveatFor(text, nonAsciiHeavy);
  if (caveat !== undefined) cost.caveat = caveat;
  return cost;
}

export function measureReadCost(paths: string[], opts: { repeat?: number } = {}): Measurement {
  const repeat = Math.max(1, Math.floor(opts.repeat ?? 1));
  const targets: string[] = [];
  const skipped: Skipped[] = [];

  for (const p of paths) {
    let stat: fs.Stats;
    try {
      stat = fs.statSync(p);
    } catch {
      skipped.push({ file: p, reason: "does not exist" });
      continue;
    }
    if (stat.isDirectory()) targets.push(...walk(p));
    else targets.push(p);
  }

  const files: FileCost[] = [];
  for (const t of [...new Set(targets)].sort()) {
    const r = measureFile(t);
    if ("reason" in r) skipped.push(r);
    else files.push(r);
  }

  const tokens = files.reduce((a, f) => a + f.tokens, 0);
  const generatedTokens = files.filter((f) => f.kind === "generated").reduce((a, f) => a + f.tokens, 0);
  return { files, skipped, tokens, totalTokens: tokens * repeat, repeat, generatedTokens };
}

function pct(part: number, whole: number): string {
  if (whole <= 0) return "—";
  return `${((part / whole) * 100).toFixed(0)}%`;
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      repeat: { type: "string", default: "1" },
      budget: { type: "string" },
      window: { type: "string", default: "200000" },
      top: { type: "string", default: "10" },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });

  const repeat = Number(values.repeat) || 1;
  const top = Number(values.top) || 10;
  const window = Number(values.window) || 200000;
  const budget = values.budget === undefined ? undefined : Number(values.budget);
  const paths = positionals.length > 0 ? positionals : ["."];

  const m = measureReadCost(paths, { repeat });

  if (m.files.length === 0) {
    if (values.json) console.log(JSON.stringify({ files: [], tokens: 0, skipped: m.skipped }));
    else if (!values.quiet) console.log("nothing measurable found (binary, unreadable, or all paths skipped)");
    return 2;
  }

  const overBudget = budget !== undefined && m.totalTokens > budget;

  if (values.json) {
    console.log(JSON.stringify({ ...m, window, budget: budget ?? null, overBudget }, null, 2));
    return overBudget ? 1 : 0;
  }

  const ranked = [...m.files].sort((a, b) => b.tokens - a.tokens);
  console.log(`${m.files.length} file(s), ~${m.tokens.toLocaleString()} tokens to read once` + ` (${pct(m.tokens, window)} of a ${window.toLocaleString()}-token window)`);
  if (m.repeat > 1) {
    console.log(
      `× ${m.repeat} passes = ~${m.totalTokens.toLocaleString()} tokens ` +
        `(${pct(m.totalTokens, window)} of the window) — cadence, not payload, is what a re-sending tool costs`,
    );
  }
  if (m.generatedTokens > 0) {
    console.log(
      `of which ~${m.generatedTokens.toLocaleString()} (${pct(m.generatedTokens, m.tokens)}) is generated/lock content — ` +
        `almost never worth reading; exclude it before quoting a baseline`,
    );
  }

  console.log("");
  for (const f of ranked.slice(0, top)) {
    console.log(`  ${String(f.tokens).padStart(7)}  ${f.kind.padEnd(9)} ${f.file}`);
    if (f.caveat !== undefined) console.log(`           ⚠ ${f.caveat}`);
  }
  if (ranked.length > top) {
    const rest = ranked.slice(top).reduce((a, f) => a + f.tokens, 0);
    console.log(`  ${String(rest).padStart(7)}  (${ranked.length - top} more file(s))`);
  }

  if (m.skipped.length > 0 && !values.quiet) {
    console.log(`\nskipped ${m.skipped.length}: ` + m.skipped.slice(0, 5).map((s) => `${s.file} (${s.reason})`).join(", "));
  }

  if (budget !== undefined) {
    console.log("");
    if (overBudget) {
      console.log(
        `✗ over budget: ~${m.totalTokens.toLocaleString()} > ${budget.toLocaleString()}. ` +
          `Send a subagent to read it and return the conclusion, or narrow the set — do not spend the main window on it.`,
      );
    } else {
      console.log(`✓ within budget: ~${m.totalTokens.toLocaleString()} ≤ ${budget.toLocaleString()}`);
    }
  }

  if (!values.quiet) {
    console.log(
      "\nEstimate, not a tokenizer: ±15% on ordinary source, worse where flagged above.\n" +
        "Good enough to choose between reading and delegating, and to be the denominator of a\n" +
        "savings claim — not good enough to publish as a precise number. See /viby-code:evaluate.",
    );
  }

  return overBudget ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
