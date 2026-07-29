---
name: docs
description: >
  Use when writing prose a human will read — "write the README", "document this", "write the PR
  description", "release notes", "write it up for the client", "an ADR for this decision",
  "explain this to the team", "the docs are out of date". Not /viby-toolkit:study,
  /viby-toolkit:handoff, /viby-toolkit:brain.
---

# Docs (write for one reader with one next action)

```
IRON LAW: Name the reader and what they will DO after reading, before writing a word.
          Every command, path and claim must be one you have actually run or verified —
          documentation is the easiest place to ship something confidently false.
```

Documentation fails in two ways and only two: nobody can find the thing they needed, or they found
it and it was wrong. Length is not the problem — a wrong command in a README costs more than ten
missing pages. Follow `/viby-toolkit:principles`.

## 1. Decide the reader and the genre

Genres are not interchangeable, and mixing them is why most docs help nobody:

| Genre | Reader | Their next action | Shape |
|---|---|---|---|
| **README** | someone who just arrived | get it running in five minutes | what it is, run it, one example, where to go next |
| **How-to** | someone with a task | complete that task | numbered steps, copy-pasteable, no theory |
| **Reference** | someone mid-task | look one thing up | exhaustive, skimmable, alphabetical or structural |
| **Explanation** | someone deciding | understand a trade-off | prose, context, why not just what |
| **ADR** | future maintainer | know why, so they don't undo it | decision, alternatives rejected and why, consequences |
| **PR description** | a reviewer | review efficiently | what changed, why, how it was verified, what to look at |
| **Release notes** | a user | decide whether to upgrade and what breaks | breaking changes first, then what's new, then fixes |

If a document is trying to be two of these, split it. A README that explains architecture stops
being a five-minute start.

## 2. Verify every executable thing

- **Run the commands you write, in the order you wrote them, from a clean state.** The command that
  works in your shell because of something set six months ago is the classic README bug.
- **Check every path and filename exists** as written.
- **Never document intent as fact.** If you did not run it, say "not verified".
- **Copy real output** rather than describing or inventing it.

This is the evidence gate (`/viby-toolkit:principles` §5) applied to prose, and it is where most
documentation quietly fails.

## 3. Write the parts people actually need

- **Lead with the answer.** No preamble about the importance of the domain.
- **Prerequisites before steps**, so nobody discovers the missing tool at step 7.
- **One worked example beats three abstract descriptions.** Real values, real output.
- **Document the failure**: what goes wrong most often, what the error looks like verbatim, and the
  fix. This is the most-read paragraph in any doc and usually the missing one.
- **Say what it does *not* do**, and the nearest alternative.
- **Prefer specifics to hedges.** "Takes about 30 seconds on a laptop" beats "may take some time".

## 4. Write the PR description a reviewer can act on

Four things, in this order: **what** changed in one sentence · **why** (the problem, not the
solution restated) · **how it was verified**, with the command and its result · **where to look**
first, and anything deliberately out of scope. A reviewer's hardest question is "what should I be
suspicious of?" — answer it yourself and the review gets better.

## 5. Record decisions so they survive

An ADR is short and its value is entirely in one section: **the alternatives you rejected and the
bar each failed.** Without that, the next person re-litigates the decision from scratch or reverses
it without knowing the constraint. State the consequences you accepted, including the bad ones.

## 6. Keep it true, or delete it

Stale documentation is worse than none, because it is trusted. So:

- **Put docs where they rot most visibly** — next to the code, in the repo, in the same PR as the
  change. A wiki page nobody edits is a liability.
- **Prefer generated over hand-maintained** for anything derivable (API surfaces, CLI help, config
  schemas). Hand-copied facts drift; `/viby-toolkit:principles` §9 covers which artifacts are
  authored and which are derived.
- **Delete confidently.** A section you would not defend today is not documentation, it is
  archaeology.
- **Date anything time-sensitive**, so a reader can judge staleness themselves.

## Output

- The document, in one genre, for one named reader.
- **Verification note**: which commands you ran and what they returned, or which you could not run.
- What you deliberately left out.
