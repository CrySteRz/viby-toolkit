#!/bin/sh
# Compatibility shim from 2.x, when hooks and skills ran `.ts` files through this script.
#
# Settings written for 2.x still call `run.sh <path>/statusline.ts` and friends, so an old path
# is mapped to its `viby` subcommand and run through the launcher. Anything else passes through
# unchanged: `run.sh check-plan docs/plans/x.md` is `viby check-plan docs/plans/x.md`.
set -u

[ $# -gt 0 ] || exit 0
command=$1
shift

case "$command" in
  *.ts)
    name=$(basename -- "$command" .ts)
    case "$name" in
      post-tool-use-format) name=format-hook ;;
      server) name=$(basename -- "$(dirname -- "$command")")-server ;; # browser/, agentgrep/
      host) name=browser-host ;;
    esac
    command=$name
    ;;
esac

exec sh "$(dirname -- "$0")/../launcher/viby" "$command" "$@"
