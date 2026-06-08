#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${LOOM_PREVIEW_PORT:-1420}"
HOST="${LOOM_PREVIEW_HOST:-127.0.0.1}"
PID_FILE="${LOOM_PREVIEW_PID_FILE:-/tmp/loom-preview-vite.pid}"
LOG_FILE="${LOOM_PREVIEW_LOG:-/tmp/loom-preview-vite.log}"

stop_pid() {
  local pid="$1"

  if [[ -z "${pid}" ]] || ! kill -0 "${pid}" 2>/dev/null; then
    return
  fi

  kill "${pid}" 2>/dev/null || true

  for _ in {1..20}; do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return
    fi
    sleep 0.1
  done

  kill -9 "${pid}" 2>/dev/null || true
}

preview_listener_pid() {
  local port_pids
  port_pids="$(lsof -tiTCP:"${PORT}" -sTCP:LISTEN 2>/dev/null || true)"

  for pid in ${port_pids}; do
    local command
    command="$(ps -p "${pid}" -o command= 2>/dev/null || true)"
    if [[ "${command}" == *"${ROOT_DIR}"*vite* ]]; then
      echo "${pid}"
      return 0
    elif [[ -n "${command}" ]]; then
      echo "Port ${PORT} is already used by a non-Loom process:" >&2
      echo "  ${pid} ${command}" >&2
      return 2
    fi
  done

  return 1
}

stop_existing() {
  if [[ -f "${PID_FILE}" ]]; then
    stop_pid "$(cat "${PID_FILE}")"
    rm -f "${PID_FILE}"
  fi

  while pid="$(preview_listener_pid)"; do
    stop_pid "${pid}"
  done
}

start_preview() {
  stop_existing
  : > "${LOG_FILE}"

  (
    cd "${ROOT_DIR}"
    pnpm dev --host "${HOST}" --port "${PORT}" --strictPort > "${LOG_FILE}" 2>&1 &
    echo "$!" > "${PID_FILE}"
  )

  local pid
  for _ in {1..80}; do
    if pid="$(preview_listener_pid)"; then
      echo "${pid}" > "${PID_FILE}"
      break
    fi
    sleep 0.1
  done

  pid="$(cat "${PID_FILE}")"
  echo "Loom preview started"
  echo "URL: http://${HOST}:${PORT}/preview/planning?step=review"
  echo "PID: ${pid}"
  echo "Log: ${LOG_FILE}"
}

case "${1:-start}" in
  start|restart)
    start_preview
    ;;
  stop)
    stop_existing
    echo "Loom preview stopped"
    ;;
  status)
    if pid="$(preview_listener_pid)"; then
      echo "${pid}" > "${PID_FILE}"
      echo "Loom preview running: PID ${pid}"
      echo "URL: http://${HOST}:${PORT}/preview/planning?step=review"
    else
      echo "Loom preview is not running"
      exit 1
    fi
    ;;
  *)
    echo "Usage: scripts/preview.sh [start|restart|stop|status]" >&2
    exit 2
    ;;
esac
