/**
 * viby-toolkit release pre-flight — the mechanical half of /viby-toolkit:release.
 *
 * Checks the things that are exactly decidable before a release, so human attention goes to
 * the thing that is not: whether the change is actually backward-compatible.
 *
 *   version-drift     the same artifact declares different versions in different manifests
 *   dirty-tree        uncommitted changes would not be in the release
 *   unpushed          local commits the tag would point at but nobody else has
 *   tag-exists        the tag for this version is already taken
 *   changelog-stale   a CHANGELOG exists but does not mention this version
 *   debug-artifact    a focused/skipped test or a debugger statement is still in the tree
 *   no-ci             nothing automated gates this release
 *
 * Usage:
 *   node check-release.ts [dir] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = not a git repo / nothing to check.
 *
 * DESIGN: every check here is mechanical and unambiguous — that is the whole selection
 * criterion. Judgement calls (is this breaking? is the version bump right?) are the skill's
 * job, not this script's, because a checker that guesses at judgement trains you to ignore
 * it. Monorepos are detected and per-package versions are then reported as expected rather
 * than flagged as drift.
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { stripNoncode } from "../../../lib/strip-noncode.ts";

export type Finding = {
  check: string;
  severity: "P1" | "P2" | "P3";
  message: string;
  detail?: string;
};

const MONOREPO_DIRS = /(^|\/)(packages|apps|crates|libs|modules|services|examples|playground)\//;
const SKIP_DIRS = new Set([
  ".git", "node_modules", "venv", ".venv", "dist", "build", "target", "out",
  "__pycache__", ".next", "vendor", "coverage", ".tox", "fixtures", "testdata",
]);

/** Manifest files that declare a version, with how to extract it. */
const VERSION_SOURCES: Array<{ name: string; extract: (text: string) => string | null }> = [
  { name: "package.json", extract: (t) => jsonField(t, "version") },
  { name: "plugin.json", extract: (t) => jsonField(t, "version") },
  { name: "marketplace.json", extract: (t) => nestedPluginVersion(t) },
  { name: "composer.json", extract: (t) => jsonField(t, "version") },
  { name: "Cargo.toml", extract: (t) => tomlVersion(t, ["package"]) },
  { name: "pyproject.toml", extract: (t) => tomlVersion(t, ["project", "tool.poetry"]) },
  { name: "setup.py", extract: (t) => match(t, /version\s*=\s*["']([^"']+)["']/) },
  { name: "pubspec.yaml", extract: (t) => match(t, /^version:\s*["']?([^"'\s]+)/m) },
  { name: "build.gradle", extract: (t) => match(t, /^\s*version\s*=?\s*["']([^"']+)["']/m) },
  { name: "VERSION", extract: (t) => (t.trim().length > 0 && t.trim().length < 40 ? t.trim() : null) },
  { name: "version.txt", extract: (t) => (t.trim().length > 0 && t.trim().length < 40 ? t.trim() : null) },
  { name: "__init__.py", extract: (t) => match(t, /__version__\s*=\s*["']([^"']+)["']/) },
  { name: "mix.exs", extract: (t) => match(t, /version:\s*["']([^"']+)["']/) },
];

function match(text: string, re: RegExp): string | null {
  const m = re.exec(text);
  return m?.[1] ?? null;
}

function jsonField(text: string, field: string): string | null {
  try {
    const d: unknown = JSON.parse(text);
    if (typeof d === "object" && d !== null && field in d) {
      const v = (d as Record<string, unknown>)[field];
      return typeof v === "string" ? v : null;
    }
  } catch {
    /* unparseable */
  }
  return null;
}

/** A plugin marketplace manifest carries the version inside its plugins array. */
function nestedPluginVersion(text: string): string | null {
  try {
    const d: unknown = JSON.parse(text);
    if (typeof d === "object" && d !== null && "plugins" in d) {
      const list = (d as { plugins?: unknown }).plugins;
      if (Array.isArray(list) && list.length > 0) {
        const first: unknown = list[0];
        if (typeof first === "object" && first !== null && "version" in first) {
          const v = (first as Record<string, unknown>).version;
          return typeof v === "string" ? v : null;
        }
      }
    }
  } catch {
    /* unparseable */
  }
  return null;
}

/**
 * Version under the first of `sections` that has one, so `[project]` beats a dependency's.
 *
 * Scans lines rather than regex-slicing the section body: with the `m` flag, a `$` in the
 * terminator lookahead matches the first end-of-LINE, so the captured body was always empty
 * and every TOML version silently read as null.
 */
function tomlVersion(text: string, sections: string[]): string | null {
  const found = new Map<string, string>();
  let current = "";
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const header = /^\[([^\]]+)\]/.exec(line);
    if (header?.[1] !== undefined) {
      current = header[1].trim();
      continue;
    }
    const v = match(line, /^version\s*=\s*["']([^"']+)["']/);
    if (v !== null && !found.has(current)) found.set(current, v);
  }
  for (const section of sections) {
    const v = found.get(section);
    if (v !== undefined) return v;
  }
  return null;
}

function git(root: string, args: string[]): { ok: boolean; out: string } {
  const r = spawnSync("git", args, { cwd: root, encoding: "utf8", timeout: 15_000 });
  return { ok: r.status === 0, out: (r.stdout ?? "").trim() };
}

function walk(root: string, depth = 0): string[] {
  if (depth > 4) return [];
  const out: string[] = [];
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(root, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    const full = path.join(root, e.name);
    if (e.isDirectory()) {
      if (!SKIP_DIRS.has(e.name)) out.push(...walk(full, depth + 1));
    } else {
      out.push(full);
    }
  }
  return out;
}

export function checkRelease(root: string): { findings: Finding[]; versions: Map<string, string[]>; isRepo: boolean } {
  const findings: Finding[] = [];
  const versions = new Map<string, string[]>();
  const isRepo = git(root, ["rev-parse", "--git-dir"]).ok;

  const files = walk(root);
  const rel = (p: string): string => path.relative(root, p).split(path.sep).join("/");

  // ---- versions
  const monorepo = files.some((f) => /(^|\/)(pnpm-workspace\.yaml|nx\.json|turbo\.json|lerna\.json)$/.test(rel(f)));
  for (const f of files) {
    const r = rel(f);
    const base = path.basename(f);
    const src = VERSION_SOURCES.find((v) => v.name === base);
    if (!src) continue;
    // In a monorepo, per-package versions are expected. Only compare the top of the tree.
    if (MONOREPO_DIRS.test(r) && monorepo) continue;
    const v = src.extract(read(f));
    if (v === null) continue;
    const list = versions.get(v) ?? [];
    list.push(r);
    versions.set(v, list);
  }

  if (versions.size > 1) {
    const summary = [...versions.entries()].map(([v, fs_]) => `${v} (${fs_.join(", ")})`).join("  vs  ");
    findings.push({
      check: "version-drift",
      severity: "P1",
      message: "manifests disagree about the version — releasing now ships an inconsistent artifact",
      detail: summary,
    });
  }

  const current = versions.size === 1 ? [...versions.keys()][0] : undefined;

  // ---- git state
  if (isRepo) {
    const status = git(root, ["status", "--porcelain"]).out;
    if (status !== "") {
      const count = status.split("\n").length;
      findings.push({
        check: "dirty-tree",
        severity: "P1",
        message: `${count} uncommitted change(s) — these would NOT be in the release`,
        detail: status.split("\n").slice(0, 8).join("\n"),
      });
    }

    const branch = git(root, ["branch", "--show-current"]).out;
    const upstream = git(root, ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]);
    if (upstream.ok) {
      const ahead = git(root, ["rev-list", "--count", `${upstream.out}..HEAD`]).out;
      if (ahead !== "" && ahead !== "0") {
        findings.push({
          check: "unpushed",
          severity: "P1",
          message: `${ahead} commit(s) on ${branch || "HEAD"} are not pushed — a tag would point at code nobody else has`,
        });
      }
    } else {
      findings.push({
        check: "unpushed",
        severity: "P2",
        message: `branch ${branch || "HEAD"} has no upstream, so push state cannot be verified`,
      });
    }

    if (current !== undefined) {
      const tags = git(root, ["tag", "--list"]).out.split("\n").filter(Boolean);
      const taken = tags.find((t) => t === current || t === `v${current}`);
      if (taken !== undefined) {
        findings.push({
          check: "tag-exists",
          severity: "P1",
          message: `tag ${taken} already exists — either the version was not bumped, or this release already happened`,
        });
      }
    }
  }

  // ---- changelog
  const changelog = files.find((f) => /^(CHANGELOG|HISTORY|RELEASES)(\.md|\.rst|\.txt)?$/i.test(path.basename(f)));
  if (changelog !== undefined && current !== undefined) {
    // Token match, not substring: `"12.0.1".includes("2.0")` is true, so an unrelated older
    // release silently satisfied the check for version 2.0.
    const versionRe = new RegExp(`(^|[^0-9.])${current.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}([^0-9.]|$)`);
    if (!versionRe.test(read(changelog))) {
      findings.push({
        check: "changelog-stale",
        severity: "P2",
        message: `${rel(changelog)} does not mention ${current} — the release would be undocumented`,
      });
    }
  }

  // ---- debug artifacts. Narrow, unambiguous patterns only.
  const DEBUG_PATTERNS: Array<{ re: RegExp; what: string }> = [
    { re: /\b(?:describe|it|test|context)\s*\.\s*only\b/, what: "focused test (.only) — the suite silently shrinks" },
    { re: /\bf(?:describe|it)\s*\(/, what: "focused test (fdescribe/fit)" },
    { re: /^\s*debugger;?\s*$/m, what: "debugger statement" },
    { re: /\bbreakpoint\s*\(\s*\)/, what: "breakpoint() call" },
    { re: /\bbinding\.pry\b/, what: "binding.pry" },
    { re: /\bdbg!\s*\(/, what: "dbg!() macro" },
    { re: /\.only\s*\(/, what: "focused block (.only)" },
  ];
  const CODE_EXT = new Set([".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".py", ".rb", ".rs", ".go", ".java"]);
  const debugHits: string[] = [];
  for (const f of files) {
    if (!CODE_EXT.has(path.extname(f))) continue;
    const r = rel(f);
    const raw = read(f);
    if (raw.length > 400_000) continue;
    // Match against CODE only. A test fixture, a doc comment, or a regex that merely mentions
    // `describe.only` is not a focused test — and this checker flagged exactly that on its
    // own repo before the shared blanking pass was wired in.
    const text = stripNoncode(raw, path.extname(f).toLowerCase());
    for (const p of DEBUG_PATTERNS) {
      // Every matching line, not just the first: findIndex reported one hit per pattern per
      // file, so a second focused test in the same file shipped silently even after the
      // first was "fixed". Deduped per line so overlapping patterns don't double-report.
      const seenLines = new Set<number>();
      text.split("\n").forEach((l, idx) => {
        if (!p.re.test(l) || seenLines.has(idx)) return;
        seenLines.add(idx);
        debugHits.push(`${r}:${idx + 1} — ${p.what}`);
      });
    }
  }
  if (debugHits.length > 0) {
    findings.push({
      check: "debug-artifact",
      severity: "P2",
      message: `${debugHits.length} debug artifact(s) still in the tree`,
      detail: debugHits.slice(0, 8).join("\n"),
    });
  }

  // ---- CI
  const hasCi = files.some((f) =>
    /(^|\/)(\.github\/workflows\/.+\.ya?ml|\.gitlab-ci\.yml|\.circleci\/config\.yml|Jenkinsfile|azure-pipelines\.yml)$/.test(
      rel(f),
    ),
  );
  if (!hasCi) {
    findings.push({
      check: "no-ci",
      severity: "P3",
      message: "no CI config found — nothing automated gates this release, so the checks are all manual",
    });
  }

  return { findings, versions, isRepo };
}

function read(p: string): string {
  try {
    // Strip a UTF-8 BOM: Node's "utf8" keeps it, and JSON.parse then throws on an
    // otherwise-valid manifest. That silently dropped the file from version detection, so a
    // real version drift went unreported because of an encoding artifact.
    return fs.readFileSync(p, "utf8").replace(/^\uFEFF/, "");
  } catch {
    return "";
  }
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  const root = path.resolve(positionals[0] ?? ".");
  const { findings, versions, isRepo } = checkRelease(root);

  if (values.json) {
    console.log(
      JSON.stringify({ root, isRepo, versions: Object.fromEntries(versions), findings }, null, 2),
    );
    return findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of [...findings].sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`[${f.severity} ${f.check}] ${f.message}`);
    if (f.detail !== undefined) for (const l of f.detail.split("\n")) console.log(`    ${l}`);
  }

  if (!values.quiet) {
    if (versions.size > 0) {
      console.log("");
      for (const [v, files] of versions) console.log(`version ${v}: ${files.join(", ")}`);
    }
    console.log("");
    if (findings.length === 0) {
      console.log("✓ mechanical pre-flight clean.");
    } else {
      console.log(`${findings.length} finding(s).`);
    }
    console.log(
      "This checks only what is exactly decidable. It does NOT judge whether the change is\n" +
        "backward-compatible — semantic versioning is violated routinely, so decide that from\n" +
        "the actual public-surface diff (see /viby-toolkit:release).",
    );
  }
  if (!isRepo) return 2;
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
