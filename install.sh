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
