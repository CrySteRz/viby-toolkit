#!/usr/bin/env python3
"""Score the routing matrix.

Three outcomes, not two — the distinction matters because they have different fixes:

  HIT       the intended skill was invoked (first skill invoked is the intended one)
  WRONG     a different skill was invoked first  -> a description/shadowing problem
  NONE      no skill was invoked at all          -> the model went straight to raw tools,
                                                    so the skill lost to the base system prompt,
                                                    which no description tuning can fix

Reports per-probe hit rate over reps, so a single sample can't masquerade as a result.
"""
import json
import os
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.abspath(__file__))


def tools_of(run_dir):
    out = []
    p = os.path.join(run_dir, "stream.jsonl")
    if not os.path.exists(p):
        return out, None
    subtype = None
    for line in open(p, errors="ignore"):
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            d = json.loads(line)
        except Exception:
            continue
        if d.get("type") == "assistant":
            for c in d.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    out.append((c.get("name"), c.get("input") or {}))
        elif d.get("type") == "result":
            subtype = d.get("subtype")
    return out, subtype


def main():
    probes = []
    for line in open(os.path.join(ROOT, os.environ.get("PROBES", "probes.tsv"))):
        parts = line.rstrip("\n").split("\t")
        if len(parts) == 3:
            probes.append(parts)

    rows = []
    incomplete: list[str] = []
    for pid, expected, prompt in probes:
        outcomes = []
        firsts = []
        for rep in range(1, 99):
            d = os.path.join(ROOT, os.environ.get("RUNS_NAME", "runs"), f"{pid}-{rep}")
            if not os.path.isdir(d):
                continue
            tools, subtype = tools_of(d)
            # A run with no terminal `result` event has not finished. Counting it as NONE is how
            # scoring mid-flight produced three different accuracies off overlapping data — the
            # in-flight runs all looked like "no skill fired yet".
            if subtype is None:
                incomplete.append(f"{pid}-{rep}")
                continue
            skills = [i.get("skill") for n, i in tools if n == "Skill" and i.get("skill")]
            if not skills:
                outcomes.append("NONE")
                firsts.append("-")
            elif skills[0] == expected:
                outcomes.append("HIT")
                firsts.append(skills[0])
            else:
                # credit a later invocation as a partial: it routed, just not first
                outcomes.append("HIT-LATE" if expected in skills else "WRONG")
                firsts.append(skills[0])
        rows.append((pid, expected, outcomes, firsts))

    total = hits = late = wrong = none = 0
    print(f"{'probe':<18}{'reps':>5}{'hit':>5}{'late':>6}{'wrong':>6}{'none':>6}   what actually fired first")
    print("-" * 100)
    for pid, expected, outcomes, firsts in rows:
        c = Counter(outcomes)
        n = len(outcomes)
        total += n
        hits += c["HIT"]
        late += c["HIT-LATE"]
        wrong += c["WRONG"]
        none += c["NONE"]
        dist = ", ".join(
            f"{k}×{v}" for k, v in Counter(firsts).most_common()
        )
        print(f"{pid:<18}{n:>5}{c['HIT']:>5}{c['HIT-LATE']:>6}{c['WRONG']:>6}{c['NONE']:>6}   {dist}")

    print("-" * 100)
    if total == 0:
        print("no runs found")
        return 2
    print(
        f"{'TOTAL':<18}{total:>5}{hits:>5}{late:>6}{wrong:>6}{none:>6}"
        f"   first-choice accuracy {hits / total * 100:.0f}%"
        f"  ·  routed-at-all {(hits + late) / total * 100:.0f}%"
    )
    if incomplete:
        print()
        print(f"!! {len(incomplete)} run(s) had not finished and were EXCLUDED: {', '.join(incomplete[:8])}")
        print("   re-run the scorer once the driver is done — a partial matrix is not a result.")
    print()
    print("NONE means the model used raw tools instead of any skill — it lost to the base system")
    print("prompt, not to a sibling skill, so description tuning cannot fix those.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
