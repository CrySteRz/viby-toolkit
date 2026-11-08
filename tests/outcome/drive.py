#!/usr/bin/env python3
"""Run the outcome matrix: each task, once per arm per rep, graded by its hidden test.

  skill  the prompt is prefixed "/viby-toolkit:<skill> ", so the skill is forced
  bare   the same prompt with no prefix, and --disable-slash-commands so no skill can load

Both arms run in the same pool, interleaved, so drift in the model or the service hits both alike.
Every run is isolated the way routing's ISOLATE=1 is: this checkout's plugin and nothing of the
owner's setup, except his hooks, which are passed back in so the safety guards stay on.

Each run writes result.json next to its stream; score.py reads only those.
"""
import fnmatch
import glob
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor

import validate

ROOT = os.path.dirname(os.path.abspath(__file__))
PLUGIN_DIR = os.path.normpath(os.path.join(ROOT, "..", "..", "plugins", "viby-toolkit"))
ARMS = ("skill", "bare")

TASKS = os.environ.get("TASKS", "*")
REPS = int(os.environ.get("REPS", "1"))
MODEL = os.environ.get("MODEL", "sonnet")
PAR = int(os.environ.get("PAR", "4"))
MAX_TURNS = os.environ.get("MAX_TURNS", "40")
TIMEOUT = int(os.environ.get("TIMEOUT", "900"))
# Results live next to the harness, as routing's do, because a scratchpad wipe once lost a whole
# matrix after its numbers had been quoted. runs*/ is gitignored.
RUNS = os.path.join(ROOT, "runs-" + os.environ.get("NAME", "main"))
USER_SETTINGS = os.path.expanduser("~/.claude/settings.json")


