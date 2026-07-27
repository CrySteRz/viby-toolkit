#!/usr/bin/env python3
"""viby-code PreToolUse safety guard.

Deterministic backstop against a small set of genuinely destructive Bash commands.

Design rules:
  - FAIL-OPEN: any error, or the VIBY_SAFETY=off kill-switch, allows the command.
  - JSON-deny form (hookSpecificOutput.permissionDecision), NOT exit-2 — plugin exit-2
    blocking has open bugs.
  - Tiered by VIBY_SAFETY level (critical < high < strict), default 'high'.
  - Portable / stack-agnostic: no project assumptions, one optional `git branch` call.

**Precision matters more than coverage here.** A guard that blocks `rm -rf node_modules`
teaches you to run VIBY_SAFETY=off, which removes the net entirely — so this decides on
the *parsed command* (program + flags + targets), never on a regex over the raw string.
That distinction is what lets `grep 'rm -rf' README.md` and `rm -rf dist` through while
`rm -rf $HOME` and `rm -rf /` still stop. See tests/test_guard.py for the contract.

This is a safety net, not a policy engine. It errs toward allowing work.
"""
import sys, os, json, re, shlex, subprocess

LEVELS = {"critical": 1, "high": 2, "strict": 3}

# Wrappers to skip so `sudo rm -rf /` is judged as `rm -rf /`.
WRAPPERS = {"sudo", "doas", "env", "nice", "ionice", "time", "command",
            "builtin", "exec", "nohup", "stdbuf", "setsid", "xargs"}
SHELLS = {"sh", "bash", "zsh", "dash", "ksh", "fish"}
INTERPRETERS = SHELLS | {"python", "python3", "perl", "ruby", "node", "php"}
READERS = {"cat", "less", "more", "head", "tail", "bat", "strings", "xxd",
           "od", "nl", "tac", "view", "hexdump"}
FETCHERS = {"curl", "wget", "fetch", "httpie", "http"}
PROTECTED_BRANCHES = {"main", "master", "prod", "production"}

# Paths where recursive delete is routine and cheap to redo.
TEMP_PREFIXES = ("/tmp/", "/private/tmp/", "/var/tmp/", "/var/folders/", "/dev/shm/")
# `.env.example` and friends are templates checked into the repo, not secrets.
SAFE_ENV_HINTS = ("example", "sample", "template", "dist", "defaults", "tpl", "tmpl", "schema")
SECRET_EXTS = (".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".ppk")
RAW_DISK = re.compile(r"^/dev/(sd|nvme|disk|hd|vd)", re.IGNORECASE)


def emit_allow():
    print("{}")
    sys.exit(0)


def emit_deny(reason):
    print(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": f"🛑 viby-code safety: {reason} "
                                       f"(set VIBY_SAFETY=off to disable, or run it yourself)",
        }
    }))
    sys.exit(0)


# ---------------------------------------------------------------- parsing

