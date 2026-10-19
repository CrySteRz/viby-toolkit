/**
 * MCP server (stdio) exposing one `agentgrep` tool. Claude Code starts one per session.
 *
 * It runs 1jehuang/agentgrep (MIT), the structure-aware code search JCode uses, and adds the
 * part JCode's harness adds around it: the session remembers which file structures and code
 * regions it has already been shown, and hands that to agentgrep (`--context-json`), which
 * then shortens them in later `trace` results (the only mode that takes it in 0.1.7). A file
 * edited since is shown in full again. An `outline` is always an explicit ask for one file, so
 * it is never shortened, but what it showed counts as seen.
 */
import { execFile } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { delimiter, isAbsolute, join, relative, resolve, sep } from "node:path";
import { serveTool, type ToolResult } from "../lib/mcp-stdio.ts";

type Json = Record<string, any>;

export const AGENTGREP_TAG = "v0.1.7";
const MAX_TEXT = 60_000;
const RUN_TIMEOUT_MS = 30_000;
const MODES = ["grep", "find", "outline", "trace"] as const;
type Mode = (typeof MODES)[number];

export const TOOL_DESCRIPTION =
  "Structure-aware code search (agentgrep). mode='grep': matches grouped by file under their enclosing " +
  "symbols. mode='find': files ranked for a topic, each with its structure. mode='outline': one file's " +
  "symbols with line ranges, to read just the part you need. mode='trace': follow a symbol with " +
  "terms like ['subject:parseConfig', 'relation:called_from'] and get the relevant regions in full. " +
  "In trace, structure and regions this session was already shown, unchanged since, come back " +
  "shortened (a 'context:' note says so); pass force=true to get them in full.";

export const INPUT_SCHEMA: Json = {
  type: "object",
  required: ["mode"],
  properties: {
    mode: { type: "string", enum: MODES },
    query: { type: "string", description: "grep: the text (a regex with regex=true). find: words describing what you look for." },
    file: { type: "string", description: "outline: the file, relative to path." },
    terms: {
      type: "array",
      items: { type: "string" },
      description:
        "trace: subject:<symbol> and relation:<defined|called_from|triggered_from|rendered|populated|comes_from|handled|implementation>, optionally support:<word> (repeatable), kind:<kind>, path:<hint>.",
    },
    regex: { type: "boolean", description: "grep: treat query as a regex." },
    path: { type: "string", description: "Folder to search. Default: the project." },
    glob: { type: "string", description: "Only files matching this glob, e.g. 'src/**/*.ts'." },
    type: { type: "string", description: "Only this file type, e.g. rust, ts, py." },
    max_files: { type: "integer", minimum: 1, description: "find (default 10) and trace (default 5)." },
    max_regions: { type: "integer", minimum: 1, description: "trace: regions to return (default 6)." },
    paths_only: { type: "boolean", description: "grep, find, trace: only the matching paths." },
    force: { type: "boolean", description: "trace: return everything in full, even what this session was already shown." },
  },
};

export class InputError extends Error {}

function need<T>(value: T | undefined, message: string): T {
  if (value === undefined || value === null || value === "" || (Array.isArray(value) && !value.length)) throw new InputError(message);
  return value;
}

