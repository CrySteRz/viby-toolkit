/**
 * viby-toolkit public-surface differ — the executable half of /viby-toolkit:api, and the
 * missing half of /viby-toolkit:release.
 *
 * `release` says the version number is a promise about the public surface rather than about
 * the size of the diff, and then asks for a judgement it gave no tool for. This computes the
 * input to that judgement: which exported symbols were added, removed, or had their signature
 * changed between two refs.
 *
 * Usage:
 *   node check-api-surface.ts [--base <ref>] [paths...] [--json] [--quiet]
 * Exit: 0 = no breaking change detected, 1 = breaking change(s) found, 2 = nothing analysable.
 *
 * WHAT IT DOES NOT DO, said plainly. It reads the SYNTACTIC surface only. The literature this
 * repo already cites is unambiguous that this is the easy half: detection handles syntactic
 * breaks well and behavioural ones poorly, and 67% of Maven artifacts violate SemVer anyway.
 * A function whose signature is unchanged and whose meaning inverted is a major break this
 * tool will call a patch. It also cannot follow `export * from` re-export barrels — the same
 * blind spot that a tree-sitter-based code graph has, and for the same reason: resolving it
 * needs module resolution, not pattern matching. So it REPORTS every barrel it could not
 * follow instead of quietly excluding it, because a surface report that silently omits part of
 * the surface is worse than no report.
 */
import { parseArgs } from "node:util";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { stripNoncode } from "../../../lib/strip-noncode.ts";

export type Symbol_ = {
  /** `name` for a value/type, or `name(params)` normalised for a callable. */
  name: string;
  kind: string;
  /** Normalised parameter list, empty for non-callables. */
  params: string;
  file: string;
};

export type Change = {
  severity: "P1" | "P2" | "P3";
  kind: "removed" | "added" | "signature" | "param-name" | "unresolved-reexport" | "not-analysed";
  symbol: string;
  file: string;
  detail: string;
};

export type SurfaceDiff = {
  base: string;
  changes: Change[];
  /** major = something was removed or re-signatured; minor = additions only; patch = neither. */
  verdict: "major" | "minor" | "patch" | "unknown";
  analysed: number;
  skipped: string[];
};

