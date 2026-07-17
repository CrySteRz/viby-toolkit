#!/usr/bin/env python3
"""forge PostToolUse auto-format (OPT-IN — not registered by default).

After a Write/Edit, run the file's formatter IF one is already installed and the
project actually uses it. Conservative by design:
  - Only runs a formatter that exists on PATH (`command -v`) AND has project config
    present, so it never imposes a style the project didn't opt into.
  - NEVER installs anything (no `npx --yes`, no network).
  - NEVER blocks Claude — always exits 0, failures are silent.

To enable, add to forge's hooks/hooks.json (or your settings.json) a PostToolUse
matcher "Write|Edit" running this script. Left off by default because auto-format
is a matter of taste and can create surprise diffs.
"""
import sys, os, json, shutil, subprocess

def run(cmd, cwd):
    try:
        subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=30)
    except Exception:
        pass

def has(name):
    return shutil.which(name) is not None

def any_exists(cwd, names):
    return any(os.path.exists(os.path.join(cwd, n)) for n in names)

def main():
    d = json.load(sys.stdin)
    ti = d.get("tool_input") or {}
    fp = ti.get("file_path") or ti.get("path")
    cwd = d.get("cwd") or os.getcwd()
    if not fp or not os.path.exists(fp):
        return
    ext = os.path.splitext(fp)[1].lower()

    web = {".js", ".jsx", ".ts", ".tsx", ".json", ".md", ".css", ".scss", ".html", ".yaml", ".yml"}
    if ext in web and has("prettier") and any_exists(cwd, [
        ".prettierrc", ".prettierrc.json", ".prettierrc.js", ".prettierrc.cjs",
        ".prettierrc.yaml", ".prettierrc.yml", "prettier.config.js", "prettier.config.cjs",
    ]):
        run(["prettier", "--write", fp], cwd)
    elif ext == ".py":
        if has("ruff") and any_exists(cwd, ["pyproject.toml", "ruff.toml", ".ruff.toml"]):
            run(["ruff", "check", "--fix", "--exit-zero", "-q", fp], cwd)
            run(["ruff", "format", "-q", fp], cwd)
        elif has("black") and any_exists(cwd, ["pyproject.toml", "setup.cfg", "tox.ini"]):
            run(["black", "-q", fp], cwd)
    elif ext == ".go" and has("gofmt"):
        run(["gofmt", "-w", fp], cwd)
    elif ext == ".rs" and has("rustfmt") and any_exists(cwd, ["Cargo.toml", "rustfmt.toml", ".rustfmt.toml"]):
        run(["rustfmt", fp], cwd)

if __name__ == "__main__":
    try:
        main()
    except Exception:
        pass
    print("{}")
    sys.exit(0)
