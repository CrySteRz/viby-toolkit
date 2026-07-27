#!/bin/sh
# Viby-code SessionStart hook.
#
# Injects the standing working-style contract so it applies on every project automatically.
#
# DELIBERATELY SHORT, and kept that way. This text is in EVERY session on EVERY project, so
# the plugin's own rule applies hardest here: for each line ask "would removing this cause a
# mistake?" If not, cut it. A bloated always-on preamble makes Claude skim the parts that
# matter — the failure mode is silent.
#
# It used to enumerate all fourteen skills (~465 tokens). That list was redundant: skill
# DESCRIPTIONS are already loaded for discovery, so naming them here bought nothing and grew
# with every addition. What stays is only what a description cannot convey: the accuracy
# contract, the delegation law, and the one routing rule that is genuinely non-obvious.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "viby-code active. Standing contract for this session:\n- EVIDENCE GATE: never claim done without running the check fresh and showing its command, output and exit code. A zero exit code is not a pass if zero tests ran, everything skipped, or the check was neutered. 'should', 'probably', 'seems' mean you have not verified — go verify. Ground every finding in file:line; label anything unverified as a hypothesis, and say plainly what you did not check.\n- FAN-OUT LAW: fan out cheap read-only subagents for breadth (search, explore, review) and keep only their conclusions; keep WRITES single-threaded, because parallel writers make conflicting decisions. Don't spawn agents for trivial or already-known work. Cheap models find, the strong main thread decides, escalate on low confidence. Target 40-60% context; /clear between unrelated tasks.\n- ROUTING: if WHAT to build is unsettled, decide that first (/viby-code:brainstorm) — a clear ticket means it is already decided, so go straight to /viby-code:plan or /viby-code:orchestrate. Otherwise pick the viby-code skill matching the task; their descriptions say when each applies. Full contract: /viby-code:principles."
  }
}
EOF
