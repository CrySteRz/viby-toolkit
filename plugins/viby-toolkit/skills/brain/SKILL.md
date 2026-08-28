---
name: brain
description: >
  Load when the question is about the memory system itself, not one lesson — "what do you
  remember", "audit your memory", "clean up the memory", "you're working from something outdated",
  "is that still true". Not /viby-toolkit:learn.

---

# Brain (the memory has to be maintained, or it starts lying)

```
IRON LAW: Every memory carries HOW IT WAS ESTABLISHED and WHEN. An entry that cannot be checked
          cannot be retired, and an entry that cannot be retired eventually misleads with the
          authority of something the agent "knows".
          Findability beats richness: an entry nobody retrieves has no effect at all.
```

A memory store fails silently. Nothing errors when an entry goes stale, contradicts another, or
becomes unfindable — the session just quietly reasons from it. Follow `/viby-toolkit:principles`;
sources for every claim here in `references/methods.md`.

## 1. Three kinds of memory, and this toolkit already has all three

The agent ecosystem converged on the taxonomy cognitive science has used for decades, and mapping it
onto what exists here is clarifying:

| Kind | What it holds | Here | Lifetime |
|---|---|---|---|
| **Episodic** | what happened, in sequence — this task, these decisions, where we stopped | `/viby-toolkit:handoff` | one task; delete it after |
| **Semantic** | what is generally true about this project, machine or user | `/viby-toolkit:learn` → project memory | until superseded |
| **Procedural** | how to do a kind of work — reusable strategy | **the skills themselves** | until the practice changes |

Two consequences. **A skill is procedural memory**, so improving one is a memory operation and
belongs to `/viby-toolkit:extend`. And **most things people want to "remember" are episodic**, which
means they should expire with the task rather than be written to semantic memory — that is the single
biggest cause of a store full of things that used to be true.

## 2. What earns a place in semantic memory

Write it only if it is **durable**, **non-obvious**, and **not derivable**:

- **Durable** — still true next month. A file's current location is not; the reason it lives there is.
- **Non-obvious** — cost someone real time to discover. A gotcha, a footgun, a build quirk.
- **Not derivable** — the repo, `git log` and the docs are the source of truth for what the code *is*
  (`/viby-toolkit:principles` §9: authored vs derived). Recording derived facts guarantees drift,
  because the code changes and the memory doesn't.

Highest-value entries, in order: a **rejected option and the bar it failed** · a **trap with its
symptom** so the next session recognises it before paying for it · a **user preference stated once**
· a **known past failure for a module**, which raises recall on future review · a **never-compact
lesson** where losing something specific broke a task.

## 3. Every entry carries provenance, a date, and its outcome

Three fields, each preventing a distinct failure:

- **How it was established** — measured, verified by running X, observed in logs, or *stated by the
  user*. This is the defence against poisoning: the proposed mechanism in the literature is
  reliability-conditional updating with a **provenance cap**, and you cannot cap what you cannot
  see. A guess and an executed check must never carry equal weight later.
- **When** — an undated memory can never be retired on evidence, only on suspicion.
- **Whether it worked** — retrieval that prefers *successful* precedent needs to know the outcome.
  An approach recorded without its result is as likely to be repeated after it failed.

**Store what was established, not what was said.** Writing a user's claim as a fact is how a store
becomes sycophantic, and there is a benchmark for exactly that failure. "The user believes the API
is rate-limited" and "the API is rate-limited (429 observed at 15:02)" are different memories.

## 4. Retrieval is the bottleneck, so optimise findability

Measured across four memory benchmarks, answer errors concentrate in cases where **retrieval
failed**; retrieval succeeding but the answer still being wrong accounts for only 5.8–13.7%. The
lever is not richer entries — it is entries that come back when they are relevant:

- **One fact per entry, named for the situation it applies to**, not for the topic. `machine-git-
  https-not-ssh` is retrievable; `git-notes` is not.
- **An index line per entry** with a *hook* — the circumstance that should make you reach for it.
- **Link related entries** so retrieving one surfaces its neighbours.
- **Keep entries small.** An entry that grew into a document drags the whole document into context
  on every partial match, which is a context-discipline failure as well as a retrieval one.

## 5. Retire and reconcile — the part nobody does

A real store must forget. Run the audit, on a schedule and after any big refactor:

```bash
MEM=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/brain/scripts/check-memory.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$MEM" <memory-dir> --root <the project this memory is about>
```

It reports entries citing paths that no longer exist, entries with no date or provenance, claims
that rest on assertion, entries missing from the index (and index links pointing at nothing),
near-duplicate topics to merge, and entries that have grown into documents.

Then act on the ones a script cannot decide:

- **Superseded, not wrong.** The hardest case in the literature: a previously valid memory rendered
  obsolete by a *structurally related but differently worded* observation. When you learn something
  that overlaps an existing entry, ask explicitly whether it **replaces** it. If it does, edit that
  entry — do not add a second one, or the store now contains both answers.
- **Contradiction is a finding, not a merge.** If two entries disagree and you cannot tell which is
  current, say so in the entry and put the way to settle it in the same line.
- **Delete confidently.** A memory you would not defend today is not knowledge, it is sediment.
- **Never let memory outrank the code.** Recalled memories are point-in-time observations. If one
  names a file, function or flag, verify it still exists before acting — that check is cheap and the
  alternative is confident wrongness.

## Output

- What changed in the store: entries added, edited, merged, retired — and why each.
- The audit result, and anything it flagged that you deliberately kept.
- Anything you could not resolve: contradictions left standing, with how to settle them.
