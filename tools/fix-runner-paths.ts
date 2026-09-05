#!/usr/bin/env node
/**
 * Rewrite the Claude-plugin-cache script lookups in SKILL.md bodies so they
 * resolve the viby-toolkit root on either host:
 *   - Claude Code:  ~/.claude/plugins/cache/market-plugin-version/
 *   - opencode / dev: ~/Projects/.../viby-toolkit/plugins/viby-toolkit/
 *
 * Replaces the per-skill ls+tail lookup lines with:
 *   (once per file) the shared VIBY_HOME locator, then VAR= the subpath under it.
 *
 * Usage: node --experimental-strip-types tools/fix-runner-paths.ts [--dry-run]
 * Idempotent: a second run finds nothing to change.
 */
import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const dry = process.argv.includes("--dry-run");
const PLUGIN = join(import.meta.dirname, "..", "plugins", "viby-toolkit");

const LOCATOR = `VIBY_HOME=$(
  for d in "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/ "$HOME"/Projects/*/*/viby-toolkit/plugins/viby-toolkit/; do
    d=\${d%/}
    [ -f "$d/hooks/run.sh" ] && [ -d "$d/skills" ] && { echo "$d"; break; }
  done
)`;

const LINE = /^\s*(\w+)=\$\(\s*ls "\$HOME"\/\.claude\/plugins\/cache\/\*\/viby-toolkit\/\*\/((?:skills|hooks)\/[^\s|]+) 2>\/dev\/null \| tail -1\)$/;

function markdownFiles(root: string): string[] {
  const out: string[] = [];
  const pushDir = (d: string) => {
    if (!existsSync(d)) return;
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const full = join(d, e.name);
      if (e.isDirectory()) pushDir(full);
      else if (e.name.endsWith(".md")) out.push(full);
    }
  };
  pushDir(root);
  return out;
}

function transform(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let locInsert = -1;
  for (const line of lines) {
    const m = line.match(LINE);
    if (!m) { out.push(line); continue; }
    const [, name, subpath] = m;
    if (locInsert === -1) locInsert = out.length;
    out.push(`${name}="$VIBY_HOME/${subpath}"`);
  }
  if (locInsert === -1) return text;
  out.splice(locInsert, 0, LOCATOR + "\n");
  return out.join("\n");
}

let changed = 0;
for (const f of markdownFiles(join(PLUGIN, "skills"))) {
  const text = readFileSync(f, "utf8");
  const next = transform(text);
  if (next !== text) {
    if (!dry) writeFileSync(f, next);
    console.log(dry ? "[dry-run] " + f.replace(PLUGIN + "/", "") : "rewrote " + f.replace(PLUGIN + "/", ""));
    changed++;
  }
}
console.log(`${changed} file(s) updated`);
