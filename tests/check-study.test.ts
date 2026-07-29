/**
 * Contract tests for the study auditor.
 *
 * Run: node --experimental-strip-types --test tests/check-study.test.ts
 *
 * Both halves pinned. This checker audits prose, which is the most false-positive-prone thing in
 * the repo, so the must-NOT half is deliberately larger than the must half: a hedge in a
 * limitations section is correct writing, a number inside a code block is example text, and a
 * figure whose citation sits in the next table row is sourced.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { checkStudy, inferMode } from "../plugins/viby-toolkit/skills/study/scripts/check-study.ts";

/** A document with every document-level requirement satisfied, so per-line checks can be
 *  tested in isolation without the boilerplate findings firing. */
const WELL_FORMED_HEADER = [
  "# Study",
  "",
  "> Status: measured 2026-07-29 · 5 angles searched · 22 sources read, 9 cited · 2 claims unverified · 1 prior refuted.",
  "",
  "Stopping rule: saturation after two rounds. Every claim below is labelled measured or inferred.",
  "What would change my mind: an independent replication reporting the opposite direction.",
  "The evidence refuted my starting assumption that the cheap path was equivalent.",
  "",
  "Sources: [a](https://example.com/a) fetched 2026-07-29, [b](https://other.org/b) fetched 2026-07-29.",
  "",
].join("\n");

function checks(body: string): string[] {
  return checkStudy(WELL_FORMED_HEADER + body).map((f) => f.check);
}

test("a well-formed study produces no findings", () => {
  const f = checkStudy(WELL_FORMED_HEADER);
  assert.deepEqual(f.map((x) => x.check), [], `expected clean, got ${JSON.stringify(f, null, 1)}`);
});

test("a bare number with no source is flagged", () => {
  const f = checks("\n## Results\n\nThroughput improved by 47% after the change.\n");
  assert.ok(f.includes("unsourced-figure"), f.join() || "none");
});

test("a number whose citation is on the SAME line is not flagged", () => {
  const f = checks("\n## Results\n\nThroughput improved by 47% ([bench](https://example.com/x), 2026-07-29).\n");
  assert.ok(!f.includes("unsourced-figure"), f.join());
});

test("a citation later in the SAME wrapped paragraph covers a figure earlier in it", () => {
  // Prose wraps. A source on the third line of a bullet still sources that bullet, and firing
  // there would make the check unusable on ordinary writing.
  const f = checks(
    "\n## Results\n\n- The independent run reported 14% precision where the vendor claimed\n  98%, which means the two are not measuring\n  the same thing ([write-up](https://example.com/x), 2026-07-29).\n",
  );
  assert.ok(!f.includes("unsourced-figure"), f.join());
});

test("in a table, each row carries its own source — a sourced sibling row does not cover it", () => {
  // Deliberately strict, and it earns its keep: this is the rule that caught a genuinely
  // unsourced row in this repo's own evidence table, which whole-table attribution would have
  // let through. The evidence table is the artifact a reader checks row by row.
  const f = checks(
    "\n## Results\n\n| claim | figure | source |\n|---|---|---|\n| link validity | 94% | [a](https://example.com/a) 2026-07-29 |\n| hallucination rate | 11–57% | reported second-hand |\n",
  );
  assert.ok(f.includes("unsourced-figure"), `the unsourced row must be caught: ${f.join() || "none"}`);
});

test("a number marked as our own measurement is not flagged", () => {
  const f = checks("\n## Results\n\nMeasured here on our repo: 19× fewer tokens than the baseline.\n");
  assert.ok(!f.includes("unsourced-figure"), f.join());
});

test("a number inside a fenced code block is example text, not a claim", () => {
  const f = checks("\n## Results\n\n```\nlatency: 12.5 ms\nsaving: 47%\n```\n");
  assert.ok(!f.includes("unsourced-figure"), f.join());
});

