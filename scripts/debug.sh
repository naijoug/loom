#!/usr/bin/env bash
set -euo pipefail

PORT="${PORT:-11420}"
HOST="${HOST:-127.0.0.1}"

kill_port() {
  local port="$1"
  local pids

  pids="$(lsof -ti "tcp:${port}" 2>/dev/null || true)"
  if [[ -z "${pids}" ]]; then
    return 0
  fi

  echo "Killing process(es) on port ${port}: ${pids}"
  kill ${pids} 2>/dev/null || true

  for _ in {1..20}; do
    if [[ -z "$(lsof -ti "tcp:${port}" 2>/dev/null || true)" ]]; then
      return 0
    fi
    sleep 0.1
  done

  pids="$(lsof -ti "tcp:${port}" 2>/dev/null || true)"
  if [[ -n "${pids}" ]]; then
    echo "Force killing process(es) on port ${port}: ${pids}"
    kill -9 ${pids} 2>/dev/null || true
  fi
}

cd "$(dirname "$0")/.."
kill_port "${PORT}"

echo "Starting Loom desktop app at http://${HOST}:${PORT}/"
export TAURI_DEV_HOST="${HOST}"
export TAURI_DEV_PORT="${PORT}"

TAURI_CONFIG="$(printf '{"build":{"devUrl":"http://%s:%s","beforeDevCommand":"pnpm dev -- --host %s --port %s --strictPort"}}' "${HOST}" "${PORT}" "${HOST}" "${PORT}")"
exec pnpm tauri dev --config "${TAURI_CONFIG}"
