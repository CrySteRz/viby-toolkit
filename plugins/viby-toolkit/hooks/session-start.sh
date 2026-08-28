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
#
# The delegation line states fan-out as a DEFAULT rather than a permission. Phrased as a permission
# ("you may fan out"), nothing ever fanned out on its own. It splits one stage from staged work
# because the evidence splits there too: parallel read-only breadth is measurably good, a script is
# what keeps a multi-stage shape from decaying, and neither justifies parallel writers. The ceilings
# are here rather than in a reference because they are the ones that turn a fan-out negative when
# exceeded, and a reference read after the mistake is too late.

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "viby-toolkit active. Standing contract for this session:\n- EVIDENCE GATE: never claim done without running the check fresh and showing its command, output and exit code. A zero exit code is not a pass if zero tests ran, everything skipped, or the check was neutered. 'should', 'probably', 'seems' mean you have not verified — go verify. Ground every finding in file:line; label anything unverified as a hypothesis, and say plainly what you did not check.\n- FAN OUT BY DEFAULT, DON'T ASK: work needing breadth — search, explore, review, audit, research — gets 3-4 cheap read-only agents dispatched IN ONE MESSAGE, not worked through in sequence. When the work has stages (fan out, then verify each result, then synthesise) author a Workflow script instead, so the stages are declared rather than re-improvised; a viby-toolkit skill telling you to is itself the authorization to call Workflow. Either way every agent is READ-ONLY and every result comes back through you — writes stay on your single thread, because parallel writers make conflicting decisions.\n- WHEN NOT TO: a known file, a one-line change, a strictly sequential chain, or fewer than ~3 independent lines of inquiry — read it inline. Fan-out is a coverage tool; past a ~45% single-agent baseline coordination makes the answer worse, not just slower. Cheap models find, the strong main thread decides, escalate on low confidence. Target 40-60% context; /clear between unrelated tasks.\n- ROUTING: if WHAT to build is unsettled, decide that first (/viby-toolkit:brainstorm) — a clear ticket means it is already decided, so go straight to /viby-toolkit:plan or /viby-toolkit:orchestrate. Otherwise pick the viby-toolkit skill matching the task; their descriptions say when each applies. Full contract: /viby-toolkit:principles."
  }
}
EOF
