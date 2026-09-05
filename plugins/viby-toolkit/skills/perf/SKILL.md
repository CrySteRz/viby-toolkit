---
name: perf
description: >
  Always load for anything about speed, memory or cost — "this is slow", "optimize this", "why
  does it take so long", "reduce memory", "it times out", "make the build faster", "is this a
  bottleneck", or to check a performance claim.

---

# Perf (measure, or it didn't happen)

```
IRON LAW: NO PERFORMANCE CLAIM WITHOUT A BEFORE AND AN AFTER NUMBER,
          produced by the same command on the same machine.
          Profile to find the bottleneck. Never optimize the code that merely looks slow.
          A speedup that breaks a test is not a speedup.
```

This skill exists because of one measured gap. A 2026 study of 407 performance PRs
([arXiv 2512.21757](https://arxiv.org/abs/2512.21757), 324 agent-authored vs 83
human-authored) found that agents and humans pick **statistically indistinguishable
optimizations** (χ²=6.10, p=0.636) — but validate them very differently:

- agent PRs included explicit performance validation **45.7%** of the time vs **63.6%** for
  humans (p=0.007),
- and of the validated agent PRs, **67.2% relied on static reasoning** rather than
  measurement — only **25%** reported benchmark results, against **49%** for humans
  (χ²=12.43, p=0.006).

So the deficit is not knowing what to optimize. It is *proving it worked*. Everything below
is built around closing that specific gap. Follow `/viby-toolkit:principles`; profiling is read
work, so fan out by default — but the numbers must come from one machine, one command, measured on
the main thread.

## 1. Define the target before touching anything

- **Which metric?** Wall-clock latency, p95 vs mean, throughput, peak RSS, allocations,
  bundle size, cold start, query count, cost per request. "Faster" is not a target.
- **Which workload?** A representative input, not a toy. Performance work on unrepresentative
  input optimizes the wrong thing convincingly.
- **What is good enough?** A number and a reason ("p95 under 200 ms because the client times
  out at 250"). Without it there is no way to stop, and no way to justify the complexity a
  fast version usually costs.

If the answer is "I don't know yet, it's just slow" — that's fine, but say so, and go to
step 2 to find out rather than guessing at a fix.

## 2. Establish the baseline — this is the step that gets skipped

```bash
VIBY_HOME=$(
  for d in "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/ "$HOME"/Projects/*/*/viby-toolkit/plugins/viby-toolkit/; do
    d=${d%/}
    [ -f "$d/hooks/run.sh" ] && [ -d "$d/skills" ] && { echo "$d"; break; }
  done
)

DETECT="$VIBY_HOME/skills/verify/scripts/detect-stack.ts"
RUN="$VIBY_HOME/hooks/run.sh"
sh "$RUN" "$DETECT" .
```

That reports the project's benchmark command if one exists, and which **profilers are
actually installed** for this stack. If there is no benchmark command, create the smallest
one that exercises the real path and commit it — a repeatable measurement is a deliverable,
not scaffolding.

Record the baseline properly:

- **Repeat it.** One run is noise. Take several and report median plus spread; a change
  smaller than the run-to-run variance is not a result.
- **Pin the conditions:** same machine, same input, same build mode (release, not debug),
  warm vs cold stated, other load quiesced. Note the versions of runtime and deps.
- **Save the raw numbers** into the plan file or a scratch note. You will need the exact
  before-figure later, and remembered numbers drift.

Beware measuring the wrong thing: a JIT that needs warmup, a cache making the second run
free, a debug build, a benchmark the optimizer deleted because its result was unused.

## 3. Profile — let the data pick the target

**Do not optimize by reading.** Human and machine intuitions about hotspots are both
routinely wrong, and the cost of being wrong is a more complex codebase that is no faster.

- Use a real profiler (the detector lists what is installed) and find where time or memory
  actually goes. A sampling profiler on the real workload beats a micro-benchmark of the
  function you suspect.
- **Attribute to a specific line or call**, then ask *why*: algorithmic complexity, N+1
  queries, work repeated in a loop, blocking I/O on a hot path, serialization, lock
  contention, allocation churn, cache misses, an accidentally quadratic string build.
- **Check the cheap structural wins first**, because they usually dominate: an index that
  isn't there, a query in a loop, a missing cache, work done eagerly that could be lazy, a
  payload larger than it needs to be. These beat micro-optimization by orders of magnitude
  and cost less clarity.
- If the profile is flat, say so. A flat profile means the answer is architectural, not a
  hot line, and a series of micro-edits will waste effort.

## 4. Change one thing, then measure again

- **One change per measurement.** Bundled optimizations make it impossible to know which one
  worked — and some cancel out.
- Re-run the *same* benchmark, same conditions. Report before → after with the spread.
- **A change that does not move the number gets reverted**, however clever. Keeping it costs
  readability and buys nothing.
- **Correctness is a gate, not a footnote.** Run the relevant tests (`/viby-toolkit:verify`).
  Optimizations break behaviour in specific ways: changed float precision, altered ordering
  or stability, weakened error handling, a cache that can now serve stale data, a
  concurrency change that introduced a race. If the fast version needs new tests to prove it
  is still correct, write them (`/viby-toolkit:test`).

## 5. Know when to stop

Stop at the target from step 1. Then state the trade-off you made, because there almost
always is one: more memory for less time, more complexity for less latency, a cache that
now needs invalidating, a denormalization that can drift. An optimization whose cost is not
stated will be maintained by someone who does not know it is load-bearing — so leave a
comment where the code is non-obvious *because* it is fast, and record anything durable with
`/viby-toolkit:learn`.

## Output

- The metric, the workload, and the target.
- **Before → after**, with the exact command, the repeat count, and the spread.
- Where the profile said the time went, at `file:line` — and what you changed there.
- The correctness evidence: which tests ran, and their result.
- The trade-off accepted, and what you tried that did **not** help (this is genuinely useful
  — it stops the next person repeating it).

If you could not measure, say that plainly and label the work a hypothesis. An unmeasured
optimization is a guess with extra confidence, and per the study above, that is the single
most likely way this task goes wrong.
