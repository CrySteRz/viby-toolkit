/**
 * viby-toolkit docs auditor — the executable half of /viby-toolkit:docs.
 *
 * `docs` says every command, path and claim must be one you have actually run or verified, because
 * documentation is the easiest place to ship something confidently false — and a wrong command in a
 * README costs more than ten missing pages, since a stale doc is *trusted*.
 *
 * This automates the mechanically checkable part of that: does the path exist, is the script real,
 * does the link resolve, does the anchor exist.
 *
 * Usage:
 *   node check-docs.ts [paths...] [--root <repo-root>] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no docs found.
 *
 * WHAT IT CANNOT DO: it does not run anything, so it cannot tell you a documented command still
 * *works* — only that the script it names exists. It cannot check prose claims, external URLs (no
 * network), or whether the example output is still what the tool prints. A clean run means the
 * document's references resolve, not that the document is true.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  file: string;
  line: number;
  rule: string;
  severity: "P1" | "P2" | "P3";
  problem: string;
  fix: string;
};

/** A path reference worth resolving: has a slash and an extension, or is a known dir form. */
const PATH_REF = /`((?:~\/|)[\w.@-]+(?:\/[\w.@-]+)+\.[a-z0-9]{1,6})`/gi;
/** Placeholders are illustrations, not references. */
const PLACEHOLDER =
  /[<>*{}$]|(^|\/)(foo|bar|baz|example|examples|sample|your|my|path|dir|some|thing|todo|xxx|name|topic|feature|date|yyyy|repo|owner|user|project|client|file|entry|title)([./-]|$)|\.\.\.|\/\/|^https?:/i;
/** A package script invocation. */
/**
 * Only explicit run-forms. A bare `make`, `yarn` or `pnpm` matches ordinary English — "make it
 * readable", "make a decision" — and produced 48 false findings on this repo's own prose.
 */
