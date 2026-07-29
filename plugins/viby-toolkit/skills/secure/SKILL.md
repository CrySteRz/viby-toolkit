---
name: secure
description: >
  Use for a security-focused pass over code, dependencies, or configuration — "check this for
  security problems", "is this secure", "security review", "did I leak a secret", "check for
  vulnerabilities", "audit the dependencies", "is this endpoint safe", before exposing something publicly, or before
  committing config and CI changes. Deeper and differently-aimed than the security dimension
  inside /viby-toolkit:review-cluster, which reviews one diff. Broader than any diff-scoped review:
  it covers credentials already in git history, the supply chain, and the agent skills you install —
  not only the pending change.
---

# Secure (credentials first, then the supply chain, then the code)

```
IRON LAW: Order the pass by what actually goes wrong, not by what is interesting.
          Credentials, then supply chain and CI, then code surfaces.
          Confirm every candidate secret before reporting it — most look-alikes are not real.
```

Follow `/viby-toolkit:principles`. Detection is read work — **fan out**. The judgement about
what is genuinely exploitable stays on the main thread.

## Why this order

A 2026 study of 16,112 file changes across 4,022 agent-assisted pull requests
([arXiv 2607.12428](https://arxiv.org/abs/2607.12428)) measured what actually reaches
`main`:

- **Hard-coded credentials were 99.6% of critical-severity findings.** Not injection, not
  crypto — credentials. Look there first.
- **81.1% of leaked credentials reached integration undetected** by bots *and* human review.
  A dedicated pass exists because ordinary review demonstrably does not catch these.
- **Supply-chain and CI misconfiguration were 82.3% of all findings** by volume.
- **67.6% of the genuine leaks were introduced by the human collaborator, not the agent.**
  So review the whole change, not just the machine-written part — and do not treat
  hand-written config as trusted.

One caution from that same paper, which cuts the other way: its automated labelling had a
**27.2% validation rate** — roughly three in four flagged "secrets" were not real. That is
why confirming each candidate is in the Iron Law. A wall of false secret reports trains
everyone to ignore the real one.

## 1. Credentials

- Scan for high-entropy strings and known key shapes: provider prefixes (`AKIA`, `ghp_`,
  `sk-`, `xox`), private-key headers, connection strings with inline passwords, JWTs,
  `Authorization:` literals, `.pem`/`.p12` contents committed as text.
- **Check history, not just the working tree** — `git log -p -S'<fragment>'`. A secret
  removed in a later commit is still published.
- Look where secrets hide from scanners: test fixtures, seed data, `.env.example` that was
  filled in for real, notebooks, CI logs, lockfiles, committed `.tfstate`, docker build args,
  container labels, screenshots and recorded HTTP fixtures.
- **For each candidate, decide: real, or a placeholder?** Test doubles, obvious dummies, and
  documented examples are not leaks. Confirm by looking at the value and its context.
- **A confirmed live credential is a rotate-first situation.** Say so immediately and
  plainly — removing it from the code does not un-publish it. Do not bury that under other
  findings, and do not attempt the rotation yourself.

## 2. Supply chain and CI — the biggest category by volume

- Run the ecosystem's own audit (`npm audit`, `pip-audit`, `cargo audit`, `govulncheck`,
  `bundler-audit`, `mvn dependency-check`). Read the output; triage by whether the vulnerable
  path is actually reachable, not by CVE count.
- **Lockfile hygiene:** is there a lockfile, is it committed, does install honour it
  (`npm ci`, `--frozen-lockfile`)? An unpinned build is a supply-chain decision made by
  whoever publishes next.
- **Dependency plausibility:** a name that is nearly a well-known package, a package added
  recently with few releases, a postinstall script, a git or URL dependency, a maintainer
  change. Typosquats and hijacked packages arrive through ordinary-looking updates.
- **CI is production.** Check: workflow secrets exposed to fork PRs
  (`pull_request_target`), actions pinned by tag rather than SHA, over-broad token
  permissions, secrets echoed into logs, caches poisonable across branches, self-hosted
  runners reachable by untrusted PRs.
- **Container and infra config:** running as root, `latest` tags, baked-in credentials,
  world-readable buckets, security groups open to `0.0.0.0/0`, disabled TLS verification.

## 2b. Agent skills and plugins are now part of that supply chain

A skill is not a document. It is instructions executed with your credentials, in your repositories,
with your agent's tool access — and it arrives with no review and no signature. The measured state
of that ecosystem: an audit of **3,984 skills** across two public marketplaces (Feb 2026) found
**36% containing security flaws, 1,467 with active malicious payloads, prompt injection in 36%**,
summarised as *"if you've installed one in the past month, there's a 13% chance it contains a
critical security flaw"*. A coordinated campaign distributing 30+ malicious skills was documented
the same month. Official marketplaces are reviewed and signed; community ones generally are not.

Before installing anything — or when auditing what is already installed:

```bash
AUD=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/skills/secure/scripts/check-skill-safety.ts 2>/dev/null | tail -1)
RUN=$(ls "$HOME"/.claude/plugins/cache/*/viby-toolkit/*/hooks/run.sh 2>/dev/null | tail -1)
sh "$RUN" "$AUD" <path-to-the-skill-or-plugin>
sh "$RUN" "$AUD" "$HOME"/.claude/plugins/cache/*        # what you already trust
```

It flags the patterns malicious and careless skills share: a credential path meeting a network call,
`curl | bash`, **instructions to act without telling the user** (the most reliable single marker —
there is no legitimate reason for a skill to require concealment), attempts to override existing
instructions, writes to the agent's own settings, encoded payloads, and invisible bidi/zero-width
characters that make what a human reads differ from what the model receives.

**Read the SKILL.md yourself anyway.** Pattern matching cannot prove a skill is safe, an attacker
who knows the check exists can phrase around it, and the three questions that matter are not
mechanical: *what does it need my tools for, what does it send anywhere, and would I have approved
that if it had asked?*

## 3. Code surfaces — reachability decides severity

Only after the above, and only where untrusted input can actually arrive:

- **Authentication vs authorization.** The common real bug is not a missing login, it is a
  missing *ownership* check: an authenticated user reaching another user's object by id.
  Check every handler that takes an identifier.
- **Injection at every interpreter boundary** — SQL, shell, template, LDAP, XPath,
  deserialization. Look for string concatenation reaching an interpreter.
- **SSRF and path traversal** — user-controlled URLs or filenames reaching a fetch or a file
  read; `../` and absolute-path handling; symlinks.
- **Secrets in transit and at rest** — logged tokens, tokens in URLs or query strings,
  unencrypted PII, overly broad error responses leaking internals.
- **Denial of surface** — unbounded input size, unbounded pagination, regexes with
  catastrophic backtracking on user input, zip bombs.
- **Crypto misuse** — home-rolled crypto, ECB, static IVs, `md5`/`sha1` for passwords,
  non-constant-time comparison of secrets.
- **Client-side trust** — validation only in the browser, secrets shipped in a bundle,
  permissive CORS, `dangerouslySetInnerHTML` on untrusted content.

For each candidate ask **reachability**: can untrusted input actually get here, and what
does an attacker gain? An unreachable "vulnerability" is a P3 note, not a finding. This is
the same grounding discipline `/viby-toolkit:review-cluster` applies — quote the line, name a
concrete exploit path, and let a fresh-context check kill anything you cannot substantiate.

## 4. Report

Order by **exploitability × blast radius**, not by category:

1. **Confirmed live credentials** — first, always, with "rotate now" stated explicitly.
2. **Reachable vulnerabilities** — with the concrete path from untrusted input to impact.
3. **Supply chain and CI** — with whether the vulnerable code path is actually reachable.
4. **Hardening** — real but not currently exploitable; label it as such.

For each: `file:line`, the quoted line, the concrete attack path, and the fix. State what
you scanned and what you did **not** — a security report that hides its scope invites the
reader to assume it was total. And say plainly when a finding is a hypothesis you could not
confirm rather than dressing it up.

Never commit a proof-of-concept exploit, and never include a real credential value in a
report, a commit message, or an issue.
