---
name: deps
description: >
  Use when moving a dependency version — "upgrade React", "bump the dependencies", "we're on an
  old version of X", "dependabot opened 30 PRs", "migrate to v3", "is this safe to update",
  "our lockfile is a mess", "replace this library". Not /viby-toolkit:secure,
  /viby-toolkit:evaluate.
---

# Deps (one at a time, changelog first, revert cheap)

```
IRON LAW: One dependency per change, its breaking-change notes READ, and the project's real checks
          green before the next one. A batch upgrade that breaks something tells you nothing about
          which upgrade broke it.
```

The cost of a dependency upgrade is almost never the version number — it is the bisect you pay when
five of them moved at once and something subtle is wrong. Follow `/viby-toolkit:principles`.

## 1. Know what you actually have before changing it

- **Direct vs transitive.** You control direct dependencies; transitive ones move underneath you. A
  vulnerability three levels down usually needs a direct bump or an override, not a rewrite.
- **Is the lockfile committed and honoured?** An upgrade against an unpinned install is not
  reproducible, and neither is the test run that "passed".
- **What actually uses it?** A library imported in one file is a different job from one threaded
  through forty. Price it before promising it.
- **Stop and reconsider if the answer is "nothing uses it"** — the cheapest upgrade is a removal.

## 2. Sort the queue by why, not by how old

Not all upgrades earn their risk:

| Reason | Priority | Note |
|---|---|---|
| Reachable vulnerability | first | confirm reachability via `/viby-toolkit:secure` — most advisories are not reachable in your usage |
| Blocking something you need | next | a feature or a peer requirement |
| Runtime/toolchain EOL | scheduled | it becomes urgent on a date you already know |
| Unreachable vulnerability | batched | still worth doing, not worth an emergency |
| Newer for its own sake | last, or never | churn has a cost and no payoff |

**Thirty bot PRs are not thirty tasks.** Group patch-level bumps of dev-only tooling into one
reviewed batch; treat anything major, anything in the runtime path, and anything security-relevant
as its own change.

## 3. Read the changelog before touching code

The single highest-value step, and the one most often skipped:

- **Read the breaking-change section of every major version you are crossing**, not just the target.
  Going 2 → 5 means reading three sets of breaking changes; the removals in v3 still apply.
- **A version number is a promise the ecosystem breaks routinely** — around two thirds of artifacts
  in one large ecosystem study violated semantic versioning, and behavioural breaks are detected far
  less reliably than syntactic ones. So a minor bump is *evidence*, not proof, of compatibility.
- **Note the deprecations, not just the removals.** They are the next upgrade's breaking changes, and
  fixing them now is cheaper than fixing them under time pressure later.
- **Check the runtime floor**: a library dropping Node 18 or Python 3.9 makes this a platform upgrade
  wearing a dependency costume.

## 4. Upgrade one thing, prove it, commit it

1. **Pin the exact target version.** "Latest" is not a change anyone can review or revert.
2. **Bump it alone.** No opportunistic edits in the same commit.
3. **Run the project's real checks** — found mechanically, not guessed
   (`/viby-toolkit:verify`, `detect-stack.ts`).
4. **Exercise the actual behaviour the library provides**, not just the build. A dependency that
   compiles and misbehaves at runtime is the normal failure, especially for anything doing dates,
   money, serialisation, or HTTP.
5. **Check the safety net did not shrink** — `check-test-drift.ts` from `/viby-toolkit:adopt`.
   "Tests pass" after someone skipped the failing one is the shape this takes.
6. **Commit with the version and why**, so `git log` answers "when did we move to v4, and what
   for?".

For a wide mechanical follow-through (a renamed API used in 40 files) hand to
`/viby-toolkit:migrate`; for a behaviour-preserving restructure the new version demands,
`/viby-toolkit:refactor`.

## 5. Make the revert cheap, and know when to stop

- **One upgrade per commit is what makes revert cheap** — the whole discipline exists for this.
- **A dependency you cannot upgrade is a decision, not a failure.** Record it: the version, why it
  is stuck, what would unstick it, and the risk of staying. That note is what stops the same
  investigation happening every quarter (`/viby-toolkit:learn`).
- **If the upgrade turns into a rewrite**, stop and re-open the choice: a library requiring a rewrite
  to stay current is a candidate for replacement, which is `/viby-toolkit:evaluate`, not this skill.
- **Supply-chain check on anything new that appears** in the lockfile — a bump can pull in a new
  transitive package, and a package whose name is nearly right is the oldest trick there is
  (`/viby-toolkit:secure`).

## Output

- **What moved**, with exact from → to versions.
- **The breaking changes crossed**, and what each required here.
- **Verification**: the commands run and their real output.
- **Deprecations noted** for next time.
- **What is deliberately stuck**, with the reason and the unsticking condition.
