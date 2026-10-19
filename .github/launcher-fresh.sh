#!/usr/bin/env bash
# The launcher on a machine that has never run viby, as Claude Code drives it. Run from the repo
# root by .github/workflows/launcher.yml on each OS a release targets; it also runs on a Mac.
# The release for plugin.json's version must be published (launcher/checksums.txt names it).
#
#   1. SessionStart hook, then MCP servers spawned WITHOUT a shell (as Windows starts them)
#   2. a skill call through the launcher: download, verify, run
#   3. a tampered asset is refused; the statusline still prints
#   4. no download, cargo present: it builds from the plugin's rust/
#   5. no download, no cargo: one line saying what to run; the statusline still prints
set -u

plugin=$PWD/plugins/viby-toolkit
launcher=$plugin/launcher/viby
version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$plugin/.claude-plugin/plugin.json" | head -n 1)
tmp=${RUNNER_TEMP:-$(mktemp -d)}/launcher-fresh
rm -rf "$tmp" && mkdir -p "$tmp"
failed=0
pass() { printf 'PASS  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*"; failed=$((failed + 1)); }

case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) windows=1 exe=viby.exe; native() { cygpath -w "$1"; } ;;
  *) windows='' exe=viby; native() { printf '%s\n' "$1"; } ;;
esac
full_path=$PATH
no_cargo_path=$(printf '%s\n' "$PATH" | tr ':' '\n' | grep -v -e '\.cargo' -e '\.rustup' | paste -sd: -)
if PATH=$no_cargo_path command -v cargo >/dev/null 2>&1; then
  echo "cannot hide cargo: $(PATH=$no_cargo_path command -v cargo)" >&2
  exit 1
fi

# An MCP client: initialize, then tools/list, from a process spawned without a shell.
mcp_probe() { # runtime server
  "$1" -e '
    const { spawn } = require("child_process");
    const [cmd, server] = process.argv.slice(-2);
    const child = spawn(cmd, [server], { stdio: ["pipe", "pipe", "inherit"] });
    let out = "";
    const done = (code, why) => { if (why) console.error(why, out); child.kill(); process.exit(code); };
    child.on("error", (e) => done(1, "spawn failed: " + e.message));
    child.on("exit", (c) => done(1, "exited " + c));
    child.stdout.on("data", (d) => { out += d; if (/"id":2,/.test(out) && /"tools"/.test(out)) done(0); });
    const send = (m) => child.stdin.write(JSON.stringify({ jsonrpc: "2.0", ...m }) + "\n");
    send({ id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "probe", version: "0" } } });
    send({ method: "notifications/initialized" });
    send({ id: 2, method: "tools/list" });
    setTimeout(() => done(1, "timed out"), 30000);
  ' -- "$(native "$launcher")" "$2"
}

echo "== 1. SessionStart, then MCP servers spawned without a shell"
export CLAUDE_PLUGIN_DATA=$tmp/hook-data
if PATH=$no_cargo_path sh "$plugin/hooks/session-start.sh" | node -e 'JSON.parse(require("fs").readFileSync(0, "utf8"))'; then
  pass "SessionStart prints valid JSON"
else
  fail "SessionStart output is not JSON"
fi
if [ -n "$windows" ]; then
  if [ -f "$plugin/launcher/viby.exe" ]; then pass "the hook put viby.exe beside the launcher"; else fail "no launcher/viby.exe after the hook"; fi
fi
for runtime in node bun; do
  command -v "$runtime" >/dev/null 2>&1 || { echo "skip: no $runtime"; continue; }
  for server in agentgrep-server browser-server; do
    if PATH=$no_cargo_path mcp_probe "$runtime" "$server" >"$tmp/mcp.out" 2>&1; then
      pass "$runtime spawns launcher/viby $server; it lists its tools"
    else
      fail "$runtime spawn of launcher/viby $server: $(tail -c 400 "$tmp/mcp.out")"
    fi
  done
done

