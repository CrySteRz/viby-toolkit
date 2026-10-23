# Plan: `viby decide` and an opt-in skill hint

**Goal**: an advisory skill hint on each prompt from a decision model, per
`docs/studies/2026-10-23-decision-model-mammoth.md` §8. The toolkit behaves exactly as today unless the
person opts in.

**Constraints**
- Opt-in needs both `AI_GATEWAY_API_KEY` and `VIBY_DECIDE=on` (decided with the owner 2026-10-23:
  the key name is Vercel's standard, so the key alone must not start sending prompts).
- No new crate: run `curl`. The key and the prompt never appear in argv or on disk; curl reads its
  whole config, header and body included, from stdin (`curl -K -`).
- Any failure is silent and the hook exits 0.

**Approach**: a shell gate in `hooks/prompt-hint.sh` exits before anything runs unless the person
opted in. Behind it, the Rust subcommand `prompt-hint` asks a `choice` question over the skill
catalogue through `hooks::decide`, which tries each model in order inside a time budget.

## Steps

1. `rust/crates/viby/src/hooks/decide.rs` (new). It holds `evaluate(state, questions) -> Option<Decision>`
   and `main`, the `viby decide` CLI: stdin `{state, questions}`, stdout `{model, answers}`, exit 0
   with an answer and exit 2 with none.
   - Models come from `VIBY_DECIDE_MODELS`, default `baseten/mammoth-1,baseten/elephant-1,liquid/d1-ultra`.
   - Each call is capped at 1.5 s and the whole run at 3 s.
   - Any non-200, unparseable JSON or missing `answers` moves on to the next model.
   - `VIBY_DECIDE_URL` overrides the endpoint for tests.
2. `src/hooks/prompt_hint.rs` (new). It reads the UserPromptSubmit JSON, skips empty prompts and
   slash commands, and builds the `{skill: description}` criteria from `checks::check_skills::load_skills`
   (model-invocable only, 600 chars each) plus `none`. It prints
   `Skill hint: viby-toolkit:<name> (confidence 0.97)` only when the choice is not `none` and the
   confidence is ≥ 0.9.
3. `src/hooks/mod.rs` and `src/lib.rs`: register both subcommands.
4. `hooks/prompt-hint.sh` (new) and `hooks/hooks.json`: add the UserPromptSubmit entry with
   `timeout: 5`, plus the gate.
5. `tests/decide.rs` (new). A fake `curl` sits first on PATH, as `format_hook.rs` does, and gets one
   canned behaviour per model: 200, 403, 404, 429, 500, a timeout, or a garbage body. Cases:
   - fallback order
   - every model failing, giving empty output
   - low confidence, or `none`, giving no line
   - slash commands and an empty prompt are skipped
   - opt-in missing (no key, or `VIBY_DECIDE` unset or `off`): curl is never run and the output is empty
   - the key never reaches argv
   - the shell gate prints nothing when the person has not opted in
6. `xtask/src/check.rs` `MIN_PASSING`: add `tests/decide.rs`.
7. `README` and `CHANGELOG`: document the opt-in, and that the prompt text goes to Vercel when it is on.

**Risky step**: step 4 runs on every prompt. De-risked by the shell gate (no binary call when off), the
hooks.json timeout, the 3 s budget in Rust, and `check-hooks`' dry run.

## Verification
- `cargo xtask check`: tests, clippy `-D warnings`, fmt, MIN_PASSING, references.
- `viby check-hooks` on the plugin.
- A live run with the real key: three prompts piped into the built binary's `prompt-hint`, plus one run
  with the key unset to show empty output.

## Not doing
- Version bump and release. Pushing this branch triggers `release.yml`; that waits for the owner.
- The A/B end-to-end routing evaluation (a follow-up, `tests/routing/drive.py` with and without the hint).
- Stop-hook or skeptic decisions (excluded or untested in the study).

## Status (2026-10-23)
- [x] 1–7 built. `cargo xtask check`: all 36 checks pass, `tests/decide.rs` 17/17 (three runs in a
  row). `check-hooks`: clean. Live smoke test with the real key: "write tests for the pricing
  module" gave test (1.00), "the checkout endpoint is slow" gave perf (0.99), "thanks" and `/clear`
  gave nothing.
- Added at the owner's request ("make sure it's not used when useless"): replies under four words
  and prompts that name a viby-toolkit skill are never sent. The shortest labelled probe is five
  words.
- Fixed during the build: a fake curl written per test raced parallel spawns ("text file busy"). It
  is now a checked-in fixture, and the two wall-clock tests run exclusively. Serially, three hung
  models take 3.3 s.
- Under evaluation (owner request): reranking agentgrep `find` with the decision model, on 40
  labelled queries across viby and viby-flow. It ships only if it clearly beats agentgrep's own
  top 1.
- Not possible yet: skeptic keep/reject. No labelled findings set exists (`docs/reviews` has one
  file with no per-finding verdicts).
