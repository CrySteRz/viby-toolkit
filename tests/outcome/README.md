# Outcome measurement — does forcing a skill beat no skill on real work?

`tests/routing/` measures whether a skill **fires**. It cannot say whether the skill **helps**. That
gap matters: gstack cut a skill that scored +2.8 points (CI −5.8 to +11.4) and cost 43% more per
passing fix, and a 2026-08 paper traces 182 of 307 skill-induced failures to efficiency
regressions — the skill made the agent slower or more expensive without making it more right. Until
this harness, there was no evidence either way for this toolkit's own skills.

## What this measures

Each task is a small Python package with either one seeded bug (`debug`) or a feature ticket
(`plan`). Two arms get the same prompt, in the same pool, interleaved:

| arm | what the agent sees |
|---|---|
| `skill` | the prompt prefixed with `/viby-toolkit:<skill> `, so the skill is forced |
| `bare` | the prompt alone, plus `--disable-slash-commands`, so no skill can load |

The outcome is **ground truth, not a judgement**: after the session ends, a hidden stdlib
`unittest` the agent never saw is copied into the run directory and run. Pass or fail. The repo's
original visible tests are run too (from a fresh copy, so editing `tests/` cannot hide a
regression). Cost, tokens, turns and wall time come from the CLI's own result message.

A skill-arm run where the skill never loaded (no `Skill` call, and the skill's IRON LAW line in neither the stream nor the session log;
in `-p` mode the stream never echoes a slash command's expansion, so the session log under
`~/.claude/projects/` is read and copied into the run as `transcript.jsonl`) is
**not** a skill run: `score.py` lists it separately and leaves it out. A run that ran out of turns
or time **is** counted: that is the efficiency failure this harness exists to catch.

```
fixtures/<id>/
  repo/             the starting code, with its own visible tests in repo/tests/
  prompt.txt        what a user would type: the symptom (debug) or the ticket (plan)
  hidden_test.py    stdlib unittest, never shown to the agent
  reference.patch   a fix that makes the hidden test pass
  meta.json         {"skill": "debug" | "plan", "about": "..."}
```

Six `debug` tasks (money rounding, a DST boundary, a shared mutable default, a cache key missing a
parameter, a pagination off-by-one, a wrong sort key) describe only the symptom, never the file or
the cause. Four `plan` tasks (soft delete, CSV export, retry with backoff, per-user rate limit) are
tickets across repository/service/API layers that fix the public names the hidden tests use.

The plan tickets end by saying the plan is approved in advance. Without that, the `plan` skill
stops for approval by design ("present the plan for approval ... and stop") and every forced run
would fail for a reason that is the skill working as written, not the skill helping or hurting.

## Ground truth first: `validate.py`

```bash
python3 tests/outcome/validate.py
```

No model involved. For every task it checks that the hidden test **fails** on the untouched repo,
**passes** once `reference.patch` is applied to a copy, and that the visible tests pass in both.
It exits 1 on any failure. Run it after touching any fixture; a task that fails here would turn
every model run on it into noise.

## Running the matrix

```bash
cd tests/outcome
TASKS='debug-*' REPS=5 MODEL=sonnet PAR=4 NAME=debug-sonnet python3 drive.py
NAME=debug-sonnet python3 score.py
```

| knob | default | meaning |
|---|---|---|
| `TASKS` | `*` | a glob or a comma/space list of task ids or globs |
| `REPS` | `1` | runs per task per arm |
| `MODEL` | `sonnet` | passed to `--model` |
| `PAR` | `4` | sessions at once; both arms share the pool |
| `MAX_TURNS` | `40` | passed to `--max-turns` |
| `TIMEOUT` | `900` | seconds before a session is killed and graded as it stands |
| `NAME` | `main` | results go to `runs-<NAME>/`, gitignored, next to the harness |

Every run is isolated: `--setting-sources project,local --strict-mcp-config --plugin-dir
<this checkout's plugins/viby-toolkit>`, with `CLAUDECODE` stripped from the environment. That
drops the machine owner's user settings, so `drive.py` writes `runs-<NAME>/hooks-settings.json`
holding **only** the `hooks` block of `~/.claude/settings.json` and passes it with `--settings`;
his safety hooks (the guard against deleting shared roots among them) stay on. Each run works in a
fresh `git init`ed copy of `repo/` under `runs-<NAME>/<task>-<arm>-<rep>/`, with
`--dangerously-skip-permissions`. Never use `--bare` for the control arm: it skips credential
loading and every run answers "Not logged in".

## Cost warning

Every run is a full agent session of up to 40 turns. The smoke run (one debug task, both arms,
Sonnet, 2026-11-08) cost **$0.17** for the two sessions ($0.11 forced-skill, 10 turns; $0.06 bare,
7 turns), on the easiest task. Harder tasks and the 40-turn cap can cost several times that. A full 10-task matrix at `REPS=5` is 100
sessions; the sample sizes below are thousands. Decide the size on purpose, and start with one
skill.

## How many runs are needed

`score.py` prints the minimum detectable difference at the run's n: the smallest true gap between
two pass rates near 50% that n runs per arm would detect with 80% power at the 5% level,
`(1.96 + 0.84) × sqrt(2 × 0.25 / n)`.

| runs per arm | detectable difference |
|---|---|
| 30 | ~36 points |
| 50 | ~28 points |
| 100 | ~20 points |
| 200 | ~14 points |
| 360 | ~10 points |

So a 10-point effect, the size gstack's skill could not even show, needs about 360 runs **per arm**
for one skill. Per-task rows in the report are far smaller than that and are there to spot a broken
task, not to rank skills. Rates near 0% or 100% need fewer runs; a ceiling, where both arms pass
nearly everything, measures nothing.

## Reading the report

For each skill and arm: pass rate with a 95% Wilson interval, regressions in the visible tests,
median and total cost, cost per passing run, median and total turns and wall time. Then each
task's skill and bare pass counts and their difference, with the mean of those paired
differences. The verdict uses the same guard as routing: if the two arms' 95% intervals overlap,
`score.py` **refuses** to call a difference and says so.

## Not in the gate

This is not part of `cargo xtask check`: model runs cost real money and minutes. `validate.py`
alone runs in about two seconds with no model, so it could join the gate as a fixture check.
