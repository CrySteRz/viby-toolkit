#!/usr/bin/env python3
"""viby-code statusline — makes the two resources that actually bind visible.

Prints one line:
  <model> · ctx NN% · cache NN% · 5h NN% · 7d NN% · $C.CC

  ctx    percentage of the context window in use (context_window.used_percentage,
         which is input-only: input + cache_creation + cache_read). Colour-banded
         green <60 / yellow <80 / red, matching the 40-60% target the toolkit aims for.
  cache  cache-read share of input tokens — confirms the prompt cache is being reused.
         On a subscription, cache hits are the cheapest tokens you have.
  5h/7d  rate-limit consumption (rate_limits.five_hour / .seven_day). On a Max plan the
         scarce resources are context and rate limit, not dollars — so these matter more
         than the cost figure. Only present for Pro/Max, after the first API response.
  $      client-side session cost estimate; omitted when not reported.

Fully generic; never blocks. On any error it prints a minimal line so the statusline
never breaks. Fields that are absent or null (early in a session, or right after
/compact) are simply skipped rather than shown as 0.

Wire it up (statuslines live in settings.json, not in plugin hooks.json). This form
survives plugin version bumps by globbing the cache directory:

  "statusLine": {
    "type": "command",
    "command": "python3 \"$(ls -d \"$HOME\"/.claude/plugins/cache/viby-toolkit/viby-code/*/hooks/statusline.py | tail -1)\""
  }

Or point it straight at your checkout: "$HOME/Projects/Personal/viby-toolkit/plugins/viby-code/hooks/statusline.py"
"""
import sys, json

DIM = "\033[2m"
RESET = "\033[0m"
GREEN, YELLOW, RED = "\033[32m", "\033[33m", "\033[31m"


def band(pct, low=60, high=80):
    """Colour a percentage: green below `low`, yellow below `high`, red beyond."""
    colour = GREEN if pct < low else (YELLOW if pct < high else RED)
    return colour, RESET


def pct_of(node, key="used_percentage"):
    """Read a percentage field, returning None when absent or null."""
    if not isinstance(node, dict):
        return None
    v = node.get(key)
    if v is None:
        return None
    try:
        return round(float(v))
    except (TypeError, ValueError):
        return None


def main():
    d = json.load(sys.stdin)
    model = (d.get("model") or {}).get("display_name") or (d.get("model") or {}).get("id") or "claude"
    parts = [f"{DIM}{model}{RESET}"]

    cw = d.get("context_window") or {}

    # --- context window
    pct = pct_of(cw)
    if pct is None:
        usage = cw.get("current_usage") or {}
        used = sum((usage.get(k) or 0) for k in
                   ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"))
        cap = cw.get("context_window_size") or 0
        pct = round(used * 100.0 / cap) if (cap and used) else None
    if pct is not None:
        c, r = band(pct)
        parts.append(f"{c}ctx {pct}%{r}")

    # --- prompt-cache reuse share
    usage = cw.get("current_usage") or {}
    inp = usage.get("input_tokens") or 0
    cc = usage.get("cache_creation_input_tokens") or 0
    cr = usage.get("cache_read_input_tokens") or 0
    total_in = inp + cc + cr
    if total_in:
        parts.append(f"{DIM}cache {round(cr * 100.0 / total_in)}%{RESET}")

    # --- rate limits: the real ceiling on a subscription
    rl = d.get("rate_limits") or {}
    for label, key in (("5h", "five_hour"), ("7d", "seven_day")):
        rpct = pct_of(rl.get(key))
        if rpct is not None:
            c, r = band(rpct, low=50, high=75)
            parts.append(f"{c}{label} {rpct}%{r}")

    # --- cost, last: informational on a subscription
    cost = (d.get("cost") or {}).get("total_cost_usd")
    if isinstance(cost, (int, float)) and cost > 0:
        parts.append(f"{DIM}${cost:.2f}{RESET}")

    print(" · ".join(parts))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("viby-code")