test("small counts are not treated as claims", () => {
  // "3 angles", "2 candidates" — flagging these would make the checker unusable.
  const f = checks("\n## Results\n\nWe compared 3 candidates across 2 workloads and kept 1.\n");
  assert.ok(!f.includes("unsourced-figure"), f.join());
});

test("a list whose introducing sentence carries the source is not flagged", () => {
  // Regression: blank lines were treated as starting a block, so the paragraph after a blank
  // line was never recognised as a block start and the introducing sentence was never found.
  // Every properly-introduced list of figures reported one finding per bullet.
  const f = checks(
    "\n## Results\n\nAll three from [the report](https://example.com/report) (fetched 2026-07-29):\n\n- 38% of pages from one year were gone.\n- 25% of pages over the decade were gone.\n- 54% of entries had a dead link.\n",
  );
  assert.ok(!f.includes("unsourced-figure"), `the intro sentence sources the list: ${f.join()}`);
});

test("a list with NO introducing source is still flagged", () => {
  const f = checks("\n## Results\n\nHere is what we found:\n\n- 38% of pages from one year were gone.\n");
  assert.ok(f.includes("unsourced-figure"), "the must-flag half of the same rule");
});

test("the paragraph directly above counts as the citation for the one below", () => {
  // Technical writing cites once and then discusses. Requiring the URL in the same paragraph as
  // every restatement fires on correctly sourced prose.
  const f = checks(
    "\n## Results\n\nThe monitored run is reported in [the paper](https://example.com/p) (fetched 2026-07-29),\nwhich gives the figures below.\n\nThe hacked rate fell from 28.57% to 0.56% under monitoring.\n",
  );
  assert.ok(!f.includes("unsourced-figure"), `an adjacent citing paragraph sources it: ${f.join()}`);
});

test("a paragraph TWO paragraphs after the citation is still flagged", () => {
  // The bound is what stops one URL from sourcing an entire section.
  const f = checks(
    "\n## Results\n\nSee [the paper](https://example.com/p) (2026-07-29).\n\nSome unrelated discussion with no numbers in it at all.\n\nThe hacked rate fell from 28.57% to 0.56%.\n",
  );
  assert.ok(f.includes("unsourced-figure"), `the bound must hold: ${f.join() || "none"}`);
});

test("a heading resets attribution — a citation does not leak across sections", () => {
  const f = checks(
    "\n## Sources\n\nSee [the paper](https://example.com/p) (2026-07-29).\n\n## Results\n\nThroughput improved 47%.\n",
  );
  assert.ok(f.includes("unsourced-figure"), `attribution must not cross a heading: ${f.join() || "none"}`);
});

test('hedging inside a section headed "Not verified" is correct writing, not a finding', () => {
  // Regression: MEASURED_HEADING matched the word "verified" inside "Not verified", so the most
  // honest paragraph in a document was flagged as a result dressed up in hedges.
  for (const heading of ["## Not verified", "## Limitations", "## Open questions", "## Unverified claims"]) {
    const f = checks(`\n${heading}\n\nThe paper reportedly builds a dataset; the page is paywalled and was never read.\n`);
    assert.ok(!f.includes("hedged-as-measured"), `${heading} must allow hedging, got ${f.join()}`);
  }
});

test("a hedge inside a results section is flagged", () => {
  const f = checks("\n## Measured results\n\nThe index appears to resolve cross-module edges correctly.\n");
  assert.ok(f.includes("hedged-as-measured"), f.join() || "none");
});

test("the same hedge in a limitations section is NOT flagged", () => {
  // Hedging about what you did not verify is honest writing. Flagging it would punish the
  // behaviour this whole skill is trying to encourage.
  const f = checks("\n## Limitations and open questions\n\nThe index appears to resolve cross-module edges, but we did not test it.\n");
  assert.ok(!f.includes("hedged-as-measured"), f.join());
});

