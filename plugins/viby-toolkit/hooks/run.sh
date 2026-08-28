#!/bin/sh
# viby-toolkit TypeScript runner shim.
#
# Hooks and skill scripts are shell commands, so a .ts entrypoint needs a runtime. This
# picks the best one available and — critically — **exits 0 silently when none is found**,
# so a machine without a TS runtime degrades to "no hook" rather than a wedged session.
#
# Order: node (ubiquitous; also runs the test suite) -> bun -> npx tsx -> give up.
#
# Node is probed for CAPABILITY, not version. Type stripping needs Node >= 22.6 built with
# amaro, and distro builds ship a new-enough Node with amaro compiled out: it clears a
# version check and then dies with ERR_NO_TYPESCRIPT. Because the next line is `exec`, that
# death is terminal — bun and tsx are never reached. So the probe runs first and any node
# that cannot strip types falls through to them.
set -u

script="${1:-}"
[ -n "$script" ] || exit 0
shift

if command -v node >/dev/null 2>&1 && node -e '
  const [maj, min] = process.versions.node.split(".").map(Number);
  const newEnough = maj > 22 || (maj === 22 && min >= 6);
  const hasAmaro = process.config.variables.node_use_amaro !== false;
  process.exit(newEnough && hasAmaro ? 0 : 1);
' >/dev/null 2>&1; then
    exec node --experimental-strip-types --disable-warning=ExperimentalWarning "$script" "$@"
fi

if command -v bun >/dev/null 2>&1; then
    exec bun "$script" "$@"
fi

if command -v npx >/dev/null 2>&1 && npx --no-install tsx --version >/dev/null 2>&1; then
    exec npx --no-install tsx "$script" "$@"
fi

# No TS runtime: emit nothing and succeed, so the hook is a no-op.
exit 0