/** The command line for one call. Options first, then `--`, so a query starting with `-` stays a query. */
export function cliArgs(input: Json, root: string, contextFile?: string): string[] {
  const mode = input.mode as Mode;
  const opts = ["--path", root];
  if (typeof input.glob === "string" && mode !== "outline") opts.push("--glob", input.glob);
  if (typeof input.type === "string" && mode !== "outline") opts.push("--type", input.type);
  if (input.paths_only === true && mode !== "outline") opts.push("--paths-only");
  if (Number.isInteger(input.max_files) && (mode === "find" || mode === "trace")) opts.push("--max-files", String(input.max_files));
  if (Number.isInteger(input.max_regions) && mode === "trace") opts.push("--max-regions", String(input.max_regions));
  if (contextFile && mode === "trace") opts.push("--context-json", contextFile);
  switch (mode) {
    case "grep":
      return ["grep", ...opts, ...(input.regex === true ? ["--regex"] : []), "--", need<string>(input.query, "query is required for grep")];
    case "find":
      return ["find", ...opts, "--", ...need<string>(input.query, "query is required for find").split(/\s+/).filter(Boolean)];
    case "outline":
      return ["outline", ...opts, "--", need<string>(input.file, "file is required for outline")];
    case "trace": {
      const terms = Array.isArray(input.terms) ? input.terms.map(String) : typeof input.terms === "string" ? input.terms.split(/\s+/) : [];
      return ["trace", ...opts, "--", ...need(terms.filter(Boolean), "terms are required for trace, e.g. ['subject:parseConfig', 'relation:called_from']")];
    }
  }
}

/** What one result showed of a file: its structure, and which regions in full. */
export type Shown = { file: string; structure: boolean; regions: [number, number][] };

/** Reads agentgrep 0.1.7's text output for what it showed. Unknown output yields nothing. */
export function shownIn(mode: Mode, output: string): Shown[] {
  if (mode === "outline") {
    const file = /^file: (.+)$/m.exec(output)?.[1];
    return file ? [{ file, structure: true, regions: [] }] : [];
  }
  if (mode !== "trace") return [];
  const shown: Shown[] = [];
  let current: Shown | undefined;
  let section = "";
  let region: [number, number] | undefined;
  for (const line of output.split("\n")) {
    const header = /^\d+\. (.+)$/.exec(line);
    if (header) {
      current = { file: header[1]!, structure: false, regions: [] };
      shown.push(current);
      section = "";
      continue;
    }
    if (!current) continue;
    if (/^ {3}\S/.test(line)) {
      section = line.trim();
      if (section === "structure:") current.structure = true;
      continue;
    }
    if (section !== "regions:") continue;
    const entry = /^ {5}- .* @ (\d+)-(\d+) \(\d+ lines?\)$/.exec(line);
    if (entry) region = [Number(entry[1]), Number(entry[2])];
    else if (line === "       full region:" && region) current.regions.push(region);
  }
  return shown;
}

type Seen = { mtimeMs: number; structure: boolean; regions: [number, number][] };

/** This session's memory: absolute path → what it was shown, and the file's mtime then. */
export const seen = new Map<string, Seen>();

function mtime(path: string): number | undefined {
  try {
    return statSync(path).mtimeMs;
  } catch {
    return undefined;
  }
}

export function remember(root: string, shown: Shown[]): void {
  for (const s of shown) {
    const path = resolve(root, s.file);
    const now = mtime(path);
    if (now === undefined) continue;
    const before = seen.get(path);
    const entry: Seen = before && before.mtimeMs === now ? before : { mtimeMs: now, structure: false, regions: [] };
    entry.structure ||= s.structure;
    entry.regions.push(...s.regions);
    seen.set(path, entry);
  }
}

const SURE = 0.95;

/**
 * agentgrep's harness context for a search under `root`: what this session saw there, minus
 * files changed since (they are forgotten, so they come back in full). Null when there is none.
 */
export function harnessContext(root: string): Json | null {
  const known_files: Json[] = [];
  const known_regions: Json[] = [];
  for (const [path, entry] of seen) {
    if (mtime(path) !== entry.mtimeMs) {
      seen.delete(path);
      continue;
    }
    const rel = relative(root, path);
    if (rel.startsWith("..") || isAbsolute(rel)) continue;
    const file = rel.split(sep).join("/");
    const sure = { current_version_confidence: SURE, prune_confidence: SURE, source_strength: "returned_earlier", reasons: ["returned earlier in this session, unchanged since"] };
    if (entry.structure) known_files.push({ path: file, structure_confidence: SURE, ...sure });
    for (const [start_line, end_line] of entry.regions) known_regions.push({ path: file, start_line, end_line, structure_confidence: SURE, body_confidence: SURE, ...sure });
  }
  return known_files.length || known_regions.length ? { version: 1, known_files, known_regions } : null;
}

