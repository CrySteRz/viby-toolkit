# How `check-api-surface.ts` should read an exported surface

> Status: **measured on this repo** (2026-07-28, commit `27f342c`) · 3 candidates, 2 measured
> directly, 1 rejected on a stated bar without being run · oracle = 4 fixture files with a
> hand-established surface · **1 claim from my own design sketch refuted by the run**.

This is `/viby-toolkit:evaluate` run on a real decision — the one the differ itself required —
rather than on a hypothetical. It is also the skill's first live use, so its own failure modes
are recorded at the bottom.

## Decision

**Regex extraction over blanked code**, accepting a stated ceiling: it cannot follow
`export * from` re-export barrels, and it reports every barrel it meets instead of pretending
the surface is complete. The TypeScript Compiler API is more accurate and was rejected without
being run, because it fails this repo's hard zero-runtime-dependency bar and covers only one of
the four languages the differ has to handle.

## The oracle (established before any code was written)

Four fixture files whose exported surface I wrote down by hand first:

| Fixture | Ground-truth surface | Why this case |
|---|---|---|
| TS with a string fixture containing `export function ghost()` | `real` only | the failure class that has cost this repo four separate defects |
| Python with `__all__ = ["only_this"]` and two public defs | `only_this` only | an explicit `__all__` overrides convention |
| Go with `Exported` and `unexported` | `Exported` only | visibility is a language rule, not a naming convention |
| TS with `export { a as publicName }` | `publicName` | the alias is what a caller imports, not the local name |

All four are now contract tests, so the oracle is executable rather than a paragraph.

## Criteria, named before looking at candidates

1. **Zero runtime dependencies** — the repo's hardest constraint; there is no `node_modules` at
   runtime and no build step.
2. **Language-agnostic** — the differ must handle TS/JS, Python, Go and Rust. A TS-only tool
   solves a quarter of the problem.
3. **Precision over coverage** — a false "MAJOR" on a positional parameter rename gets the tool
   switched off, and then it protects nothing.
4. **Works between two git refs**, which is the actual question at release time.

## Cost × correctness

All three approaches can list top-level exports of a single TS file, so that row is omitted —
only the differences are tabled.

| | regex over blanked code | TS Compiler API | `git diff` of export lines |
|---|---|---|---|
| Zero runtime deps | ✅ measured | ❌ needs `typescript` at runtime | ✅ measured |
| Four languages | ✅ measured (20/20 fixtures) | ❌ TS/JS only | ✅ but ⚠️ text-only |
| Resolves `export *` barrels | ❌ **measured: cannot** | ✅ inferred, not run | ❌ |
| Tells rename from re-signature | ✅ measured (P2 vs P1) | ✅ inferred | ❌ measured: reports both as a line change |
| Survives an export inside a string fixture | ✅ measured | ✅ inferred | ❌ measured: matches text |

Labels are per cell: **measured** = run against the fixtures; **inferred** = not run, reasoned
from documented behaviour. The Compiler API column is entirely inferred, deliberately — it was
disqualified on criterion 1 before accuracy mattered, and running it anyway would have produced
a more impressive table without changing the decision.

## The winner's failure case

**`export * from './x'` is unresolvable**, and this is the same blind spot a tree-sitter code
graph has, for the same reason: following a re-export needs module resolution, not pattern
matching. The mitigation is not to hide it — every unfollowed barrel is emitted as a P2 line
saying the symbols behind it are absent from the report. A surface report that silently omits
part of the surface is worse than no report, because it gets trusted.

Second, smaller: the whole tool sees **syntax only**. The literature this repo already cites is
explicit that syntactic breaks are the easy half and behavioural ones are poorly detected. So
the verdict is a floor — a `major` is authoritative, a `minor`/`patch` still owes a behavioural
read. That sentence is printed on every run rather than living only here.

## What the run refuted

My design sketch asserted that extracting on blanked code was strictly correct, since the
repo's precision rule is *decide on parsed code, never raw text*. **The run refuted it.** Two
constructs — a re-export's module path and Python's `__all__` — carry their value *inside a
string literal*, which the blanking pass by definition erases. The barrel path came out as ten
NUL bytes, and `__all__` produced an **empty surface**, the worst possible failure: every
symbol would have been reported as removed, or none at all.

The rule needed splitting, not abandoning: **decide WHERE from the blanked code, read WHAT from
the raw text at the same offset**. That works only because the blanking pass overwrites in place
and preserves offsets — a load-bearing property that was previously incidental and is now
commented as a contract. Both bugs were caught by tests written before the fix, from the oracle.

## Rejections

- **TS Compiler API / `ts-morph`** — fails criterion 1 (a runtime dependency, and the repo has
  no `node_modules` at runtime) and criterion 2 (TS/JS only). It remains the reference for what
  compiler-accurate resolution looks like; it is not viable as *this* tool.
- **`git diff` of export lines** — fails criterion 3 outright: it cannot distinguish a
  positional rename from a signature change, and it matches export-shaped text inside string
  fixtures, which is the defect class this repo has already paid for four times.
- **Not building it at all** — the honest baseline, rejected because `release` demands a
  public-surface diff and shipped nothing to compute one, so the judgement was being made by
  eye on every release including this project's own 2.0.0 call.

## Back-out

Delete `skills/api/scripts/check-api-surface.ts` and `tests/check-api-surface.test.ts`, drop the
gate entry in `tests/run-all.ts`, and revert the two skill sections that reference it (`release`
§2, `api` §5). Nothing else: no dependency, no config, no generated artifact, no state.

## Notes on `evaluate` itself, from using it

- Naming the criteria before the candidates did real work: it disqualified the most accurate
  option in one line, and stopped a benchmark that would have been interesting and irrelevant.
- The measured/inferred labels are what make an unrun column honest rather than an omission.
- **Gap found:** the skill says to establish the oracle before installing anything, but says
  nothing about making the oracle *executable*. Turning all four fixtures into contract tests is
  what caught the refutation above, and it should be a step rather than a lucky habit.
