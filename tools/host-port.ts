#!/usr/bin/env node
/*
 * viby-toolkit to opencode installer.
 *
 * One source of truth: <repo>/plugins/viby-toolkit/. Claude Code loads it as a plugin;
 * this script wires opencode to the same files so both hosts stay in lockstep.
 *
 * Produced (bare names -- opencode resolves skill/agent NAME from the SKILL.md / agent
 * frontmatter, and the repo already uses "name: adopt", "name: scout", etc.):
 *
 *   ~/.config/opencode/opencode.json
 *       skills.paths   ->  <repo>/plugins/viby-toolkit/skills   (opencode scans SKILL.md recursively)
 *       instructions   ->  ~/.config/opencode/instructions/viby-contract.md
 *   ~/.config/opencode/agents/<name>.md    ->  symlink into the plugin's agents/
 *   ~/.config/opencode/command/<name>.md   ->  symlink into the plugin's commands/
 *
 * Usage: node --experimental-strip-types tools/host-port.ts [--dry-run] [--uninstall]
 * Idempotent; re-running after an upstream update keeps the symlinks in place.
 */
import { mkdirSync, rmSync, symlinkSync, readFileSync, writeFileSync, existsSync, readdirSync, lstatSync } from "node:fs";
import { join, dirname } from "node:path";

const args = process.argv.slice(2);
const dry = args.includes("--dry-run");
const uninstall = args.includes("--uninstall");
const REPO = join(import.meta.dirname, "..");
const PLUGIN = join(REPO, "plugins", "viby-toolkit");
const OC = join(process.env.HOME ?? "/Users/ionutblidaru", ".config", "opencode");
const AGENTS_DIR = join(OC, "agents");
const COMMANDS_DIR = join(OC, "command");
const CONTRACT = join(OC, "instructions", "viby-contract.md");
const CONFIG = join(OC, "opencode.json");
const SKILLS_DIR = join(PLUGIN, "skills");

const log = (m: string) => (dry ? "[dry-run] " : "") + m;
const isLink = (p: string) => { try { return lstatSync(p).isSymbolicLink(); } catch { return false; } };
const isFile = (p: string) => { try { return lstatSync(p).isFile(); } catch { return false; } };
const fileOk = (p: string) => { try { readFileSync(p); return true; } catch { return false; } };

function ensure(dir: string) { if (!dry) mkdirSync(dir, { recursive: true }); else log("mkdir " + dir); }

function replaceLink(target: string, linkPath: string) {
  if (uninstall) {
    if (dry) { log("rm " + linkPath); return; }
    rmSync(linkPath, { force: true });
    return;
  }
  if (dry) { log("ln -s " + target + " " + linkPath); return; }
  if (existsSync(linkPath) || isLink(linkPath)) rmSync(linkPath, { force: true });
  symlinkSync(target, linkPath);
}

// ---- Agents ----
// The repo's agent files use Claude-Code frontmatter (model: haiku, effort, maxTurns,
// tools) which is not valid opencode agent frontmatter, and they omit `mode`. We generate
// an opencode-native agent that keeps the SAME prompt body and a compatible description,
// but drops the Claude-only fields and declares `mode: subagent`. The body is re-read from
// the repo on every run, so an upstream edit flows straight through (no second source).
function agentBodyAndDesc(srcFile: string): { name: string; description: string; body: string; canEdit: boolean } {
  const text = readFileSync(srcFile, "utf8");
  if (!text.startsWith("---")) return { name: "", description: "", body: text, canEdit: false };
  const end = text.indexOf("\n---", 3);
  const fm = text.slice(3, end);
  const rest = text.slice(end + 4).replace(/^\n/, "");
  const name = /^name:\s*(.+)$/m.exec(fm)?.[1]?.trim() ?? "";
  // description: either inline (`description: one line`) or a folded block (`description: >`
  // followed by more-indented lines). Grab one or the other, then trim.
  let description = "";
  const fmLines = fm.split("\n");
  for (let i = 0; i < fmLines.length; i++) {
    const dm = /^description:\s*(.*)$/.exec(fmLines[i]);
    if (!dm) continue;
    if (dm[1].trim() === "" || dm[1].trim() === ">" || dm[1].trim() === "|") {
      for (let j = i + 1; j < fmLines.length; j++) {
        if (!/^\s/.test(fmLines[j])) break;
        const line = fmLines[j].trim();
        if (!line) break;
        description += (description ? " " : "") + line;
      }
    } else {
      description = dm[1].trim();
    }
    if (description) break;
  }
  const tools = /^tools:\s*(.+)$/m.exec(fm)?.[1] ?? "";
  const canEdit = /(^|,)\s*(Edit|Write)\b/.test(tools);
  return { name, description, body: rest.trim(), canEdit };
}

