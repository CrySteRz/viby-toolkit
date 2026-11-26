#!/bin/sh
# Viby-code SessionStart hook.
#
# Injects the standing working-style contract so it applies on every project automatically.
# It also fires on `compact` (hooks.json), so the contract is restated after a compaction instead
# of surviving only as a paraphrase in the summary.
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
#
# SMALLEST COMPLETE CHANGE is the only guidance anywhere in the toolkit on how MUCH code to write.
# Adopted 2026-10-15 from Ponytail (github.com/dietrichgebert/ponytail), whose self-run benchmark
# (Opus 5.5, 39 tasks x 5 runs) measured -45% output tokens and -26% cost at equal hidden-check
# passes. The caller-grep sentence is its one rule with a before/after: 1/6 -> 6/6 on a trace bug,
# where a plain-prose version of the same idea scored 0/3, so keep the wording operational.
# implementer.md and worker.md carry the same rule because subagents never receive this hook.
#
# The COST line is main-session only, which is why it lives here (subagents do not receive this
# hook). Measured 2026-10-13 on the owner's transcripts: a main-session cache was rebuilt after 0-1.2%
# of gaps up to 60 min but 77% at 60-65 min, hence a single wake-up at ~55 min while a run is in
# flight; sleep-polling and reading big outputs inline were the other two main-session leaks.
# 2026-11-26: 145 `sleep N>=20` calls and 43 "status?/done?" pings in 12 of 71 sessions showed that
# the wake-up clause alone was not holding, so the line now names what to arm before a turn ends.
# FAN OUT names the typed agents because 59 of 85 untyped general-purpose agents in that window
# were read-only audits that a scout, researcher or worker would have run on a smaller context.
#
# ACT vs ASK, same window: 6 corrections for acting beyond the question (unasked tickets and
# comments), 22 bare "yes/go" replies to asks about reversible steps, a merge that went through on
# a failed CI run, and a retried push that duplicated an outward action.

# Windows starts the MCP servers without a shell, so `launcher/viby` there has to be a real
# viby.exe already: put it beside the launcher before they start. Its output would corrupt the
# JSON below. $OS is set by Windows itself, so other systems pay for no extra process.
if [ "${OS:-}" = Windows_NT ]; then
  sh "$(dirname -- "$0")/../launcher/viby" --install >/dev/null 2>&1 || true
fi

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "viby-toolkit active. Standing contract for this session:\n- EVIDENCE GATE: never claim done without running the check fresh and showing its command, output and exit code. A zero exit code is not a pass if zero tests ran, everything skipped, or the check was neutered. 'should', 'probably', 'seems' mean you have not verified — go verify. Ground every finding in file:line; label anything unverified as a hypothesis, and say plainly what you did not check.\n- SMALLEST COMPLETE CHANGE: before writing code, take the first option that fully works — what the repo already has, then the standard library or platform, then an installed dependency (never add one for a few lines), only then the minimum new code. No option, wrapper or abstraction nobody asked for. Bug fix: grep every caller of the function you touch, then fix the root cause once. Never cut validation at trust boundaries, data-loss handling, security or accessibility.\n- FAN OUT BY DEFAULT, DON'T ASK: work needing breadth — search, explore, review, audit, research — gets 3-4 read-only typed agents (viby-toolkit:scout for code, researcher for the web, worker when it needs an MCP tool; never the untyped general-purpose one) dispatched IN ONE MESSAGE, not worked through in sequence. When the work has stages (fan out, then verify each result, then synthesise) author a Workflow script instead, so the stages are declared rather than re-improvised; a viby-toolkit skill telling you to is itself the authorization to call Workflow. Either way every agent is READ-ONLY and every result comes back through you — writes stay on your single thread, because parallel writers make conflicting decisions.\n- WHEN NOT TO: a known file, a one-line change, a strictly sequential chain, or fewer than ~3 independent lines of inquiry — read it inline. Fan-out is a coverage tool; past a ~45% single-agent baseline coordination makes the answer worse, not just slower. Cheap models find, the strong main thread decides, escalate on low confidence. Target 40-60% context; /clear between unrelated tasks.\n- COST: never end a turn on 'waiting for X' with nothing armed: a background Workflow or agent wakes you itself; for CI, a deploy or a build, start a Monitor or a background command whose exit wakes you, and say when the next report comes. Past ~55 min also arm one wake-up (the cache expires at 60); never sleep-poll; send reads over ~10K chars to an agent that returns a digest.\n- ACT vs ASK: answer the question asked; a how-to is not an order. Reversible local steps: act and report. What another person sees or what cannot be undone (tickets, messages, comments, merges, deploys, money): show the exact text or command and wait for a yes on it; a general 'keep going' is not that yes. After an interrupted outward command, check the remote before retrying.\n- ROUTING: if WHAT to build is unsettled, decide that first (/viby-toolkit:brainstorm) — a clear ticket means it is already decided, so go straight to /viby-toolkit:plan or /viby-toolkit:orchestrate. Otherwise pick the viby-toolkit skill matching the task; their descriptions say when each applies. Full contract: /viby-toolkit:principles."
  }
}
EOF