const ANALYSED_EXT = new Set([".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".py", ".go", ".rs"]);
const SKIP_DIRS = new Set([
  ".git", "node_modules", "vendor", "venv", ".venv", "__pycache__", "dist", "build", "out",
  "target", "coverage", ".next", ".turbo",
]);

function normaliseParams(raw: string): string {
  return raw.replace(/\s+/g, " ").trim();
}

/** Parameter identifiers only — used to tell "renamed a param" from "changed the signature". */
function paramNames(params: string): string[] {
  if (params === "") return [];
  return params
    .split(",")
    .map((p) => {
      const m = /^\s*(?:\*{1,2})?([A-Za-z_$][\w$]*)/.exec(p.replace(/^\s*(?:pub\s+)?/, ""));
      return m?.[1] ?? "";
    })
    .filter((n) => n !== "" && n !== "self" && n !== "cls");
}

/**
 * Extract the exported surface. Runs on CODE, never raw text — a `export function` inside a
 * string fixture or a comment is not part of anyone's API. That rule is in this repo's memory
 * because four separate defects came from ignoring it.
 */
export function extractSurface(text: string, file: string): { symbols: Symbol_[]; barrels: string[] } {
  const ext = path.extname(file).toLowerCase();
  const code = stripNoncode(text, ext);
  // Two constructs here have their VALUE inside a string literal — a re-export's module path
  // and Python's `__all__` — and stripping blanks exactly that. So: decide WHERE from the
  // stripped code (a barrel in a comment is still not a barrel), then read WHAT from the raw
  // text at the same offset. This relies on the blanking pass being offset-preserving, which
  // it is: it overwrites in place rather than deleting.
  const rawAt = (index: number, length: number): string => text.slice(index, index + length);
  const symbols: Symbol_[] = [];
  const barrels: string[] = [];
  const add = (name: string, kind: string, params = ""): void => {
    symbols.push({ name, kind, params: normaliseParams(params), file });
  };

  if ([".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"].includes(ext)) {
    for (const m of code.matchAll(/^\s*export\s+(?:async\s+)?function\s*\*?\s*([\w$]+)\s*\(([^)]*)\)/gm)) {
      add(m[1] ?? "", "function", m[2] ?? "");
    }
    for (const m of code.matchAll(/^\s*export\s+(?:abstract\s+)?class\s+([\w$]+)/gm)) add(m[1] ?? "", "class");
    for (const m of code.matchAll(/^\s*export\s+interface\s+([\w$]+)/gm)) add(m[1] ?? "", "interface");
    for (const m of code.matchAll(/^\s*export\s+type\s+([\w$]+)\s*(?:<[^=]*>)?\s*=\s*([^\n;]*)/gm)) {
      add(m[1] ?? "", "type", m[2] ?? "");
    }
    for (const m of code.matchAll(/^\s*export\s+(?:declare\s+)?(?:const|let|var)\s+([\w$]+)/gm)) {
      add(m[1] ?? "", "const");
    }
    for (const m of code.matchAll(/^\s*export\s+default\b/gm)) add("default", "default");
    // `export { a, b as c }` — the alias is the exported name, which is what a caller imports.
    for (const m of code.matchAll(/^\s*export\s*\{([^}]*)\}/gm)) {
      for (const part of (m[1] ?? "").split(",")) {
        const alias = /(?:\bas\s+)?([\w$]+)\s*$/.exec(part.trim());
        if (alias?.[1] !== undefined) add(alias[1], "re-export");
      }
    }
    // `export * from './x'` — cannot be resolved without module resolution. Reported, not dropped.
    for (const m of code.matchAll(/^\s*export\s+\*(?:\s+as\s+[\w$]+)?\s+from\s+['"][^'"]*['"]/gm)) {
      const raw = rawAt(m.index, m[0].length);
      barrels.push(/['"]([^'"]+)['"]/.exec(raw)?.[1] ?? "?");
    }
  } else if (ext === ".py") {
    // An explicit __all__ IS the surface; anything else is convention.
    const all = /^__all__\s*=\s*[\[(][^\])]*[\])]/m.exec(code);
    if (all) {
      for (const m of rawAt(all.index, all[0].length).matchAll(/['"]([\w]+)['"]/g)) add(m[1] ?? "", "__all__");
    } else {
      for (const m of code.matchAll(/^(?:async\s+)?def\s+([\w]+)\s*\(([^)]*)\)/gm)) {
        if (!(m[1] ?? "_").startsWith("_")) add(m[1] ?? "", "function", m[2] ?? "");
      }
      for (const m of code.matchAll(/^class\s+([\w]+)/gm)) {
        if (!(m[1] ?? "_").startsWith("_")) add(m[1] ?? "", "class");
      }
    }
  } else if (ext === ".go") {
    // Exported means capitalised in Go — the language's own visibility rule.
    for (const m of code.matchAll(/^func\s+(?:\([^)]*\)\s*)?([A-Z][\w]*)\s*\(([^)]*)\)/gm)) {
      add(m[1] ?? "", "func", m[2] ?? "");
    }
    for (const m of code.matchAll(/^type\s+([A-Z][\w]*)/gm)) add(m[1] ?? "", "type");
    for (const m of code.matchAll(/^(?:var|const)\s+([A-Z][\w]*)/gm)) add(m[1] ?? "", "var");
  } else if (ext === ".rs") {
    for (const m of code.matchAll(/^\s*pub\s+(?:async\s+)?fn\s+([\w]+)\s*(?:<[^>]*>)?\s*\(([^)]*)\)/gm)) {
      add(m[1] ?? "", "fn", m[2] ?? "");
    }
    for (const m of code.matchAll(/^\s*pub\s+(?:struct|enum|trait|type)\s+([\w]+)/gm)) add(m[1] ?? "", "type");
  }
  return { symbols, barrels };
}

function git(args: string[], cwd: string): { ok: boolean; out: string } {
  const p = spawnSync("git", args, { cwd, encoding: "utf8" });
  return { ok: p.status === 0, out: p.stdout ?? "" };
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
      } else if (e.isFile() && ANALYSED_EXT.has(path.extname(e.name).toLowerCase())) {
        yield full;
      }
    }
  }
}

function key(s: Symbol_): string {
  return `${s.kind === "re-export" ? "" : ""}${s.name}`;
}

