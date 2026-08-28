#!/usr/bin/env -S node --experimental-strip-types
/**
 * viby-toolkit hook-wiring auditor — the one category of shipped artifact with no checker.
 *
 * Every other executable surface (skills, statusline, test scanner, migration linter...) has
 * a contract test. hooks/hooks.json did not: it is hand-edited JSON with three ways to fail
 * silently — a typo'd event name that Claude Code never recognises, a relative path that
 * resolves against the user's cwd instead of the plugin, and a .ts entry invoked with bare
 * `node` instead of through hooks/run.sh (which is the thing that degrades to a no-op on a
 * machine with no TypeScript runtime; bare `node` just crashes there). None of these throw at
 * parse time — they throw, or silently do nothing, the first time a real session hits them.
 *
 * This also closes the opposite failure: `post-tool-use-format.ts` and `statusline.ts` are
 * shipped deliberately UNREGISTERED (opt-in, wired by the user in their own settings.json).
 * A checker that flags every unregistered script as "orphaned" would be wrong about the
 * artifacts that are working exactly as designed. The only defensible rule here is the
 * inverse one: a hook script that is neither registered in hooks.json NOR documented as
 * opt-in in README.md is unreachable by any path — that is the actual defect to catch.
 *
 * Usage:
 *   node check-hooks.ts [plugin-root] [readme-path] [--json] [--quiet]
 * Exit: 1 if any P1 finding, 0 otherwise (P2/P3 are advisory, not blocking).
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  check: string;
  severity: "P1" | "P2" | "P3";
  location: string;
  message: string;
  detail?: string;
};

export type HookEntry = { type?: unknown; command?: unknown };
export type MatcherBlock = { matcher?: unknown; hooks?: unknown };
export type HooksConfig = { hooks?: Record<string, unknown> };

/**
 * The event names Claude Code's hook system recognises. Hardcoded rather than discovered,
 * because there is nowhere in this repo that enumerates them — if Claude Code adds an event,
 * this list needs a manual update, and a name here that Claude Code no longer accepts would
 * start producing false P1s. That tradeoff is deliberate: a typo silently never firing is a
 * worse failure mode than an occasional false alarm on a genuinely new event name.
 */
const KNOWN_EVENTS = new Set([
  "PreToolUse",
  "PostToolUse",
  "Notification",
  "UserPromptSubmit",
  "Stop",
  "SubagentStop",
  "PreCompact",
  "SessionStart",
  "SessionEnd",
]);

/** A representative stdin event per hook type, for the dry-run pass. */
const EVENT_STDIN: Record<string, unknown> = {
  SessionStart: { hook_event_name: "SessionStart", session_id: "check-hooks", cwd: process.cwd(), matcher: "startup" },
  PostToolUse: {
    hook_event_name: "PostToolUse",
    session_id: "check-hooks",
    cwd: process.cwd(),
    tool_name: "Write",
    tool_input: { file_path: "/tmp/check-hooks-sentinel.ts" },
    tool_response: {},
  },
  PreToolUse: {
    hook_event_name: "PreToolUse",
    session_id: "check-hooks",
    cwd: process.cwd(),
    tool_name: "Bash",
    tool_input: { command: "echo hi" },
  },
  UserPromptSubmit: { hook_event_name: "UserPromptSubmit", session_id: "check-hooks", cwd: process.cwd(), prompt: "hello" },
  Stop: { hook_event_name: "Stop", session_id: "check-hooks", cwd: process.cwd(), stop_hook_active: false },
  SubagentStop: { hook_event_name: "SubagentStop", session_id: "check-hooks", cwd: process.cwd(), stop_hook_active: false },
  PreCompact: { hook_event_name: "PreCompact", session_id: "check-hooks", cwd: process.cwd(), trigger: "manual" },
  SessionEnd: { hook_event_name: "SessionEnd", session_id: "check-hooks", cwd: process.cwd(), reason: "exit" },
  Notification: { hook_event_name: "Notification", session_id: "check-hooks", cwd: process.cwd(), message: "test" },
};

export function parseHooksJson(text: string): { config: HooksConfig | null; findings: Finding[] } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (e) {
    return {
      config: null,
      findings: [{ check: "invalid-json", severity: "P1", location: "hooks.json", message: `does not parse as JSON: ${(e as Error).message}` }],
    };
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return {
      config: null,
      findings: [{ check: "invalid-shape", severity: "P1", location: "hooks.json", message: "top-level value is not an object" }],
    };
  }
  return { config: parsed as HooksConfig, findings: [] };
}

