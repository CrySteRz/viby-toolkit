#!/bin/sh
# viby-toolkit TypeScript runner shim.
#
# Hooks and skill scripts are shell commands, so a .ts entrypoint needs a runtime. This
# picks the best one available and — critically — **exits 0 silently when none is found**,
# so a machine without a TS runtime degrades to "no hook" rather than a wedged session.
#
# Order: node (ubiquitous; also runs the test suite) -> bun -> npx tsx -> give up.
# Type stripping needs Node >= 22.6; older Node falls through to the next option.
#
# Usage: sh run.sh <script.ts> [args...]
set -u

script="${1:-}"
[ -n "$script" ] || exit 0
shift

if command -v node >/dev/null 2>&1; then
    major=$(node -p 'process.versions.node.split(".")[0]' 2>/dev/null || echo 0)
    minor=$(node -p 'process.versions.node.split(".")[1]' 2>/dev/null || echo 0)
    if [ "$major" -gt 22 ] 2>/dev/null || { [ "$major" -eq 22 ] && [ "$minor" -ge 6 ]; } 2>/dev/null; then
        exec node --experimental-strip-types --disable-warning=ExperimentalWarning "$script" "$@"
    fi
fi

if command -v bun >/dev/null 2>&1; then
    exec bun "$script" "$@"
fi

if command -v npx >/dev/null 2>&1 && npx --no-install tsx --version >/dev/null 2>&1; then
    exec npx --no-install tsx "$script" "$@"
fi

# No TS runtime: emit nothing and succeed, so the hook is a no-op.
exit 0
