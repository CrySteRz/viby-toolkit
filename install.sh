#!/usr/bin/env bash
# viby-toolkit portable installer — install on any machine WITHOUT a GitHub account.
#
# Usage: copy this whole folder to the target machine (USB / scp / cloud drive /
# Syncthing), then from inside it run:
#
#     bash install.sh
#
# Re-run it any time to update after copying a newer version of the folder.
# Requires: the Claude Code `claude` CLI on PATH. No GitHub, no login, no network.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MARKET="viby-toolkit"
PLUGIN="viby-toolkit"

# 1. Must be run from the toolkit folder (has the marketplace manifest).
if [ ! -f "$ROOT/.claude-plugin/marketplace.json" ]; then
  echo "✗ Run this from the viby-toolkit folder (missing .claude-plugin/marketplace.json)." >&2
  exit 1
fi

# 2. Need the Claude Code CLI.
if ! command -v claude >/dev/null 2>&1; then
  echo "✗ The 'claude' CLI isn't on PATH. Install Claude Code first, then re-run." >&2
  exit 1
fi

# 3. Warn (do NOT fail) if no TypeScript runtime is present. The skills, agents and
# prompts are plain markdown and work regardless; only the executable extras — the
# statusline and the test-quality scanner — need a runtime. Silence here would mean
# installing a toolkit whose scanner never runs and never says why.
runtime=""
if command -v node >/dev/null 2>&1; then
  node_major=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
  node_minor=$(node -p 'process.versions.node.split(".")[1]' 2>/dev/null || echo 0)
  if [ "$node_major" -gt 22 ] 2>/dev/null || { [ "$node_major" -eq 22 ] && [ "$node_minor" -ge 6 ]; } 2>/dev/null; then
    runtime="node $(node --version)"
  fi
fi
if [ -z "$runtime" ] && command -v bun >/dev/null 2>&1; then
  runtime="bun $(bun --version)"
fi
if [ -z "$runtime" ]; then
  echo "⚠ No TypeScript runtime found (need Node >= 22.6, or bun)."
  echo "  Skills, agents and commands will work fine — they're plain markdown."
  echo "  The statusline and the /viby-toolkit:test scanner will silently no-op until you"
  echo "  install one. Everything else installs normally; continuing."
else
  echo "  ✓ TypeScript runtime: $runtime"
fi

echo "→ Installing $PLUGIN from: $ROOT"

# 3. REFUSE to hijack a marketplace of this name that points somewhere else.
#
# ⚠️ `claude plugin marketplace add` REPLACES a same-named marketplace instead of failing on the
# collision — measured. So on a machine already installed from github:CrySteRz/viby-toolkit, running
# this script silently repointed the marketplace at a local folder and reported "added", and the
# next auto-update pulled from a directory the user had forgotten about instead of from GitHub.
# An earlier version of this script tried to detect that by checking whether `add` FAILED, which it
# never does — the warning was dead code.
#
# The repo is public, so installing from GitHub is the normal path and this folder-based install is
# the deliberate exception (air-gapped machines). An exception should not overwrite the normal path
# by accident, so this refuses unless asked twice.
SETTINGS="$HOME/.claude/settings.json"
read_source() {
  # Prints the registered source for $MARKET, or nothing. Best-effort: no runtime, no check.
  if command -v node >/dev/null 2>&1; then
    node -e '
      const fs = require("fs");
      try {
        const s = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        const m = (s.extraKnownMarketplaces || {})[process.argv[2]];
        if (m && m.source) process.stdout.write(m.source.path || m.source.repo || "");
      } catch {}
    ' "$SETTINGS" "$MARKET" 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c '
import json, sys
try:
    s = json.load(open(sys.argv[1]))
    m = s.get("extraKnownMarketplaces", {}).get(sys.argv[2]) or {}
    src = m.get("source") or {}
    sys.stdout.write(src.get("path") or src.get("repo") or "")
except Exception:
    pass
' "$SETTINGS" "$MARKET" 2>/dev/null
  fi
}

existing="$(read_source)"
if [ -n "$existing" ] && [ "$existing" != "$ROOT" ]; then
  if [ "${VIBY_INSTALL_FORCE_LOCAL:-}" != "1" ]; then
    echo "✗ '$MARKET' is already installed from: $existing" >&2
    echo "  Installing from this folder would REPOINT it at $ROOT and future updates would come" >&2
    echo "  from this directory instead of that source — silently." >&2
    echo >&2
    echo "  If that is what you want (an air-gapped machine, or a local fork), re-run as:" >&2
    echo "      VIBY_INSTALL_FORCE_LOCAL=1 bash install.sh" >&2
    echo "  To keep the current source and just refresh it:" >&2
    echo "      claude plugin marketplace update $MARKET && claude plugin update $PLUGIN@$MARKET" >&2
    exit 1
  fi
  echo "  ! repointing '$MARKET' from $existing to this folder (VIBY_INSTALL_FORCE_LOCAL=1)"
fi

# 3b. Register (or refresh) the LOCAL marketplace — points at this folder, no network.
if claude plugin marketplace add "$ROOT" 2>/dev/null; then
  echo "  ✓ marketplace '$MARKET' points at this folder"
else
  claude plugin marketplace update "$MARKET" >/dev/null 2>&1 || true
  echo "  ✓ marketplace '$MARKET' refreshed"
fi

# 4. Drop this plugin's cached copy so an unchanged version number still re-materialises.
#
# ⚠️ WITHOUT THIS, RE-RUNNING THIS SCRIPT SILENTLY DOES NOTHING — which is what the footer below
# used to promise it did. Two compounding reasons, both measured:
#
#   - `claude plugin install` EXITS 0 as a no-op when the plugin is already installed, so the
#     `else` branch that ran `plugin update` never fired at all.
#   - `claude plugin update` compares VERSION STRINGS and answers "already at the latest version"
#     when the manifest version has not changed, so editing files in this folder and re-running
#     left the cached copy stale. The cache under ~/.claude/plugins/cache is a COPY, not a symlink,
#     so a stale copy is what Claude Code actually loads.
#
# The effect was a fix sitting in this folder, absent from the running agent, with the script
# reporting success. Clearing the cached copy first makes the documented update path real.
CACHE="$HOME/.claude/plugins/cache/$MARKET/$PLUGIN"
if [ -d "$CACHE" ]; then
  rm -rf "$CACHE"
  echo "  ✓ cleared stale cached copy"
fi

# 5. Install (or update) the plugin at user scope so it applies to every project.
# `install` is the idempotent path; `update` runs regardless, because `install` no-ops silently
# when the plugin is already present.
claude plugin install "$PLUGIN@$MARKET" --scope user >/dev/null 2>&1 || true
claude plugin update "$PLUGIN@$MARKET" >/dev/null 2>&1 || true
if [ -d "$CACHE" ]; then
  echo "  ✓ plugin '$PLUGIN' installed at user scope"
else
  echo "  ✗ plugin '$PLUGIN' did not materialise — run 'claude plugin install $PLUGIN@$MARKET --scope user' and read the error" >&2
  exit 1
fi

echo
echo "✓ Done. Restart Claude Code, then type /viby-toolkit: to see the skills."
echo "  Keep this folder around — the plugin loads from it. To update later:"
echo "  copy a newer copy of the folder over this one and re-run: bash install.sh"
