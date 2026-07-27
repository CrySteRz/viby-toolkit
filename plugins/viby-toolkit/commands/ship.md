---
description: Run the full viby-toolkit pipeline autonomously on a task — orchestrate, build, verify, self-review with the false-positive filter — and don't stop until it's verified done.
argument-hint: [what to build, fix, or change]
---

Take the following task and drive it all the way to a verified, self-reviewed result using
the viby-toolkit workflow. Be autonomous: make reasonable decisions and proceed rather than
asking permission for reversible steps; only stop for genuinely destructive actions or a
real scope decision that's the user's to make.

Task: $ARGUMENTS

Do this:

1. Invoke the `/viby-toolkit:orchestrate` skill and follow its full pipeline (scope → explore →
   plan → implement → verify → self-review) for this task, scaling effort to its size.
2. In the verify phase, run `/viby-toolkit:verify` — actually exercise the change, screen the
   output for silent-pass modes, and show the command + exit code. Do not report done on
   faith.
3. In the self-review phase, run `/viby-toolkit:review-cluster` on your own diff and fix every
   **confirmed** finding (re-verifying each fix). Surface any unconfirmed findings for the
   user to judge.
4. Follow `/viby-toolkit:principles` throughout: fan out cheap scout agents for breadth,
   keep the main thread's context clean, route work to the cheapest model that can do it
   correctly, and never trade correctness for cost.

End with a tight summary: what changed (file:line), the verification evidence, anything
you deliberately left and why, and follow-ups. Lead with the outcome.
