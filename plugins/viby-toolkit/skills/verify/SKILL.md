---
name: verify
description: >
  Use before claiming any change is done, working, or fixed — the evidence gate made
  executable. Use when the user says "is it working", "did that work", "verify this",
  "prove it", "is this ready to ship", "are we done", "are the tests passing", or when you're
  about to write "should work" /
  "that should fix it". Also the Verify phase of /viby-toolkit:orchestrate and the closing
  step of /viby-toolkit:debug and /viby-toolkit:migrate. Distinct from /viby-toolkit:test,
  which designs and writes tests — this one runs the project's real checks and screens their
  output for silent passes.
---

# Verify (the evidence gate, executed)

```
IRON LAW: A completion claim requires a command you ran FRESH, its real output, and its
          exit code. No exit code, no claim. "Should work" is not a result.
          If it failed, the failure IS the deliverable — report it, don't bury it.
```

Claude stops when work *looks* done; absent a check, "looks done" is the only signal it
has. This skill replaces that signal with evidence. Follow `/viby-toolkit:principles` §5.

## 1. Identify the real checks — don't guess them

Guessing `npm test` on a project that uses `pnpm vitest run` wastes a cycle and produces a
misleading failure. **Run the detector first** — it works for any language and reports where
each answer came from:

```bash
DETECT=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/verify/scripts/detect-stack.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$DETECT" .
```

It ranks **[ci] above [task-runner] above [convention]**, and prints `unknown` rather than
inventing a command — so an empty test slot is real information, not a detector failure. If
it reports the repo is polyglot, one test command almost certainly does not cover it.

Then confirm and fill gaps by hand:

- **CI config is authoritative** — `.github/workflows/*.yml`, `.gitlab-ci.yml`, or similar
  define what "green" means for this repo. Read it first; it's the contract.
- Then the task runner: `package.json` scripts, `Makefile`, `justfile`, `pyproject.toml`
  (`[tool.poetry.scripts]`, `[tool.pytest.ini_options]`), `tox.ini`, `Cargo.toml`,
  `go.mod`, `mix.exs`, `composer.json`.
- Respect the package manager actually in use — infer it from the lockfile
  (`pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`, `package-lock.json`, `uv.lock`), not habit.
- If a project memory or CLAUDE.md records the command (see `/viby-toolkit:learn`), use it —
  but confirm it still exists before relying on it.

If you genuinely cannot find a check, say so explicitly rather than inventing one. "No
test suite exists in this repo" is an honest, useful result; a fabricated passing claim
is not.

## 2. Scope the check to the change — narrow first, then widen

Naming the *specific* tests relevant to the change is what actually cuts regressions; a
generic full-suite ritual is slower and teaches nothing. So:

1. **Named relevant tests first** — the test file(s) covering what you touched, run by name.
2. **Then the broader gate** — typecheck / build / lint, and the wider suite if the change
   could reach beyond its own module.

For a behavior change, tests alone are not enough — continue to step 3.

## 3. Exercise the actual behavior, not just its tests

A green suite proves the tests pass, not that the feature works. For anything with runtime
behavior, drive the real path and observe real output:

- CLI → invoke it with real arguments and read what it prints.
- HTTP endpoint → hit it (`curl`) and read the status and body.
- UI → render it and look (`/run` or the project's dev-server skill; screenshot it).
- Data/migration → query the resulting state, don't infer it from the migration file.
- Library function → call it in a throwaway script with a realistic input.

**Wait on observable conditions, never on `sleep`.** Poll for the state or await the
event. An arbitrary timeout is how flaky verification is born.

## 4. Read the output — a zero exit code is not automatically a pass

This is where verification most often lies to you. Check for the silent-pass modes:

- **Zero tests ran.** "0 passed", "no tests collected", "Ran 0 tests" with exit 0.
- **Everything skipped.** All-skip is not all-pass.
- **The check was neutered** — `|| true`, `continue-on-error`, `--exit-zero`, a mock that
  swallows the assertion, a suppressed non-zero exit inside a pipeline (`set -o pipefail`
  is off by default).
- **Cached or stale output** — a build system reporting success without rebuilding, or a
  test runner replaying a cached pass. Force a fresh run if in doubt.
- **Wrong target** — the command succeeded against a different file, package, or
  environment than the one you changed.
- **The failure moved** — the original error is gone but a new one appeared downstream.

If the diff *is* a guard (a CI gate, a lint rule, a coverage threshold, a test), verify it
**fails when it should**: break the thing deliberately, confirm the guard goes red, restore.
A guard never observed failing is not known to work.

## 5. Report the evidence, not a summary of your confidence

For each check, show: the **exact command**, its **exit code**, and the **decisive output
lines** (the pass/fail summary, counts, the error). Trim the noise, keep the proof.

```
$ pnpm vitest run src/auth/session.test.ts      → exit 0   ·  14 passed, 0 skipped
$ pnpm tsc --noEmit                             → exit 0   ·  no errors
$ curl -s -o /dev/null -w '%{http_code}' localhost:3000/api/session   → 200
```

## 6. When it fails

The failure is the result — report it plainly, with the output. Then:

- Fix the **code**, never the check. Editing a test so it stops failing, adding `|| true`,
  loosening an assertion, or bumping a timeout to make things pass is falsifying evidence.
  If a test is genuinely wrong, say so explicitly and separately — don't quietly retune it.
- Re-run the same check after the fix. A fix isn't verified by a *different* command.
- Two failed correction attempts on the same issue → stop patching. Reassess (and consider
  `/viby-toolkit:debug` for a real root-cause pass) rather than iterating blind.
- Never present a partially-verified change as done. Say exactly what you verified, what
  you couldn't, and why.

## Anti-patterns (each one is a false completion claim)

- Claiming from memory of a run made *before* the last edit.
- "The logic looks correct" / "this should work" in place of running anything.
- Reporting the pass and hiding the unrelated failure that appeared alongside it.
- Verifying only the happy path when the change was about error handling.
- Declaring done with uncommitted debug prints, skipped tests, or a `.only` still in place.
