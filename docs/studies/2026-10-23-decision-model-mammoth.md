# Study: decision models inside viby-toolkit — optional, never required

**Status (2026-10-23):** protocol approved, then run. 6 research angles (4 planned + 2 added on free-tier
eligibility), ~58 sources read, 27 cited. Local measurements: 5 decision models × 73 routing probes,
4 models × 48 injection cases, 27 eligibility probes, all on a free-tier key, $0.013 spent. 4 claims
could not be verified (listed in §7). **Priors refuted: 3** (see §2).

## 1. The answer

**Candidate A survives, in a narrowed form**: a decision model earns a place as an *advisory skill-routing
hint*, and nowhere that gates evidence. On the toolkit's own labelled routing probes, three of the five
decision models tried were 97–100 % top-1 correct with 0.6–0.8 s median latency, and the top three ignored a
planted "answer release" instruction in 12/12 cases. Task-completion ("should the agent stop?") is
**excluded** by the literature (no judge above AUROC 0.65), and skeptic keep/reject is **untested** here.

The integration must be **model-agnostic and fail-open**: the promoted models are free only until
27 October (Mammoth, Elephant) and 31 October (Laya), so a fixed model ID is a dead integration in days.
Default order: `baseten/mammoth-1` → `baseten/elephant-1` → `liquid/d1-ultra`. Anything that fails —
no key, 403, 404, 429, 5xx, timeout, bad JSON — moves to the next model, then to today's behaviour.

Confidence: **high** that the models classify our probes well; **medium** that a hint improves real
skill firing in Claude Code. That second claim is the one that pays, and it is not yet measured (§7).

Decision it serves: build `viby decide` + a `UserPromptSubmit` hint now; do not wire Stop; do skeptic
ordering only after a labelled findings set exists.

## 2. What would change it

- An end-to-end routing run (Opus, `REPS=5`, `tests/routing/drive.py`) where the hint arm does **not**
  beat the no-hint arm outside the Wilson interval → keep the subcommand, drop the hook from defaults.
- Mammoth and Elephant losing `availableToFreeTier` after 27 Oct → d1-ultra becomes the default (it is
  priced, $0.042/M input, and eligible today).
- A real prompt flipping a routing hint by injection → raise the confidence threshold or drop the hook.

Priors refuted by the evidence:
1. "Free-tier limits make a per-prompt hook unfit" — partly refuted. The limit is 5 req/min **per model**,
   so a three-model fallback gives ~15/min, enough for a person typing.
2. "The free $5 cannot pay for priced models" — refuted. Eligibility is a per-model flag; 221 of 404
   models are eligible, priced ones included, and the flag predicted 19/19 live calls.
3. "Laya is free with no end date" — refuted: free through 31 October.

## 3. Competing answers

| Candidate | Verdict | The observation that decided it |
|---|---|---|
| A. Real decision points | **Survives for routing only** | 67/67, 66/67, 65/67 on labelled probes; 0/12 injection flips for the top three |
| B. Pre-filter only | Not excluded, not tested | Needs a labelled set of review findings, which the repo does not have |
| C. Not worth it | **Excluded for routing** | Accuracy above the threshold the protocol set; latency p95 ≤ 1.34 s |
| D. Local Spark model better | Not tested (deviation) | — |

Stop/continue is excluded on literature alone: "no configuration across 5 judges, 5 prompt strategies,
and full task specifications exceeds AUROC 0.65 on tau2-bench" (arXiv 2606.09863), and Claude Code
already ships `type: "prompt"` hooks that use Claude for that judgement.

## 4. Evidence

### Measured here (raw data and scripts: `2026-10-23-decision-model-data/`)

Routing: 32-option `choice` question (31 skill descriptions + `none`), 67 labelled probes from
`tests/routing/probes*.tsv` plus 6 invented negatives (no skill should fire), 1 rep, paced to the free-tier limit.

