# §2 in depth — context discipline

Read when you are deciding what to compact, whether to `/clear`, or why an agent that managed
something earlier has started refusing it now.

## Why context is the master resource

The LLM is a stateless function; **the contents of the context window are the only lever on output
quality** (humanlayer/ACE). On a Max subscription the scarce resources are the **main thread's
context window** and your **rate-limit budget** — not dollars.

**Context quality priority: Correctness > Completeness > Size.** Wrong context is worst; missing
context second; excess tokens are the *least* damaging. Don't over-trim and drop something
load-bearing to save tokens.

## The shape of the degradation is not what you expect

Measured across formats (arXiv 2607.19257), recall holds at ceiling to roughly 64–128k tokens and
then falls away sharply. Near the ceiling the dominant failure is **refusal, climbing to 79–90%** —
not fabrication, which measured *exactly zero* across 5,760 absent-fact probes.

So budget for "it declined to answer", not for "it made something up": if an agent starts refusing or
hedging on work it managed earlier, **suspect context pressure before suspecting the prompt**. A 1M
window does not exempt you — degradation was severe at 100k in windows many times that.

Target **40–60% utilization** (Frequent Intentional Compaction). Reserve headroom for iteration and
error handling. Auto-compaction at ~90% produces noisy summaries; compact *early*, at task
boundaries, on purpose.

## Compact with a ledger, not blindly

Before a compaction or `/clear`, take stock of what is actually in context — the durable artifacts,
the files still needed, the large stale tool outputs — and evict the stale bulk first. Raw tool output
older than a few turns is worth replacing with its one-line conclusion; the subagent already returned
that conclusion, so keep only it. Deciding what to drop from an explicit inventory beats hitting a
blind threshold. **Less context frequently beats full context on accuracy**, not just on cost —
verbatim old tool spew distracts.

`/clear` liberally: between unrelated tasks, and **after two failed corrections on the same issue**.
A clean session with a better prompt beats a long session full of accumulated corrections.

## The two mechanics that do the most work

- **Subagents are context firewalls.** A subagent that greps 40 files and reads 10 returns a
  ~200-token conclusion; the 30k tokens of file dumps die with it and never touch main context. This
  is the single biggest lever.
- **Cost is payload × cadence.** A 15k-token payload re-sent after every step of a six-step flow is
  90k, and loses to a 40k one-shot read. Measure the *flow you will actually run*, not one call. This
  is what makes a tool that "returns less per call" the more expensive option, and it is invisible
  until you count the repeats. `skills/evaluate/scripts/measure-read-cost.ts` prices a read set
  (`--repeat` for cadence, `--budget` to gate it) so this stays a measurement rather than a feeling.

Hold references (paths, queries); load content on demand with targeted reads/grep/`head`/`tail`.
Don't pre-load whole files.
