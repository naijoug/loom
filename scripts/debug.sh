#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_PID_FILE="${LOOM_DESKTOP_PID_FILE:-/tmp/loom-tauri-dev.pid}"
PORT="${PORT:-1420}"
HOST="${HOST:-127.0.0.1}"

usage() {
  cat <<'USAGE'
Usage: scripts/debug.sh [desktop|web|stop|status]

Commands:
  desktop  Start the Tauri desktop app in the foreground. This is the default.
  web      Start the browser preview on http://127.0.0.1:1420.
  stop     Stop Loom preview/dev processes started from this repository.
  status   Show current preview/dev process status.

Environment variables:
  PORT     Dev server port (default: 1420)
  HOST     Dev server host (default: 127.0.0.1)
USAGE
}

stop_pid() {
  local pid="$1"

  if [[ -z "${pid}" ]] || ! kill -0 "${pid}" 2>/dev/null; then
    return
  fi

  kill "${pid}" 2>/dev/null || true

  for _ in {1..30}; do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return
    fi
    sleep 0.1
  done

  kill -9 "${pid}" 2>/dev/null || true
}

process_cwd_in_root() {
  local pid="$1"
  local cwd

  cwd="$(lsof -a -p "${pid}" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1 || true)"
  [[ "${cwd}" == "${ROOT_DIR}" || "${cwd}" == "${ROOT_DIR}/"* ]]
}

is_desktop_dev_command() {
  local command="$1"

  [[ "${command}" == *"tauri"* && "${command}" == *"dev"* ]] ||
    [[ "${command}" == *"target/debug/loom"* ]] ||
    [[ "${command}" == *"src-tauri/target/debug/bundle/macos/Loom.app/Contents/MacOS/loom"* ]]
}

stop_desktop_dev() {
  if [[ -f "${DESKTOP_PID_FILE}" ]]; then
    stop_pid "$(cat "${DESKTOP_PID_FILE}")"
    rm -f "${DESKTOP_PID_FILE}"
  fi

  while IFS= read -r line; do
    local pid command
    pid="${line%% *}"
    command="${line#* }"

    if [[ "${pid}" == "$$" || "${pid}" == "${PPID}" ]]; then
      continue
    fi

    if ! is_desktop_dev_command "${command}"; then
      continue
    fi

    if [[ "${command}" != *"${ROOT_DIR}"* ]] && ! process_cwd_in_root "${pid}"; then
      continue
    fi

    stop_pid "${pid}"
  done < <(ps -axo pid=,command= 2>/dev/null | sed 's/^ *//' || true)
}

stop_all() {
  "${ROOT_DIR}/scripts/preview.sh" stop >/dev/null 2>&1 || true
  stop_desktop_dev
}

start_desktop() {
  command -v pnpm >/dev/null 2>&1 || {
    echo "pnpm is required. Install it before starting Loom." >&2
    exit 1
  }

  stop_all
  echo "$$" > "${DESKTOP_PID_FILE}"

  echo "Starting Loom desktop app at http://${HOST}:${PORT}..."
  echo "Stop: press Ctrl+C, or run scripts/debug.sh stop from another terminal."

  cd "${ROOT_DIR}"
  if [[ "${PORT}" == "1420" && "${HOST}" == "127.0.0.1" ]]; then
    exec pnpm tauri dev
  else
    export TAURI_DEV_HOST="${HOST}"
    export TAURI_DEV_PORT="${PORT}"
    local tauri_config
    tauri_config="$(printf '{"build":{"devUrl":"http://%s:%s","beforeDevCommand":"pnpm dev --host %s --port %s --strictPort"}}' "${HOST}" "${PORT}" "${HOST}" "${PORT}")"
    exec pnpm tauri dev --config "${tauri_config}"
  fi
}

status() {
  "${ROOT_DIR}/scripts/preview.sh" status || true

  if [[ -f "${DESKTOP_PID_FILE}" ]] && kill -0 "$(cat "${DESKTOP_PID_FILE}")" 2>/dev/null; then
    echo "Loom desktop dev running: PID $(cat "${DESKTOP_PID_FILE}")"
  else
    echo "Loom desktop dev is not running"
  fi
}

case "${1:-desktop}" in
  desktop)
    start_desktop
    ;;
  web)
    stop_desktop_dev
    exec "${ROOT_DIR}/scripts/preview.sh" start
    ;;
  stop)
    stop_all
    echo "Loom local dev stopped"
    ;;
  status)
    status
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