| Model | Labelled correct | Negatives | p50 / p95 latency | Min confidence when right | Injection: routing flipped | Injection: failing log called pass | Free-tier eligible after promo |
|---|---|---|---|---|---|---|---|
| `baseten/mammoth-1` | 66/67 (99 %) | 6/6 | 0.82 / 1.34 s | **0.92** | 0/12 | 0/12 | unknown after 27 Oct |
| `baseten/elephant-1` | **67/67** | 6/6 | **0.63 / 0.89 s** | 0.59 | 0/12 | 0/12 | unknown after 27 Oct |
| `liquid/d1-ultra` | 65/67 (97 %) | 6/6 | 0.61 / 1.03 s | 0.75 | 0/12 | 0/12 | **yes (priced, eligible today)** |
| `perplexity/pplx-decider-v1.2` | 67/67 | 6/6 | 0.63 / 1.04 s | 0.75 | **6/12** | 0/12 | yes |
| `convaiinnovations/laya` | — | — | — | — | — | — | 422 on every call (rejects a 32-option list) |

Mammoth is first in the default order despite Elephant's 67/67. One probe is not a difference (n = 67), but
Mammoth's confidence is the better-behaved signal for a threshold: it never answered correctly below 0.92,
while Elephant had 3 correct answers under 0.8. The hook gates on confidence, so that is what matters.

The single shared miss — "checkout fails silently and we have no idea what's actually happening" labelled
`observe`, answered `debug` by Mammoth and d1-ultra — is arguably a label problem.

**Not like-for-like with the Opus 87 %** in `tests/routing/README.md`: that figure is whether the skill
*actually fired* in a live session; this is forced-choice classification. It shows the model can pick the
skill, not that a hint makes Claude fire it.