const SCRIPT_CALL = /\b(?:npm run|pnpm run|yarn run|bun run|make -C \S+)\s+([\w:.-]+)/g;
/** A relative markdown link. */
const MD_LINK = /\[[^\]]*\]\((?!https?:|mailto:|#)([^)\s#]+)(#[^)\s]*)?\)/g;

/**
 * Resolution by SUFFIX against an index of the repo's real files, not by guessing a base directory.
 * A document legitimately cites a path relative to the repo root, to a package subdirectory, or to
 * its own folder — guessing produced 28 false "stale" findings on this repo, every one a real file
 * living one directory deeper than assumed.
 */
function resolves(ref: string, root: string, docDir: string, index: Set<string>): boolean | null {
  if (PLACEHOLDER.test(ref)) return null;
  if (ref.startsWith("~/")) return fs.existsSync(path.join(process.env.HOME ?? "", ref.slice(2)));
  if (ref.startsWith("/")) return fs.existsSync(ref);
  if ([path.join(root, ref), path.join(docDir, ref)].some((p) => fs.existsSync(p))) return true;
  const needle = "/" + ref.replace(/^\.\//, "");
  for (const p of index) if (p.endsWith(needle)) return true;
  return false;
}

/** Every file path in the repo, once, for suffix matching. */
function fileIndex(root: string): Set<string> {
  const out = new Set<string>();
  const skip = new Set([".git", "node_modules", "vendor", "dist", "build", "target", ".next", "coverage", ".venv", "venv"]);
  const stack = [root];
  let visited = 0;
  while (stack.length > 0 && visited < 60_000) {
    const dir = stack.pop();
    if (dir === undefined) continue;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const e of entries) {
      visited += 1;
      const full = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (!skip.has(e.name)) stack.push(full);
      } else out.add(full);
    }
  }
  return out;
}

function anchorsOf(text: string): Set<string> {
  const out = new Set<string>();
  for (const m of text.matchAll(/^#{1,6}\s+(.+)$/gm)) {
    const slug = (m[1] ?? "")
      .toLowerCase()
      .replace(/[`*_[\]()]/g, "")
      .replace(/[^\w\s-]/g, "")
      .trim()
      .replace(/\s+/g, "-");
    if (slug !== "") out.add(slug);
  }
  return out;
}

export function auditDoc(file: string, text: string, root: string, scripts: Set<string>, index: Set<string> = new Set()): Finding[] {
  const findings: Finding[] = [];
  const lines = text.split("\n");
  const docDir = path.dirname(path.resolve(file));

  // Self-calibrating, exactly as in check-memory: if NOTHING resolves, the root is wrong rather than
  // the document being entirely stale. A comparison that could not happen must not read as findings.
  const refs: Array<{ ref: string; line: number; exists: boolean }> = [];
  for (let i = 0; i < lines.length; i += 1) {
    for (const m of (lines[i] ?? "").matchAll(PATH_REF)) {
      const ref = m[1];
      if (ref === undefined) continue;
      const r = resolves(ref, root, docDir, index);
      if (r === null) continue;
      refs.push({ ref, line: i + 1, exists: r });
    }
  }
  const rootLooksRight = refs.length === 0 || refs.some((r) => r.exists);
  if (!rootLooksRight) {
    findings.push({
      file,
      line: refs[0]?.line ?? 1,
      rule: "root-unknown",
      severity: "P3",
      problem: `none of its ${refs.length} path reference(s) resolve, so staleness could NOT be checked — the root is probably wrong`,
      fix: "re-run with --root pointing at the repository this document describes",
    });
  } else {
    for (const r of refs.filter((x) => !x.exists)) {
      findings.push({
        file,
        line: r.line,
        rule: "stale-path",
        severity: "P1",
        problem: `documents \`${r.ref}\`, which does not exist — a reader follows it, finds nothing, and stops trusting the rest`,
        fix: "update it or delete the sentence; stale documentation is worse than none, because it is trusted",
      });
    }
  }

  // Documented scripts must exist. This is the classic wrong-README: the command that works in the
  // author's shell because of something set six months ago.
  if (scripts.size > 0) {
    for (let i = 0; i < lines.length; i += 1) {
      for (const m of (lines[i] ?? "").matchAll(SCRIPT_CALL)) {
        const name = m[1];
        if (name === undefined || name.startsWith("-")) continue;
        if (/^(install|ci|test|start|build|dev|run|help|version|audit|init|create|exec|dlx|add|remove|why|link|publish|pack|outdated|update|upgrade|list|ls|info|login|logout|whoami|clean|all|-C)$/i.test(name)) continue;
        if (!scripts.has(name)) {
          findings.push({
            file,
            line: i + 1,
            rule: "unknown-script",
            severity: "P1",
            problem: `documents a command \`${m[0]}\` but no such script is defined — a reader's first command fails`,
            fix: "fix the name, or add the script; run every command you document from a clean state",
          });
        }
      }
    }
  }

  // Relative links and their anchors.
  for (let i = 0; i < lines.length; i += 1) {
    for (const m of (lines[i] ?? "").matchAll(MD_LINK)) {
      const target = m[1];
      const anchor = m[2];
      if (target === undefined || PLACEHOLDER.test(target)) continue;
      const resolved = [path.join(docDir, target), path.join(root, target)].find((p) => fs.existsSync(p)) ??
        [...index].find((p) => p.endsWith("/" + target.replace(/^\.\//, "")));
      if (resolved === undefined) {
        findings.push({
          file,
          line: i + 1,
          rule: "dead-link",
          severity: "P1",
          problem: `links to \`${target}\`, which does not exist`,
          fix: "fix the path or remove the link",
        });
        continue;
      }
      if (anchor !== undefined && anchor.length > 1 && /\.md$/i.test(resolved)) {
        const slug = anchor.slice(1).toLowerCase();
        try {
          if (!anchorsOf(fs.readFileSync(resolved, "utf8")).has(slug)) {
            findings.push({
              file,
              line: i + 1,
              rule: "dead-anchor",
              severity: "P2",
              problem: `links to \`${target}${anchor}\`, but that heading does not exist in the target`,
              fix: "fix the anchor — a heading was probably renamed",
            });
          }
        } catch {
          /* unreadable target */
        }
      }
    }
  }

  return findings;
}

function readScripts(root: string): Set<string> {
  const out = new Set<string>();
  try {
    const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8").replace(/^﻿/, "")) as {
      scripts?: Record<string, string>;
    };
    for (const k of Object.keys(pkg.scripts ?? {})) out.add(k);
  } catch {
    /* not a node project */
  }
  try {
    for (const m of fs.readFileSync(path.join(root, "Makefile"), "utf8").matchAll(/^([\w.-]+):/gm)) {
      const t = m[1];
      if (t !== undefined) out.add(t);
    }
  } catch {
    /* no makefile */
  }
  return out;
}

function* walk(root: string): Generator<string> {
  const stack = [root];
  const skip = new Set([".git", "node_modules", "vendor", "dist", "build", "target", ".next", "coverage", ".venv", "venv"]);
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
        if (!skip.has(e.name)) stack.push(full);
      } else if (/\.(md|markdown)$/i.test(e.name)) {
        yield full;
      }
    }
  }
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { root: { type: "string" }, json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  const root = values.root ?? process.cwd();
  let targets: string[] = [];
  if (positionals.length === 0) targets = [...walk(root)];
  else {
    for (const p of positionals) {
      try {
        if (fs.statSync(p).isDirectory()) targets.push(...walk(p));
        else targets.push(p);
      } catch {
        /* skip */
      }
    }
  }
  targets = [...new Set(targets)].sort();
  if (targets.length === 0) {
    if (values.json) console.log(JSON.stringify({ scanned: 0, findings: [] }));
    else if (!values.quiet) console.log("no markdown found");
    return 2;
  }

  const scripts = readScripts(root);
  const index = fileIndex(root);
  const findings = targets.flatMap((t) => {
    try {
      return auditDoc(path.relative(root, t) || t, fs.readFileSync(t, "utf8"), root, scripts, index);
    } catch {
      return [];
    }
  });

  if (values.json) {
    console.log(JSON.stringify({ scanned: targets.length, findings }, null, 2));
    return findings.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of findings.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file))) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.rule}]`);
    console.log(`    ${f.problem}`);
    console.log(`    fix: ${f.fix}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(findings.length === 0 ? `clean: ${targets.length} document(s), references resolve` : `${findings.length} finding(s) across ${targets.length} document(s)`);
    console.log(
      "It resolves references; it does not RUN anything. A documented command whose script exists can\n" +
        "still be broken, and prose claims are not checkable at all. See /viby-toolkit:docs.",
    );
  }
  return findings.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
