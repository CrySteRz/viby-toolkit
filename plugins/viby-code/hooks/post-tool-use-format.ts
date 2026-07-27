#!/usr/bin/env -S node --experimental-strip-types
/**
 * viby-code PostToolUse auto-format (OPT-IN — not registered by default).
 *
 * After a Write/Edit, run the file's formatter IF one is already installed and the
 * project actually uses it. Conservative by design:
 *   - Only runs a formatter that exists on PATH (`command -v`) AND has project config
 *     present, so it never imposes a style the project didn't opt into.
 *   - NEVER installs anything (no `npx --yes`, no network).
 *   - NEVER blocks Claude — always exits 0, failures are silent.
 *
 * To enable, add to viby-code's hooks/hooks.json (or your settings.json) a PostToolUse
 * matcher "Write|Edit" running this script via the shim:
 *   sh plugins/viby-code/hooks/run.sh plugins/viby-code/hooks/post-tool-use-format.ts
 * Left off by default because auto-format is a matter of taste and can create surprise
 * diffs.
 */
import { accessSync, constants, existsSync, statSync } from "node:fs";
import { join, extname, delimiter } from "node:path";
import { spawnSync } from "node:child_process";

function run(cmd: string[], cwd: string): void {
  try {
    spawnSync(cmd[0]!, cmd.slice(1), { cwd, timeout: 30_000, stdio: "ignore" });
  } catch {
    // swallow — never block
  }
}

/**
 * Equivalent of Python's `shutil.which(name) is not None`, which checks
 * `exists AND executable AND not a directory`. Plain `existsSync` is not enough: a
 * directory named `prettier` on PATH, or a non-executable file, would claim the tool is
 * installed and make this hook try to format with something that cannot run.
 */
function has(name: string): boolean {
  const pathEnv = process.env.PATH || "";
  for (const dir of pathEnv.split(delimiter)) {
    if (!dir) continue;
    const candidate = join(dir, name);
    try {
      if (!statSync(candidate).isFile()) continue;
      accessSync(candidate, constants.X_OK);
      return true;
    } catch {
      // missing, not a file, or not executable — keep looking
    }
  }
  return false;
}

function anyExists(cwd: string, names: string[]): boolean {
  return names.some((n) => existsSync(join(cwd, n)));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function main(input: string): void {
  const parsed: unknown = JSON.parse(input);
  const d = isRecord(parsed) ? parsed : {};
  const ti = isRecord(d.tool_input) ? d.tool_input : {};
  const fp =
    (typeof ti.file_path === "string" ? ti.file_path : null) ||
    (typeof ti.path === "string" ? ti.path : null);
  const cwd = typeof d.cwd === "string" ? d.cwd : process.cwd();
  if (!fp || !existsSync(fp)) return;
  const ext = extname(fp).toLowerCase();

  const web = new Set([
    ".js",
    ".jsx",
    ".ts",
    ".tsx",
    ".json",
    ".md",
    ".css",
    ".scss",
    ".html",
    ".yaml",
    ".yml",
  ]);

  if (
    web.has(ext) &&
    has("prettier") &&
    anyExists(cwd, [
      ".prettierrc",
      ".prettierrc.json",
      ".prettierrc.js",
      ".prettierrc.cjs",
      ".prettierrc.yaml",
      ".prettierrc.yml",
      "prettier.config.js",
      "prettier.config.cjs",
    ])
  ) {
    run(["prettier", "--write", fp], cwd);
  } else if (ext === ".py") {
    if (has("ruff") && anyExists(cwd, ["pyproject.toml", "ruff.toml", ".ruff.toml"])) {
      run(["ruff", "check", "--fix", "--exit-zero", "-q", fp], cwd);
      run(["ruff", "format", "-q", fp], cwd);
    } else if (has("black") && anyExists(cwd, ["pyproject.toml", "setup.cfg", "tox.ini"])) {
      run(["black", "-q", fp], cwd);
    }
  } else if (ext === ".go" && has("gofmt")) {
    run(["gofmt", "-w", fp], cwd);
  } else if (
    ext === ".rs" &&
    has("rustfmt") &&
    anyExists(cwd, ["Cargo.toml", "rustfmt.toml", ".rustfmt.toml"])
  ) {
    run(["rustfmt", fp], cwd);
  }
}

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk) => {
      data += chunk;
    });
    process.stdin.on("end", () => resolve(data));
    process.stdin.on("error", reject);
  });
}

readStdin()
  .then((input) => {
    try {
      main(input);
    } catch {
      // swallow — never block
    }
    console.log("{}");
  })
  .catch(() => {
    console.log("{}");
  });
