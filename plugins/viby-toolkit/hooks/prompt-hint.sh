#!/bin/sh
# UserPromptSubmit: an advisory skill hint from a decision model (viby prompt-hint).
#
# Opt-in. Unless AI_GATEWAY_API_KEY is set AND VIBY_DECIDE=on, exit here: no binary runs, nothing
# is printed and the prompt never leaves the machine.
[ -n "${AI_GATEWAY_API_KEY:-}" ] || exit 0
case "${VIBY_DECIDE:-}" in [Oo][Nn]) ;; *) exit 0 ;; esac
# Exit 0 whatever happens: on UserPromptSubmit exit 2 blocks the prompt, and a viby too old to know
# prompt-hint exits 2. A cache miss is skipped rather than downloaded inside the hook's timeout.
here=$(dirname -- "$0")
VIBY_CACHED_ONLY=1 sh "$here/../launcher/viby" prompt-hint "$here/../skills" 2>/dev/null
exit 0
