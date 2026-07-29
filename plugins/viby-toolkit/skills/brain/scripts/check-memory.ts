/**
 * viby-toolkit memory auditor — the executable half of /viby-toolkit:brain.
 *
 * A memory store degrades silently. Nothing errors when a memory goes stale, contradicts another,
 * or becomes unfindable — it just quietly starts misleading every future session, with the
 * authority of something the agent "knows". This audits the store itself.
 *
 * Usage:
 *   node check-memory.ts <memory-dir> [--root <repo-root>] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = no memory found.
 *
 * WHY THESE CHECKS. Three findings from the agent-memory literature shape the rule set:
 *
 *  1. **Retrieval failure is where the errors are.** Across LongMemEval, LoCoMo, STALE and
 *     PersonaMem, answer errors concentrate in cases where retrieval failed; retrieval succeeding
 *     but the answer still being wrong accounts for only 5.8–13.7%. So the highest-value property
 *     of a memory store is that entries are *findable* — hence the index checks.
 *  2. **Staleness is the hard, under-measured failure.** A dedicated benchmark exists for exactly
 *     this question ("can agents know when their memories are no longer valid?") and the noted gap
 *     is that benchmarks "rarely isolate whether a model can determine that a previously valid
 *     memory has been rendered obsolete". In a coding project the obsolete memory is usually
 *     mechanically detectable: it cites a path that no longer exists.
 *  3. **Poisoning is defended with provenance.** The proposed defense is reliability-conditional
 *     updating with a provenance cap — an entry's influence bounded by how well-sourced it is. That
 *     requires every entry to carry provenance at all, which is checkable.
 *
 * WHAT IT CANNOT DO: it cannot tell you a memory is *true*, cannot detect a contradiction phrased
 * differently from its counterpart, and cannot know which of two conflicting entries is current. A
 * clean run means the store is well-formed and its references resolve — not that it is correct.
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  file: string;
  line: number;
  check: string;
  severity: "P1" | "P2" | "P3";
  problem: string;
  fix: string;
};

const DATE = /\b(20\d{2}-\d{2}-\d{2}|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+20\d{2})\b/;
/** How the entry knows what it claims. */
const PROVENANCE =
  /\b(measured|verified|confirmed|tested|reproduced|observed|ran\b|source:|per the|according to|from the (docs?|changelog|error|logs?)|file:line|because|found (that|out)|turns out|it was|the error|the fix|documented in|see \[|https?:\/\/)/i;
/** A claim that rests only on assertion. Sycophantic memory is storing what was said as if true. */
const HEARSAY = /\b(the user (said|says|thinks|believes|mentioned)|apparently|I think|probably|seems? like|assume[ds]?)\b/i;

/** A path reference worth resolving: backticked, has a slash, has an extension. */
const PATH_REF = /`([~\w./-]*\/[\w.-]+\.[a-z]{1,5})(?::\d+)?`/gi;
/** Placeholders that are illustrations, not references. */
const PLACEHOLDER = /(^|\/)(foo|bar|baz|example|sample|your|my|path|dir|some|thing|todo|xxx)([./]|$)|[<>*]|\.\.\./i;

/**
 * Candidate absolute paths for a reference. A memory about a project naturally cites paths relative
 * to the repo root, to a package subdirectory, or to home — so a single assumed root produces false
 * "stale" findings on perfectly good references. Try the root and two ancestors before concluding a
 * path is gone.
 */
function candidates(ref: string, root: string): string[] {
  if (PLACEHOLDER.test(ref)) return [];
  if (ref.startsWith("~/")) return [path.join(process.env.HOME ?? "", ref.slice(2))];
  if (ref.startsWith("/")) return [ref];
  return [path.join(root, ref), path.join(root, "..", ref), path.join(root, "..", "..", ref)];
}

function existsAnywhere(ref: string, root: string): boolean | null {
  const c = candidates(ref, root);
  if (c.length === 0) return null;
  return c.some((p) => fs.existsSync(p));
}

export function auditEntry(file: string, text: string, root: string): Finding[] {
  const findings: Finding[] = [];
  const lines = text.split("\n");
  const body = text.replace(/^---[\s\S]*?\n---\n/, "");

  // 1. Stale references — the mechanically detectable half of the staleness problem.
  //
  // SELF-CALIBRATING, because the first run of this check was confidently wrong. A memory store's
  // relative paths belong to ITS project, and auditing five stores against one root reported 23
  // stale references that were simply resolved against the wrong tree. So: if NOTHING in a file
  // resolves, the root is wrong, not the memory — say that instead of inventing findings.
  const refs: Array<{ ref: string; line: number; exists: boolean }> = [];
  for (let i = 0; i < lines.length; i += 1) {
    for (const m of (lines[i] ?? "").matchAll(PATH_REF)) {
      const ref = m[1];
      if (ref === undefined) continue;
      const exists = existsAnywhere(ref, root);
      if (exists !== null) refs.push({ ref, line: i + 1, exists });
    }
  }
  const resolvable = refs.filter((r) => r.exists);
  const rootLooksRight = refs.length === 0 || resolvable.length > 0;
  if (!rootLooksRight) {
    findings.push({
      file,
      line: refs[0]?.line ?? 1,
      check: "root-unknown",
      severity: "P3",
      problem: `none of its ${refs.length} path reference(s) resolve against the given root, so staleness could NOT be checked — the root is probably wrong for this store`,
      fix: "re-run with --root pointing at the project this memory belongs to",
    });
  }
  for (const r of rootLooksRight ? refs : []) {
    {
      if (!r.exists) {
        const ref = r.ref;
        const i = r.line - 1;
        findings.push({
          file,
          line: i + 1,
          check: "stale-reference",
          severity: "P1",
          problem: `cites \`${ref}\`, which does not exist — a memory pointing at a moved or deleted path misleads with the authority of something the agent "knows"`,
          fix: "update the reference, or delete the entry if the thing it describes is gone; a memory that cannot be checked cannot be trusted",
        });
      }
    }
  }

  // 2. Provenance and dating — the poisoning defense requires both.
  if (!DATE.test(text)) {
    findings.push({
      file,
      line: 1,
      check: "undated",
      severity: "P2",
      problem: "carries no date, so nothing can decide whether it has been superseded",
      fix: "add the date it was established; a memory without a timestamp can never be retired on evidence",
    });
  }
  if (!PROVENANCE.test(body)) {
    findings.push({
      file,
      line: 1,
      check: "no-provenance",
      severity: "P2",
      problem: "states facts without saying how they were established, so a guess and an executed check carry equal weight in future sessions",
      fix: "say how you know — measured, verified by running X, observed in the logs, stated by the user",
    });
  }
  const hearsay = lines.findIndex((l) => HEARSAY.test(l) && !PROVENANCE.test(l));
  if (hearsay !== -1) {
    findings.push({
      file,
      line: hearsay + 1,
      check: "unverified-claim",
      severity: "P2",
      problem: "records something asserted rather than established — storing what was said as if it were true is how a memory store becomes sycophantic",
      fix: "verify it and record the check, or label it explicitly as a claim rather than a fact",
    });
  }

  // 3. Size — a memory that grew into a document defeats retrieval, because it all comes back.
  const words = body.split(/\s+/).filter(Boolean).length;
  if (words > 900) {
    findings.push({
      file,
      line: 1,
      check: "oversized-entry",
      severity: "P2",
      problem: `${words} words in one entry — it has become a document, and retrieving any part of it drags the whole thing into context`,
      fix: "split it by topic into separate entries that can be recalled independently, and link them",
    });
  }

  return findings;
}

export function auditStore(dir: string, root: string): { findings: Finding[]; entries: number } {
  let names: string[];
  try {
    names = fs.readdirSync(dir).filter((f) => f.endsWith(".md"));
  } catch {
    return { findings: [], entries: 0 };
  }
  const findings: Finding[] = [];
  const indexName = names.find((n) => n.toUpperCase() === "MEMORY.MD");
  const entries = names.filter((n) => n !== indexName);

  for (const n of entries) {
    const full = path.join(dir, n);
    try {
      findings.push(...auditEntry(n, fs.readFileSync(full, "utf8"), root));
    } catch {
      /* unreadable */
    }
  }

  // 4. Findability. Retrieval failure is where memory errors concentrate, so an entry missing from
  //    the index is the highest-severity structural defect in a store.
  if (indexName === undefined) {
    if (entries.length > 0) {
      findings.push({
        file: "(store)",
        line: 1,
        check: "no-index",
        severity: "P1",
        problem: `${entries.length} entr(ies) and no MEMORY.md index — nothing tells a new session what is in here`,
        fix: "add MEMORY.md with one line per entry: title, link, and a hook describing when it is relevant",
      });
    }
  } else {
    const index = fs.readFileSync(path.join(dir, indexName), "utf8");
    for (const n of entries) {
      if (!index.includes(n)) {
        findings.push({
          file: n,
          line: 1,
          check: "not-indexed",
          severity: "P1",
          problem: "exists but is not listed in MEMORY.md, so it will not be found when it matters — and retrieval failure, not reasoning, is where memory errors concentrate",
          fix: "add a one-line pointer to MEMORY.md",
        });
      }
    }
    for (const m of index.matchAll(/\(([\w.-]+\.md)\)/g)) {
      const target = m[1];
      if (target !== undefined && !entries.includes(target) && target.toUpperCase() !== "MEMORY.MD") {
        findings.push({
          file: indexName,
          line: 1,
          check: "index-dangling",
          severity: "P1",
          problem: `the index links \`${target}\`, which does not exist — a session told to read it finds nothing`,
          fix: "fix the link or remove the line",
        });
      }
    }
  }

  // 5. Duplicate topics — real stores need to merge duplicates, and near-identical names are the
  //    detectable case.
  for (let i = 0; i < entries.length; i += 1) {
    for (let j = i + 1; j < entries.length; j += 1) {
      const a = entries[i];
      const b = entries[j];
      if (a === undefined || b === undefined) continue;
      const ta = new Set(a.replace(/\.md$/, "").split(/[-_]/).filter((w) => w.length > 2));
      const tb = new Set(b.replace(/\.md$/, "").split(/[-_]/).filter((w) => w.length > 2));
      if (ta.size === 0 || tb.size === 0) continue;
      const shared = [...ta].filter((w) => tb.has(w)).length;
      const ratio = shared / Math.min(ta.size, tb.size);
      if (ratio >= 0.75) {
        findings.push({
          file: `${a} + ${b}`,
          line: 1,
          check: "duplicate-topic",
          severity: "P2",
          problem: "two entries cover nearly the same topic — when they drift apart, a future session gets whichever it happens to retrieve",
          fix: "merge them into one entry, or make the distinction explicit in both names",
        });
      }
    }
  }

  return { findings, entries: entries.length };
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: { root: { type: "string" }, json: { type: "boolean", default: false }, quiet: { type: "boolean", default: false } },
  });
  if (positionals.length === 0) {
    if (!values.quiet) console.log("usage: check-memory.ts <memory-dir> [--root <repo-root>]");
    return 2;
  }
  const root = values.root ?? process.cwd();
  const all: Finding[] = [];
  let entries = 0;
  for (const d of positionals) {
    const r = auditStore(d, root);
    entries += r.entries;
    all.push(...r.findings.map((f) => ({ ...f, file: `${path.basename(d)}/${f.file}` })));
  }

  if (entries === 0) {
    if (values.json) console.log(JSON.stringify({ entries: 0, findings: [] }));
    else if (!values.quiet) console.log("no memory entries found");
    return 2;
  }

  if (values.json) {
    console.log(JSON.stringify({ entries, findings: all }, null, 2));
    return all.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of all.sort((a, b) => order[a.severity] - order[b.severity] || a.file.localeCompare(b.file))) {
    console.log(`${f.file}${f.line > 1 ? `:${f.line}` : ""}  [${f.severity} ${f.check}]`);
    console.log(`    ${f.problem}`);
    console.log(`    fix: ${f.fix}`);
  }
  if (!values.quiet) {
    console.log("");
    console.log(all.length === 0 ? `clean: ${entries} entr(ies), well-formed and resolvable` : `${all.length} finding(s) across ${entries} entr(ies)`);
    console.log(
      "Form and resolvability only. It cannot tell you a memory is TRUE, cannot see a contradiction\n" +
        "phrased differently from its counterpart, and cannot know which of two conflicting entries is\n" +
        "current. See /viby-toolkit:brain.",
    );
  }
  return all.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