/** agentgrep on PATH or in ~/.cargo/bin (where `cargo install` puts it). VIBY_AGENTGREP_BIN overrides. */
export function findBinary(env: NodeJS.ProcessEnv = process.env, home = homedir(), platform = process.platform): string | null {
  if (env.VIBY_AGENTGREP_BIN) return existsSync(env.VIBY_AGENTGREP_BIN) ? env.VIBY_AGENTGREP_BIN : null;
  const exe = platform === "win32" ? "agentgrep.exe" : "agentgrep";
  const dirs = [...(env.PATH ?? "").split(delimiter), join(home, ".cargo", "bin")];
  return dirs.map((d) => d && join(d, exe)).find((p) => p && existsSync(p)) || null;
}

export const INSTALL_GUIDE = [
  "agentgrep is not installed. It is a Rust program; install it once (about a minute):",
  `  cargo install --git https://github.com/1jehuang/agentgrep --tag ${AGENTGREP_TAG} --locked`,
  "No cargo? Install Rust from https://rustup.rs first. Ask the person before installing; until then, use Grep and Read.",
].join("\n");

function run(binary: string, args: string[]): Promise<{ code: number; stdout: string; stderr: string }> {
  return new Promise((done) => {
    execFile(binary, args, { timeout: RUN_TIMEOUT_MS, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      // A non-zero exit puts the exit code in err.code; a spawn failure puts a string there.
      const code = !err ? 0 : typeof err.code === "number" ? err.code : 1;
      done({ code, stdout: String(stdout), stderr: err && !stderr ? err.message : String(stderr) });
    });
  });
}

function clip(text: string): string {
  return text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}\n… [truncated ${text.length - MAX_TEXT} characters; narrow with glob, type, path or max_files]` : text;
}

let contextDir: string | undefined;

export async function callTool(input: Json): Promise<ToolResult> {
  const fail = (text: string): ToolResult => ({ content: [{ type: "text", text }], isError: true });
  const mode = input.mode;
  if (!(MODES as readonly string[]).includes(mode)) return fail(`Unknown mode: ${String(mode)}. Use one of ${MODES.join(", ")}.`);
  const binary = findBinary();
  if (!binary) return fail(INSTALL_GUIDE);
  try {
    const root = resolve(typeof input.path === "string" ? input.path : process.cwd());
    if (!existsSync(root)) throw new InputError(`No such folder: ${root}`);
    let contextFile: string | undefined;
    const context = mode === "trace" && input.force !== true ? harnessContext(root) : null;
    if (context) {
      contextDir ??= mkdtempSync(join(tmpdir(), "viby-agentgrep-"));
      contextFile = join(contextDir, "context.json");
      writeFileSync(contextFile, JSON.stringify(context), { mode: 0o600 });
    }
    const args = cliArgs(input, root, contextFile);
    const { code, stdout, stderr } = await run(binary, args);
    if (code !== 0) return fail(`agentgrep ${mode} failed: ${stderr.trim() || `exit ${code}`}`);
    // Only trace takes the context, so only what trace and outline show is remembered.
    if (mode === "trace" || mode === "outline") remember(root, shownIn(mode, stdout));
    return { content: [{ type: "text", text: clip(stdout.trimEnd()) }] };
  } catch (err) {
    return fail(err instanceof InputError ? err.message : `agentgrep ${mode} failed: ${(err as Error).message}`);
  }
}

function pluginVersion(): string {
  try {
    return String(JSON.parse(readFileSync(join(import.meta.dirname, "..", ".claude-plugin", "plugin.json"), "utf8")).version);
  } catch {
    return "unknown";
  }
}

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  process.on("exit", () => contextDir && rmSync(contextDir, { recursive: true, force: true }));
  serveTool({ name: "viby-agentgrep", version: pluginVersion() }, { name: "agentgrep", description: TOOL_DESCRIPTION, inputSchema: INPUT_SCHEMA }, callTool);
}
