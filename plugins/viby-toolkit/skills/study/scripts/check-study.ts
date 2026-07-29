/**
 * viby-toolkit study auditor — the executable half of /viby-toolkit:study.
 *
 * Checks the structural properties of a research document that are mechanically decidable, and
 * refuses to guess at the ones that aren't. It exists because the alternative — asking a model
 * to score its own report on a quality rubric — is measurably unreliable: LLM-defined rubrics
 * misalign with expert judgement, are coarse, and push the judge onto knowledge nobody can
 * verify (see references/methods.md). So this checks form, a human checks substance.
 *
 * Usage:
 *   node check-study.ts <file.md> [more.md...] [--json] [--quiet]
 * Exit: 0 = clean, 1 = findings, 2 = nothing to check.
 *
 * WHAT IT CANNOT DO, stated so a clean run is not mistaken for a verified study: it does no
 * network access, so it cannot tell you whether a URL resolves, whether a quoted sentence really
 * appears at that URL, or whether the source says what you claim. Those are the checks that
 * matter most and they are yours. A clean exit means "the document has the parts a checkable
 * study needs", never "the study is right".
 */
import { parseArgs } from "node:util";
import fs from "node:fs";
import path from "node:path";

export type Finding = {
  line: number;
  check: string;
  severity: "P1" | "P2" | "P3";
  message: string;
  fix: string;
};

/** Hedges that mean "I did not verify this". Harmless in a limitations section; a defect inside
 *  a claim presented as measured. */
const HEDGES = [
  "appears to",
  "appear to",
  "seems to",
  "seem to",
  "presumably",
  "the title suggests",
  "suggests that",
  "likely that",
  "it is likely",
  "probably",
  "i believe",
  "should be roughly",
  "reportedly",
];

/** Section headings whose content is being presented as a measurement. */
const MEASURED_HEADING = /\b(measur|benchmark|result|finding|verified|tested|experiment|data)/i;

/** A quantity strong enough that citing it unsourced is a defect. Bare small integers are not
 *  claims ("2 candidates", "3 angles"), so they do not count. */
const FIGURE =
  /(\d[\d,]*\.?\d*\s?%|\d[\d,]*\.?\d*\s?[×x]\b|\bn\s?=\s?\d+|\$\s?\d[\d,]*|\d[\d,]{4,}\b|\d+\.\d+\s?(ms|s|GB|MB|kB|k tokens|tokens))/;

/** Global — for matchAll only. NEVER call .test() on a /g regex: it advances lastIndex between
 *  calls, so the same input alternates true/false. Use HAS_URL for predicates. */
const URL_RE = /https?:\/\/[^\s)\]<>"']+/g;
const HAS_URL = /https?:\/\//;
const DATE_RE = /\b(20\d{2}-\d{2}-\d{2}|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+20\d{2}|\b20\d{2}\b)/;

/** Dates are not quantities. A year or an ISO date is four digits and would otherwise trip the
 *  bare-integer branch of FIGURE on every properly date-stamped citation — punishing the exact
 *  habit this checker is meant to encourage. */
function blankDates(line: string): string {
  return line
    .replace(/\b20\d{2}-\d{2}-\d{2}\b/g, "")
    .replace(/\b(19|20)\d{2}\b/g, "")
    .replace(/\b\d{1,2}:\d{2}\b/g, "");
}

function domainOf(url: string): string {
  const m = /^https?:\/\/([^/]+)/.exec(url);
  return (m?.[1] ?? "").replace(/^www\./, "").toLowerCase();
}

/**
 * Group lines into blocks — a paragraph, one list item (including its wrapped continuation
 * lines), or one table row. A citation anywhere in the block covers the whole block, which is
 * how a reader attributes it. Line windows do not work here: a three-line bullet whose source
 * sits on its third line reported two unsourced figures, and a checker that fires on correctly
 * sourced prose gets switched off.
 */
export function blockOf(lines: string[]): number[] {
  const owner: number[] = [];
  let start = 0;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    // A blank line ENDS the current block; the next line opens a fresh one. Treating it as the
    // start of a block made every paragraph after a blank line inherit the blank line's index,
    // so no paragraph was ever its own block start and the "sentence introducing this list"
    // lookup silently never found anything.
    if (line.trim() === "") {
      owner.push(i);
      start = i + 1;
      continue;
    }
    if (/^\s*#{1,6}\s/.test(line) || /^\s*([-*+]|\d+\.)\s/.test(line) || /^\s*\|/.test(line) || /^\s*>/.test(line)) {
      start = i;
    }
    owner.push(start);
  }
  return owner;
}

