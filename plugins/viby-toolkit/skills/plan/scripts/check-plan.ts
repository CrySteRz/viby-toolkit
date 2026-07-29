/**
 * viby-toolkit plan validator — the executable half of /viby-toolkit:plan.
 *
 * `principles` §3 permits parallel writes only when you "can name the partition and the hubs". That
 * has always been a rule with no way to check it: a plan either had a real partition or it didn't,
 * and you found out when two agents edited the same file. This validates the partition mechanically
 * BEFORE anything is dispatched.
 *
 * Usage:
 *   node check-plan.ts <plan.md>... [--json] [--quiet]
 * Exit: 0 = the plan is dispatchable, 1 = findings, 2 = no task list found.
 *
 * Expected task-line shape (order of fields does not matter, `·` or `|` both separate):
 *
 *   - [ ] T1 — add the CSV parser · files: src/parse.ts · verify: npm test -- parse · deps: —
 *   - [x] T2 — wire it into the route · files: src/route.ts · verify: npm test · deps: T1
 *
 * WHAT IT CANNOT DO: it cannot tell you the plan is a good plan, that the steps are in a sensible
 * order, or that the tasks add up to the goal. It checks that the plan is *dispatchable* — that
 * every task says what it touches and how it is proved, and that no two tasks which could run at
 * the same time own the same file. That last check is the one worth the whole script.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";

export type Task = {
  id: string;
  title: string;
  files: string[];
  verify: string;
  deps: string[];
  done: boolean;
  line: number;
};

export type Finding = {
  line: number;
  task: string;
  check: string;
  severity: "P1" | "P2" | "P3";
  problem: string;
  fix: string;
};

const FIELD = /(?:^|[·|])\s*(files|verify|deps|depends on|wave)\s*:\s*([^·|]*)/gi;

export function parseTasks(text: string): Task[] {
  const tasks: Task[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    const m = /^\s*[-*]\s*\[( |x|X)\]\s*([\w.-]+)\s*[—–:-]\s*(.+)$/.exec(line);
    if (!m) continue;
    const [, box, id, rest] = m;
    if (id === undefined || rest === undefined) continue;
    const task: Task = { id, title: rest.split(/[·|]/)[0]?.trim() ?? "", files: [], verify: "", deps: [], done: box !== " ", line: i + 1 };
    for (const f of rest.matchAll(FIELD)) {
      const key = (f[1] ?? "").toLowerCase();
      const val = (f[2] ?? "").trim();
      if (key === "files") task.files = val.split(/[,\s]+/).filter((v) => v !== "" && v !== "—" && v !== "-");
      else if (key === "verify") task.verify = val;
      else if (key === "deps" || key === "depends on") task.deps = val.split(/[,\s]+/).filter((v) => v !== "" && v !== "—" && v !== "-" && v.toLowerCase() !== "none");
    }
    tasks.push(task);
  }
  return tasks;
}

/** Every task that must finish before `id` can start, transitively. */
function ancestors(id: string, byId: Map<string, Task>, seen = new Set<string>()): Set<string> {
  const t = byId.get(id);
  if (t === undefined) return seen;
  for (const d of t.deps) {
    if (seen.has(d)) continue;
    seen.add(d);
    ancestors(d, byId, seen);
  }
  return seen;
}

