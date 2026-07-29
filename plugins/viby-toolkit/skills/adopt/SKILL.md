---
name: adopt
description: >
  Use when taking on code you did not write and do not trust yet, and bringing it up to
  standard — "someone handed us this repo", "we inherited this codebase", "the contractor
  delivered it", "take over this project", "clean this up to our conventions", "this came from
  a vendor / an agency / an old team". Not /viby-toolkit:refactor, /viby-toolkit:explore.
---

# Adopt (inherit foreign code, conform it, prove it still works)

```
IRON LAW: Capture the behaviour BEFORE you change a line, and keep the acceptance suite OUT of
          the agents' reach. Nothing is "done" until each required functionality is demonstrated
          by a check the refactoring agents never saw — and until the safety net is proven to
          have grown, not shrunk.
```

Two failure modes define this work, and both are measured. First, **repository-level refactoring
is genuinely hard**: on a benchmark of 1,099 developer-written, behaviour-preserving refactorings
mined from real projects, the best model succeeded **41.58%** of the time, and an agent managed
**39.4%** on compound cases — so roughly six attempts in ten fail, and multi-agent workflows
helped more than any other single strategy. Second, under that failure pressure the cheapest
route to a green run is **to change the check instead of the code**. Measured: trajectory-level
behaviour monitoring "reduces average hacked-resolved rate from **28.57% to 0.56%**, while
improving clean resolved rate from **40.22% to 60.53%**". Watching for the shortcut did not merely
stop cheating — it made the honest work **half again more likely to land**.

That is the whole design: fan out aggressively, and gate mechanically. Follow
`/viby-toolkit:principles`. Sources: `references/methods.md`.

## 1. The provenance gate — before you invest a single hour

Foreign code is a supply-chain event, not just a code review. In order:

- **Licence.** May you use, modify and ship it, and does its licence infect yours? Check every
  vendored directory and dependency, not just the top-level file. A week of refactoring code you
  cannot legally ship is the most expensive possible mistake here.
- **Secrets, including in history.** Someone else's repo routinely arrives with live credentials
  in old commits. Rotate anything found — it is compromised the moment it reaches you.
- **What does it do at runtime?** Network calls, `exec`/`eval`, obfuscated or minified blobs,
  install-time scripts, unexpected telemetry. Read the dependency list for typosquats and
  unmaintained packages.
- **Provenance of the code itself.** Who wrote it, is it actually theirs to hand over, and is any
  of it copied wholesale from somewhere with different terms.

Run `/viby-toolkit:secure` for the deep pass. **Stop and report** if the licence or a live secret
is unresolved: that is a decision for the user, not something to refactor around.

## 2. Map it, and find the seams

`/viby-toolkit:explore` first — you cannot conform code you cannot describe. Beyond the normal
map, look for what makes this *adoptable*: where the **seams** are (places you can change
behaviour without editing that code — an injection point, a config hook, a boundary you can wrap),
what the entry points are, and where state lives. Price the read first with the read-cost meter;
inherited repos are usually bigger than the task implies.

**Assume there are no usable tests.** That is essentially the definition of the problem: code
without tests, whatever its age. If tests do exist, verify they *fail when the code is broken*
before trusting them — an inherited suite that passes unconditionally is worse than none.

## 3. Capture the behaviour before you touch it

You cannot preserve behaviour you never recorded. Before any edit:

- **Characterization / approval tests (golden master).** Run the code on realistic inputs, record
  whatever it outputs, and pin that as the baseline — explicitly *without judging whether it is
  correct*. The point is not "this is right", it is "this is what it did on Tuesday". Weird output
  gets pinned as weird; if it turns out to be a bug, that is a separate, deliberate change.
- **Feed it real inputs.** Recorded production or sample inputs beat invented ones, because they
  exercise the paths that actually matter and the edge cases nobody documented.
- **Keep the original runnable, and diff against it.** If you can still execute the old code, run
  old and new on the same inputs and compare outputs — differential testing gives you an oracle
  without anyone writing a spec, which is exactly what you lack here. Any divergence is a finding
  until explained.
- **Sprout and wrap before you cut.** To add behaviour, add it in a new unit and call it (sprout),
  or wrap the existing call so the old path stays intact (wrap), and only break dependencies at a
  seam once the region is pinned. Do not begin by restructuring the tangle.

Hand the test design itself to `/viby-toolkit:test`.

## 4. Write the functionality matrix — and hold part of it back

The user's real requirement is "at the end it still does what it must", so make that explicit and
checkable rather than a feeling:

| ID | Required functionality | How it is verified | Pass criteria | Visible to agents? |
|---|---|---|---|---|
| F-1 | imports a CSV of any delimiter | test | 3 fixtures round-trip byte-identical | yes |
| F-2 | rejects a malformed row without aborting the batch | test | 1 bad row → 1 error, other rows land | **no — held out** |
| F-3 | admin UI still renders the report | demo | screenshot + one manual pass | no |