/** Fenced code blocks are not prose: a figure or hedge inside one is example text, not a claim. */
function maskCodeBlocks(lines: string[]): boolean[] {
  const inCode: boolean[] = [];
  let open = false;
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      open = !open;
      inCode.push(true);
      continue;
    }
    inCode.push(open);
  }
  return inCode;
}

export type Mode = "study" | "notes";

/**
 * Which contract to hold this document to.
 *
 * Deliberately decided from the PATH and TITLE, never from the document's content. Inferring it
 * from the presence of a status line would let a study missing its status line silently opt out
 * of every other structural check — one absent field disarming the gates that would have caught
 * it. This repo has shipped that exact class of defect once before.
 */
export function inferMode(file: string, text: string): Mode {
  const norm = file.split(path.sep).join("/");
  // Path wins over title, because a title is prose and prose lies. "Where the study rules come
  // from" is a reference file about studies, not a study — and a decision record is a study
  // under another name. Getting this backwards gives the real study the weaker audit, which is
  // exactly what happened the first time this ran.
  if (/(^|\/)(references|reference|notes|appendix)\//i.test(norm)) return "notes";
  if (/(^|\/)docs\/(studies|research|decisions|adr)\//i.test(norm)) return "study";
  if (/stud(y|ies)|research|spike/i.test(norm.split("/").pop() ?? "")) return "study";
  const h1 = /^#\s+(.*)$/m.exec(text)?.[1] ?? "";
  if (/^\s*(a |the )?(study|research|spike|evaluation|investigation)\b/i.test(h1)) return "study";
  return "notes";
}

/** Evidence that a document's claims rest on a local measurement rather than on citations. */
const LOCAL_EVIDENCE = /\b(oracle|ground truth|fixture|measured (here|on|locally)|our own repo|contract test)/i;

export function checkStudy(text: string, mode: Mode = "study"): Finding[] {
  const lines = text.split("\n");
  const inCode = maskCodeBlocks(lines);
  const owner = blockOf(lines);
  const findings: Finding[] = [];
  const lower = text.toLowerCase();

  // ---- per-line checks
  let currentHeading = "";
  /** The last ordinary sentence before a list or table — its citation covers the block it
   *  introduces. Without this, "All three from [source]:" followed by three bullets reports
   *  three unsourced figures, which trains the reader to ignore the check. */
  let introLine = "";
  /** Last line index of the intro paragraph. An intro only covers the block IMMEDIATELY below
   *  it — without this bound, one URL early in the document silently sourced every list and
   *  table after it, which is source laundering with extra steps. */
  let introEnd = -99;
  /** First line of the current run of list items / table rows. The intro covers the WHOLE run,
   *  not just its first item — otherwise the third bullet of a properly-introduced list is
   *  "unsourced" purely because it is further from the sentence. */
  let runStart = -1;
  const domains = new Set<string>();
  let urlCount = 0;
  let undatedCitations = 0;
  let firstUndated = 1;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    if (inCode[i] === true) continue;
    if (/^\s*#{1,6}\s+/.test(line)) {
      currentHeading = line;
      introLine = "";
      introEnd = -99;
      continue;
    }

    const urls = [...line.matchAll(URL_RE)].map((m) => m[0]);
    for (const u of urls) {
      urlCount += 1;
      domains.add(domainOf(u));
    }

    // A cited source with no date anywhere on its line. Sources rot — a quarter of pages that
    // existed over a recent decade are already gone — so an undated citation may be
    // unverifiable later, and nobody will know when it was true.
    if (urls.length > 0 && !DATE_RE.test(line)) {
      undatedCitations += 1;
      if (undatedCitations === 1) firstUndated = i + 1;
    }

    // A figure with no source on the same line and none on the next (tables and footnotes
    // routinely put the citation in the adjacent cell or line).
    const isBlockItem = /^\s*([-*+]|\d+\.)\s/.test(line) || /^\s*\|/.test(line);
    if (!isBlockItem && line.trim() !== "") {
      if (owner[i] === i) introLine = line;
      introEnd = i;
      runStart = -1; // ordinary prose ends any list/table run
    }
    // The figure's own block (paragraph / list item / table row), plus the sentence introducing
    // the block — but only when that sentence is directly above it.
    const blockStart = owner[i] ?? i;
    let blockEnd = blockStart;
    while (blockEnd + 1 < lines.length && owner[blockEnd + 1] === blockStart) blockEnd += 1;
    if (isBlockItem && runStart === -1) runStart = blockStart;
    const introApplies = isBlockItem && runStart >= 0 && runStart - introEnd <= 2;
    const near = lines.slice(blockStart, blockEnd + 1).join("\n") + (introApplies ? `\n${introLine}` : "");
    if (
      FIGURE.test(blankDates(line)) &&
      !HAS_URL.test(near) &&
      !/\[[^\]]+\]\([^)]+\)|\barXiv\b|\bDOI\b|\bfetched\b|\bmeasured (?:here|locally|on)\b|\bref\b|\[\^?\d+\]|§/i.test(near)
    ) {
      findings.push({
        line: i + 1,
        check: "unsourced-figure",
        severity: "P1",
        message: `a quantity with no source on or next to its line: "${line.trim().slice(0, 90)}"`,
        fix: "attach the source and the quoted sentence, or mark it as your own measurement — an unattributed number is the most repeated citation defect there is",
      });
    }

    // A hedge inside a section that presents itself as measured. The exact prose this repo
    // has produced before when a fetch silently failed: "appears to", "the title suggests".
    if (MEASURED_HEADING.test(currentHeading)) {
      const hit = HEDGES.find((h) => line.toLowerCase().includes(h));
      if (hit !== undefined) {
        findings.push({
          line: i + 1,
          check: "hedged-as-measured",
          severity: "P1",
          message: `"${hit}" inside a section headed ${currentHeading.trim().slice(0, 50)} — hedged language presented as a result`,
          fix: "either verify it and state it plainly, or move it out of the results section and label it inferred / not tested",
        });
      }
    }
  }

  // ---- document-level checks
  const has = (re: RegExp): boolean => re.test(lower);

  if (urlCount > 0 && domains.size === 1) {
    findings.push({
      line: 1,
      check: "single-source",
      severity: "P1",
      message: `every citation points at one domain (${[...domains][0]}) — no triangulation, and if that source is wrong the study is wrong`,
      fix: "find an independent source, or state plainly that the conclusion rests on a single source",
    });
  }
  if (undatedCitations > 0) {
    findings.push({
      line: firstUndated,
      check: "undated-citation",
      severity: "P3",
      message: `${undatedCitations} citation line(s) carry no date — sources rot, and an undated link cannot be re-checked against what it said`,
      fix: "record the date you fetched it alongside the URL",
    });
  }

  // Everything below is the contract for a STUDY. A notes/reference file legitimately has no
  // status line or falsifier, and demanding them there is scope noise, not a finding.
  if (mode === "notes") return findings;

  if (!has(/status[: ]|what was actually done|searched|sources read/)) {
    findings.push({
      line: 1,
      check: "no-status-line",
      severity: "P2",
      message: "no status line saying what was actually done (date, angles searched, sources read vs cited, claims unverified)",
      fix: "add it as the first thing, so a reader can weigh the document before reading it",
    });
  }
  if (!has(/would change (my|the) mind|would change (this|the) (conclusion|answer)|falsif|what would decide/)) {
    findings.push({
      line: 1,
      check: "no-falsifier",
      severity: "P1",
      message: 'no "what would change this conclusion" — an unfalsifiable study cannot be checked or revisited',
      fix: "state the observations that would overturn the answer, before you become attached to it",
    });
  }
  if (!has(/refut|corrected|proved wrong|contradict|i was wrong|prior was/)) {
    findings.push({
      line: 1,
      check: "nothing-refuted",
      severity: "P2",
      message: "nothing recorded as refuted or corrected — a study that changed none of its author's beliefs usually tested none of them",
      fix: "record what the evidence contradicted, including your own starting assumption; if genuinely nothing, say so explicitly",
    });
  }
  if (!has(/stopping rule|saturation|top \d+|exhaust|effort.bounded|searched until/)) {
    findings.push({
      line: 1,
      check: "no-stopping-rule",
      severity: "P2",
      message: "no stated stopping rule, so the reader cannot tell thorough from tired",
      fix: "name it: saturation, effort-bounded (and the N), or exhaustion",
    });
  }
  if (!has(/measured|inferred|not tested|unverified|verified/)) {
    findings.push({
      line: 1,
      check: "no-evidence-labels",
      severity: "P2",
      message: "no measured / inferred / not-tested labelling anywhere — unlabelled inferences get promoted to results by the summary",
      fix: "label each claim at the point it appears, not in a closing caveat",
    });
  }
  if (urlCount === 0) {
    // A study whose evidence is a local measurement legitimately cites nothing external — that
    // is the strongest evidence there is for your own context. It just has to say so.
    const local = LOCAL_EVIDENCE.test(text);
    // Declared outright — nothing left to tell the author.
    if (/local measurement|no external (citation|source)|measured on (this|our) repo/i.test(text)) return findings;
    findings.push({
      line: 1,
      check: "no-sources",
      severity: local ? "P3" : "P1",
      message: local
        ? "no external sources — fine for a locally-measured study, but the reader has to be told that is what this is"
        : "no sources cited at all",
      fix: local
        ? "say in the status line that the evidence is a local measurement, and name the oracle"
        : "if this is genuinely a first-principles argument, say so in the status line — otherwise cite what you read",
    });
  }

  return findings;
}

