/**
 * viby-code PreToolUse safety guard.
 *
 * Deterministic backstop against a small set of genuinely destructive Bash commands.
 *
 * Design rules:
 *   - FAIL-OPEN: any error, or the VIBY_SAFETY=off kill-switch, allows the command.
 *   - JSON-deny form (hookSpecificOutput.permissionDecision), NOT exit-2 — plugin exit-2
 *     blocking has open bugs.
 *   - Tiered by VIBY_SAFETY level (critical < high < strict), default 'high'.
 *   - Portable / stack-agnostic: no project assumptions, one optional `git branch` call.
 *
 * Precision matters more than coverage here. A guard that blocks `rm -rf node_modules`
 * teaches you to run VIBY_SAFETY=off, which removes the net entirely — so this decides on
 * the *parsed command* (program + flags + targets), never on a regex over the raw string.
 * That distinction is what lets `grep 'rm -rf' README.md` and `rm -rf dist` through while
 * `rm -rf $HOME` and `rm -rf /` still stop. See tests/guard.test.ts for the contract.
 *
 * This is a safety net, not a policy engine. It errs toward allowing work.
 */
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

type Level = "critical" | "high" | "strict";

const LEVELS: Record<Level, number> = { critical: 1, high: 2, strict: 3 };

function levelThreshold(level: string): number {
  if (level === "critical" || level === "high" || level === "strict") {
    return LEVELS[level];
  }
  return LEVELS.high;
}

type Hit = { level: Level; reason: string };

// Wrappers to skip so `sudo rm -rf /` is judged as `rm -rf /`.
const WRAPPERS = new Set([
  "sudo", "doas", "env", "nice", "ionice", "time", "command",
  "builtin", "exec", "nohup", "stdbuf", "setsid", "xargs",
]);
const SHELLS = new Set(["sh", "bash", "zsh", "dash", "ksh", "fish"]);
const INTERPRETERS = new Set([...SHELLS, "python", "python3", "perl", "ruby", "node", "php"]);
const READERS = new Set([
  "cat", "less", "more", "head", "tail", "bat", "strings", "xxd",
  "od", "nl", "tac", "view", "hexdump",
]);
const FETCHERS = new Set(["curl", "wget", "fetch", "httpie", "http"]);
const PROTECTED_BRANCHES = new Set(["main", "master", "prod", "production"]);

// Paths where recursive delete is routine and cheap to redo.
const TEMP_PREFIXES = ["/tmp/", "/private/tmp/", "/var/tmp/", "/var/folders/", "/dev/shm/"];
// `.env.example` and friends are templates checked into the repo, not secrets.
const SAFE_ENV_HINTS = ["example", "sample", "template", "dist", "defaults", "tpl", "tmpl", "schema"];
const SECRET_EXTS = [".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".ppk"];
const RAW_DISK = /^\/dev\/(sd|nvme|disk|hd|vd)/i;

function startsWithAny(s: string, prefixes: string[]): boolean {
  return prefixes.some((p) => s.startsWith(p));
}

function emitAllow(): never {
  console.log("{}");
  process.exit(0);
}

function emitDeny(reason: string): never {
  console.log(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: `🛑 viby-code safety: ${reason} `
        + `(set VIBY_SAFETY=off to disable, or run it yourself)`,
    },
  }));
  process.exit(0);
}

// ---------------------------------------------------------------- parsing

const WHITESPACE = " \t\r\n";
const QUOTES = "'\"";
const ESCAPE = "\\";
const ESCAPED_QUOTES = "\""; // only double-quote content is escape-aware
const PUNCTUATION = "();<>|&"; // includes ';' — punctuation_chars=True's default set

/**
 * Tokenize a shell command line the way `shlex.shlex(cmd, posix=True,
 * punctuation_chars=True)` with `whitespace_split=True` and `commenters=""` does.
 *
 * POSIX quoting: single/double quotes are stripped and their contents become one
 * token, so the dangerous-looking text in `grep 'rm -rf' f` lands inside a single
 * argument of `grep`, not as a command. `();<>|&` are tokenized as standalone
 * operator tokens, and runs of them merge into one token (`&&`, `>>`, ...).
 * `#` is an ordinary character (no comment handling). Throws on unbalanced
 * quotes/escapes so the caller can fall back, mirroring shlex raising ValueError.
 */
