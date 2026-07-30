#!/usr/bin/env python3
"""Run the routing matrix. Replaces the bash driver, whose `while read` loop kept hitting EOF
because backgrounded children inherit every file descriptor, not just stdin — it silently ran 6
then 12 of 50 runs and exited 0 both times, which is exactly the kind of quiet truncation that
turns into a wrong published number."""
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.abspath(__file__))
REPS = int(os.environ.get("REPS", "5"))
PAR = int(os.environ.get("PAR", "6"))
MODEL = os.environ.get("MODEL", "sonnet")
MAX_TURNS = os.environ.get("MAX_TURNS", "4")
ARM = os.environ.get("ARM", "with")  # "with" = plugins as installed; "bare" = control arm

RUNS = os.path.join(ROOT, os.environ.get("RUNS_NAME", "runs" if ARM == "with" else f"runs-{ARM}"))


def one(job):
    pid, rep, prompt = job
    work = os.path.join(RUNS, f"{pid}-{rep}")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work, exist_ok=True)
    for entry in os.listdir(os.path.join(ROOT, "fixture")):
        src = os.path.join(ROOT, "fixture", entry)
        dst = os.path.join(work, entry)
        shutil.copytree(src, dst) if os.path.isdir(src) else shutil.copy2(src, dst)

    cmd = ["claude", "-p", prompt, "--output-format", "stream-json", "--verbose",
           "--max-turns", MAX_TURNS, "--model", MODEL]
    if ARM == "bare":
        # Control arm: no plugins loaded at all. If the task still gets done correctly here, the
        # skill was not adding anything on that probe — the finding Superpowers insists you check.
        cmd.append("--bare")
    env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
    try:
        with open(os.path.join(work, "stream.jsonl"), "w") as out, \
             open(os.path.join(work, "stderr.txt"), "w") as err:
            subprocess.run(cmd, cwd=work, stdout=out, stderr=err,
                           stdin=subprocess.DEVNULL, env=env, timeout=300)
    except subprocess.TimeoutExpired:
        with open(os.path.join(work, "timeout"), "w") as f:
            f.write("1")
    return f"{pid}-{rep}"


def main():
    probes = []
    with open(os.path.join(ROOT, os.environ.get("PROBES", "probes.tsv"))) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) == 3:
                probes.append(parts)
    jobs = [(pid, rep, prompt) for pid, _exp, prompt in probes for rep in range(1, REPS + 1)]
    shutil.rmtree(RUNS, ignore_errors=True)
    os.makedirs(RUNS, exist_ok=True)
    print(f"{len(jobs)} runs · model={MODEL} · arm={ARM} · parallelism={PAR}", flush=True)
    done = 0
    with ThreadPoolExecutor(max_workers=PAR) as ex:
        for name in ex.map(one, jobs):
            done += 1
            print(f"  [{done}/{len(jobs)}] {name}", flush=True)
    print(f"DONE {done}/{len(jobs)}")
    return 0 if done == len(jobs) else 1


if __name__ == "__main__":
    sys.exit(main())