def _routing_score():
    """Reuse routing's parser for the result message, so both harnesses read cost the same way."""
    path = os.path.join(ROOT, "..", "routing", "score.py")
    spec = importlib.util.spec_from_file_location("routing_score", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


extract_metrics = _routing_score().extract_metrics


def selected_tasks():
    """TASKS is a glob ("debug-*") or a comma- or space-separated list of ids or globs."""
    patterns = TASKS.replace(",", " ").split()
    every = validate.task_ids()
    return [t for t in every if any(fnmatch.fnmatch(t, p) for p in patterns)]


def write_hooks_settings():
    """--setting-sources project,local drops the owner's user settings, and with them his hooks
    (the guard against deleting shared roots among them). Hand back ONLY the hooks block."""
    hooks = {}
    if os.path.exists(USER_SETTINGS):
        with open(USER_SETTINGS) as f:
            hooks = json.load(f).get("hooks", {})
    path = os.path.join(RUNS, "hooks-settings.json")
    with open(path, "w") as f:
        json.dump({"hooks": hooks}, f, indent=2)
    return path, sum(len(groups) for groups in hooks.values())


def skill_marker(skill):
    """A line only the skill's own text contains: its IRON LAW line, else its title."""
    with open(os.path.join(PLUGIN_DIR, "skills", skill, "SKILL.md")) as f:
        lines = [line.strip() for line in f]
    for line in lines:
        if line.startswith("IRON LAW:"):
            return line
    return next(line for line in lines if line.startswith("# "))


def find_transcript(session_id):
    """The CLI's own session log. In -p mode the stream never echoes a slash command's expansion,
    so the skill's text shows up only here (checked on 2.1.319)."""
    config = os.environ.get("CLAUDE_CONFIG_DIR", os.path.expanduser("~/.claude"))
    hits = glob.glob(os.path.join(config, "projects", "*", f"{session_id}.jsonl"))
    return hits[0] if hits else None


def text_has(path, marker):
    escaped = json.dumps(marker)[1:-1]
    with open(path, errors="ignore") as f:
        return any(marker in line or escaped in line for line in f)


def read_stream(work, skill):
    """Return (result message or None, whether the skill loaded). The skill counts as loaded when
    a Skill tool call names it, or its marker line appears in the stream or the session log; the
    session log is copied into the run as transcript.jsonl so the evidence stays with the run."""
    path = os.path.join(work, "stream.jsonl")
    if not os.path.exists(path):
        return None, False
    marker = skill_marker(skill)
    loaded = text_has(path, marker)
    result = None
    session_id = None
    with open(path, errors="ignore") as f:
        for line in f:
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                d = json.loads(line)
            except ValueError:
                continue
            session_id = session_id or d.get("session_id")
            if d.get("type") == "assistant":
                for c in d.get("message", {}).get("content", []):
                    if c.get("type") == "tool_use" and c.get("name") == "Skill":
                        name = str((c.get("input") or {}).get("skill", ""))
                        if name.split(":")[-1] == skill:
                            loaded = True
            elif d.get("type") == "result":
                result = d
    transcript = find_transcript(session_id) if session_id else None
    if transcript:
        shutil.copyfile(transcript, os.path.join(work, "transcript.jsonl"))
        loaded = loaded or text_has(transcript, marker)
    return result, loaded


def setup_repo(task, work):
    validate.copy_repo(task, work)

    def git(*args):
        subprocess.run(["git", *args], cwd=work, stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, check=False)
    git("init", "-q")
    git("add", "-A")
    git("-c", "user.email=fixture@example.invalid", "-c", "user.name=fixture", "commit", "-qm", "init")


def one(job, settings_path):
    task, arm, rep = job
    meta = validate.load_meta(task)
    skill = meta["skill"]
    name = f"{task}-{arm}-{rep}"
    work = os.path.join(RUNS, name)
    shutil.rmtree(work, ignore_errors=True)
    setup_repo(task, work)

    prompt = validate.load_prompt(task)
    if arm == "skill":
        prompt = f"/viby-toolkit:{skill} {prompt}"
    cmd = ["claude", "-p", prompt, "--output-format", "stream-json", "--verbose",
           "--max-turns", MAX_TURNS, "--model", MODEL, "--dangerously-skip-permissions",
           "--setting-sources", "project,local", "--strict-mcp-config",
           "--plugin-dir", PLUGIN_DIR, "--settings", settings_path]
    if arm == "bare":
        # Never --bare: it skips credential loading and every run answers "Not logged in".
        cmd.append("--disable-slash-commands")
    env = {k: v for k, v in os.environ.items() if k != "CLAUDECODE"}
    stream = os.path.join(work, "stream.jsonl")
    timed_out = False
    started = time.monotonic()
    try:
        with open(stream, "w") as out, open(os.path.join(work, "stderr.txt"), "w") as err:
            subprocess.run(cmd, cwd=work, stdout=out, stderr=err, stdin=subprocess.DEVNULL,
                           env=env, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        timed_out = True
    wall_s = time.monotonic() - started

    result, loaded = read_stream(work, skill)
    hidden_pass, hidden_out = validate.run_hidden(task, work)
    visible_pass, visible_out = validate.run_visible(task, work)
    record = {
        "task": task, "skill": skill, "arm": arm, "rep": rep, "model": MODEL,
        "hidden_pass": hidden_pass, "visible_pass": visible_pass,
        "skill_loaded": loaded, "timed_out": timed_out,
        "finished": result is not None,
        "subtype": result.get("subtype") if result else None,
        "is_error": result.get("is_error") if result else None,
        "wall_s": round(wall_s, 1),
        **extract_metrics(result or {}),
        "hidden_tail": hidden_out[-400:], "visible_tail": visible_out[-400:],
    }
    with open(os.path.join(work, "result.json"), "w") as f:
        json.dump(record, f, indent=2)
    verdict = "PASS" if hidden_pass else "fail"
    cost = f"${record['cost']:.2f}" if record["cost"] is not None else "$n/a"
    return f"{name}: {verdict} · visible {'ok' if visible_pass else 'REGRESSED'} · " \
           f"skill loaded {loaded} · {cost} · {record['turns']} turns · {wall_s:.0f}s" + \
           (" · TIMEOUT" if timed_out else "")


def main():
    tasks = selected_tasks()
    if not tasks:
        print(f"no task matches TASKS={TASKS!r}")
        return 2
    # Interleaved: rep by rep, task by task, the two arms side by side in the same pool.
    jobs = [(t, arm, rep) for rep in range(1, REPS + 1) for t in tasks for arm in ARMS]
    shutil.rmtree(RUNS, ignore_errors=True)
    os.makedirs(RUNS)
    settings_path, hook_groups = write_hooks_settings()
    print(f"{len(jobs)} runs · {len(tasks)} task(s) · model={MODEL} · reps={REPS} · par={PAR} · "
          f"max-turns={MAX_TURNS} · timeout={TIMEOUT}s · {hook_groups} user hook group(s) kept · "
          f"-> {RUNS}", flush=True)
    done = 0
    with ThreadPoolExecutor(max_workers=PAR) as ex:
        for line in ex.map(lambda job: one(job, settings_path), jobs):
            done += 1
            print(f"  [{done}/{len(jobs)}] {line}", flush=True)
    print(f"DONE {done}/{len(jobs)}")
    return 0 if done == len(jobs) else 1


if __name__ == "__main__":
    sys.exit(main())
