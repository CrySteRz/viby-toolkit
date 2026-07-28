---
name: release
description: >
  Use when cutting a release or preparing to publish — "ship a release", "bump the version",
  "tag this", "publish", "write the changelog", "is this a major or a minor", "what changed
  since the last release", or before pushing anything downstream consumers will install.
---

# Release (the version number is a promise)

```
IRON LAW: Decide major/minor/patch from the PUBLIC-SURFACE DIFF, never from the size or
          feel of the change. Everything mechanical gets checked by a script; your judgement
          goes to the one question it cannot answer — is this backward-compatible?
          Never tag code that is not pushed.
```

Follow `/viby-toolkit:principles`.

## Why the judgement call is the whole job

Semantic versioning is a promise to people who will never read your diff, and it is broken
routinely. A systematic review of **97 primary studies** across Maven, npm, Python, Web APIs
and Linux distributions ([arXiv 2605.24397](https://arxiv.org/abs/2605.24397)) found **67% of
Maven artifacts introduce at least one semantic-versioning violation**, and names *"the
failure of semantic versioning as a trust mechanism"* as a central open problem — downstream
consumers are routinely broken by minor and patch releases that were supposed to be safe.

The same review surveyed 43 detection approaches and found they reach *"high accuracy on
syntactic breaks but limited coverage on behavioral ones."* That is the shape of the risk:
the compiler-visible breaks are the easy half. A changed default, a narrowed accepted input,
a different error type, a reordered result, a stricter validation — these break callers
silently and no tool will tell you. So this skill spends its effort there.

## 1. Run the mechanical pre-flight first

```bash
CHECK=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/release/scripts/check-release.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$CHECK" .
```

It reports only what is exactly decidable, so anything it flags is real: version drift
between manifests, an uncommitted tree, unpushed commits, a tag already taken, a changelog
that omits this version, a focused test or `debugger` left in, and whether CI gates the
release at all. It deliberately does **not** guess whether the change is breaking. Monorepos
are detected, so independent per-package versions are reported as normal rather than as drift.

Fix everything it finds before continuing — these are the failures that make a release
embarrassing rather than wrong.

## 2. Read the public-surface diff

```bash
git describe --tags --abbrev=0            # the last release
git diff --stat $(git describe --tags --abbrev=0)..HEAD
git log --oneline $(git describe --tags --abbrev=0)..HEAD
```

Then look specifically at what a **consumer** can observe — not at your internals:

- exported functions, types, classes, constants; removed or renamed anything
- required vs optional parameters, and any narrowing of accepted input
- **default values** — changing one is a behaviour change for every caller who relied on it
- return shapes, including a new nullable field or a changed field order where order matters
- error types, messages consumers match on, and exit codes
- routes, event and message schemas, serialized formats, database shapes
- config and env-var names, and whether an old name still works
- the minimum supported runtime, dependency ranges, and peer requirements

For a library, list the exported surface before and after and diff **that**, rather than
reading the whole change. It is faster and it is what the version number actually describes.
Compute it rather than eyeballing it:

```bash
SURFACE=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/api/scripts/check-api-surface.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$SURFACE" --base "$(git describe --tags --abbrev=0)" src   # exit 1 = breaking
```

It reports added / removed / re-signatured exports for TS/JS, Python, Go and Rust, tells a
positional parameter rename (P2) from a real signature change (P1), and lists any
`export *` barrel it could not follow instead of pretending the surface is complete.

**Its verdict is a floor, not the answer.** It sees syntax; the list above is mostly about
behaviour, and a function whose signature held while its meaning changed is a major break the
tool will call a patch. So: if it says major, it is major. If it says minor or patch, you
still owe the behavioural read.

## 3. Choose the number, and be willing to say major

- **major** — anything a consumer must change to adopt: a removal, a rename, a narrowed
  input, a changed default, a new required field, a stricter validation, a raised minimum
  runtime.
- **minor** — new capability, fully backward-compatible.
- **patch** — a fix with no surface change.

**When you are unsure between minor and major, it is major.** The cost is asymmetric: an
over-cautious major bump costs a moment of downstream attention, while a breaking change
hidden in a patch costs debugging time in someone else's codebase and is precisely the
failure the research above measures. If the project uses a different scheme (calver, a train,
zero-major "anything goes"), follow the project — but say which rule you applied.

Pre-1.0 does not suspend the promise; it just widens what people expect. Say what broke
regardless.

## 4. Write the changelog for the reader, not the log

Group by what it means to a user: **Breaking**, **Added**, **Fixed**, **Deprecated**,
**Security**. Then:

- **Every breaking change needs a migration line** — the before and the after. A breaking
  change noted without the fix is a support ticket you have already earned.
- Say what changed *observably*, not which function you refactored. "Rejects empty tags,
  which previously returned an empty list" beats "refactored tag validation".
- Credit the fix to the symptom people searched for, so the person with the bug finds it.
- Do not autogenerate from commit subjects and stop there — commit subjects describe the
  code, and a changelog describes the consequence.

## 5. Cut it

In this order, because each step depends on the previous being true:

1. Bump the version in **every** manifest the pre-flight listed — one artifact, one number.
2. Update the changelog and commit the release together with the bump.
3. **Push, and confirm CI is green on that exact commit.** A tag on unpushed or unverified
   code is a promise you have not checked.
4. Tag that commit, then push the tag.
5. Publish, then **verify the published artifact** — install it fresh somewhere clean and
   confirm the version and that it imports/runs. The evidence gate applies here too: a
   registry accepting an upload is not proof it works.

If any step fails, stop and fix forward. Deleting a published version breaks anyone who
already fetched it; a follow-up patch does not.

## Output

- The version chosen, and **the specific surface change that justifies it** — this is the
  claim to defend.
- Pre-flight results: what it flagged and what you did about each.
- Breaking changes with their migration lines.
- The verification evidence: CI green on the tagged commit, and the published artifact
  installed and checked.
- Anything you deliberately deferred to the next release.