test('"the title suggests" in a findings section is flagged — the exact prose a failed fetch produces', () => {
  const f = checks("\n## Findings\n\nWe could not open the PDF; the title suggests it supports our approach.\n");
  assert.ok(f.includes("hedged-as-measured"), f.join() || "none");
});

test("a study with no falsifier is P1", () => {
  const text = [
    "# Study",
    "Status: 2026-07-29, 4 angles, saturation reached. Nothing refuted my prior, stated explicitly.",
    "Claim is measured. Source: [a](https://example.com) 2026-07-29",
  ].join("\n");
  const f = checkStudy(text);
  const hit = f.find((x) => x.check === "no-falsifier");
  assert.ok(hit, `expected no-falsifier, got ${f.map((x) => x.check).join()}`);
  assert.equal(hit.severity, "P1");
});

test("a study citing exactly one domain is flagged as untriangulated", () => {
  const text = WELL_FORMED_HEADER.replace("[b](https://other.org/b)", "[b](https://example.com/b)");
  const f = checkStudy(text).map((x) => x.check);
  assert.ok(f.includes("single-source"), f.join() || "none");
});

test("two independent domains are NOT flagged", () => {
  assert.ok(!checkStudy(WELL_FORMED_HEADER).some((x) => x.check === "single-source"));
});

test("subdomains of the same host still count as one domain", () => {
  const text = WELL_FORMED_HEADER.replace("https://other.org/b", "https://www.example.com/b");
  assert.ok(checkStudy(text).some((x) => x.check === "single-source"), "www. must normalise to the same domain");
});

test("a document with no sources at all is P1", () => {
  const f = checkStudy("# Study\n\nStatus: thought about it. What would change my mind: nothing. Refuted: no.\n");
  const hit = f.find((x) => x.check === "no-sources");
  assert.ok(hit);
  assert.equal(hit.severity, "P1");
});

test("an undated citation is reported, at low severity", () => {
  const text = WELL_FORMED_HEADER + "\n## Extra\n\nSee [c](https://third.net/c) for background.\n";
  const f = checkStudy(text).find((x) => x.check === "undated-citation");
  assert.ok(f, "an undated citation must be reported");
  assert.equal(f.severity, "P3", "undated is a hygiene issue, not a blocker");
});

test("missing stopping rule, status line, refutation and labels are each caught", () => {
  const bare = "# Study\n\nThe answer is yes. What would change my mind: a contrary replication.\n\n[a](https://example.com) 2026-07-29\n";
  const f = checkStudy(bare).map((x) => x.check);
  for (const expected of ["no-status-line", "no-stopping-rule", "nothing-refuted", "no-evidence-labels"]) {
    assert.ok(f.includes(expected), `expected ${expected}, got ${f.join()}`);
  }
});

