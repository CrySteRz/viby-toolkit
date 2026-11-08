#!/usr/bin/env python3
"""Score the routing matrix.

Three outcomes, not two — the distinction matters because they have different fixes:

  HIT       the intended skill was invoked (first skill invoked is the intended one)
  WRONG     a different skill was invoked first  -> a description/shadowing problem
  NONE      no skill was invoked at all          -> the model went straight to raw tools,
                                                    so the skill lost to the base system prompt,
                                                    which no description tuning can fix

Reports per-probe hit rate over reps, so a single sample can't masquerade as a result.

Every rate printed here carries a 95% Wilson score interval, because a point estimate from a
handful of reps has already produced a false "difference": 83% (145 runs) vs 87% (87 runs) read
as a change, and a per-skill rate on the same data swung 0/3 to 5/5 on a re-run at 5 reps. The
interval is what stops that from happening again.
"""
import json
import math
import os
import statistics
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.abspath(__file__))

Z = 1.959963984540054  # 95% two-sided normal quantile


def wilson_interval(hits, n, z=Z):
    """95% Wilson score interval for a binomial rate, closed form — no scipy needed."""
    if n <= 0:
        return (0.0, 1.0)
    phat = hits / n
    z2 = z * z
    denom = 1 + z2 / n
    center = phat + z2 / (2 * n)
    margin = z * math.sqrt((phat * (1 - phat) + z2 / (4 * n)) / n)
    lo = (center - margin) / denom
    hi = (center + margin) / denom
    return (max(0.0, lo), min(1.0, hi))


def fmt_rate(hits, n):
    if n == 0:
        return "n/a"
    lo, hi = wilson_interval(hits, n)
    return f"{hits / n * 100:.0f}% [{lo * 100:.0f}-{hi * 100:.0f}%]"


def intervals_overlap(a, b):
    a_lo, a_hi = a
    b_lo, b_hi = b
    return a_lo <= b_hi and b_lo <= a_hi


