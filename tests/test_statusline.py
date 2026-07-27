#!/usr/bin/env python3
"""Smoke tests for the viby-code statusline.

Run:  python3 tests/test_statusline.py
Exit: 0 = every payload produced a sane single line, 1 = a failure.

Payload shapes follow the documented statusLine stdin contract, including the
documented null cases: `current_usage` is null before the first API call and again
after /compact, `used_percentage` may be null early, and `rate_limits` appears only
for Pro/Max subscribers after the first API response.
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPT = os.path.join(ROOT, "plugins", "viby-code", "hooks", "statusline.py")

FULL = {
    "model": {"display_name": "Opus 5", "id": "claude-opus-5"},
    "cost": {"total_cost_usd": 1.2345},
    "context_window": {
        "context_window_size": 1000000,
        "used_percentage": 43,
        "current_usage": {
            "input_tokens": 1200,
            "cache_creation_input_tokens": 800,
            "cache_read_input_tokens": 18000,
            "output_tokens": 400,
        },
    },
    "rate_limits": {
        "five_hour": {"used_percentage": 23.5},
        "seven_day": {"used_percentage": 81.2},
    },
    "exceeds_200k_tokens": False,
}


def drop(d, *path):
    """Deep-copy `d` with the key at `path` removed."""
    out = json.loads(json.dumps(d))
    node = out
    for k in path[:-1]:
        node = node[k]
    node.pop(path[-1], None)
    return out


def nullify(d, *path):
    out = json.loads(json.dumps(d))
    node = out
    for k in path[:-1]:
        node = node[k]
    node[path[-1]] = None
    return out


CASES = [
    # cache share = cache_read / (input + cache_creation + cache_read) = 18000/20000 = 90%
    ("full payload", FULL, ["Opus 5", "ctx 43%", "cache 90%", "5h 24%", "7d 81%", "$1.23"], []),
    ("no rate_limits (non-subscriber)", drop(FULL, "rate_limits"),
     ["ctx 43%"], ["5h ", "7d "]),
    ("five_hour only", drop(FULL, "rate_limits", "seven_day"), ["5h 24%"], ["7d "]),
    ("current_usage null (pre-first-call)", nullify(FULL, "context_window", "current_usage"),
     ["ctx 43%"], ["cache "]),
    ("used_percentage null, usage present", nullify(FULL, "context_window", "used_percentage"),
     ["ctx 2%"], []),
    ("used_percentage null + no size", drop(nullify(FULL, "context_window", "used_percentage"),
                                           "context_window", "context_window_size"),
     [], ["ctx "]),
    ("no context_window at all", drop(FULL, "context_window"), ["Opus 5"], ["ctx ", "cache "]),
    ("zero cost omitted", {**FULL, "cost": {"total_cost_usd": 0}}, ["ctx 43%"], ["$"]),
    ("no cost key", drop(FULL, "cost"), ["ctx 43%"], ["$"]),
    ("model id fallback", {"model": {"id": "claude-sonnet-5"}}, ["claude-sonnet-5"], ["ctx "]),
    ("empty object", {}, ["claude"], ["ctx ", "$"]),
    ("red band at high ctx", {**FULL, "context_window": {**FULL["context_window"],
                                                         "used_percentage": 91}},
     ["\033[31mctx 91%"], []),
    ("green band at low ctx", {**FULL, "context_window": {**FULL["context_window"],
                                                          "used_percentage": 12}},
     ["\033[32mctx 12%"], []),
]


def run(payload):
    p = subprocess.run([sys.executable, SCRIPT], input=json.dumps(payload),
                       capture_output=True, text=True)
    return p.returncode, p.stdout, p.stderr


def main():
    failures = []
    print(f"statusline: {SCRIPT}\n")
    for name, payload, must, must_not in CASES:
        rc, out, err = run(payload)
        problems = []
        if rc != 0:
            problems.append(f"exit {rc}: {err.strip()[:60]}")
        if out.count("\n") != 1:
            problems.append(f"expected exactly one line, got {out.count(chr(10))}")
        if out.strip() == "":
            problems.append("empty output")
        for m in must:
            if m not in out:
                problems.append(f"missing {m!r}")
        for m in must_not:
            if m in out:
                problems.append(f"should not contain {m!r}")

        visible = re.sub(r"\033\[[0-9;]*m", "", out).strip()
        if problems:
            failures.append((name, problems, visible))
        print(f"{'ok  ' if not problems else 'FAIL'} {name:<38} {visible}")

    print("\n" + "─" * 74)
    if failures:
        print(f"✗ {len(failures)} of {len(CASES)} cases failed:\n")
        for name, problems, visible in failures:
            print(f"  {name}: {'; '.join(problems)}\n    output: {visible!r}")
        return 1
    print(f"✓ all {len(CASES)} cases produced a sane line")
    return 0


if __name__ == "__main__":
    sys.exit(main())