test("CLI: a clean document exits 0, a defective one exits 1, no document exits 2", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "study", "scripts", "check-study.ts",
  );
  const run = (args: string[]): { status: number | null; stdout: string } => {
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, ...args],
      { encoding: "utf8" },
    );
    return { status: p.status, stdout: p.stdout ?? "" };
  };

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "study-"));
  try {
    const good = path.join(dir, "good.md");
    const bad = path.join(dir, "bad.md");
    fs.writeFileSync(good, WELL_FORMED_HEADER);
    fs.writeFileSync(bad, "# Study\n\nIt is 47% faster.\n");
    assert.equal(run([good, "--quiet"]).status, 0, "a well-formed study must pass");
    assert.equal(run([bad, "--quiet"]).status, 1, "a defective study must fail");
    assert.equal(run(["--quiet"]).status, 2, "no document is exit 2, not a pass");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("CLI: the report states it checks form, not truth", () => {
  // A clean run must never read as "this study is verified" — the checks it cannot do are the
  // ones that matter most.
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "study", "scripts", "check-study.ts",
  );
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "study-"));
  try {
    const good = path.join(dir, "good.md");
    fs.writeFileSync(good, WELL_FORMED_HEADER);
    const p = spawnSync(
      process.execPath,
      ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, good],
      { encoding: "utf8" },
    );
    assert.match(p.stdout ?? "", /checks FORM, not truth/);
    assert.match(p.stdout ?? "", /whether a quote is real/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("mode is decided from path and title, NOT from the document's own fields", () => {
  // The critical property: if the mode were inferred from the presence of a status line, a study
  // that forgot its status line would silently opt out of every other structural check — one
  // missing field disarming the gates meant to catch it.
  const naked = "# Study of caching approaches\n\nno status line here at all\n";
  assert.equal(inferMode("scratch.md", naked), "study", "an H1 saying Study means study mode");
  assert.equal(inferMode("docs/studies/x.md", "# Untitled\n"), "study", "the studies/ path means study mode");
  assert.equal(inferMode("2026-caching-research.md", "# Untitled\n"), "study", "a research filename means study mode");
  assert.equal(inferMode("references/methods.md", "# Where the rules come from\n"), "notes");
});

test("a study missing its status line is still held to the rest of the contract", () => {
  const naked = "# Study of caching\n\nThe answer is yes. See https://example.com and https://other.org\n";
  const f = checkStudy(naked, inferMode("study.md", naked)).map((x) => x.check);
  assert.ok(f.includes("no-status-line"), f.join());
  assert.ok(f.includes("no-falsifier"), `the other checks must still run: ${f.join()}`);
});

test("notes mode keeps the per-line checks but drops the study-structure ones", () => {
  const notes = "# Where the rules come from\n\n## Results\n\nIt is 47% faster.\n";
  const f = checkStudy(notes, "notes").map((x) => x.check);
  assert.ok(f.includes("unsourced-figure"), `sourcing still matters in notes: ${f.join()}`);
  for (const structural of ["no-status-line", "no-falsifier", "nothing-refuted", "no-stopping-rule", "no-sources"]) {
    assert.ok(!f.includes(structural), `${structural} must not fire in notes mode, got ${f.join()}`);
  }
});

test("a locally-measured study with no external sources is P3, not P1", () => {
  // A labelled local measurement is the strongest evidence for your own context; demanding
  // citations for it would push the author toward decorative references.
  const local = [
    "# Study: which extraction approach",
    "Status: measured 2026-07-29 on our own repo against a hand-established oracle. 1 prior refuted.",
    "Stopping rule: exhaustion — three candidates is the whole population.",
    "Ground truth: four fixtures, now contract tests. Labelled measured or inferred throughout.",
    "What would change my mind: a fixture where the chosen approach loses.",
  ].join("\n");
  const hit = checkStudy(local, "study").find((x) => x.check === "no-sources");
  assert.ok(hit, "it should still be mentioned");
  assert.equal(hit.severity, "P3", "a declared local measurement is not a sourcing defect");
  assert.match(hit.message, /locally-measured/);
});

test("a study with no sources AND no local-evidence marker stays P1", () => {
  const vague = [
    "# Study: which approach",
    "Status: I thought about it for a while. Nothing refuted.",
    "Stopping rule: saturation. Claims are inferred.",
    "What would change my mind: better arguments.",
  ].join("\n");
  const hit = checkStudy(vague, "study").find((x) => x.check === "no-sources");
  assert.equal(hit?.severity, "P1", "unsourced opinion is still unsourced");
});

test("an unreadable file is reported, never silently treated as clean", () => {
  const script = path.join(
    path.dirname(import.meta.dirname),
    "plugins", "viby-toolkit", "skills", "study", "scripts", "check-study.ts",
  );
  const p = spawnSync(
    process.execPath,
    ["--experimental-strip-types", "--disable-warning=ExperimentalWarning", script, "/nope/missing.md", "--json"],
    { encoding: "utf8" },
  );
  const parsed = JSON.parse(p.stdout ?? "{}") as { findings: Array<{ check: string }> };
  assert.ok(parsed.findings.some((f) => f.check === "unreadable"), p.stdout);
  assert.equal(p.status, 1);
});