Two rules make it work:

- **Every requirement names its verification method and its pass criterion** — test, demonstration,
  or inspection — decided now, not after the work, and traceable both ways: every requirement has
  a check, and every check maps to a requirement.
- **Hold a slice of the acceptance suite out of the agents' reach.** Give the workers a visible
  suite to iterate against and keep a hidden one for the final gate. This is the split that
  benchmarks use to detect agents that overfit to the tests they can see, and it is the only
  version of "we have the required functionality" that survives contact with an optimiser. Keep
  the held-out suite outside the worktree the agents write in.

## 5. Refactor in Mikado steps, not in one heroic pass

Inherited code punishes plans made from the outside, so discover the order by trying:

1. Attempt the change you want, naively.
2. If it breaks something, **do not push through**. Write down what has to be true first — that is
   a prerequisite — and **revert**.
3. Do the prerequisites, deepest first, each landing green on its own.
4. Repeat. The graph you build *is* the plan.

The reverting feels wasteful and isn't: the discarded attempt bought you the dependency map, which
is the thing you actually lacked. It is also what produces the partition the fan-out law demands —
`/viby-toolkit:principles` §3 permits parallel writes only when you can name the partition and the
hub files, and a Mikado graph is exactly that naming. Take the hub nodes yourself, sequentially.

For a genuinely large adoption, prefer **strangle over rewrite**: stand the new implementation
beside the old, route traffic across piece by piece, and delete the old path only once nothing
calls it. A twelve-month rewrite abandoned at month eighteen is the default outcome otherwise.

## 6. Conform to the language and to the instructions — in that order of specificity

"Refactor it to our standards" has three inputs, and they rank:

1. **The user's explicit instructions** win. If they conflict with convention, follow the
   instruction and say plainly which convention you broke and where.
2. **The target language's own idiom** comes next — detect it mechanically rather than assuming
   (`skills/verify/scripts/detect-stack.ts`), and write what a native of *that* language would
   write. Do not carry one language's patterns into another: a Python file full of Java ceremony,
   or a Go file full of inheritance, is worse than what you inherited, because it is now uncanny
   as well as unfamiliar.
3. **The project's existing conventions** last, but they still bind: naming, error handling,
   logging, layout, and how similar things are already done here.

Conform in the order a reviewer can follow: mechanical and formatting-only changes in their own
commit, then structural moves, then anything that touches behaviour. Never mix them — a diff that
reformats and restructures at once is unreviewable, and unreviewable is where inherited bugs hide.

## 7. Run it with agents, and monitor the trajectory

- **Fan out for reading**: one `scout` per subsystem to map, one per language in a polyglot repo.
- **Partition the writes** by the Mikado graph, one `implementer` per independent node, each in
  its own worktree (`/viby-toolkit:worktrees` — including the decision about where shared
  generated artifacts live).
- **Give every worker the same brief**: the invariant it must preserve, the characterization tests
  that pin it, the language idiom, the instruction, and the explicit rule that **tests are not
  editable** for this task. Most multi-agent failures are interface failures, not reasoning
  failures — spend the care on the brief.
- **Then check what they did to the safety net, not just whether it is green:**

```bash
DRIFT=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/adopt/scripts/check-test-drift.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$DRIFT" --base <ref-before-the-agents> .     # exit 1 = the net shrank
```

Deleted test files, removed test cases, removed assertions, new skips, a new `.only`, or a
zero-status exit inside the suite are **failures of the task**, not progress toward it. Treat any
of them as a rejected result and re-brief; never accept a green run that got there by shrinking
the net.

## 8. The acceptance gate

In order, and none of them optional:

1. Characterization suite green — the recorded behaviour is unchanged.
2. `check-test-drift.ts` clean — the net grew or held.
3. **The held-out acceptance suite green**, run once, at the end, by you rather than the agents.
4. Every row of the functionality matrix demonstrated, with the evidence attached.
5. `/viby-toolkit:verify` over the whole thing, to catch the silent-pass modes — zero tests
   collected, an all-skip run, a cached result.

Anything that fails goes back to §5, not into the report.

## Output — the adoption record

- **Provenance**: licence verdict, secrets found and rotated, runtime behaviour notes, and what
  you could not clear.
- **What it does** — the map, and the behaviour you pinned, including the weird bits you pinned
  deliberately as-is.
- **The functionality matrix**, filled in, with which rows were held out.
- **What changed**, split into mechanical / structural / behavioural, and the conventions you
  broke on instruction.
- **The Mikado graph** as it ended up — this is the durable artifact for whoever adopts it next.
- **Test-net delta**: tests and assertions before → after.
- **What is still not trusted**, and what it would take to trust it.

Record the durable lessons with `/viby-toolkit:learn` — inherited-code traps are the highest-value
memories there are, because nobody else in the project knows them either.
