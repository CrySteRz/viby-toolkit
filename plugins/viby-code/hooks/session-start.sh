#!/bin/sh
# Viby-code SessionStart hook.
# Injects a compact standing instruction so the toolkit's accuracy-first + token-discipline
# defaults apply on every project automatically, and the viby-code skills are discoverable.
# Kept deliberately short so the per-session cost is negligible: for every line, ask
# "would removing this cause a mistake?" — if not, cut it.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "viby-code toolkit active. Default working style for this session:\n- Accuracy first (evidence gate): never claim done without running the check fresh and showing its output/exit code — and a zero exit code isn't a pass if zero tests ran. The words 'should', 'probably', 'seems' mean you haven't verified — go verify. Ground findings in file:line; label unverified claims as hypotheses.\n- Fan-out law: fan out cheap read-only subagents for breadth (search/explore/review) and keep only their conclusions; keep WRITES single-threaded — parallel writers make conflicting decisions. Don't spawn agents for trivial or already-known work. Cheap models find; the strong main thread decides; escalate on low confidence. Target 40-60% context; /clear between unrelated tasks.\n- When the WHAT is unclear (fuzzy idea, ambiguous ticket), decide it first with /viby-code:brainstorm; when you already have a clear ticket/spec, skip brainstorm and go to /viby-code:plan or /viby-code:orchestrate. Reach for viby-code skills when they fit: /viby-code:orchestrate (build end-to-end), /viby-code:explore (unfamiliar codebase — detects the stack, maps it, any language), /viby-code:verify (prove it works before claiming done), /viby-code:test (design/audit tests; see each new test fail before trusting it — don't over-mock), /viby-code:secure (credentials first, then supply chain/CI, then code), /viby-code:review-cluster (review + grounded validator false-positive filter), /viby-code:debug (root-cause, repro-test-first), /viby-code:migrate (wide change), /viby-code:refactor (restructure without changing behaviour — pin it with tests first), /viby-code:perf (measure before and after; never optimise by reading), /viby-code:learn (record a lesson), /viby-code:handoff (save mid-task state), /viby-code:worktrees (isolate parallel work). Full contract: /viby-code:principles."
  }
}
EOF