/** Validates the shape Claude Code accepts: hooks.<event>[].hooks[].{type, command}. */
export function checkShape(config: HooksConfig): Finding[] {
  const findings: Finding[] = [];
  if (typeof config.hooks !== "object" || config.hooks === null || Array.isArray(config.hooks)) {
    findings.push({ check: "missing-hooks-key", severity: "P1", location: "hooks.json", message: 'no top-level "hooks" object' });
    return findings;
  }
  for (const [event, blocks] of Object.entries(config.hooks)) {
    if (!KNOWN_EVENTS.has(event)) {
      findings.push({
        check: "unknown-event",
        severity: "P1",
        location: event,
        message: `"${event}" is not a Claude Code hook event — most likely a typo, and it will silently never fire`,
      });
    }
    if (!Array.isArray(blocks)) {
      findings.push({ check: "bad-matcher-array", severity: "P1", location: event, message: `value for "${event}" must be an array of matcher blocks` });
      continue;
    }
    blocks.forEach((block, i) => {
      const loc = `${event}[${i}]`;
      if (typeof block !== "object" || block === null) {
        findings.push({ check: "bad-matcher-block", severity: "P1", location: loc, message: "matcher entry is not an object" });
        return;
      }
      const hooksList = (block as MatcherBlock).hooks;
      if (!Array.isArray(hooksList)) {
        findings.push({ check: "missing-hooks-array", severity: "P1", location: loc, message: 'matcher entry has no "hooks" array' });
        return;
      }
      hooksList.forEach((h, j) => {
        const hloc = `${loc}.hooks[${j}]`;
        if (typeof h !== "object" || h === null) {
          findings.push({ check: "bad-hook-entry", severity: "P1", location: hloc, message: "hook entry is not an object" });
          return;
        }
        const entry = h as HookEntry;
        if (entry.type !== "command") {
          findings.push({ check: "bad-hook-type", severity: "P1", location: hloc, message: `type is ${JSON.stringify(entry.type)}, expected "command"` });
        }
        if (typeof entry.command !== "string" || entry.command.trim() === "") {
          findings.push({ check: "missing-command", severity: "P1", location: hloc, message: 'no non-empty "command" string' });
        }
      });
    });
  }
  return findings;
}

export type CommandRef = { event: string; command: string };

export function extractCommands(config: HooksConfig): CommandRef[] {
  const out: CommandRef[] = [];
  const hooks = config.hooks ?? {};
  for (const [event, blocks] of Object.entries(hooks)) {
    if (!Array.isArray(blocks)) continue;
    for (const block of blocks) {
      if (typeof block !== "object" || block === null) continue;
      const list = (block as MatcherBlock).hooks;
      if (!Array.isArray(list)) continue;
      for (const h of list) {
        if (typeof h === "object" && h !== null && typeof (h as HookEntry).command === "string") {
          out.push({ event, command: (h as HookEntry).command as string });
        }
      }
    }
  }
  return out;
}

/** Shell-aware tokenizer (respects quotes) filtered to path-shaped tokens (has a slash, ends in an extension). */
export function pathTokens(command: string): string[] {
  const tokens: string[] = [];
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(command)) !== null) {
    const tok = m[1] ?? m[2] ?? m[3] ?? "";
    if (tok !== "") tokens.push(tok);
  }
  return tokens.filter((t) => /[/~]/.test(t) && /\.\w+$/.test(t));
}

/**
 * Every path a command references must be ${CLAUDE_PLUGIN_ROOT}-relative and must resolve to
 * a real file, and any .ts entry must be executed through hooks/run.sh — never bare `node`,
 * because run.sh is what degrades to a silent no-op on a machine with no TS runtime.
 */
export function checkPathUsage(refs: CommandRef[], pluginRoot: string): Finding[] {
  const findings: Finding[] = [];
  for (const ref of refs) {
    for (const tok of pathTokens(ref.command)) {
      const loc = `${ref.event}: ${tok}`;
      if (!tok.startsWith("${CLAUDE_PLUGIN_ROOT}")) {
        findings.push({
          check: "not-plugin-root-relative",
          severity: "P1",
          location: loc,
          message: "a relative or absolute path resolves against the user's cwd, not the plugin, and silently breaks — use ${CLAUDE_PLUGIN_ROOT}",
        });
        continue;
      }
      const rel = tok.slice("${CLAUDE_PLUGIN_ROOT}".length).replace(/^\/+/, "");
      const resolved = path.join(pluginRoot, rel);
      if (!fs.existsSync(resolved)) {
        findings.push({ check: "missing-script", severity: "P1", location: loc, message: `${resolved} does not exist on disk` });
        continue;
      }
      if (resolved.endsWith(".ts") && !/run\.sh/.test(ref.command)) {
        findings.push({
          check: "ts-bypasses-run-sh",
          severity: "P1",
          location: loc,
          message: "a .ts hook must be invoked through hooks/run.sh, never bare node — run.sh is what degrades to a silent no-op when no TS runtime is present",
        });
      }
    }
  }
  return findings;
}

/**
 * Dry-runs every registered command with a representative stdin event and asserts exit 0. A
 * SessionStart hook additionally must emit parseable JSON on stdout — malformed JSON there
 * wedges the session rather than just failing quietly.
 */