echo "== 2. A skill call on a fresh machine (no cargo)"
export CLAUDE_PLUGIN_DATA=$tmp/fresh
if out=$(PATH=$no_cargo_path sh "$launcher" detect-stack . 2>&1) && printf '%s' "$out" | grep -q 'languages'; then
  pass "downloaded, verified and ran detect-stack"
else
  fail "fresh download: $out"
fi
if [ -x "$CLAUDE_PLUGIN_DATA/bin/$version/$exe" ]; then pass "cached at bin/$version/$exe"; else fail "nothing at bin/$version/$exe"; fi
if [ -n "$windows" ]; then
  # SmartScreen judges files carrying the Mark of the Web (a Zone.Identifier stream); curl sets none.
  zone=$(powershell -NoProfile -Command "Get-Item -LiteralPath '$(native "$CLAUDE_PLUGIN_DATA/bin/$version/$exe")' -Stream * | ForEach-Object Stream" 2>&1)
  if printf '%s' "$zone" | grep -q Zone.Identifier; then fail "viby.exe carries a Mark of the Web: $zone"; else pass "viby.exe carries no Mark of the Web (streams: $(printf '%s' "$zone" | tr '\n' ' '))"; fi
fi

echo "== 3. A tampered asset is refused"
mkdir -p "$tmp/srv/v$version"
for t in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-pc-windows-msvc; do
  printf 'not viby\n' | gzip >"$tmp/srv/v$version/viby-$t.tar.gz"
done
python=$(command -v python3 || command -v python)
(cd "$tmp/srv" && exec "$python" -m http.server 8765 --bind 127.0.0.1 >/dev/null 2>&1) &
server_pid=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do curl -fs -o /dev/null http://127.0.0.1:8765/ && break; sleep 1; done
export CLAUDE_PLUGIN_DATA=$tmp/tampered VIBY_RELEASE_URL=http://127.0.0.1:8765
out=$(PATH=$no_cargo_path sh "$launcher" detect-stack . 2>&1)
code=$?
if [ "$code" = 1 ] && printf '%s' "$out" | grep -q 'refused viby-'; then pass "refused, exit 1"; else fail "tampered asset: exit $code: $out"; fi
if [ ! -e "$CLAUDE_PLUGIN_DATA/bin/$version/$exe" ]; then pass "nothing installed"; else fail "a tampered binary was installed"; fi
line=$(echo '{}' | PATH=$no_cargo_path sh "$launcher" statusline 2>/dev/null)
code=$?
if [ "$code" = 0 ] && [ "$line" = viby-toolkit ]; then pass "statusline prints viby-toolkit, exit 0"; else fail "statusline: exit $code: $line"; fi
kill "$server_pid" 2>/dev/null; wait "$server_pid" 2>/dev/null

echo "== 4. No download, cargo present: build from source"
export CLAUDE_PLUGIN_DATA=$tmp/build VIBY_RELEASE_URL=http://127.0.0.1:9
start=$(date +%s)
out=$(PATH=$full_path sh "$launcher" detect-stack . 2>&1)
code=$?
if [ "$code" = 0 ] && printf '%s' "$out" | grep -q 'building viby' && printf '%s' "$out" | grep -q languages; then
  pass "built with cargo and ran, $(($(date +%s) - start)) s"
else
  fail "cargo fallback: exit $code: $(printf '%s' "$out" | tail -c 1500)"
fi

echo "== 5. No download, no cargo"
export CLAUDE_PLUGIN_DATA=$tmp/neither
out=$(PATH=$no_cargo_path sh "$launcher" detect-stack . 2>&1)
code=$?
if [ "$code" = 1 ] && printf '%s' "$out" | grep -q 'sh.rustup.rs'; then pass "one line to act on, exit 1"; else fail "no cargo: exit $code: $out"; fi
line=$(echo '{}' | PATH=$no_cargo_path sh "$launcher" statusline 2>/dev/null)
code=$?
if [ "$code" = 0 ] && [ "$line" = viby-toolkit ]; then pass "statusline prints viby-toolkit, exit 0"; else fail "statusline: exit $code: $line"; fi

echo
if [ "$failed" = 0 ]; then echo "all passed"; else echo "$failed failed"; fi
[ "$failed" = 0 ]
