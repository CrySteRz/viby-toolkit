---
name: explore
description: >
  Use when meeting an unfamiliar codebase or an unfamiliar corner of a known one — "what is
  this repo", "how does X work here", "where does Y live", "walk me through this", "I just
  cloned this", "help me get oriented", "where should this change go". Also the research
  phase of /viby-code:orchestrate when the area is genuinely unknown. Builds a durable map
  instead of a one-off answer.
---

# Explore (build a map, not a tour)

```
IRON LAW: Report the code that EXISTS, at file:line. Never describe the architecture you
          would expect a project like this to have.
          Say "I did not find X" — never let absence imply absence of evidence.
```

The failure mode here is fluent fiction: a confident description of a conventional layout
that this repo does not actually use. Every claim needs a location. Follow
`/viby-code:principles` — this is pure read work, so **fan out**.

## 1. Get the ground truth mechanically, before reading anything

```bash
DETECT=$(ls "$HOME"/.claude/plugins/cache/*/viby-code/*/skills/verify/scripts/detect-stack.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-code/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$DETECT" .          # languages, package manager, monorepo tool, real commands
```

That gives you the languages and their shares, the package manager, monorepo layout, test
frameworks, CI files, and the actual build/test/lint commands **with their source** — CI
config ranked above task-runner above convention. Then:

```bash
git log --oneline -15                       # what is being worked on right now
git log --format='%an' | sort | uniq -c | sort -rn | head    # who knows this code
git ls-files | wc -l                        # scale
```

**Read the entry points, in this order:** README and `docs/`, then the CI workflow (it
encodes what the project believes about itself), then the manifest, then the actual entry
point (`main`, `index`, `cmd/`, `src/app`). A stale README is itself a finding worth stating.

## 2. Fan out — one scout per question, not per directory

Dispatch `scout` agents in parallel, each on **one specific question**, each returning
`file:line` anchors rather than file contents. Good questions:

- Where does a request/command enter, and what is the path to a response?
- Where does state live — schema, migrations, caches, external stores?
- What are the module boundaries, and which are load-bearing vs incidental?
- Where are the tests, what level are they, and what do they actually cover?
- What is configuration and how does it reach the code (env, files, flags)?
- What is generated or vendored, and therefore must not be hand-edited?

**In a polyglot repo, scout each language separately.** Cross-language seams — an API
contract, a shared schema, a queue message, an FFI boundary — are where the real complexity
sits and where a single-language reading silently misleads. Name the seams explicitly.

Honour the escalation ladder: a scout returning `escalate: true` or low confidence means
that area needs a stronger model, not a confident summary built on a shaky map.

## 3. Find the shape from evidence, not from expectation

- **Follow one real trace end to end.** Pick a single feature and trace it from entry to
  persistence to response. One traced path teaches more than five summarised modules.
- **Let the code rank itself.** `git log` churn shows what is alive; file size and fan-in
  show what is load-bearing. Rank by evidence, not by directory prominence.
- **Look for the seams that hurt:** global state, singletons, implicit ordering,
  God-objects, duplicated logic that has drifted, dead code that looks alive.
- **Record the conventions**, because they are what a change must obey to be mergeable:
  naming, error handling, logging, test layout, how similar things already get done.

## 4. Write the map to a file

For anything you will act on later, save it (`docs/notes/<topic>-map.md` or the project's
convention). This is the artifact that survives compaction and pays for the exploration a
second and third time. Include:

- **What this is** — in two sentences, plus the stack from step 1.
- **How to run, test and build it** — the verified commands, with their source.
- **The map** — components with `file:line`, and how data flows between them.
- **One traced path** — the concrete end-to-end example you followed.
- **Conventions to obey** and **gotchas** — non-obvious coupling, things that look editable
  but are generated, sharp edges.
- **Open questions** — what you could not determine, and where the answer probably lives.
- **Where a change of the kind I care about would go**, if you had a task in mind.

## 5. Close the loop

- If the task is now clear, hand to `/viby-code:plan` or `/viby-code:orchestrate` — the map
  is your research phase, so do not redo it.
- Record anything durable and non-obvious with `/viby-code:learn` (a build quirk, a
  convention the repo insists on, a trap) so the next session starts warm.

## Output

Lead with the two-sentence summary and the verified run/test commands, then the map. Mark
every uncertain claim as uncertain, and list what you did not look at — an exploration that
hides its own gaps is worse than one that admits them, because the gaps get treated as
"nothing there".
