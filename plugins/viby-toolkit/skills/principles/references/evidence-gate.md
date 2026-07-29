# §5–§6 in depth — the evidence gate and adversarial verification

Read when you are about to claim something works, or about to surface a finding to the user.

## Why the gate exists

Claude stops when work *looks* done; absent a check, "looks done" is the only signal it has. The gate
replaces that signal with evidence.

Red-flag words that mean you are about to violate it: "should", "probably", "seems to", and a
premature "Done!/Perfect!/Great!". If you did not run the check, say so explicitly. Claiming complete
without verification is dishonesty, not efficiency.

## Label every claim measured, inferred, or not tested

In the table or line where the claim appears — not in a caveat at the end. "Equivalent to the one we
did test" is a legitimate finding *and* an inference; written as one, a reader can weight it. Label at
the point of the claim because unlabeled inferences get promoted to results by your own summary three
paragraphs later, and by then nothing distinguishes them.

## What a zero exit code does not prove

Zero tests collected, an all-skip run, a `|| true`, a `continue-on-error`, or a cached result all
exit 0. `/viby-toolkit:verify` runs this as a procedure: find the real checks, scope them to the
change, exercise the actual behaviour, then screen the output for those silent-pass modes.

**TDAD nuance:** run the **specific tests relevant to the change**, named explicitly — not a generic
"do TDD" ritual. Telling an agent *which* tests to check cuts regressions; a vague TDD lecture makes
them worse.

**The same gate applies to the tests themselves:** a test never observed failing is not known to test
anything. Coverage proves a line executed, not that a wrong value would be caught — a suite can be
fully covered and still survive nearly every mutation. See each new test go red for the right reason
before trusting it, and be suspicious of tests that assert on mocks rather than outcomes
(`/viby-toolkit:test`).

## Why one fresh validator beats a vote

Same-family model panels share blind spots, so a majority can rubber-stamp a correlated
hallucination. One independent validator that will *execute* a checkable claim beats N agreeing
opinions. A gap-hunting reviewer always finds gaps — so reviewers flag **correctness only** (taste →
`/simplify`), and validators see the claim, not the author's reasoning. Prefer a fresh-context
reviewer over self-review: models are weak at judging their own output. Full protocol in
`review-cluster`.

## Do not trust a confidence number

Agents are systematically overconfident: measured, some that succeed only **22%** of the time predict
**77%** success (arXiv 2602.06948). A subagent's self-reported confidence is a weak input, never the
gate. Prefer an executed check. When you must elicit a judgement, use an **adversarial framing** —
reframing assessment as bug-finding measured the best calibration of the methods tried.
Counterintuitively a **pre-execution** estimate ("can this be done, and what would make it fail?")
discriminated better than post-execution self-review, despite having less information.

Corollary for subagents: **an agent's own success report is not evidence.** Check the diff.