Free tier, measured: 5 requests/minute per model per region (429 body: "this team's limit of 5 requests per
minute (per region) was reached"). `$0` models leave the balance untouched; priced eligible models drew it
down (balance $5 → $4.987 across all tests).

### Cited

| Claim | Source | Quote | Grade |
|---|---|---|---|
| Mammoth and Elephant are free for one week | vercel.com/changelog/basetens-mammoth-1-and-elephant-1-now-available-on-ai-gateway-for-free | "Free access is available through October 27." | fetched, direct |
| Laya free only until 31 Oct | vercel.com/changelog/laya-decision-model-now-available-on-ai-gateway-free-through-october-31 | "The -free ID routes to Boundless and stops serving after the promotion; the standard ID begins billing then." | fetched, direct |
| The free tier is a subset of models | vercel.com/docs/ai-gateway/pricing | "The free tier includes a subset of models, not the full catalog." | fetched, direct |
| Limits are unpublished and can change | vercel.com/docs/ai-gateway/rate-limits | "Limits can change, so this page describes behavior rather than fixed numbers." | fetched, direct |
| 16K context on both Baseten models | Vercel changelog (above) | "Both support a 16k-token context window" | fetched, direct |
| Calibration not quantified by the vendor | baseten.co/blog/the-best-decision-models-in-the-world-mammoth-1-and-elephant-1 | "The models can also be post-trained so their confidence better reflects how often they're right." | fetched, vendor |
| No ZDR on the free tier | vercel.com/kb/guide/control-ai-provider-data | "ZDR requires Pro or Enterprise." | fetched, direct |
| A sibling decision model is injectable | braintrust.dev/evals/jev-vs-gpt-eval-judges | "Jev's answer-correctness verdicts changed in 22% of cases when a misleading note was embedded in the judged content" | fetched, independent, other model |
| Completion judging is weak | arxiv.org/abs/2606.09863 | "no configuration … exceeds AUROC 0.65 on tau2-bench" | fetched, preprint |
| A timed-out hook is dropped, not fatal | code.claude.com/docs/en/hooks | "a `UserPromptSubmit` command, HTTP, or MCP tool hook that reaches its timeout is canceled and its output, including …" | fetched, direct |
| `UserPromptSubmit` default timeout is 30 s | code.claude.com/docs/en/hooks | "Claude Code lowers the `command`, `http`, and `mcp_tool` default to 30 on `UserPromptSubmit`" | fetched, direct |
| An LLM pre-routing hook barely beat nothing | scottspence.com/posts/how-to-make-claude-code-skills-activate-reliably | "Requires API key and external API call on every prompt" (80 % LLM vs 84 % static hook, 50 prompts) | fetched, n = 1 author |
| Routers often fail to beat simple baselines | arxiv.org/abs/2601.07206 | "several recent approaches, including commercial routers, fail to reliably outperform a simple baseline" | fetched, peer benchmark |
| Ended promo `-free` IDs die | vercel.com/changelog/minimax-m3-and-m2-7-are-free-on-ai-gateway | "These model IDs will return an error after the free period ends." | fetched, direct; confirmed live (404) |

## 5. Disagreements between sources

- Vercel's changelog says Mammoth is "#1 on the Jev Decision Index"; an Artificial Analysis mirror
  (benchlm.ai, 2026-10-21) lists Elephant-1 second and does not list Mammoth. Unresolved; our measurement
  puts them level on routing.
- The `/v1/models` API tags `reflection/beam` "free" at $0, yet free-tier calls get a 403. The
  `availableToFreeTier` page flag is the reliable signal, not the price or the tag.

## 6. Deviations and exploratory results

- **Added** after the user asked: two researcher angles on free-credit eligibility and live probes of
  27 models. This produced the per-model eligibility flag (19/19 agreement with live calls).
- **Added** an injection test (not in the protocol) because routing did not separate the models.
- **Dropped** candidate D (Spark) — not measured.
- **Exploratory**: the six negative probes are mine, not from the labelled set; treat 6/6 as a smoke test.

## 7. What I could not find out

1. **Whether a hint actually raises skill firing** in Claude Code (the load-bearing claim). Decided by a hint
   vs no-hint run of `tests/routing/drive.py` on Opus, `REPS=5`, scored with `score.py`'s Wilson guard. This is
   `/viby-toolkit:evaluate` work, about 2 × 30 min of `claude -p` time.
2. Mammoth and Elephant prices and free-tier status after 27 October. One call on 28 October settles it.
3. Whether free-tier gateway traffic to Baseten is retained. Undocumented; treat prompt text as leaving the
   machine — hence opt-in only.
4. Skeptic keep/reject accuracy — no labelled findings set exists.

## 8. The design this supports

- `viby decide` subcommand in the existing Rust binary. No new crate: it shells out to `curl`, which every
  supported platform has (the crate has no HTTP client today). Input: JSON state + questions. Output: the
  answer, or nothing.
- Enabled only when `AI_GATEWAY_API_KEY` is set (decided with the owner). `VIBY_DECIDE_MODELS` overrides
  the order; `VIBY_DECIDE=off` disables it.
- Per call: 1.5 s timeout, at most the three models, total budget 3 s. Any failure → next model → silent
  empty output. Never retries a 429 on the same model (the limit is per model, so the next one is fresh).
- First consumer: a `UserPromptSubmit` hook adding one line —
  `Skill hint: viby-toolkit:<name> (confidence 0.97)` — only when confidence ≥ 0.9 and the answer is not
  `none`. It is a hint, never a forced call.
- Tests: every wired path runs twice, key set and key unset, plus a fake gateway that returns
  403/404/429/500/timeout/garbage; the unset run must be byte-identical to today's behaviour.

## Sources

Verified (opened): all URLs in §4's table, plus vercel.com/docs/ai-gateway/faq,
vercel.com/docs/ai-gateway/modalities/decision.md, vercel.com/ai-gateway/models?freeTier=true,
vercel.com/changelog/reflection-ai-beam-now-available-on-ai-gateway-for-free,
vercel.com/changelog/ling-3-1-flash-is-now-available-on-ai-gateway,
vercel.com/changelog/nemotron-3-5-lightning-now-available-on-ai-gateway-for-free, arxiv.org/html/2406.18665,
arxiv.org/html/2403.12031, arxiv.org/abs/2505.13348, raw.githubusercontent.com/openai/codex-plugin-cc/…/stop-review-gate-hook.mjs,
community.vercel.com threads 42526 and 43373.
Unverified (search summary only): digg.com/ai/2hvp6374 (Artificial Analysis relay), tianpan.co routing blog,
arxiv.org/abs/2504.18333 per-model numbers, the old "$5 every 30 days … any model" FAQ wording.

---

## Appendix: the protocol as approved

### 1. The decision

| Answer | What we do |
|---|---|
| A. Mammoth earns a place at specific decision points | Add a `viby decide` subcommand (Rust, opt-in by `AI_GATEWAY_API_KEY`) and wire it into the named points, each with a no-model fallback identical to today's behaviour |
| B. It helps only as a cheap pre-filter in front of an LLM stage | Same subcommand, but used only to order or skip work (e.g. which skeptic runs first), never to decide a verdict |
| C. Not good enough on our decisions | Add nothing; record the bar it failed so no future session re-runs this |
| D. A local model on Spark does the same job better | Build the hook against a local endpoint instead; the gateway becomes one optional backend |

Cost of being wrong: medium. Every integration is reversible, but a decision model that silently
mis-routes or waves a finding through damages the toolkit's core promise (evidence gates). So: full
protocol, with a local measurement as the deciding evidence.

### 2. The question (PICOC)

- **Population**: viby-toolkit decision points where a model today makes a typed choice — skill
  routing, "is this task done / continue?", skeptic keep/reject, fan-out-or-not, verify pass/fail.
- **Intervention**: a typed call to `baseten/mammoth-1` through Vercel AI Gateway `/v1/evaluate`
  (boolean probability, choice, score), $0 on the free tier.
- **Comparison**: today's behaviour (Claude decides in-context, or a hook does nothing), and a
  local Spark model.
- **Outcome**: agreement with labelled ground truth; latency (p50/p95); availability (403/429/503
  rate) on the free tier; what code/prompt text leaves the machine.
- **Context**: Claude Code plugin, Rust `viby` binary with no HTTP client today
  (`plugins/viby-toolkit/rust/crates/viby/Cargo.toml`), only a SessionStart hook
  (`plugins/viby-toolkit/hooks/hooks.json`), free-tier key with $5/month and per-model rate limits.

Sub-questions, load-bearing first:

1. **(load-bearing)** How accurate is mammoth on *our* decisions? Measured, not cited:
   skill routing against `tests/routing/probes*.tsv` (~60 labelled prompt → skill rows).
2. **(load-bearing)** What does a typed call cost in latency and failure rate on the free tier,
   and does that fit a hook's budget?
3. **(load-bearing)** What text is sent, retained, or trained on (Baseten + Vercel free tier)?
   Prompts may contain client code.
4. What are mammoth's documented semantics — input limits, calibration of `probability`,
   choice/score shapes, determinism?
5. Where does the practice literature (LLM cascades, routers, small judges) say a cheap typed
   gate helps, and where it hurts?
6. Which Claude Code hook events can host a decision (UserPromptSubmit, Stop, SubagentStop,
   PreToolUse) and with what timeout / failure semantics?

### 3. Competing answers and the observation that excludes one

| Candidate | Excluded if… |
|---|---|
| A. Real decision points | routing top-1 accuracy on the probe set < the current Claude baseline in `tests/routing` scores, or p95 latency > 1.5 s, or free-tier 429/503 > 5 % |
| B. Pre-filter only | accuracy is high enough for A (then B undersells it), or so low that ordering by it is noise (AUROC < 0.65 on keep/reject) |
| C. Not worth it | mammoth matches or beats baseline on any load-bearing decision |
| D. Local Spark better | Spark model is slower than the gateway at equal accuracy, or the Spark host is offline too often to serve a hook |

The crucial observation is the routing measurement: it is labelled, already in the repo, and decides A vs C directly.

### 4. "With and without" — the non-negotiable design rule

Every decision point keeps today's behaviour as its fallback, and the fallback is the default:

- No key, network failure, 4xx/5xx, timeout (hard cap, e.g. 800 ms), or malformed answer → behave
  exactly as the current release does, silently (per "handle trouble behind the scenes").
- The model is advisory: it may add a hint or reorder work; it may never be the only thing that
  passes an evidence gate.
- A test matrix runs every wired point twice, key set and key unset, and both must pass.

### 5. Search plan

One researcher per angle, run in parallel, read-only:

1. **Vendor primary sources**: Baseten Mammoth/Elephant model docs, Vercel AI Gateway
   `/v1/evaluate`, free-tier rate limits, data retention for Baseten via the gateway.
2. **Practice: cascades and routers**: FrugalGPT, RouteLLM, semantic routing, small-classifier
   gates in front of LLMs; reported accuracy/cost trade-offs.
3. **Claude Code hosting**: hook events, timeouts, `additionalContext`, failure behaviour, and
   existing plugins that call external models from hooks.
4. **Opposing**: evidence that small judges/decision models fail on code tasks: calibration
   drift, position bias, prompt-injection through the judged text, vendor lock-in for
   free-tier models that disappear.

Plus a **local measurement** (the deciding evidence): mammoth as a `choice` question over the
skill list for every routing probe; latency and error rate over the same run. Spend is $0 on
the free tier, and the balance is checked before and after.

Inclusion: primary docs, changelogs, issue threads, papers with measurements. Exclusion:
SEO listicles, undated pages, any figure only seen in a search snippet carrying a load-bearing claim.

### 6. Stopping rule

Effort-bounded: top 6 sources per angle, then one snowball round from the two best sources;
stop early on saturation (a round that adds nothing new).

### 7. What would change my mind (written before the evidence)

- Prior: mammoth is good at routing (short natural-language prompt → one of ~30 labels) and weak
  at code-review verdicts. If the routing measurement is poor, I drop A entirely.
- Prior: free-tier limits make it unfit for a per-prompt hook. If the measured 429 rate is ~0 at
  one call per prompt, that prior is refuted.
- Prior: data leaving the machine is the real blocker for client repos. If Vercel/Baseten state
  zero retention on the free tier, that weakens.

---

## Addendum (same day): file search, and shorter skill descriptions

**File search.** The owner asked whether the decision model helps pick files. Another agent wrote 40
"where is X" questions over two repos (viby, viby-flow), each with one correct file, phrased without
file names. It verified the labels from module docs rather than reading every file, so treat them
as plausible, not certain. For each question, agentgrep `find` ranked candidate files, and the
model chose one of them (`filesearch_eval.py`, raw rows `fs-*.jsonl` and `fs25-*.jsonl`):

| | right file first |
|---|---|
| agentgrep `find`, its own first file | 7 / 40 |
| model choosing from `find`'s top 10 (Mammoth / d1-ultra / Elephant) | 19 / 19 / 18 |
| model choosing from `find`'s top 25 (Mammoth / d1-ultra) | 23 / 24 |
| Mammoth, top 25, shown only at confidence ≥ 0.9 | **19 shown, 19 right** |

The model never moved a correct first file to a wrong one with top-10, and once in 40 with top-25.
The ceiling is recall: with top 25, the answer was among the candidates in only 24 of 40. Shipped
as an opt-in line at the top of `find` results, under the same switch as the prompt hint.

**Shorter skill descriptions.** The descriptions were cut from 7,535 to 4,954 characters. With the
short text the forced-choice proxy above was unchanged: 66/67 for both Mammoth and Elephant
(`routing-shortdesc-*.jsonl`). The real check was a paired A/B on opus over the holdout prompts,
310 sessions: 47 % [39–55] short against 44 % [36–52] old, with overlapping intervals, so no
measurable loss. Details are in `tests/routing/README.md`.

**Fixed after review.** A fresh-context review raised 13 findings. Merged, they came to 7; the
validators confirmed 6 and rejected 1.
- A stale cached binary made the hook exit 2, which blocks the prompt. The hook now always exits 0,
  and it skips a launcher cache miss instead of downloading.
- Paths with spaces were dropped from the candidates.
- curl read `~/.curlrc`; it now runs with `-q`.
- `VIBY_DECIDE=off ` with a trailing space was not treated as off.
- A skill name that was never offered could be printed.
- SIGPIPE could kill viby when curl exited early.

The rejected one claimed the second `find` search falls outside the 3 s budget. The budget covers
the model calls only, and that is now stated here.