export function dryRunHooks(refs: CommandRef[], pluginRoot: string): Finding[] {
  const findings: Finding[] = [];
  const seen = new Set<string>();
  for (const ref of refs) {
    const key = `${ref.event}|${ref.command}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const payload = EVENT_STDIN[ref.event] ?? { hook_event_name: ref.event, session_id: "check-hooks", cwd: process.cwd() };
    const result = spawnSync("sh", ["-c", ref.command], {
      input: JSON.stringify(payload),
      encoding: "utf8",
      timeout: 15_000,
      env: { ...process.env, CLAUDE_PLUGIN_ROOT: pluginRoot },
    });
    if (result.error) {
      findings.push({ check: "dry-run-crash", severity: "P1", location: ref.event, message: result.error.message });
      continue;
    }
    if (result.status !== 0) {
      findings.push({
        check: "dry-run-nonzero-exit",
        severity: "P1",
        location: ref.event,
        message: `exited ${String(result.status)} on a representative ${ref.event} event: ${(result.stderr || "").trim().slice(0, 300)}`,
      });
      continue;
    }
    if (ref.event === "SessionStart") {
      const out = (result.stdout || "").trim();
      if (out === "") {
        findings.push({ check: "sessionstart-empty-stdout", severity: "P1", location: ref.event, message: "SessionStart hook produced no stdout" });
        continue;
      }
      try {
        JSON.parse(out);
      } catch {
        findings.push({
          check: "sessionstart-malformed-json",
          severity: "P1",
          location: ref.event,
          message: "SessionStart hook stdout is not valid JSON — a malformed SessionStart payload wedges the session",
          detail: out.slice(0, 300),
        });
      }
    }
  }
  return findings;
}

/** Does `readme` document `basename` as opt-in? Both must appear on the same line. */
export function documentedAsOptIn(readme: string, basename: string): boolean {
  return readme.split("\n").some((line) => /opt-in/i.test(line) && line.includes(basename));
}

/**
 * A hook script reachable by no path at all: not wired in hooks.json, not documented as
 * opt-in in README.md. Neither post-tool-use-format.ts nor statusline.ts should ever trip
 * this — they are shipped deliberately unregistered and documented as such.
 */
export function checkOrphanScripts(hooksDir: string, config: HooksConfig, readme: string): Finding[] {
  const findings: Finding[] = [];
  let entries: string[];
  try {
    entries = fs.readdirSync(hooksDir);
  } catch {
    return findings;
  }
  const registered = new Set(extractCommands(config).flatMap((ref) => pathTokens(ref.command).map((t) => path.basename(t))));
  for (const file of entries) {
    if (!/\.(ts|sh)$/.test(file)) continue;
    if (file === "run.sh") continue; // the TS runtime shim, not a hook script itself
    if (registered.has(file)) continue;
    if (documentedAsOptIn(readme, file)) continue;
    findings.push({
      check: "orphaned-hook-script",
      severity: "P1",
      location: file,
      message: `${file} is neither registered in hooks.json nor documented as opt-in in README.md — it is unreachable`,
    });
  }
  return findings;
}

export function checkHooks(pluginRoot: string, readmePath: string): Finding[] {
  const hooksJsonPath = path.join(pluginRoot, "hooks", "hooks.json");
  let text: string;
  try {
    text = fs.readFileSync(hooksJsonPath, "utf8");
  } catch {
    return [{ check: "missing-hooks-json", severity: "P1", location: hooksJsonPath, message: "hooks.json not found" }];
  }
  const { config, findings } = parseHooksJson(text);
  if (config === null) return findings;

  const shapeFindings = checkShape(config);
  const refs = extractCommands(config);
  const pathFindings = checkPathUsage(refs, pluginRoot);
  const dryRunFindings = dryRunHooks(refs, pluginRoot);

  let readme = "";
  try {
    readme = fs.readFileSync(readmePath, "utf8");
  } catch {
    // orphan check degrades gracefully without a README to consult
  }
  const orphanFindings = checkOrphanScripts(path.join(pluginRoot, "hooks"), config, readme);

  return [...findings, ...shapeFindings, ...pathFindings, ...dryRunFindings, ...orphanFindings];
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });
  const pluginRoot = path.resolve(positionals[0] ?? path.join("plugins", "viby-toolkit"));
  const readmePath = path.resolve(positionals[1] ?? path.join(pluginRoot, "..", "..", "README.md"));

  const findings = checkHooks(pluginRoot, readmePath);

  if (values.json) {
    console.log(JSON.stringify({ findings }, null, 2));
    return findings.some((f) => f.severity === "P1") ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`[${f.severity} ${f.check}] ${f.location}`);
    console.log(`    ${f.message}`);
    if (f.detail !== undefined) console.log(`    detail: ${f.detail}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(findings.length === 0 ? "✓ hook wiring is clean" : `${findings.length} finding(s)`);
  }
  return findings.some((f) => f.severity === "P1") ? 1 : 0;
}

if (process.argv[1] && import.meta.filename === process.argv[1]) {
  process.exit(main());
}