export function checkPlan(text: string): { tasks: Task[]; findings: Finding[] } {
  const tasks = parseTasks(text);
  const findings: Finding[] = [];
  if (tasks.length === 0) return { tasks, findings };

  const byId = new Map(tasks.map((t) => [t.id, t]));

  for (const t of tasks) {
    if (t.files.length === 0) {
      findings.push({
        line: t.line,
        task: t.id,
        check: "no-files",
        severity: "P1",
        problem: "does not name the files it owns, so it cannot be partitioned — this is the task that collides with another agent",
        fix: "add `files: <paths>`; if you genuinely do not know yet, the task is research, not implementation",
      });
    }
    if (t.verify === "") {
      findings.push({
        line: t.line,
        task: t.id,
        check: "no-verify",
        severity: "P1",
        problem: "has no verification, so 'done' is a feeling — the evidence gate cannot be applied to it",
        fix: "add `verify: <command or observable outcome>` — the thing you will run to prove it works",
      });
    }
    for (const d of t.deps) {
      if (!byId.has(d)) {
        findings.push({
          line: t.line,
          task: t.id,
          check: "unknown-dep",
          severity: "P1",
          problem: `depends on "${d}", which is not a task in this plan`,
          fix: "fix the reference, or add the missing task",
        });
      }
    }
    // A cycle means no valid order exists at all.
    if (ancestors(t.id, byId).has(t.id)) {
      findings.push({
        line: t.line,
        task: t.id,
        check: "dep-cycle",
        severity: "P1",
        problem: "is part of a dependency cycle, so there is no order in which this plan can be executed",
        fix: "break the cycle — usually one of the two tasks needs splitting into the part that can go first",
      });
    }
  }

  // THE check: two tasks that could run at the same time must not own the same file. This is the
  // partition `principles` §3 demands before any parallel write, made mechanical.
  const owners = new Map<string, Task[]>();
  for (const t of tasks) {
    for (const f of t.files) {
      const list = owners.get(f) ?? [];
      list.push(t);
      owners.set(f, list);
    }
  }
  for (const [file, list] of owners) {
    if (list.length < 2) continue;
    for (let i = 0; i < list.length; i += 1) {
      for (let j = i + 1; j < list.length; j += 1) {
        const a = list[i];
        const b = list[j];
        if (a === undefined || b === undefined) continue;
        const ordered = ancestors(a.id, byId).has(b.id) || ancestors(b.id, byId).has(a.id);
        if (!ordered) {
          findings.push({
            line: a.line,
            task: `${a.id}+${b.id}`,
            check: "unpartitioned-file",
            severity: "P1",
            problem: `both own \`${file}\` with no dependency between them, so they can be dispatched in parallel and will conflict`,
            fix: `either add a dependency so they are ordered, merge them, or split \`${file}\` — parallel writes need disjoint ownership`,
          });
        }
      }
    }
    if (list.length > 2) {
      findings.push({
        line: list[0]?.line ?? 1,
        task: list.map((t) => t.id).join("+"),
        check: "hub-file",
        severity: "P2",
        problem: `\`${file}\` is touched by ${list.length} tasks — that is a structural hub, and everything else depends on it`,
        fix: "take the hub yourself, sequentially, before dispatching the tasks around it (principles §3)",
      });
    }
  }

  return { tasks, findings };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  const files = positionals.filter((p) => /\.(md|markdown|txt)$/i.test(p));
  if (files.length === 0) {
    if (!values.quiet) console.log("usage: check-plan.ts <plan.md>");
    return 2;
  }

  const all: Array<Finding & { file: string }> = [];
  let taskCount = 0;
  let parallelisable = 0;
  for (const f of files) {
    let text: string;
    try {
      text = fs.readFileSync(f, "utf8");
    } catch {
      all.push({ file: f, line: 1, task: "-", check: "unreadable", severity: "P2", problem: "could not read this plan, so it was NOT checked", fix: "fix the path and re-run" });
      continue;
    }
    const r = checkPlan(text);
    taskCount += r.tasks.length;
    // How many tasks have no unfinished dependency — the first wave you could dispatch.
    const byId = new Map(r.tasks.map((t) => [t.id, t]));
    parallelisable += r.tasks.filter((t) => t.deps.every((d) => byId.get(d)?.done === true)).length;
    for (const finding of r.findings) all.push({ ...finding, file: f });
  }

  if (taskCount === 0) {
    if (values.json) console.log(JSON.stringify({ tasks: 0, findings: all }));
    else if (!values.quiet) console.log("no task list found — expected lines like `- [ ] T1 — title · files: … · verify: … · deps: …`");
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify({ tasks: taskCount, dispatchableNow: parallelisable, findings: all }, null, 2));
    return all.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of all.sort((a, b) => order[a.severity] - order[b.severity] || a.line - b.line)) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.check}]  ${f.task}`);
    console.log(`    ${f.problem}`);
    console.log(`    fix: ${f.fix}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(
      all.length === 0
        ? `dispatchable: ${taskCount} task(s), ${parallelisable} ready now, ownership is disjoint`
        : `${all.length} finding(s) across ${taskCount} task(s)`,
    );
    console.log(
      "This checks the plan is DISPATCHABLE, not that it is a good plan — it cannot tell you the\n" +
        "steps are sensible or that they add up to the goal. See /viby-toolkit:plan.",
    );
  }
  return all.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
