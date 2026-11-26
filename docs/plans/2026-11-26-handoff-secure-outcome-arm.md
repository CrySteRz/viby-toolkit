# Handoff: a /secure vs plain-audit outcome arm (delete when done)

**End goal.** Find out whether `/viby-toolkit:secure` finds more real vulnerabilities than a
plain "audit this for security problems" request, on Opus. Report it as a measured result with a
confidence interval. If the bare arm matches the skill at lower cost, the follow-up is to shrink or
cut `skills/secure`, not to add to it.

**Why.** [gstack PR #4833](https://github.com/garrytan/gstack/pull/4833) (fetched 2026-11-26) cut
its own security skill after this comparison on Opus 5.5: 13 vs 7 and 12 vs 8 of 51 tasks. Source
and context: `docs/studies/2026-11-26-papers-and-github-sweep.md`, candidate C6. The owner chose
"add the 4 lines, plan the eval" on 2026-11-26.

**Current approach and its blocker.**
- `tests/outcome/` grades each run by a hidden test against a patch.
  - `drive.py:28`: `ARMS = ("skill", "bare")`.
  - `drive.py:179`: `validate.run_hidden(task, work)`.
- `/secure` writes a report and does not patch, so the hidden-test pass does not fit.
- First build report scoring: each fixture names its planted vulnerability (file, line range, CWE
  class). A run passes when its report names that location and class. Grade mechanically from
  the report text, not with an LLM judge.
- Ruled out: running the existing harness unchanged. The skeptic found the pass/fail model does
  not fit a report.

**Done so far.** Nothing for this task. Branch `improve/3.5.0` is ahead of `main` by:
- `ea19883`: check-study fix and the sweep study.
- `ca16303`: four protocol lines in verify, worker and skeptic.

Not released; a push of a `plugin.json` bump is the release.

**Update 2026-11-26, pilot done.** The branch `outcome/secure-arm` (`ebf41bc`) has three fixtures
and report scoring. On Sonnet the pilot came out 9/9 against 9/9: a ceiling, and the skill cost
1.7×. Run `drive.py` with plain `python3`; `-I` breaks `import validate`.

**Next step, revised.** Write 8–12 harder `secure-*` fixtures that make the arms differ:
- 300+ lines across several files;
- the taint enters in one file and the sink sits in another;
- two or three decoys;
- authorization or IDOR and SSRF classes.
Then rerun on Sonnet, and on Opus only with the owner's go.

**Original next step (done).** Read `tests/outcome/README.md` and `validate.py`, then write a 2-3 task pilot:
- `secure-*` fixtures, each planting one vulnerability modelled on a public advisory.
- A report-scoring path in `validate.py`, proven with no model: a reference report passes and an
  empty report fails.
- Then run `TASKS='secure-*' REPS=3` on both arms, with the owner's go first. It costs usage, and
  on 2026-11-08 the owner said "not now" to an Opus outcome run.

**Rules that apply.**
- Runs live outside the checkout (`~/.cache/viby-toolkit/outcome`).
- Use `cargo xtask check` before each commit, with no AI attribution in commits.
- Keep the vulnerable fixture code generic and harmless outside the test.
