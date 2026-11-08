#!/usr/bin/env python3
"""Ground truth for the outcome harness, with no model involved.

For every task under fixtures/ it proves three things, and exits 1 if any of them is false:

  1. the hidden test FAILS on the untouched repo   (otherwise a pass proves nothing)
  2. the hidden test PASSES once reference.patch is applied to a copy   (the task is solvable)
  3. the repo's own visible tests pass in both   (they do not catch the bug, and the fix keeps them)

drive.py grades every model run with the same functions, so a run is judged exactly the way the
fixture was validated.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.abspath(__file__))
# fixtures/, like routing's fixture/: the toolkit's test-quality scanner treats a fixtures/ folder
# under tests/ as support code, so the seeded repos are not audited as the toolkit's own tests.
TASKS_DIR = os.path.join(ROOT, "fixtures")
TEST_TIMEOUT = 120
TEST_ENV = dict(os.environ, PYTHONDONTWRITEBYTECODE="1")
SKILLS = ("debug", "plan")


def task_ids():
    return sorted(d for d in os.listdir(TASKS_DIR) if os.path.isdir(os.path.join(TASKS_DIR, d)))


def load_meta(task):
    with open(os.path.join(TASKS_DIR, task, "meta.json")) as f:
        return json.load(f)


def load_prompt(task):
    with open(os.path.join(TASKS_DIR, task, "prompt.txt")) as f:
        return f.read().strip()


def copy_repo(task, dest):
    shutil.copytree(os.path.join(TASKS_DIR, task, "repo"), dest)


def run_unittest(cwd, args):
    """Return (passed, last lines of output). A hang counts as a failure, not as a pass."""
    try:
        p = subprocess.run([sys.executable, "-m", "unittest", *args], cwd=cwd, env=TEST_ENV,
                           stdin=subprocess.DEVNULL, capture_output=True, text=True,
                           timeout=TEST_TIMEOUT)
    except subprocess.TimeoutExpired:
        return False, f"timed out after {TEST_TIMEOUT}s"
    return p.returncode == 0, (p.stdout + p.stderr)[-800:]


def run_visible(task, cwd):
    """Run the task's ORIGINAL visible tests against the code in cwd. They are copied in fresh, so
    an agent that edits or deletes tests/ cannot turn a regression into a pass."""
    dest = os.path.join(cwd, ".orig_tests")
    shutil.rmtree(dest, ignore_errors=True)
    shutil.copytree(os.path.join(TASKS_DIR, task, "repo", "tests"), dest)
    return run_unittest(cwd, ["discover", "-s", ".orig_tests", "-t", ".orig_tests"])


def run_hidden(task, cwd):
    shutil.copy2(os.path.join(TASKS_DIR, task, "hidden_test.py"), os.path.join(cwd, "hidden_test.py"))
    return run_unittest(cwd, ["hidden_test"])


def apply_patch(cwd, patch):
    """git apply inside a fresh repo, so paths resolve against cwd and never a parent checkout."""
    subprocess.run(["git", "init", "-q"], cwd=cwd, check=True, stdin=subprocess.DEVNULL)
    p = subprocess.run(["git", "apply", "--whitespace=nowarn", patch], cwd=cwd,
                       stdin=subprocess.DEVNULL, capture_output=True, text=True)
    return p.returncode == 0, p.stderr


def py_lines(repo):
    total = 0
    for dirpath, _dirs, files in os.walk(repo):
        for name in files:
            if name.endswith(".py"):
                with open(os.path.join(dirpath, name)) as f:
                    total += sum(1 for _ in f)
    return total


def check(task, scratch):
    """Return a list of problems with one task; empty means it is valid ground truth."""
    base = os.path.join(TASKS_DIR, task)
    problems = []
    for name in ("repo", "prompt.txt", "hidden_test.py", "reference.patch", "meta.json"):
        if not os.path.exists(os.path.join(base, name)):
            problems.append(f"missing {name}")
    if problems:
        return problems
    if load_meta(task).get("skill") not in SKILLS:
        problems.append(f"meta.json skill must be one of {SKILLS}")
    if not os.path.isdir(os.path.join(base, "repo", "tests")):
        return problems + ["repo/tests/ is missing"]
    if os.path.exists(os.path.join(base, "repo", "hidden_test.py")):
        problems.append("repo/ contains hidden_test.py; the agent would see it")

    untouched = os.path.join(scratch, task, "untouched")
    copy_repo(task, untouched)
    ok, out = run_visible(task, untouched)
    if not ok:
        problems.append(f"visible tests fail on the untouched repo:\n{out}")
    ok, out = run_hidden(task, untouched)
    if ok:
        problems.append("hidden test PASSES on the untouched repo, so it cannot detect a fix")

    fixed = os.path.join(scratch, task, "reference")
    copy_repo(task, fixed)
    ok, err = apply_patch(fixed, os.path.join(base, "reference.patch"))
    if not ok:
        return problems + [f"reference.patch does not apply:\n{err}"]
    ok, out = run_visible(task, fixed)
    if not ok:
        problems.append(f"visible tests fail after reference.patch:\n{out}")
    ok, out = run_hidden(task, fixed)
    if not ok:
        problems.append(f"hidden test fails after reference.patch:\n{out}")
    return problems


def main():
    tasks = task_ids()
    if not tasks:
        print("no tasks found")
        return 1
    failed = 0
    scratch = tempfile.mkdtemp(prefix="outcome-validate-")
    try:
        for task in tasks:
            problems = check(task, scratch)
            skill = load_meta(task).get("skill", "?") if os.path.exists(
                os.path.join(TASKS_DIR, task, "meta.json")) else "?"
            lines = py_lines(os.path.join(TASKS_DIR, task, "repo"))
            if problems:
                failed += 1
                print(f"FAIL  {task:<24}{skill:<7}{lines:>4} lines")
                for p in problems:
                    print("      " + p.replace("\n", "\n      "))
            else:
                print(f"ok    {task:<24}{skill:<7}{lines:>4} lines   untouched: visible pass, hidden fail"
                      "   reference: visible pass, hidden pass")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    print(f"{len(tasks) - failed}/{len(tasks)} task(s) valid")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
