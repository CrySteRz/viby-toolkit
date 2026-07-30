#!/usr/bin/env -S node --experimental-strip-types
/**
 * The manifest version must not lag the newest git tag.
 *
 * Cut two tags in one session (v2.21.1, v2.21.2) without bumping plugin.json, which still said
 * 2.21.0. The marketplace serves the MANIFEST, not the tag — so both tags would have installed as
 * 2.21.0, and `claude plugin update` would have reported "already at the latest version" while the
 * user sat on older code. The tag said one thing and the thing you install said another.
 *
 * Exit: 0 = manifest >= newest tag, 1 = manifest is behind, 0 with a note if git/tags unavailable.
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..");

function parse(v: string): number[] {
  return (v.replace(/^v/, "").split(".").map((n) => Number.parseInt(n, 10)) ?? []).map((n) => (Number.isNaN(n) ? 0 : n));
}

function cmp(a: number[], b: number[]): number {
  for (let i = 0; i < 3; i += 1) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

function main(): number {
  let manifest: string;
  try {
    manifest = JSON.parse(readFileSync(join(ROOT, "plugins", "viby-toolkit", ".claude-plugin", "plugin.json"), "utf8")).version;
  } catch {
    console.log("cannot read plugin.json — skipping");
    return 0;
  }
  let newestTag: string;
  try {
    newestTag = execFileSync("git", ["tag", "--sort=-v:refname"], { cwd: ROOT, encoding: "utf8" })
      .split("\n")
      .map((t) => t.trim())
      .filter((t) => /^v\d+\.\d+\.\d+$/.test(t))[0] ?? "";
  } catch {
    console.log("git unavailable — skipping");
    return 0;
  }
  if (newestTag === "") {
    console.log("no version tags yet — nothing to compare");
    return 0;
  }
  const d = cmp(parse(manifest), parse(newestTag));
  if (d < 0) {
    console.log(
      `plugin.json is ${manifest}, but the newest tag is ${newestTag}.\n` +
        `The marketplace serves the MANIFEST, so ${newestTag} would install as ${manifest} — and\n` +
        `\`claude plugin update\` would say "already at the latest version" while shipping older code.\n` +
        `Bump plugin.json to ${newestTag.replace(/^v/, "")} or higher BEFORE tagging.`,
    );
    return 1;
  }
  console.log(`plugin.json ${manifest} >= newest tag ${newestTag}`);
  return 0;
}

process.exit(main());
