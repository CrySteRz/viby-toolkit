# agentgrep against Grep/Read: measured 2026-10-19

Question: does the `agentgrep` tool make code exploration cheaper than searching with grep and
reading files? The plan (`docs/plans/2026-10-19-browser-parity-and-agentgrep.md`, step B5) said
to measure before claiming savings.

## Setup

- Repo: `viby` (Rust and TSX, 112 `.rs` files), a real project, not a demo.
- Six Sonnet scout agents, one per question per arm, run at the same time with the same prompt
  apart from the tools:
  - **Baseline**: Bash limited to `rg`, `grep`, `find`, `ls`, plus Read. Grep/Glob tools are not
    available to agents in this environment, so this is how they search here.
  - **agentgrep**: the agentgrep 0.1.7 CLI through Bash (grep, find, outline, trace), plus Read
    for a known line range. No other search.
- Tokens are the agent's total (`subagent_tokens`), which includes the fixed prompt overhead
  (about 6.5k, from three agents that stopped at once).
- One run per cell. Differences of a few thousand tokens are within noise.

## Results

| Question | Baseline | agentgrep | Which answer was more complete |
|---|---|---|---|
| Q1: how sessions move off a failed Lavalink node | 13,718 tokens, 5 calls | 10,194 tokens, 3 calls | Baseline: all three triggers (watchdog, DNS discovery, session lost), orphan collection, `play_at` on the new node. agentgrep missed the DNS trigger and did not read `migrate_one`. |
| Q2: what calls queue persistence; when a queue is restored | 10,153 tokens, 2 calls | 13,322 tokens, 5 calls | Same callers and restore points; the baseline also found `worker_api.rs:239`. |
| Q3: `/skip` from dispatch to the player | 12,027 tokens, 4 calls | 12,384 tokens, 2 calls | Baseline: found the dispatch arm (`handlers/commands.rs:77`) and the final Lavalink call. agentgrep did not find the dispatch. |
| **Total** | **35,898** | **35,900** | Baseline in two of three, tie in one. |

The agentgrep arm used fewer calls in two of three questions, but each result was larger, so
the totals came out the same.

The "already seen" shortening was measured separately, on a repeated `trace` in this repo's
`browser/` folder: 3,073 characters the first time, 1,977 the second (36% smaller), full again
after the file changed or with `force: true`. It only helps when a session traces the same code
more than once.

## Limits found

- agentgrep 0.1.7 extracts structure for Rust, TypeScript/JavaScript and Python only. A Swift
  file outlines as `language: text` with no symbols, so for the Swift-heavy repos here
  (viby-shot's Mac app) it is plain grep.
- `find` and `grep` do not use the session context; only `trace` does.

## Decision

Ship the tool, since it is cheap to have and the owner asked for it. Do not claim savings, and
do not route the scout or explore agents to it. Revisit with more runs per cell, or after an
agentgrep release adds languages.
