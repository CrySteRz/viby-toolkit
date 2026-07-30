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
# CLEAN=1 skips the dirty patch, so fixture state can be isolated as a single variable. `docs` went
# 0/5 -> 4/5 across a window in which BOTH the description and the fixture changed; this is how you
# tell which one did it without reinstalling an old release.
CLEAN = os.environ.get("CLEAN") == "1"

# Results live NEXT TO the harness, not in a temp dir. A whole 155-run matrix was lost to a session
# restart wiping the scratchpad — about half an hour of compute, and worse, the numbers were already
# quoted in a status message before anyone noticed the directory was gone. `runs*/` is gitignored.
RUNS = os.path.join(ROOT, os.environ.get("RUNS_NAME", "runs" if ARM == "with" else f"runs-{ARM}"))


# A dirty working tree, created deterministically. "review my changes" and "is this ready to ship" are
# unanswerable in a clean checkout, and the first run of this harness scored them NONE for that reason
# alone — `verify` went 1/5 to 5/5 once real pending changes existed. So the fixture's git state is part
# of the harness, not something a human sets up by hand and forgets to reproduce.
DIRTY_PATCH = """
function applyPromo(total, code) {
  const pct = PROMOS[code];
  return pct ? total - total * pct : total;
}
const PROMOS = { SAVE10: 0.1, SAVE20: 0.2 };
"""


def setup_repo_state(work):
    def git(*args):
        subprocess.run(["git", *args], cwd=work, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, check=False)
    git("init", "-q")
    git("add", "-A")
    git("-c", "user.email=fixture@example.invalid", "-c", "user.name=fixture", "commit", "-qm", "init")
    if CLEAN:
        return
    pricing = os.path.join(work, "src", "pricing.js")
    with open(pricing, "a") as f:
        f.write(DIRTY_PATCH)


def one(job):
    pid, rep, prompt = job
    work = os.path.join(RUNS, f"{pid}-{rep}")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work, exist_ok=True)
    for entry in os.listdir(os.path.join(ROOT, "fixture")):
        src = os.path.join(ROOT, "fixture", entry)
        dst = os.path.join(work, entry)
        shutil.copytree(src, dst) if os.path.isdir(src) else shutil.copy2(src, dst)
    setup_repo_state(work)

    cmd = ["claude", "-p", prompt, "--output-format", "stream-json", "--verbose",
           "--max-turns", MAX_TURNS, "--model", MODEL]
    if ARM == "bare":
        # Control arm: skills unavailable, everything else identical. NOT `--bare`, which also skips
        # credential loading — every run came back "Not logged in · Please run /login", a 67-character
        # non-answer that a keyword grader would happily have scored as "met 0/5 of the bar" and
        # reported as the base model failing. `--disable-slash-commands` removes the skills and keeps
        # the session intact.
        cmd.append("--disable-slash-commands")
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
