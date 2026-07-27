#!/usr/bin/env bash
# viby-code portable installer — install on any machine WITHOUT a GitHub account.
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
PLUGIN="viby-code"

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
  echo "  The statusline and the /viby-code:test scanner will silently no-op until you"
  echo "  install one. Everything else installs normally; continuing."
else
  echo "  ✓ TypeScript runtime: $runtime"
fi

echo "→ Installing $PLUGIN from: $ROOT"

# 3. Register (or refresh) the LOCAL marketplace — points at this folder, no network.
if claude plugin marketplace add "$ROOT" 2>/dev/null; then
  echo "  ✓ marketplace '$MARKET' added"
else
  claude plugin marketplace update "$MARKET" >/dev/null 2>&1 || true
  echo "  ✓ marketplace '$MARKET' already present — refreshed"
fi

# 4. Install (or update) the plugin at user scope so it applies to every project.
if claude plugin install "$PLUGIN@$MARKET" --scope user 2>/dev/null; then
  echo "  ✓ plugin '$PLUGIN' installed"
else
  claude plugin update "$PLUGIN@$MARKET" >/dev/null 2>&1 || true
  echo "  ✓ plugin '$PLUGIN' already installed — updated"
fi

echo
echo "✓ Done. Restart Claude Code, then type /viby-code: to see the skills."
echo "  Keep this folder around — the plugin loads from it. To update later:"
echo "  copy a newer copy of the folder over this one and re-run: bash install.sh"