def extract_metrics(d):
    """Pull cost/latency/turn/token fields off a terminal `result` event. Any field that is
    missing or the wrong type degrades to None (rendered "n/a") rather than 0 — a 0 there would
    be silently averaged into the medians as a real, fast, free run."""
    metrics = {"cost": None, "duration_ms": None, "turns": None, "tokens": None}
    cost = d.get("total_cost_usd")
    if isinstance(cost, (int, float)):
        metrics["cost"] = cost
    dur = d.get("duration_ms")
    if isinstance(dur, (int, float)):
        metrics["duration_ms"] = dur
    turns = d.get("num_turns")
    if isinstance(turns, (int, float)):
        metrics["turns"] = turns
    usage = d.get("usage")
    if isinstance(usage, dict):
        parts = [
            usage.get(k)
            for k in (
                "input_tokens",
                "output_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            )
        ]
        numeric = [p for p in parts if isinstance(p, (int, float))]
        if numeric:
            metrics["tokens"] = sum(numeric)
    return metrics


def median_or_na(values, fmt=lambda v: f"{v:.0f}"):
    clean = [v for v in values if v is not None]
    if not clean:
        return "n/a"
    return fmt(statistics.median(clean))


def parse_run(run_dir):
    """Return (tool_calls, terminal subtype or None, metrics dict) for one run directory."""
    tools = []
    p = os.path.join(run_dir, "stream.jsonl")
    metrics = {"cost": None, "duration_ms": None, "turns": None, "tokens": None}
    if not os.path.exists(p):
        return tools, None, metrics
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
                    tools.append((c.get("name"), c.get("input") or {}))
        elif d.get("type") == "result":
            subtype = d.get("subtype")
            metrics = extract_metrics(d)
    return tools, subtype, metrics


def load_probes(path):
    probes = []
    for line in open(path):
        parts = line.rstrip("\n").split("\t")
        if len(parts) == 3:
            probes.append(parts)
    return probes


def score_runs(runs_name, probes):
    """Score one run directory against the probe list. Returns rows plus totals plus the list
    of incomplete runs plus per-probe telemetry samples."""
    rows = []
    incomplete = []
    telemetry = {}
    for pid, expected, prompt in probes:
        outcomes = []
        firsts = []
        samples = []
        for rep in range(1, 99):
            d = os.path.join(ROOT, runs_name, f"{pid}-{rep}")
            if not os.path.isdir(d):
                continue
            tools, subtype, metrics = parse_run(d)
            # A run with no terminal `result` event has not finished. Counting it as NONE is how
            # scoring mid-flight produced three different accuracies off overlapping data — the
            # in-flight runs all looked like "no skill fired yet".
            if subtype is None:
                incomplete.append(f"{pid}-{rep}")
                continue
            samples.append(metrics)
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
        telemetry[pid] = samples

    total = hits = late = wrong = none = 0
    for _pid, _expected, outcomes, _firsts in rows:
        c = Counter(outcomes)
        total += len(outcomes)
        hits += c["HIT"]
        late += c["HIT-LATE"]
        wrong += c["WRONG"]
        none += c["NONE"]

    return {
        "rows": rows,
        "incomplete": incomplete,
        "telemetry": telemetry,
        "total": total,
        "hits": hits,
        "late": late,
        "wrong": wrong,
        "none": none,
    }


def print_outcome_table(result):
    rows = result["rows"]
    min_reps = min((len(outcomes) for _p, _e, outcomes, _f in rows if outcomes), default=0)
    print(
        f"{'probe':<18}{'reps':>5}{'hit':>5}{'late':>6}{'wrong':>6}{'none':>6}"
        f"   hit-rate [95% CI]           what actually fired first"
    )
    print("-" * 120)
    for pid, expected, outcomes, firsts in rows:
        c = Counter(outcomes)
        n = len(outcomes)
        rate = fmt_rate(c["HIT"], n) if n else "n/a"
        dist = ", ".join(f"{k}x{v}" for k, v in Counter(firsts).most_common())
        print(
            f"{pid:<18}{n:>5}{c['HIT']:>5}{c['HIT-LATE']:>6}{c['WRONG']:>6}{c['NONE']:>6}"
            f"   {rate:<28}{dist}"
        )
    print("-" * 120)
    total, hits, late, wrong, none = (
        result["total"],
        result["hits"],
        result["late"],
        result["wrong"],
        result["none"],
    )
    if total == 0:
        print("no runs found")
        return
    print(
        f"{'TOTAL':<18}{total:>5}{hits:>5}{late:>6}{wrong:>6}{none:>6}"
        f"   first-choice {fmt_rate(hits, total)}"
        f"  routed-at-all {fmt_rate(hits + late, total)}"
    )
    if result["incomplete"]:
        print()
        print(
            f"!! {len(result['incomplete'])} run(s) had not finished and were EXCLUDED: "
            f"{', '.join(result['incomplete'][:8])}"
        )
        print("   re-run the scorer once the driver is done — a partial matrix is not a result.")
    if 0 < min_reps <= 5:
        print()
        print(
            f"!! per-probe rates here are over {min_reps} rep(s): the 95% CI on a single-skill"
            " rate at that sample size typically spans 40-60 percentage points. Treat each"
            " per-probe number as 'not yet ruled out', not as a measurement — only the TOTAL"
            " row over all probes has enough samples to be informative at this rep count."
        )


def print_telemetry_table(result, probes):
    print()
    print(f"{'probe':<18}{'n':>4}   {'tokens':>10}   {'cost':>10}   {'wall-clock':>11}   {'turns':>6}")
    print("-" * 70)
    for pid, _expected, _prompt in probes:
        samples = result["telemetry"].get(pid, [])
        n = len(samples)
        tokens = median_or_na([s["tokens"] for s in samples], lambda v: f"{v:.0f}")
        cost = median_or_na([s["cost"] for s in samples], lambda v: f"${v:.3f}")
        duration = median_or_na([s["duration_ms"] for s in samples], lambda v: f"{v / 1000:.1f}s")
        turns = median_or_na([s["turns"] for s in samples], lambda v: f"{v:.0f}")
        print(f"{pid:<18}{n:>4}   {tokens:>10}   {cost:>10}   {duration:>11}   {turns:>6}")
    print()
    print("medians, not means — a single retried or max-turns-capped run has a long tail that a")
    print("mean would let dominate the number.")


def print_comparison(name_a, result_a, name_b, result_b):
    print()
    print(f"=== comparing arms: {name_a} vs {name_b} ===")
    a = wilson_interval(result_a["hits"], result_a["total"])
    b = wilson_interval(result_b["hits"], result_b["total"])
    rate_a = fmt_rate(result_a["hits"], result_a["total"])
    rate_b = fmt_rate(result_b["hits"], result_b["total"])
    print(f"{name_a}: {rate_a}  ({result_a['total']} runs)")
    print(f"{name_b}: {rate_b}  ({result_b['total']} runs)")
    if intervals_overlap(a, b):
        print(
            "REFUSING to report a delta: the 95% confidence intervals overlap, so this data"
            " does not establish which arm is better. This is exactly the shape of the 83%"
            " (145 runs) vs 87% (87 runs) comparison that was reported as a difference and"
            " was not one."
        )
        return
    delta = result_b["hits"] / result_b["total"] - result_a["hits"] / result_a["total"]
    print(f"non-overlapping intervals: {name_b} - {name_a} = {delta * 100:+.0f} points")


def main():
    probes_path = os.path.join(ROOT, os.environ.get("PROBES", "probes.tsv"))
    probes = load_probes(probes_path)

    runs_name = os.environ.get("RUNS_NAME", "runs")
    result = score_runs(runs_name, probes)

    print_outcome_table(result)
    if result["total"] == 0:
        return 2
    print()
    print(
        "NONE means the model used raw tools instead of any skill — it lost to the base system"
    )
    print("prompt, not to a sibling skill, so description tuning cannot fix those.")

    print_telemetry_table(result, probes)

    compare_name = os.environ.get("COMPARE_RUNS_NAME")
    if compare_name:
        result_b = score_runs(compare_name, probes)
        if result_b["total"] == 0:
            print()
            print(f"!! no runs found under COMPARE_RUNS_NAME={compare_name!r}, skipping comparison")
        else:
            print_comparison(runs_name, result, compare_name, result_b)

    return 0


if __name__ == "__main__":
    sys.exit(main())
