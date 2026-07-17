#!/usr/bin/env python3
"""forge statusline — makes the context-discipline number visible.

Reads Claude Code's statusline JSON on stdin and prints one line:
  <model> · ctx NN% (green<60, yellow<80, red) · cache HH% · $C.CC
Context % is measured against the auto-compact threshold (what the 40-60%
discipline actually cares about), not the raw window. Fully generic; never
blocks. On any error it prints a minimal line so the statusline never breaks.

Wire it up (this file is not auto-registered — statuslines live in settings.json):
  "statusLine": { "type": "command",
                  "command": "python3 \"$HOME/.claude/plugins/cache/ionut-toolkit/forge/<ver>/hooks/statusline.py\"" }
Or just use `bunx ccusage statusline`. See the README telemetry section.
"""
import sys, json

def main():
    d = json.load(sys.stdin)
    model = (d.get("model") or {}).get("display_name") or (d.get("model") or {}).get("id") or "claude"

    cw = d.get("context_window") or {}
    usage = cw.get("current_usage") or {}
    inp = usage.get("input_tokens", 0) or 0
    cc = usage.get("cache_creation_input_tokens", 0) or 0
    cr = usage.get("cache_read_input_tokens", 0) or 0
    ctx_tokens = inp + cc + cr

    pct = cw.get("used_percentage")
    if pct is None:
        cap = cw.get("context_window_size") or 200000
        pct = (ctx_tokens * 100.0 / cap) if cap else 0
    pct = round(pct)

    # cache-hit share of input — validates prompt-cache reuse
    total_in = inp + cc + cr
    cache_pct = round(cr * 100.0 / total_in) if total_in else 0

    cost = (d.get("cost") or {}).get("total_cost_usd")

    # color the context number by band (ANSI); green under 60, yellow under 80, red beyond
    if pct < 60:   ctx = f"\033[32mctx {pct}%\033[0m"
    elif pct < 80: ctx = f"\033[33mctx {pct}%\033[0m"
    else:          ctx = f"\033[31mctx {pct}%\033[0m"

    parts = [f"\033[2m{model}\033[0m", ctx]
    if total_in:
        parts.append(f"\033[2mcache {cache_pct}%\033[0m")
    if isinstance(cost, (int, float)):
        parts.append(f"\033[2m${cost:.2f}\033[0m")
    print(" · ".join(parts))

if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("forge")
