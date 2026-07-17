#!/bin/sh
# Forge SessionStart hook.
# Injects a compact standing instruction so the toolkit's accuracy-first + token-discipline
# defaults apply on every project automatically, and the forge skills are discoverable.
# Kept deliberately short (~120 tokens) so the per-session cost is negligible.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "forge toolkit active. Default working style for this session:\n- Accuracy first: never claim done without showing the evidence (test run, reproduced+fixed bug, file:line grounding). Label unverified claims as hypotheses.\n- Token/rate-limit discipline: for wide or unknown searches, fan out cheap read-only subagents (the `scout` agent) and keep only their conclusions — don't pull file dumps into main context. Don't spawn agents for trivial or already-known work. Cheap models find; the main thread decides.\n- Reach for forge skills when they fit: /forge:orchestrate (build a feature end-to-end), /forge:review-cluster (review a diff + adversarial false-positive filter), /forge:debug (root-cause a bug), /forge:migrate (wide mechanical change), /forge:plan (plan before building). See /forge:forge-principles for the full contract."
  }
}
EOF
