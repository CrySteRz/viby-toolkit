#!/usr/bin/env python3
"""forge PreToolUse safety guard.

Deterministic backstop against a small set of genuinely destructive Bash commands.
Design rules (from current Claude Code hook practice, mid-2026):
  - FAIL-OPEN: any error, or the FORGE_SAFETY=off kill-switch, allows the command.
  - JSON-deny form (hookSpecificOutput.permissionDecision), NOT exit-2 — plugin exit-2
    blocking has open bugs.
  - Tiered by FORGE_SAFETY level (critical < high < strict), default 'high'. Only blocks
    what is dangerous-and-rarely-legitimate; the level knob is the escape valve.
  - Portable / stack-agnostic: pure regex over the command string + one `git branch` call.

This is a safety net, not a policy engine. It errs toward allowing work.
"""
import sys, os, json, re, subprocess

def allow():
    print("{}")
    sys.exit(0)

def deny(reason):
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": f"🛑 forge safety: {reason} "
                                        f"(set FORGE_SAFETY=off to disable, or run it yourself)",
        }
    }))
    sys.exit(0)

def main():
    level = os.environ.get("FORGE_SAFETY", "high").strip().lower()
    if level in ("off", "0", "false", "none"):
        allow()
    rank = {"critical": 1, "high": 2, "strict": 3}
    threshold = rank.get(level, 2)

    data = json.load(sys.stdin)
    if data.get("tool_name") != "Bash":
        allow()
    cmd = (data.get("tool_input") or {}).get("command", "")
    if not cmd:
        allow()

    # (regex, level, reason). Fires only if rank[level] <= threshold.
    rules = [
        # critical — catastrophic, essentially never intended by an agent
        (r"\brm\s+-[a-z]*r[a-z]*f|\brm\s+-[a-z]*f[a-z]*r", "critical", "recursive force-delete (rm -rf)"),
        (r"\brm\s+-[rf].*\s(/|~|\$HOME)(\s|$)", "critical", "rm targeting / or home"),
        (r"\bdd\b.*\bof=/dev/(sd|nvme|disk|hd)", "critical", "dd writing to a raw disk device"),
        (r"\bmkfs\.", "critical", "filesystem format (mkfs)"),
        (r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}", "critical", "fork bomb"),
        (r">\s*/dev/(sd|nvme|disk)", "critical", "redirect over a raw disk device"),
        # high — destructive or secret-leaking, occasionally legitimate (use the level knob)
        (r"\bgit\s+push\b(?=.*(?:\s|=)(?:-f|--force)(?:\s|$))(?!.*--force-with-lease)(?=.*\b(?:main|master|prod|production)\b)",
         "high", "force-push to a protected branch"),
        (r"\bgit\s+reset\s+--hard\b", "high", "git reset --hard (discards work)"),
        (r"\bgit\s+clean\s+-[a-z]*f", "high", "git clean -f (deletes untracked files)"),
        (r"\b(cat|less|more|head|tail|bat)\b[^|;&]*\.(env|pem|key)\b", "high", "reading a secret/.env/key file"),
        (r"\b(cat|less|more|head|tail)\b[^|;&]*(id_rsa|id_ed25519|credentials|\.aws/|\.ssh/id_)", "high", "reading credentials"),
        (r"\bchmod\s+-R?\s*777\b", "high", "chmod 777"),
        (r"\bcurl\b[^|]*\|\s*(sudo\s+)?(sh|bash|zsh)\b", "high", "piping curl straight into a shell"),
        (r"\bwget\b[^|]*\|\s*(sudo\s+)?(sh|bash|zsh)\b", "high", "piping wget straight into a shell"),
        # strict — opt-in stricter posture
        (r"\bgit\s+push\b.*(?:\s|=)(?:-f|--force)(?:\s|$)", "strict", "any force-push"),
        (r"\bsudo\s+rm\b", "strict", "sudo rm"),
        (r"\bgit\s+checkout\s+\.", "strict", "git checkout . (discards local changes)"),
        (r"\bcrontab\s+-r\b", "strict", "crontab -r (wipes crontab)"),
    ]

    for pat, lvl, reason in rules:
        if rank[lvl] <= threshold and re.search(pat, cmd, re.IGNORECASE):
            deny(reason)

    # Branch-aware: block destructive git ops while ON a protected branch, even without -f.
    if threshold >= 2 and re.search(r"\bgit\s+(push|merge|rebase|reset|commit)\b", cmd, re.IGNORECASE):
        try:
            branch = subprocess.run(["git", "branch", "--show-current"],
                                    capture_output=True, text=True, timeout=3).stdout.strip()
        except Exception:
            branch = ""
        if branch in ("main", "master", "prod", "production") and re.search(r"\bgit\s+(reset|rebase)\b", cmd, re.IGNORECASE):
            deny(f"destructive git op directly on protected branch '{branch}'")

    allow()

if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Never wedge a session on a guard bug.
        allow()
