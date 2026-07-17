# claude-toolkit

My personal Claude Code toolkit, distributed as a private plugin marketplace. One repo,
installed once per machine at **user scope**, so it applies automatically to every
project — work and personal — and travels with me to any new computer.

Marketplace: **`ionut-toolkit`** · Plugin: **`forge`**

---

## What's in it

`forge` is an accuracy-first, token-disciplined set of engineering workflows. It's
stack-agnostic (detects the project at runtime; assumes nothing).

### Skills (auto-trigger by context, or call with `/forge:<name>`)

| Skill | What it does |
|---|---|
| `/forge:orchestrate` | Drives a task end-to-end: scope → explore → plan → implement → verify → self-review. Fans out cheap scouts for discovery, keeps main context clean. |
| `/forge:review-cluster` | **Review cluster + false-positive filter.** Parallel per-dimension reviewers find candidates; adversarial skeptics try to refute each one; only findings that survive reach you. Reports how many false positives it killed. |
| `/forge:debug` | Root-cause debugging by hypothesis and evidence — reproduce → localize → confirm → fix → verify. No speculative patching. |
| `/forge:migrate` | Wide mechanical changes (renames, upgrades, pattern sweeps): discover every site → transform in batches → verify each → final zero-remaining sweep. |
| `/forge:plan` | Turns a fuzzy task into an ordered, file-anchored change-list with the risky step and verification strategy called out. |
| `/forge:forge-principles` | The operating contract everything follows: accuracy rules, context hygiene, the model-routing table. Read-only reference. |

### Command

- `/forge:ship <task>` — run the whole pipeline autonomously and don't stop until verified.

### Agents (dispatched by the skills; cheap models by design)

`scout` (haiku, read-only recon) · `implementer` (sonnet) · `reviewer` (sonnet,
one per review dimension) · `skeptic` (sonnet, adversarial false-positive filter) ·
`debugger` (sonnet, evidence gathering).

### Hook

A tiny `SessionStart` hook injects the accuracy-first + token-discipline defaults (~120
tokens/session) so the working style applies even when no skill is explicitly invoked.

---

## The token / rate-limit strategy (without losing accuracy)

Designed for a Claude Max subscription, where the scarce resources are the **main
thread's context window** and your **rate-limit budget** — not dollars.

1. **Subagents are context firewalls.** Bulk reading (grep 40 files, read 10) happens in
   disposable subagents that return a ~200-token conclusion. The 30k tokens of file dumps
   die with the subagent and never touch main context.
2. **Model routing.** Mechanical search → `haiku`. Parallel reading/reviewing/implementing
   → `sonnet`. Planning, synthesis, and final judgment → the strong main-thread model.
   Cheap models *find*; the strong model *decides*.
3. **Don't fan out for what you already know.** Known file, known symbol → read it inline.
   Fan-out pays off only when the search space is wide.
4. **Adversarial verification** keeps accuracy high while most tokens are spent cheaply —
   many cheap voices get cross-checked, so a single cheap voice being wrong doesn't sink
   the result.

Full contract: `/forge:forge-principles`.

---

## Install on a new machine

Prereqs: `gh` authenticated (`gh auth status`) or SSH access to this private repo.

**Option A — via Claude Code (recommended):**

```
/plugin marketplace add ionutblidaruvsp/claude-toolkit
/plugin install forge@ionut-toolkit
```

Then confirm it's enabled at **user scope** so it applies to every project.

**Option B — via settings.json** (makes it declarative / reproducible). Add to
`~/.claude/settings.json`:

```jsonc
{
  "extraKnownMarketplaces": {
    "ionut-toolkit": {
      "source": { "source": "github", "repo": "ionutblidaruvsp/claude-toolkit" }
    }
  },
  "enabledPlugins": {
    "forge@ionut-toolkit": true
  }
}
```

Restart Claude Code. Verify with `/plugin` (should list `forge` as enabled) and by typing
`/forge:` (skills should autocomplete).

### Private-repo auto-update note

Background auto-update disables git credential helpers, so HTTPS pulls of a private
marketplace can fail silently. Either:
- use **SSH** (loaded key in `ssh-agent`) for the marketplace, or
- run `gh auth setup-git` once, or
- set `CLAUDE_CODE_PLUGIN_KEEP_MARKETPLACE_ON_FAILURE=1` to keep the working clone if a
  refresh fails.

Force a manual update anytime with `/plugin update forge`.

---

## Editing the toolkit

Everything lives here as plain files:

```
.claude-plugin/marketplace.json      # marketplace manifest
plugins/forge/
  .claude-plugin/plugin.json         # plugin manifest
  skills/<name>/SKILL.md             # the workflows
  agents/<name>.md                   # the subagents (model routing in frontmatter)
  commands/ship.md                   # the autonomous entry command
  hooks/hooks.json + session-start.sh
```

After editing, bump `version` in `plugin.json`, commit, push. Validate before pushing:

```
claude plugin validate .
```

Machines pick up the change on next session (or `/plugin update forge`).

**Secrets:** this repo syncs across work and personal machines — never commit tokens,
credentials, client names, or internal hostnames. `.gitignore` blocks the common ones;
keep skills/agents project-agnostic and put anything project-specific in that project's
own `.claude/`.