function main(): number {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    options: {
      json: { type: "boolean", default: false },
      quiet: { type: "boolean", default: false },
      study: { type: "boolean", default: false },
      notes: { type: "boolean", default: false },
    },
  });

  const files = positionals.filter((p) => /\.(md|markdown|txt)$/i.test(p));
  if (files.length === 0) {
    if (values.json) console.log(JSON.stringify({ files: 0, findings: [] }));
    else if (!values.quiet) console.log("no study document given (expects a .md path)");
    return 2;
  }

  const all: Array<Finding & { file: string }> = [];
  const modes = new Map<string, Mode>();
  let read = 0;
  for (const f of files) {
    let text: string;
    try {
      text = fs.readFileSync(f, "utf8");
    } catch {
      all.push({
        file: f,
        line: 1,
        check: "unreadable",
        severity: "P2",
        message: "could not read this file, so it was NOT checked",
        fix: "fix the path and re-run — do not treat this run as clean for it",
      });
      continue;
    }
    read += 1;
    const mode: Mode = values.study ? "study" : values.notes ? "notes" : inferMode(f, text);
    modes.set(f, mode);
    for (const finding of checkStudy(text, mode)) all.push({ ...finding, file: f });
  }

  if (values.json) {
    console.log(JSON.stringify({ files: read, modes: Object.fromEntries(modes), findings: all }, null, 2));
    return all.length > 0 ? 1 : 0;
  }

  const order = { P1: 0, P2: 1, P3: 2 };
  for (const f of all.sort((a, b) => order[a.severity] - order[b.severity] || a.line - b.line)) {
    console.log(`${f.file}:${f.line}  [${f.severity} ${f.check}]`);
    console.log(`    ${f.message}`);
    console.log(`    fix: ${f.fix}`);
  }

  if (!values.quiet) {
    console.log("");
    for (const [file, mode] of modes) {
      // Always say which contract was applied: a clean run in notes mode must never be mistaken
      // for passing the full study contract.
      console.log(
        mode === "study"
          ? `${file}: audited as a STUDY (full contract)`
          : `${file}: audited as NOTES — structural study checks skipped; pass --study to enforce them`,
      );
    }
    console.log(
      all.length === 0
        ? `clean: ${read} document(s) have the parts their contract requires`
        : `${all.length} finding(s) across ${read} document(s)`,
    );
    console.log(
      "This checks FORM, not truth. It does no network access, so it cannot tell you whether a\n" +
        "link resolves, whether a quote is real, or whether the source supports the claim — and\n" +
        "that last one fails often enough to be the main job. See /viby-toolkit:study.",
    );
  }
  return all.length > 0 ? 1 : 0;
}

if (import.meta.filename === process.argv[1]) {
  process.exit(main());
}
