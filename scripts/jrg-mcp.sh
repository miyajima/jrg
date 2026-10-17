#!/bin/sh
# One fresh retrieval session per MCP connection, using the installed binary.
set -eu
root=${1:?usage: sh jrg-mcp.sh /absolute/project}
binary=${HOME:?HOME is not set}/.local/bin/jrg
session_dir=$(mktemp -d "${TMPDIR:-/tmp}/jrg-mcp.XXXXXX")
child=
cleanup() {
    if [ -n "$child" ]; then
        kill "$child" 2>/dev/null || :
        wait "$child" 2>/dev/null || :
    fi
    rm -rf "$session_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
exec 3<&0
"$binary" --smart --mcp --root "$root" --session "$session_dir/session.json" <&3 3<&- &
child=$!
exec 3<&-
status=0
wait "$child" || status=$?
child=
exit "$status"