export function tokenizeShellLike(cmd: string): string[] {
  let i = 0;
  const n = cmd.length;

  function nextChar(): string | undefined {
    if (i >= n) return undefined;
    return cmd[i++];
  }
  function pushBack(): void {
    i--;
  }

  function readToken(): string | undefined {
    let token = "";
    let quoted = false;
    // " " = between tokens, "a" = accumulating a word, "c" = accumulating punctuation,
    // "\\" = just after an escape char, "'"/"\"" = inside that quote kind.
    let state = " ";
    let escapedState = " ";

    for (;;) {
      const c = nextChar();

      if (state === " ") {
        if (c === undefined) return token === "" && !quoted ? undefined : token;
        if (WHITESPACE.includes(c)) continue;
        if (c === ESCAPE) { escapedState = "a"; state = ESCAPE; continue; }
        if (PUNCTUATION.includes(c)) { token = c; state = "c"; continue; }
        if (QUOTES.includes(c)) { state = c; continue; }
        token = c;
        state = "a";
        continue;
      }

      if (state === "'" || state === "\"") {
        quoted = true;
        if (c === undefined) throw new Error("No closing quotation");
        if (c === state) { state = "a"; continue; }
        if (c === ESCAPE && ESCAPED_QUOTES.includes(state)) { escapedState = state; state = ESCAPE; continue; }
        token += c;
        continue;
      }

      if (state === ESCAPE) {
        if (c === undefined) throw new Error("No escaped character");
        // In posix shells, only the quote itself or the escape character may be
        // escaped within quotes; anything else keeps the backslash literally.
        if (QUOTES.includes(escapedState) && c !== ESCAPE && c !== escapedState) token += ESCAPE;
        token += c;
        state = escapedState;
        continue;
      }

      // state "a" (word) or "c" (punctuation run)
      if (c === undefined) return token;
      if (WHITESPACE.includes(c)) { state = " "; return token; }
      if (state === "c") {
        if (PUNCTUATION.includes(c)) { token += c; continue; }
        pushBack();
        state = " ";
        return token;
      }
      if (QUOTES.includes(c)) { state = c; continue; }
      if (c === ESCAPE) { escapedState = "a"; state = ESCAPE; continue; }
      if (!PUNCTUATION.includes(c)) { token += c; continue; }
      pushBack();
      state = " ";
      return token;
    }
  }

  const tokens: string[] = [];
  for (;;) {
    const tok = readToken();
    if (tok === undefined) break;
    tokens.push(tok);
  }
  return tokens;
}

type Command = { argv: string[]; op: string };

const OPERATOR_CHARS = ";|&()<>";

/**
 * Tokenize into [{argv, op}, ...]. Uses tokenizeShellLike so quoting is respected:
 * the dangerous-looking text in `grep 'rm -rf' f` lands inside a single argument of
 * `grep`, not as a command. Throws on syntax the tokenizer can't parse (caller falls back).
 */
function splitCommands(cmd: string): Command[] {
  const tokens = tokenizeShellLike(cmd);
  const out: Command[] = [];
  let cur: string[] = [];
  for (const t of tokens) {
    if (t.length > 0 && [...t].every((c) => OPERATOR_CHARS.includes(c))) {
      out.push({ argv: cur, op: t });
      cur = [];
    } else {
      cur.push(t);
    }
  }
  out.push({ argv: cur, op: "" });
  return out;
}

/** Drop leading sudo/env/etc. Returns {argv, escalated} with escalated=true for sudo. */
function stripWrappers(argv: string[]): { argv: string[]; escalated: boolean } {
  let escalated = false;
  let i = 0;
  while (i < argv.length) {
    const cur = argv[i];
    if (cur === undefined) break;
    const prog = path.posix.basename(cur);
    if (!WRAPPERS.has(prog)) break;
    if (prog === "sudo" || prog === "doas") escalated = true;
    i += 1;
    // skip that wrapper's own flags and env assignments (env FOO=bar cmd)
    while (i < argv.length) {
      const a = argv[i];
      if (a !== undefined && (a.startsWith("-") || /^\w+=/.test(a))) {
        i += 1;
      } else {
        break;
      }
    }
  }
  return { argv: argv.slice(i), escalated };
}