def split_commands(cmd):
    """Tokenize into [(argv, following_operator), ...].

    Uses shlex so quoting is respected: the dangerous-looking text in
    `grep 'rm -rf' f` lands inside a single argument of `grep`, not as a command.
    Raises ValueError on syntax shlex can't parse (caller falls back).
    """
    lex = shlex.shlex(cmd, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    lex.commenters = ""
    tokens = list(lex)

    out, cur = [], []
    for t in tokens:
        if t and all(c in ";|&()<>" for c in t):
            out.append((cur, t))
            cur = []
        else:
            cur.append(t)
    out.append((cur, ""))
    return out


def strip_wrappers(argv):
    """Drop leading sudo/env/etc. Returns (argv, escalated) with escalated=True for sudo."""
    escalated = False
    i = 0
    while i < len(argv):
        prog = os.path.basename(argv[i])
        if prog not in WRAPPERS:
            break
        if prog in ("sudo", "doas"):
            escalated = True
        i += 1
        # skip that wrapper's own flags and env assignments (env FOO=bar cmd)
        while i < len(argv) and (argv[i].startswith("-") or re.match(r"^\w+=", argv[i])):
            i += 1
    return argv[i:], escalated


def parse_flags(args):
    """Split args into (flag set, positional targets). Short clusters expand: -rf -> -r,-f."""
    flags, targets, end_of_opts = set(), [], False
    for a in args:
        if end_of_opts:
            targets.append(a)
        elif a == "--":
            end_of_opts = True
        elif a.startswith("--"):
            flags.add(a)
        elif a.startswith("-") and len(a) > 1:
            for ch in a[1:]:
                flags.add("-" + ch)
        else:
            targets.append(a)
    return flags, targets


# ---------------------------------------------------------------- classifiers

def rm_target_danger(target, cwd):
    """Return a reason string if recursively deleting `target` is dangerous, else None."""
    if re.search(r"\$\{?\w+", target):
        # The hook sees the pre-expansion string; an empty variable turns
        # `rm -rf $DIR/` into `rm -rf /`. Classic footgun.
        return f"unexpanded variable in a recursive-delete target ({target})"
    if target.startswith("~"):
        return f"recursive delete inside the home directory ({target})"
    if os.path.normpath(target) in (".", ".."):
        return f"recursive delete of the working directory itself ({target})"

    first = target.split("/")[0]
    if "*" in first or "?" in first:
        return f"unscoped glob in a recursive-delete target ({target})"

    if target.startswith("/"):
        norm = os.path.normpath(target)
        if norm.rstrip("/") in ("", "/"):
            return "recursive delete of the filesystem root"
        if norm.startswith(TEMP_PREFIXES):
            return None
        if cwd and (norm == cwd or norm.startswith(cwd.rstrip("/") + "/")):
            return None
        if norm.count("/") <= 1:
            return f"recursive delete of a top-level system directory ({target})"
        return f"recursive delete of an absolute path outside the project ({target})"

    if os.path.normpath(target).startswith(".."):
        return f"recursive delete escaping the project directory ({target})"
    return None


def secret_target(arg):
    """Return the offending basename if `arg` looks like a secret file, else None."""
    base = os.path.basename(arg)
    low = base.lower()
    if low == ".env" or low.startswith(".env.") or low.endswith(".env"):
        if any(h in low for h in SAFE_ENV_HINTS):
            return None
        return base
    if low.endswith(SECRET_EXTS):
        return base
    if re.search(r"id_(rsa|dsa|ecdsa|ed25519)(\.pub)?$", low):
        return None if low.endswith(".pub") else base
    if re.search(r"(^|/)\.ssh/", arg) or ".aws/credentials" in arg:
        return base
    if low in ("credentials", ".netrc", ".pgpass", "secrets.json", "secrets.yaml", "secrets.yml"):
        return base
    return None


def git_subcommand(args):
    """Return (subcommand, remaining args), skipping git's own global flags."""
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path"):
            i += 2
            continue
        if a.startswith("-"):
            i += 1
            continue
        return a, args[i + 1:]
    return None, []


def mentions_protected_branch(args):
    for a in args:
        if a in PROTECTED_BRANCHES:
            return a
        if ":" in a:
            for part in a.split(":"):
                if part in PROTECTED_BRANCHES:
                    return part
    return None


def current_branch(cwd=None):
    try:
        return subprocess.run(["git", "branch", "--show-current"], cwd=cwd or None,
                              capture_output=True, text=True, timeout=3).stdout.strip()
    except Exception:
        return ""


# ---------------------------------------------------------------- rules

def check_argv(argv, escalated, threshold, cwd, depth=0):
    """Return (level, reason) for the first violation in one simple command, else None."""
    if not argv:
        return None
    prog = os.path.basename(argv[0])
    args = argv[1:]

    # Recurse into `bash -c "<cmd>"` so a wrapped command is still judged.
    if prog in INTERPRETERS and depth < 2:
        for i, a in enumerate(args):
            if a == "-c" and i + 1 < len(args):
                hit = scan(args[i + 1], threshold, cwd, depth + 1)
                if hit:
                    return hit
                break

    if prog == "rm":
        flags, targets = parse_flags(args)
        if flags & {"-r", "-R", "--recursive"}:
            # Target danger is checked FIRST so `sudo rm -rf /` is judged on the
            # target (critical), not downgraded to the sudo rule (strict).
            for t in targets:
                reason = rm_target_danger(t, cwd)
                if reason:
                    return ("critical", reason)
            if escalated:
                return ("strict", "sudo recursive delete")
            # Scoped recursive delete inside the project: routine cleanup.
            # Only the paranoid posture blocks it.
            return ("strict", "any recursive delete (VIBY_SAFETY=strict)")
        return None

    if prog in READERS:
        for a in args:
            if a.startswith("-"):
                continue
            hit = secret_target(a)
            if hit:
                return ("high", f"reading a secret file ({hit})")
        return None

    if prog == "dd":
        for a in args:
            if a.startswith("of=") and RAW_DISK.match(a[3:]):
                return ("critical", f"dd writing to a raw disk device ({a})")
        return None

    if prog.startswith("mkfs"):
        return ("critical", "filesystem format (mkfs)")

    if prog in ("shred", "wipefs"):
        return ("critical", f"destructive disk utility ({prog})")

    if prog == "chmod":
        flags, targets = parse_flags(args)
        if ({"-R", "--recursive"} & flags) and any(t in ("777", "0777", "a+rwx") for t in targets):
            return ("high", "recursive chmod 777")
        return None

    if prog == "crontab":
        if "-r" in args:
            return ("strict", "crontab -r (wipes the crontab)")
        return None

    if prog == "git":
        sub, rest = git_subcommand(args)
        flags, targets = parse_flags(rest)
        if sub == "push":
            forced = bool(flags & {"-f", "--force"})
            leased = any(f.startswith("--force-with-lease") or f.startswith("--force-if-includes")
                         for f in flags)
            if forced and not leased:
                branch = mentions_protected_branch(targets)
                if branch:
                    return ("high", f"force-push to protected branch '{branch}'")
                return ("strict", "force-push")
        elif sub == "reset" and (flags & {"--hard"}):
            return ("high", "git reset --hard (discards uncommitted work)")
        elif sub == "clean" and (flags & {"-f", "--force"}):
            return ("high", "git clean -f (deletes untracked files)")
        elif sub == "checkout" and targets and targets[0] == ".":
            return ("strict", "git checkout . (discards local changes)")
        elif sub == "rebase" and threshold >= LEVELS["high"]:
            # Rewriting history on a shared branch. `reset` is not included:
            # --hard is already covered above, and plain/--soft reset is routine.
            if current_branch(cwd) in PROTECTED_BRANCHES:
                return ("high", f"rebase directly on protected branch '{current_branch(cwd)}'")
        return None

    return None


def scan(cmd, threshold, cwd, depth=0):
    """Return (level, reason) for the first violation in a full command line, else None."""
    # Fork bomb is shell syntax, not a program — match it structurally on the raw text.
    if re.search(r":\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}", cmd):
        return ("critical", "fork bomb")

    # Command substitution bodies hide inside a single quoted token, so shlex would
    # hand them over as one opaque argument. Scan them in their own right.
    if depth < 3:
        for body in re.findall(r"\$\(([^()]+)\)", cmd) + re.findall(r"`([^`]+)`", cmd):
            hit = scan(body, threshold, cwd, depth + 1)
            if hit:
                return hit

    try:
        commands = split_commands(cmd)
    except ValueError:
        # Unparseable (unbalanced quotes, exotic syntax). Fall back to the
        # unambiguous catastrophic patterns only — never guess and block real work.
        for pat, reason in (
            (r"\brm\s+-[a-zA-Z]*[rR][a-zA-Z]*f?\s+(/|~|\$HOME)(\s|$)", "recursive delete of / or home"),
            (r"\bmkfs\.", "filesystem format (mkfs)"),
            (r"\bdd\b[^|;]*\bof=/dev/(sd|nvme|disk|hd)", "dd writing to a raw disk device"),
        ):
            if re.search(pat, cmd):
                return ("critical", reason)
        return None

    for idx, (argv, op) in enumerate(commands):
        stripped, escalated = strip_wrappers(argv)

        hit = check_argv(stripped, escalated, threshold, cwd, depth)
        if hit and LEVELS[hit[0]] <= threshold:
            return hit

        # Cross-command shapes: `curl ... | sh` and `... > /dev/sda`.
        nxt = commands[idx + 1][0] if idx + 1 < len(commands) else []
        if op == "|" and stripped and nxt and os.path.basename(stripped[0]) in FETCHERS:
            nxt_argv = strip_wrappers(nxt)[0]
            nxt_prog = os.path.basename(nxt_argv[0]) if nxt_argv else ""
            if nxt_prog in INTERPRETERS and LEVELS["high"] <= threshold:
                return ("high", f"piping {os.path.basename(stripped[0])} straight into a shell")
        if op in (">", ">>") and nxt and RAW_DISK.match(nxt[0]):
            if LEVELS["critical"] <= threshold:
                return ("critical", f"redirect over a raw disk device ({nxt[0]})")

    return None


def main():
    level = os.environ.get("VIBY_SAFETY", "high").strip().lower()
    if level in ("off", "0", "false", "none"):
        emit_allow()
    threshold = LEVELS.get(level, LEVELS["high"])

    data = json.load(sys.stdin)
    if data.get("tool_name") != "Bash":
        emit_allow()
    cmd = (data.get("tool_input") or {}).get("command", "")
    if not cmd.strip():
        emit_allow()
    cwd = os.path.normpath(data.get("cwd") or os.getcwd())

    hit = scan(cmd, threshold, cwd)
    if hit and LEVELS[hit[0]] <= threshold:
        emit_deny(hit[1])
    emit_allow()


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Never wedge a session on a guard bug.
        emit_allow()
