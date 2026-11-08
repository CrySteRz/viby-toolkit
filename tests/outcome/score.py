#!/usr/bin/env python3
"""Score the outcome matrix: does forcing a skill beat no skill, on whether the work is right?

A run counts when the agent finished or ran out of turns or time (running out is an outcome:
the efficiency failures this harness exists to catch). It is left out, and listed, when the
harness failed (no result message and no timeout), when the run is still going, when a skill-arm
run never loaded its skill (that is not a skill run), or when a bare-arm run loaded one.

Every rate carries a 95% Wilson interval, and a difference between arms is only called when the
two intervals do not overlap, exactly as routing's score.py does.
"""
import glob
import importlib.util
import json
import math
import os
import statistics
import sys

ROOT = os.path.dirname(os.path.abspath(__file__))
RUNS = os.path.join(ROOT, "runs-" + os.environ.get("NAME", "main"))
ARMS = ("skill", "bare")
Z_POWER = 0.8416212335729143  # one-sided normal quantile for 80% power


def _routing_score():
    path = os.path.join(ROOT, "..", "routing", "score.py")
    spec = importlib.util.spec_from_file_location("routing_score", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_r = _routing_score()
wilson_interval, fmt_rate, intervals_overlap = _r.wilson_interval, _r.fmt_rate, _r.intervals_overlap


def min_detectable_difference(n):
    """Smallest true difference between two rates near 0.5 that n runs per arm detect with 80%
    power at a two-sided 5% level: (z_a/2 + z_b) * sqrt(2 * 0.25 / n)."""
    return (_r.Z + Z_POWER) * math.sqrt(0.5 / n) if n else float("nan")


def load():
    counted, excluded, unfinished = [], [], []
    for run_dir in sorted(glob.glob(os.path.join(RUNS, "*-*-*"))):
        if not os.path.isdir(run_dir):
            continue
        path = os.path.join(run_dir, "result.json")
        if not os.path.exists(path):
            unfinished.append(os.path.basename(run_dir))
            continue
        with open(path) as f:
            r = json.load(f)
        name = f"{r['task']}-{r['arm']}-{r['rep']}"
        if not r["finished"] and not r["timed_out"]:
            excluded.append((name, "harness error: no result message (see stderr.txt)"))
        elif r["arm"] == "skill" and not r["skill_loaded"]:
            excluded.append((name, "skill arm, but the skill never loaded"))
        elif r["arm"] == "bare" and r["skill_loaded"]:
            excluded.append((name, "bare arm, but the skill loaded anyway"))
        else:
            counted.append(r)
    return counted, excluded, unfinished


def med(values, fmt):
    clean = [v for v in values if v is not None]
    return fmt(statistics.median(clean)) if clean else "n/a"


def total(values):
    return sum(v for v in values if v is not None)


def arm_row(label, runs):
    n = len(runs)
    passes = sum(r["hidden_pass"] for r in runs)
    regressions = sum(not r["visible_pass"] for r in runs)
    costs = [r["cost"] for r in runs]
    per_pass = f"${total(costs) / passes:.2f}" if passes else "n/a"
    print(f"  {label:<6}{n:>4}  {fmt_rate(passes, n):<20}{regressions:>5}"
          f"  {med(costs, lambda v: f'${v:.2f}'):>8} {'$%.2f' % total(costs):>9} {per_pass:>9}"
          f"  {med([r['turns'] for r in runs], lambda v: f'{v:.0f}'):>6} {total([r['turns'] or 0 for r in runs]):>6.0f}"
          f"  {med([r['wall_s'] for r in runs], lambda v: f'{v:.0f}s'):>7} {total([r['wall_s'] for r in runs]):>7.0f}s")
    return n, passes


def report_skill(skill, runs):
    print(f"\n=== {skill} ===")
    print(f"  {'arm':<6}{'n':>4}  {'pass [95% CI]':<20}{'regr':>5}  {'cost med':>8} {'total':>9} "
          f"{'per pass':>9}  {'turns':>6} {'total':>6}  {'wall':>7} {'total':>8}")
    by_arm = {arm: [r for r in runs if r["arm"] == arm] for arm in ARMS}
    stats = {arm: arm_row(arm, by_arm[arm]) for arm in ARMS}

    print(f"\n  {'task':<24}{'skill':>12}{'bare':>12}{'diff':>8}")
    diffs = []
    for task in sorted({r["task"] for r in runs}):
        cells = {}
        for arm in ARMS:
            rs = [r for r in by_arm[arm] if r["task"] == task]
            cells[arm] = (sum(r["hidden_pass"] for r in rs), len(rs))
        shown = {arm: f"{p}/{n}" for arm, (p, n) in cells.items()}
        if all(n for _p, n in cells.values()):
            d = cells["skill"][0] / cells["skill"][1] - cells["bare"][0] / cells["bare"][1]
            diffs.append(d)
            shown["diff"] = f"{d * 100:+.0f}"
        else:
            shown["diff"] = "n/a"
        print(f"  {task:<24}{shown['skill']:>12}{shown['bare']:>12}{shown['diff']:>8}")
    if diffs:
        print(f"  paired per-task difference, mean over {len(diffs)} task(s): "
              f"{statistics.mean(diffs) * 100:+.1f} points (skill - bare)")

    (n_s, p_s), (n_b, p_b) = stats["skill"], stats["bare"]
    n = min(n_s, n_b)
    if n == 0:
        print("  no comparison: one arm has no counted runs")
        return
    mdd = min_detectable_difference(n)
    shown = f"~{mdd * 100:.0f} points" if mdd < 1 else "none: no difference is detectable"
    print(f"  minimum detectable difference at n={n} per arm (80% power, rates near 0.5): {shown}")
    if intervals_overlap(wilson_interval(p_s, n_s), wilson_interval(p_b, n_b)):
        print("  REFUSING to call a difference: the 95% intervals overlap, so this data does not"
              " establish that either arm does better.")
    else:
        delta = p_s / n_s - p_b / n_b
        print(f"  non-overlapping intervals: skill - bare = {delta * 100:+.0f} points")


def main():
    if not os.path.isdir(RUNS):
        print(f"no runs at {RUNS}")
        return 2
    counted, excluded, unfinished = load()
    print(f"{RUNS}: {len(counted)} counted, {len(excluded)} excluded, {len(unfinished)} unfinished")
    for skill in sorted({r["skill"] for r in counted}):
        report_skill(skill, [r for r in counted if r["skill"] == skill])
    timeouts = [f"{r['task']}-{r['arm']}-{r['rep']}" for r in counted if r["timed_out"]]
    if timeouts:
        print(f"\ncounted as attempts that ran out of time ({len(timeouts)}): {', '.join(timeouts)}")
    if excluded:
        print(f"\n!! excluded ({len(excluded)}):")
        for name, why in excluded:
            print(f"   {name}: {why}")
    if unfinished:
        print(f"\n!! {len(unfinished)} run(s) have no result.json yet and were left out: "
              f"{', '.join(unfinished[:8])}. A partial matrix is not a result.")
    print("\nregr = runs whose original visible tests failed afterwards. Costs are the CLI's"
          " total_cost_usd; medians, not means, so one capped run cannot dominate.")
    return 0 if counted else 2


if __name__ == "__main__":
    sys.exit(main())
