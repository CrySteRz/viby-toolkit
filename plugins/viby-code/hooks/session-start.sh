#!/bin/sh
# Viby-code SessionStart hook.
# Injects a compact standing instruction so the toolkit's accuracy-first + token-discipline
# defaults apply on every project automatically, and the viby-code skills are discoverable.
# Kept deliberately short (~120 tokens) so the per-session cost is negligible.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "viby-code toolkit active. Default working style for this session:\n- Accuracy first (evidence gate): never claim done without running the check fresh and showing its output/exit code — and a zero exit code isn't a pass if zero tests ran. The words 'should', 'probably', 'seems' mean you haven't verified — go verify. Ground findings in file:line; label unverified claims as hypotheses.\n- Fan-out law: fan out cheap read-only subagents for breadth (search/explore/review) and keep only their conclusions; keep WRITES single-threaded — parallel writers make conflicting decisions. Don't spawn agents for trivial or already-known work. Cheap models find; the strong main thread decides; escalate on low confidence. Target 40-60% context; /clear between unrelated tasks.\n- When the WHAT is unclear (fuzzy idea, ambiguous ticket), decide it first with /viby-code:brainstorm; when you already have a clear ticket/spec, skip brainstorm and go to /viby-code:plan or /viby-code:orchestrate. Reach for viby-code skills when they fit: /viby-code:orchestrate (build end-to-end), /viby-code:verify (prove it works before claiming done), /viby-code:review-cluster (review + grounded validator false-positive filter), /viby-code:debug (root-cause, repro-test-first), /viby-code:migrate (wide change), /viby-code:learn (record a lesson), /viby-code:handoff (save mid-task state), /viby-code:worktrees (isolate parallel work). Full contract: /viby-code:principles.\n- A PreToolUse safety guard blocks a few genuinely destructive Bash commands (recursive delete of /, home, or an unexpanded variable; force-push to a protected branch; secret reads). Scoped cleanup like 'rm -rf node_modules' is allowed. Set VIBY_SAFETY=off to disable, or =strict for a paranoid posture."
  }
}
EOF
