/**
 * Verify every cross-reference in the plugin actually resolves.
 *
 * Run:  node --experimental-strip-types --disable-warning=ExperimentalWarning tests/check-references.ts
 * Exit: 0 = all references resolve, 1 = at least one is broken.
 *
 * This exists because two shipped bugs were exactly this class, and both were invisible
 * to every other check:
 *
 *  1. `principles/SKILL.md` set `disable-model-invocation: true`, which makes a skill
 *     user-invocable ONLY. Nine skills plus the SessionStart hook instructed Claude to
 *     "Follow /viby-toolkit:principles" — an instruction it could not execute.
 *  2. `test/SKILL.md` built a script path from `${CLAUDE_PLUGIN_ROOT}`, which is set for
 *     HOOKS only and is empty in a skill body, so the path silently broke.
 *
 * Neither is a syntax error, a type error, or a failing test. Only an explicit
 * reachability check catches them.
 */
import { readdirSync, readFileSync, existsSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const PLUGIN = join(ROOT, "plugins", "viby-toolkit");

type Problem = { file: string; line: number; message: string };
const problems: Problem[] = [];

function lineOf(text: string, index: number): number {
  return text.slice(0, index).split("\n").length;
}

function frontmatter(text: string): string {
  if (!text.startsWith("---")) return "";
  const end = text.indexOf("\n---", 3);
  return end === -1 ? "" : text.slice(3, end);
}

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

// ---- inventory
const skillsDir = join(PLUGIN, "skills");
const skills = readdirSync(skillsDir).filter((d) => existsSync(join(skillsDir, d, "SKILL.md")));
const commands = readdirSync(join(PLUGIN, "commands")).map((f) => f.replace(/\.md$/, ""));
const agents = readdirSync(join(PLUGIN, "agents")).map((f) => f.replace(/\.md$/, ""));

/** agent name → the tools its frontmatter grants. */
const agentTools = new Map<string, string[]>(
  agents.map((a) => {
    const fm = frontmatter(readFileSync(join(PLUGIN, "agents", `${a}.md`), "utf8"));
    const line = /^tools:\s*(.+)$/m.exec(fm)?.[1] ?? "";
    return [a, line.split(",").map((t) => t.trim()).filter(Boolean)];
  }),
);

/**
 * Capabilities a skill body can instruct, and the tools an agent needs to actually do them.
 * Deliberately short and specific: a vague pattern here would fire on prose that merely
 * mentions the web, and a check that cries wolf gets deleted.
 */
const CAPABILITIES: Array<{ name: string; impliedBy: RegExp; tools: string[] }> = [
  {
    name: "web research fanned out to agents",
    impliedBy: /\b(?:dispatch|fan out|spawn)[^.\n]{0,80}\b(?:researcher|search|web)\b|one\s+`?researcher`?\s+per/i,
    tools: ["WebSearch", "WebFetch"],
  },
  {
    name: "agents that write code",
    impliedBy: /\bone\s+`implementer`\s+per\b/i,
    tools: ["Edit", "Write"],
  },
];

// Skills Claude cannot invoke. `disable-model-invocation: true` means user-only, so any
// instruction telling Claude to load it is unfollowable.
const modelBlocked = skills.filter((s) =>
  /^disable-model-invocation:\s*true/m.test(frontmatter(readFileSync(join(skillsDir, s, "SKILL.md"), "utf8"))),
);

const allFiles = [...walk(PLUGIN), join(ROOT, "README.md")].filter((f) => f.endsWith(".md"));

for (const file of allFiles) {
  const text = readFileSync(file, "utf8");
  const rel = file.replace(ROOT + "/", "");

  // 1. /viby-toolkit:<name> must exist and be model-invocable
  for (const m of text.matchAll(/\/viby-toolkit:([a-z-]+)/g)) {
    const name = m[1];
    if (name === undefined) continue;
    const line = lineOf(text, m.index);
    if (!skills.includes(name) && !commands.includes(name)) {
      problems.push({ file: rel, line, message: `/viby-toolkit:${name} does not exist` });
    } else if (modelBlocked.includes(name)) {
      problems.push({
        file: rel,
        line,
        message: `/viby-toolkit:${name} is model-blocked (disable-model-invocation) — Claude cannot follow this reference`,
      });
    }
  }

  // 2. CLAUDE_PLUGIN_ROOT is a hook-only variable; in a skill body it expands to nothing.
  //    Match only an actual EXPANSION (`$CLAUDE_PLUGIN_ROOT` / `${CLAUDE_PLUGIN_ROOT}`) —
  //    prose explaining that the variable is unavailable is correct documentation, not a
  //    defect, and flagging it made this very check cry wolf on its own fix.
  if (rel.includes("/skills/")) {
    for (const m of text.matchAll(/\$\{?CLAUDE_PLUGIN_ROOT\}?/g)) {
      problems.push({
        file: rel,
        line: lineOf(text, m.index),
        message: "CLAUDE_PLUGIN_ROOT is set for hooks only — it is empty in a skill body, so this path breaks",
      });
    }
  }

  // 3. Referenced agents must exist
  for (const m of text.matchAll(/`(scout|reviewer|skeptic|implementer|debugger|researcher)`/g)) {
    const name = m[1];
    if (name !== undefined && !agents.includes(name)) {
      problems.push({ file: rel, line: lineOf(text, m.index), message: `agent \`${name}\` does not exist` });
    }
  }

  // 3b. A skill that instructs a CAPABILITY must name at least one agent that HAS it.
  //     Existing only gets you so far: `study` told the reader to fan out multi-angle web
  //     searches while every agent in the library was filesystem-only, so the fan-out it
  //     described had nowhere to go and every search had to run on the main thread — the exact
  //     context pollution subagents exist to prevent. "The agent exists" was true; "the agent
  //     can do the thing" was not, and nothing checked the second.
  if (rel.includes("/skills/") && rel.endsWith("SKILL.md")) {
    const named = agents.filter((a) => new RegExp("`" + a + "`").test(text));
    for (const cap of CAPABILITIES) {
      if (!cap.impliedBy.test(text)) continue;
      const covered = named.some((a) => (agentTools.get(a) ?? []).some((t) => cap.tools.includes(t)));
      if (!covered) {
        problems.push({
          file: rel,
          line: lineOf(text, cap.impliedBy.exec(text)?.index ?? 0),
          message:
            `instructs ${cap.name} but names no agent that can do it ` +
            `(named: ${named.length > 0 ? named.join(", ") : "none"} — none grants ${cap.tools.join("/")})`,
        });
      }
    }
  }

  // 4. Referenced .ts scripts inside the plugin must exist.
  //    A shell variable expansion — `X="$VIBY_HOME/skills/.../script.ts"` — is a runtime path,
  //    not a repo reference; it is immediately preceded by `$` (the expansion marker).
  for (const m of text.matchAll(/([\w/.-]*)\/([\w-]+\.ts)\b/g)) {
    const whole = m[0];
    const index = m.index ?? 0;
    if (!whole.includes("skills/") && !whole.includes("hooks/") && !whole.includes("tests/")) continue;
    if (index > 0 && text[index - 1] === "$") continue;
    const candidate = whole.startsWith("plugins/") ? join(ROOT, whole) : join(PLUGIN, whole);
    const alt = join(ROOT, whole);
    if (!existsSync(candidate) && !existsSync(alt)) {
      problems.push({ file: rel, line: lineOf(text, m.index), message: `referenced script ${whole} does not exist` });
    }
  }
}

// 5. Hook commands in hooks.json must point at files that exist
const hooksJsonPath = join(PLUGIN, "hooks", "hooks.json");
const hooksJson: unknown = JSON.parse(readFileSync(hooksJsonPath, "utf8"));
for (const m of JSON.stringify(hooksJson).matchAll(/\$\{CLAUDE_PLUGIN_ROOT\}\/([\w/.-]+)/g)) {
  const relPath = m[1];
  if (relPath === undefined) continue;
  if (!existsSync(join(PLUGIN, relPath))) {
    problems.push({ file: "plugins/viby-toolkit/hooks/hooks.json", line: 1, message: `hook target ${relPath} does not exist` });
  }
}

// ---- report
console.log(`skills (${skills.length}): ${skills.join(", ")}`);
console.log(`commands: ${commands.join(", ")}`);
console.log(`agents: ${agents.join(", ")}`);
console.log(`model-blocked skills: ${modelBlocked.length ? modelBlocked.join(", ") : "none"}`);
console.log(`scanned ${allFiles.length} markdown files\n`);

if (problems.length > 0) {
  for (const p of problems) console.log(`✗ ${p.file}:${p.line}  ${p.message}`);
  console.log(`\n${problems.length} broken reference(s)`);
  process.exit(1);
}
console.log("✓ every cross-reference resolves");