const agentsDir = join(PLUGIN, "agents");
ensure(AGENTS_DIR);
const agentNames = readdirSync(agentsDir).filter((f) => f.endsWith(".md")).map((f) => f.replace(/\.md$/, ""));
// Remove prior files this tool owned (symlinks or generated bare files), leave foreign agents alone
for (const n of readdirSync(AGENTS_DIR)) {
  if (!n.endsWith(".md")) continue;
  const bare = n.replace(/\.md$/, "");
  if (!agentNames.includes(bare) && !bare.startsWith("viby-")) continue;
  if (dry) log("rm agents/" + n);
  else rmSync(join(AGENTS_DIR, n), { force: true });
}
for (const n of agentNames) {
  const { name, description, body, canEdit } = agentBodyAndDesc(join(agentsDir, `${n}.md`));
  // scout/skeptic/reviewer/debugger/researcher are read-only by discipline AND sandbox (edit: deny);
  // implementer is the toolkit's write path and keeps edit: allow.
  const editPerm = canEdit ? "allow" : "deny";
  const out = [
    "---",
    `name: ${name || n}`,
    "mode: subagent",
    `description: ${description || "viby-toolkit helper agent"}`,
    "permission:",
    `  edit: ${editPerm}`,
    "---",
    "",
    body,
    "",
  ].join("\n");
  const target = join(AGENTS_DIR, `${n}.md`);
  if (dry) { log("agents/" + n + ".md  (edit:" + editPerm + ")"); continue; }
  if (fileOk(target) && readFileSync(target, "utf8") === out) { log(`agents/${n}.md (unchanged)`); continue; }
  writeFileSync(target, out);
  log(`agents/${n}.md  (edit:${editPerm})`);
}

// ---- Commands ----
if (true) {
  ensure(COMMANDS_DIR);
  const names = readdirSync(join(PLUGIN, "commands")).filter((f) => f.endsWith(".md")).map((f) => f.replace(/\.md$/, ""));
  for (const n of readdirSync(COMMANDS_DIR)) {
    if (!n.endsWith(".md")) continue;
    const bare = n.replace(/\.md$/, "");
    if (!names.includes(bare) && !bare.startsWith("viby-")) continue;
    if (dry) log("rm command/" + n);
    else rmSync(join(COMMANDS_DIR, n), { force: true });
  }
  for (const n of names) {
    replaceLink(join(PLUGIN, "commands", `${n}.md`), join(COMMANDS_DIR, `${n}.md`));
    log(`command/${n}.md`);
  }
}

// ---- Standing contract (opencode flavor — bare names — this file is opencode-only) ----
const CONTRACT_BODY = [
  "# viby-toolkit standing contract (opencode)",
  "",
  "The viby-toolkit skills and agents are loaded from the plugin repo; invoke them by bare name:",
  "`review`, `verify`, `plan`, `orchestrate`, `brainstorm`, `principles`, `learn`, `handoff`,",
  "`debug`, `incident`, `refactor`, `migrate`, `schema`, `observe`, `api`, `perf`, `test`, `deps`,",
  "`docs`, `ui`, `explore`, `kpi`, `analytics`, `study`, `evaluate`, `secure`, `adopt`, `brain`,",
  "`worktrees`, `extend`, `release`, and the `ship` command.",
  "",
  "Agents (bare names): `scout`, `researcher`, `implementer`, `reviewer`, `skeptic`, `debugger`.",
  "",
  "Standing rules for this session, regardless of skill in play:",
  "- EVIDENCE GATE: never claim done without running the check fresh and showing its command, output, and exit code.",
  "- FAN OUT BY DEFAULT, DON'T ASK: work needing breadth gets 3-4 read-only subagents (scout, researcher,",
  "  reviewer, skeptic) dispatched IN ONE MESSAGE, each read-only. Writes stay single-threaded on the",
  "  main thread (parallel writers make conflicting decisions).",
  "- CHEAP MODELS FIND, MAIN THREAD DECIDES; escalate on low confidence.",
  "- ROUTING: if WHAT to build is unsettled, run `brainstorm` first; else `plan` / `orchestrate`.",
  "- NEAR A CONTEXT LIMIT: expect a refusal, not a hallucination -- clear and re-dispatch beats re-asking.",
  "- NEVER SHIP SPECULATIVE FIXES to production under pressure; use `incident` before `debug`.",
  "",
  "Full contract: plugins/viby-toolkit/skills/principles/SKILL.md -- read it before non-trivial work.",
].join("\n");

function writeContract() {
  ensure(dirname(CONTRACT));
  if (dry) { log("write " + CONTRACT); return; }
  if (uninstall) { rmSync(CONTRACT, { force: true }); log("rm " + CONTRACT); return; }
  if (fileOk(CONTRACT) && readFileSync(CONTRACT, "utf8") === CONTRACT_BODY) return;
  writeFileSync(CONTRACT, CONTRACT_BODY);
  log("wrote " + CONTRACT);
}
writeContract();

// ---- Config ----
function mergeConfig() {
  const raw = fileOk(CONFIG) ? readFileSync(CONFIG, "utf8") : "{}";
  const out = JSON.parse(raw);
  if (uninstall) {
    if (out.skills?.paths) out.skills.paths = out.skills.paths.filter((p: string) => p !== SKILLS_DIR);
    if (Object.keys(out.skills ?? {}).length === 0) delete out.skills;
    if (out.instructions) out.instructions = out.instructions.filter((p: string) => p !== CONTRACT);
    if (out.instructions?.length === 0) delete out.instructions;
  } else {
    out.skills = { ...(out.skills ?? {}), paths: [SKILLS_DIR, ...(out.skills?.paths ?? []).filter((p: string) => p !== SKILLS_DIR)] };
    out.instructions = [CONTRACT, ...(out.instructions ?? []).filter((p: string) => p !== CONTRACT)];
  }
  if (dry) { log("config:"); log(JSON.stringify(out, null, 2)); return; }
  const cur = fileOk(CONFIG) ? readFileSync(CONFIG, "utf8") : null;
  const next = JSON.stringify(out, null, 2) + "\n";
  if (cur !== next) writeFileSync(CONFIG, next);
  log("wrote " + CONFIG);
}
mergeConfig();

console.log(dry ? "dry-run complete" : "port complete — restart opencode to load");
