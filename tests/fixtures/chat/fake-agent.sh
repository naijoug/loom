#!/bin/sh
# Deterministic subprocess fixture; no model, network or credentials.
set -eu
case "${1:-}" in
  stderr-flood)
    i=0
    while [ "$i" -lt 4096 ]; do
      printf '%s\n' 'stderr pressure: 012345678901234567890123456789012345678901234567890123456789' >&2
      i=$((i + 1))
    done
    printf 'answer after stderr\n'
    ;;
  partial-failure)
    printf 'partial answer\n'
    printf 'fixture failed\n' >&2
    exit 7
    ;;
  audit-logs)
    printf '%s\n' 'first 中 output' 'api_key=fixture-private-value'
    printf '%s\n' 'stderr diagnostic' 'Authorization: Bearer fixture-bearer-value' >&2
    exit 7
    ;;
  ignore-term)
    trap '' TERM
    printf 'READY\n'
    while :; do sleep 0.05; done
    ;;
  descendant-pipes|descendant-closed)
    if [ "$1" = descendant-closed ]; then
      sh -c 'trap "" TERM; echo $$ > child.pid; exec sleep 30' >/dev/null 2>&1 &
    else
      sh -c 'trap "" TERM; echo $$ > child.pid; exec sleep 30' &
    fi
    while [ ! -s child.pid ]; do sleep 0.01; done
    printf 'parent finished\n'
    ;;
  stdin)
    cat
    ;;
  utf8)
    printf '\344'
    sleep 0.05
    printf '\270\255\n'
    ;;
  unknown-json)
    printf '%s\n' '{"type":"telemetry","textless":"do not show"}'
    printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"structured answer"}}'
    ;;
  batched-tools)
    printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"read-1","content":"first result"},{"type":"tool_result","tool_use_id":"read-2","content":"SENTINEL-SECOND"},{"type":"tool_result","tool_use_id":"edit-3","is_error":true,"content":"permission denied"}]}}'
    ;;
  huge-line)
    while :; do printf '012345678901234567890123456789012345678901234567890123456789'; done
    ;;
  *) printf 'unknown fixture mode\n' >&2; exit 2 ;;
esac
