#!/bin/sh
# One fresh retrieval session per MCP connection, using the installed binary.
set -eu
umask 077
root=${1:?usage: sh jrg-mcp.sh /absolute/project}
script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
binary=${HOME:?HOME is not set}/.local/bin/jrg
session_dir=$(mktemp -d "${TMPDIR:-/tmp}/jrg-mcp.XXXXXX")
child=
embedding_child=
unset JRG_EMBEDDING_SOCKET
cleanup() {
    if [ -n "$child" ]; then
        kill "$child" 2>/dev/null || :
        wait "$child" 2>/dev/null || :
    fi
    if [ -n "$embedding_child" ]; then
        kill "$embedding_child" 2>/dev/null || :
        wait "$embedding_child" 2>/dev/null || :
    fi
    rm -rf "$session_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [ "${JRG_EMBEDDING_ENABLED:-0}" = "1" ]; then
    cache_dir=${HOME}/Library/Caches/jrg-embedding
    embedding_socket=$session_dir/embedding.sock
    if mkdir -p "$cache_dir" 2>/dev/null; then
        chmod 700 "$cache_dir" 2>/dev/null || :
        touch "$cache_dir/runtime.jsonl" 2>/dev/null || :
        chmod 600 "$cache_dir/runtime.jsonl" 2>/dev/null || :
        JRG_EMBEDDING_PARENT_PID=$$ python3 "$script_dir/embedding.py" \
            --cache "$cache_dir/vectors.sqlite3" \
            --socket "$embedding_socket" \
            --log "$cache_dir/runtime.jsonl" \
            >/dev/null 2>&1 &
        embedding_child=$!
        attempt=0
        while [ "$attempt" -lt 50 ]; do
            [ -S "$embedding_socket" ] && break
            if ! kill -0 "$embedding_child" 2>/dev/null; then
                embedding_child=
                break
            fi
            sleep 0.1
            attempt=$((attempt + 1))
        done
        if [ -S "$embedding_socket" ]; then
            JRG_EMBEDDING_SOCKET=$embedding_socket
            export JRG_EMBEDDING_SOCKET
        elif [ -n "$embedding_child" ]; then
            kill "$embedding_child" 2>/dev/null || :
            wait "$embedding_child" 2>/dev/null || :
            embedding_child=
        fi
    fi
fi

exec 3<&0
"$binary" --smart --mcp --root "$root" --session "$session_dir/session.json" <&3 3<&- &
child=$!
exec 3<&-
status=0
wait "$child" || status=$?
child=
exit "$status"