/** Split args into {flags, targets}. Short clusters expand: -rf -> -r,-f. */
function parseFlags(args: string[]): { flags: Set<string>; targets: string[] } {
  const flags = new Set<string>();
  const targets: string[] = [];
  let endOfOpts = false;
  for (const a of args) {
    if (endOfOpts) {
      targets.push(a);
    } else if (a === "--") {
      endOfOpts = true;
    } else if (a.startsWith("--")) {
      flags.add(a);
    } else if (a.startsWith("-") && a.length > 1) {
      for (const ch of a.slice(1)) flags.add("-" + ch);
    } else {
      targets.push(a);
    }
  }
  return { flags, targets };
}

// ---------------------------------------------------------------- classifiers

/** Return a reason string if recursively deleting `target` is dangerous, else null. */
function rmTargetDanger(target: string, cwd: string): string | null {
  if (/\$\{?\w+/.test(target)) {
    // The hook sees the pre-expansion string; an empty variable turns
    // `rm -rf $DIR/` into `rm -rf /`. Classic footgun.
    return `unexpanded variable in a recursive-delete target (${target})`;
  }
  if (target.startsWith("~")) return `recursive delete inside the home directory (${target})`;
  const normTarget = path.posix.normalize(target);
  if (normTarget === "." || normTarget === "..") {
    return `recursive delete of the working directory itself (${target})`;
  }

  const first = target.split("/")[0] ?? "";
  if (first.includes("*") || first.includes("?")) {
    return `unscoped glob in a recursive-delete target (${target})`;
  }

  if (target.startsWith("/")) {
    const norm = path.posix.normalize(target);
    const trimmed = norm.replace(/\/+$/, "");
    if (trimmed === "") return "recursive delete of the filesystem root";
    if (startsWithAny(norm, TEMP_PREFIXES)) return null;
    if (cwd && (norm === cwd || norm.startsWith(cwd.replace(/\/+$/, "") + "/"))) return null;
    if ((norm.match(/\//g) ?? []).length <= 1) {
      return `recursive delete of a top-level system directory (${target})`;
    }
    return `recursive delete of an absolute path outside the project (${target})`;
  }

  if (path.posix.normalize(target).startsWith("..")) {
    return `recursive delete escaping the project directory (${target})`;
  }
  return null;
}

/** Return the offending basename if `arg` looks like a secret file, else null. */
function secretTarget(arg: string): string | null {
  const base = path.posix.basename(arg);
  const low = base.toLowerCase();
  if (low === ".env" || low.startsWith(".env.") || low.endsWith(".env")) {
    if (SAFE_ENV_HINTS.some((h) => low.includes(h))) return null;
    return base;
  }
  if (SECRET_EXTS.some((ext) => low.endsWith(ext))) return base;
  if (/id_(rsa|dsa|ecdsa|ed25519)(\.pub)?$/.test(low)) {
    return low.endsWith(".pub") ? null : base;
  }
  if (/(^|\/)\.ssh\//.test(arg) || arg.includes(".aws/credentials")) return base;
  if (["credentials", ".netrc", ".pgpass", "secrets.json", "secrets.yaml", "secrets.yml"].includes(low)) {
    return base;
  }
  return null;
}

/** Return {sub, rest}, skipping git's own global flags. */
function gitSubcommand(args: string[]): { sub: string | null; rest: string[] } {
  const globalFlagsWithValue = new Set([
    "-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path",
  ]);
  let i = 0;
  while (i < args.length) {
    const a = args[i];
    if (a === undefined) break;
    if (globalFlagsWithValue.has(a)) { i += 2; continue; }
    if (a.startsWith("-")) { i += 1; continue; }
    return { sub: a, rest: args.slice(i + 1) };
  }
  return { sub: null, rest: [] };
}

function mentionsProtectedBranch(args: string[]): string | null {
  for (const a of args) {
    if (PROTECTED_BRANCHES.has(a)) return a;
    if (a.includes(":")) {
      for (const part of a.split(":")) {
        if (PROTECTED_BRANCHES.has(part)) return part;
      }
    }
  }
  return null;
}

function currentBranch(cwd?: string): string {
  try {
    const result = spawnSync("git", ["branch", "--show-current"], {
      cwd: cwd || undefined,
      encoding: "utf8",
      timeout: 3000,
    });
    if (result.error) return "";
    return (result.stdout ?? "").trim();
  } catch {
    return "";
  }
}

// ---------------------------------------------------------------- rules

/** Return the first violation in one simple command, else null. */
function checkArgv(argv: string[], escalated: boolean, threshold: number, cwd: string, depth = 0): Hit | null {
  if (argv.length === 0) return null;
  const first = argv[0];
  if (first === undefined) return null;
  const prog = path.posix.basename(first);
  const args = argv.slice(1);

  // Recurse into `bash -c "<cmd>"` so a wrapped command is still judged.
  if (INTERPRETERS.has(prog) && depth < 2) {
    for (let i = 0; i < args.length; i++) {
      if (args[i] === "-c" && i + 1 < args.length) {
        const body = args[i + 1];
        if (body !== undefined) {
          const hit = scan(body, threshold, cwd, depth + 1);
          if (hit) return hit;
        }
        break;
      }
    }
  }

  if (prog === "rm") {
    const { flags, targets } = parseFlags(args);
    if (flags.has("-r") || flags.has("-R") || flags.has("--recursive")) {
      // Target danger is checked FIRST so `sudo rm -rf /` is judged on the
      // target (critical), not downgraded to the sudo rule (strict).
      for (const t of targets) {
        const reason = rmTargetDanger(t, cwd);
        if (reason) return { level: "critical", reason };
      }
      if (escalated) return { level: "strict", reason: "sudo recursive delete" };
      // Scoped recursive delete inside the project: routine cleanup.
      // Only the paranoid posture blocks it.
      return { level: "strict", reason: "any recursive delete (VIBY_SAFETY=strict)" };
    }
    return null;
  }

  if (READERS.has(prog)) {
    for (const a of args) {
      if (a.startsWith("-")) continue;
      const hit = secretTarget(a);
      if (hit) return { level: "high", reason: `reading a secret file (${hit})` };
    }
    return null;
  }

  if (prog === "dd") {
    for (const a of args) {
      if (a.startsWith("of=") && RAW_DISK.test(a.slice(3))) {
        return { level: "critical", reason: `dd writing to a raw disk device (${a})` };
      }
    }
    return null;
  }

  if (prog.startsWith("mkfs")) return { level: "critical", reason: "filesystem format (mkfs)" };

  if (prog === "shred" || prog === "wipefs") {
    return { level: "critical", reason: `destructive disk utility (${prog})` };
  }

  if (prog === "chmod") {
    const { flags, targets } = parseFlags(args);
    if ((flags.has("-R") || flags.has("--recursive"))
      && targets.some((t) => t === "777" || t === "0777" || t === "a+rwx")) {
      return { level: "high", reason: "recursive chmod 777" };
    }
    return null;
  }

  if (prog === "crontab") {
    if (args.includes("-r")) return { level: "strict", reason: "crontab -r (wipes the crontab)" };
    return null;
  }

  if (prog === "git") {
    const { sub, rest } = gitSubcommand(args);
    const { flags, targets } = parseFlags(rest);
    if (sub === "push") {
      const forced = flags.has("-f") || flags.has("--force");
      const leased = [...flags].some(
        (f) => f.startsWith("--force-with-lease") || f.startsWith("--force-if-includes"),
      );
      if (forced && !leased) {
        const branch = mentionsProtectedBranch(targets);
        if (branch) return { level: "high", reason: `force-push to protected branch '${branch}'` };
        return { level: "strict", reason: "force-push" };
      }
    } else if (sub === "reset" && flags.has("--hard")) {
      return { level: "high", reason: "git reset --hard (discards uncommitted work)" };
    } else if (sub === "clean" && (flags.has("-f") || flags.has("--force"))) {
      return { level: "high", reason: "git clean -f (deletes untracked files)" };
    } else if (sub === "checkout" && targets.length > 0 && targets[0] === ".") {
      return { level: "strict", reason: "git checkout . (discards local changes)" };
    } else if (sub === "rebase" && threshold >= LEVELS.high) {
      // Rewriting history on a shared branch. `reset` is not included:
      // --hard is already covered above, and plain/--soft reset is routine.
      const branch = currentBranch(cwd);
      if (PROTECTED_BRANCHES.has(branch)) {
        return { level: "high", reason: `rebase directly on protected branch '${branch}'` };
      }
    }
    return null;
  }

  return null;
}

/** Return the first violation in a full command line, else null. */
function scan(cmd: string, threshold: number, cwd: string, depth = 0): Hit | null {
  // Fork bomb is shell syntax, not a program — match it structurally on the raw text.
  if (/:\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}/.test(cmd)) {
    return { level: "critical", reason: "fork bomb" };
  }

  // Command substitution bodies hide inside a single quoted token, so the tokenizer
  // would hand them over as one opaque argument. Scan them in their own right.
  if (depth < 3) {
    const bodies: string[] = [];
    for (const m of cmd.matchAll(/\$\(([^()]+)\)/g)) {
      if (m[1] !== undefined) bodies.push(m[1]);
    }
    for (const m of cmd.matchAll(/`([^`]+)`/g)) {
      if (m[1] !== undefined) bodies.push(m[1]);
    }
    for (const body of bodies) {
      const hit = scan(body, threshold, cwd, depth + 1);
      if (hit) return hit;
    }
  }

  let commands: Command[];
  try {
    commands = splitCommands(cmd);
  } catch {
    // Unparseable (unbalanced quotes, exotic syntax). Fall back to the
    // unambiguous catastrophic patterns only — never guess and block real work.
    const fallbacks: [RegExp, string][] = [
      [/\brm\s+-[a-zA-Z]*[rR][a-zA-Z]*f?\s+(\/|~|\$HOME)(\s|$)/, "recursive delete of / or home"],
      [/\bmkfs\./, "filesystem format (mkfs)"],
      [/\bdd\b[^|;]*\bof=\/dev\/(sd|nvme|disk|hd)/, "dd writing to a raw disk device"],
    ];
    for (const [pat, reason] of fallbacks) {
      if (pat.test(cmd)) return { level: "critical", reason };
    }
    return null;
  }

  for (let idx = 0; idx < commands.length; idx++) {
    const entry = commands[idx];
    if (entry === undefined) continue;
    const { argv, op } = entry;
    const { argv: stripped, escalated } = stripWrappers(argv);

    const hit = checkArgv(stripped, escalated, threshold, cwd, depth);
    if (hit && LEVELS[hit.level] <= threshold) return hit;

    // Cross-command shapes: `curl ... | sh` and `... > /dev/sda`.
    const nextEntry = commands[idx + 1];
    const nxt = nextEntry ? nextEntry.argv : [];
    const strippedFirst = stripped[0];
    if (op === "|" && strippedFirst !== undefined && nxt.length > 0
      && FETCHERS.has(path.posix.basename(strippedFirst))) {
      const nxtArgv = stripWrappers(nxt).argv;
      const nxtFirst = nxtArgv[0];
      const nxtProg = nxtFirst !== undefined ? path.posix.basename(nxtFirst) : "";
      if (INTERPRETERS.has(nxtProg) && LEVELS.high <= threshold) {
        return { level: "high", reason: `piping ${path.posix.basename(strippedFirst)} straight into a shell` };
      }
    }
    const nxtFirst = nxt[0];
    if ((op === ">" || op === ">>") && nxtFirst !== undefined && RAW_DISK.test(nxtFirst)) {
      if (LEVELS.critical <= threshold) {
        return { level: "critical", reason: `redirect over a raw disk device (${nxtFirst})` };
      }
    }
  }

  return null;
}

type ToolInput = { command?: string };
type Payload = { tool_name?: string; tool_input?: ToolInput; cwd?: string };

function main(): void {
  const level = (process.env.VIBY_SAFETY ?? "high").trim().toLowerCase();
  if (level === "off" || level === "0" || level === "false" || level === "none") emitAllow();
  const threshold = levelThreshold(level);

  const raw = readFileSync(0, "utf8");
  const data = JSON.parse(raw) as Payload;
  if (data.tool_name !== "Bash") emitAllow();
  const cmd = data.tool_input?.command ?? "";
  if (!cmd.trim()) emitAllow();
  const cwd = path.posix.normalize(data.cwd || process.cwd());

  const hit = scan(cmd, threshold, cwd);
  if (hit && LEVELS[hit.level] <= threshold) emitDeny(hit.reason);
  emitAllow();
}

// Only run when executed directly (as the hook entrypoint), not when imported
// (e.g. by tests importing tokenizeShellLike) — mirrors `if __name__ == "__main__"`.
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch {
    // Never wedge a session on a guard bug.
    emitAllow();
  }
}
