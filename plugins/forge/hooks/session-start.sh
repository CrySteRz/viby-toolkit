#!/bin/sh
# Forge SessionStart hook.
# Injects a compact standing instruction so the toolkit's accuracy-first + token-discipline
# defaults apply on every project automatically, and the forge skills are discoverable.
# Kept deliberately short (~120 tokens) so the per-session cost is negligible.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "forge toolkit active. Default working style for this session:\n- Accuracy first (evidence gate): never claim done without running the check fresh and showing its output/exit code. The words 'should', 'probably', 'seems' mean you haven't verified — go verify. Ground findings in file:line; label unverified claims as hypotheses.\n- Fan-out law: fan out cheap read-only subagents for breadth (search/explore/review) and keep only their conclusions; keep WRITES single-threaded — parallel writers make conflicting decisions. Don't spawn agents for trivial or already-known work. Cheap models find; the main thread decides. Target 40-60% context; /clear between unrelated tasks.\n- Reach for forge skills when they fit: /forge:orchestrate (build end-to-end), /forge:review-cluster (review + adversarial false-positive filter), /forge:debug (root-cause), /forge:migrate (wide mechanical change), /forge:plan (plan first), /forge:learn (record a reusable lesson to memory). Full contract: /forge:forge-principles."
  }
}
EOF