export function diffSurface(cwd: string, base: string, targets: string[]): SurfaceDiff {
  const changes: Change[] = [];
  const skipped: string[] = [];
  const files = targets.flatMap((t) => {
    try {
      return fs.statSync(t).isDirectory() ? [...walk(t)] : [t];
    } catch {
      return [];
    }
  });

  const inRepo = git(["rev-parse", "--git-dir"], cwd).ok;
  if (!inRepo) {
    return { base, changes: [], verdict: "unknown", analysed: 0, skipped: ["not a git repository — nothing to compare against"] };
  }

  const now = new Map<string, Symbol_>();
  const before = new Map<string, Symbol_>();
  let analysed = 0;

  for (const file of [...new Set(files)].sort()) {
    let head: string;
    try {
      head = fs.readFileSync(file, "utf8");
    } catch {
      continue;
    }
    analysed += 1;
    const rel = path.relative(cwd, path.resolve(file)) || file;
    const cur = extractSurface(head, file);
    for (const s of cur.symbols) now.set(`${rel}#${key(s)}`, s);
    for (const b of cur.barrels) {
      changes.push({
        severity: "P2",
        kind: "unresolved-reexport",
        symbol: `export * from "${b}"`,
        file: rel,
        detail: "re-export barrel — the symbols behind it are NOT in this report; resolving it needs module resolution, so check it by hand",
      });
    }

    const old = git(["show", `${base}:${rel}`], cwd);
    if (!old.ok) continue; // new file: its symbols show up as additions
    for (const s of extractSurface(old.out, file).symbols) before.set(`${rel}#${key(s)}`, s);
  }

  for (const [k, s] of before) {
    const still = now.get(k);
    if (still === undefined) {
      changes.push({
        severity: "P1",
        kind: "removed",
        symbol: `${s.kind} ${s.name}`,
        file: s.file,
        detail: "removed from the public surface — any caller importing it breaks on upgrade: MAJOR",
      });
      continue;
    }
    if (still.params !== s.params) {
      const wasNames = paramNames(s.params);
      const nowNames = paramNames(still.params);
      const sameShape = wasNames.length === nowNames.length && s.params !== "" && still.params !== "";
      if (sameShape && wasNames.join() !== nowNames.join()) {
        changes.push({
          severity: "P2",
          kind: "param-name",
          symbol: `${s.kind} ${s.name}`,
          file: s.file,
          detail: `parameter renamed (${wasNames.join(", ")} → ${nowNames.join(", ")}) — breaking only where callers pass it by keyword`,
        });
      } else {
        changes.push({
          severity: "P1",
          kind: "signature",
          symbol: `${s.kind} ${s.name}`,
          file: s.file,
          detail: `signature changed (${s.params || "∅"} → ${still.params || "∅"}) — existing calls may no longer compile: MAJOR`,
        });
      }
    }
  }
  for (const [k, s] of now) {
    if (!before.has(k) && before.size > 0) {
      changes.push({
        severity: "P3",
        kind: "added",
        symbol: `${s.kind} ${s.name}`,
        file: s.file,
        detail: "new public symbol — additive: MINOR",
      });
    }
  }

  const breaking = changes.some((c) => c.severity === "P1");
  const additive = changes.some((c) => c.kind === "added");
  const verdict = breaking ? "major" : additive ? "minor" : "patch";
  return { base, changes, verdict, analysed, skipped };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      base: { type: "string", default: "HEAD" },
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
    },
  });

  const cwd = process.cwd();
  const targets = positionals.length > 0 ? positionals : ["."];
  const d = diffSurface(cwd, values.base ?? "HEAD", targets);

  if (d.verdict === "unknown" || d.analysed === 0) {
    if (values.json) console.log(JSON.stringify(d, null, 2));
    else if (!values.quiet) {
      console.log(d.skipped[0] ?? "no files in a language this understands (ts/js, py, go, rs) — surface unknown, decide by hand");
    }
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify(d, null, 2));
    return d.verdict === "major" ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const c of [...d.changes].sort((a, b) => order[a.severity] - order[b.severity])) {
    console.log(`[${c.severity} ${c.kind}] ${c.symbol}  (${c.file})`);
    console.log(`    ${c.detail}`);
  }

  if (!values.quiet) {
    console.log("");
    console.log(`${d.analysed} file(s) compared against ${d.base}: surface change suggests **${d.verdict.toUpperCase()}**`);
    console.log(
      "Syntactic surface only. A signature that stayed the same while its MEANING changed is a\n" +
        "major break this cannot see — behavioural compatibility is still your call. Re-export\n" +
        "barrels are listed above rather than followed. See /viby-toolkit:release.",
    );
  }
  return d.verdict === "major" ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
