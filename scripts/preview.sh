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
    if [[ "${LOOM_PREVIEW_TARGET:-loom}" == "browser-probe" ]]; then
      cd "${ROOT_DIR}/experiments/browser-host/web"
      node "${ROOT_DIR}/node_modules/vite/bin/vite.js" --host "${HOST}" --port "${PORT}" --strictPort > "${LOG_FILE}" 2>&1 &
    elif [[ "${LOOM_PREVIEW_TARGET:-loom}" == "craft-reference" ]]; then
      local reference="${ROOT_DIR}/.ref/repos/craft-agents-oss"
      [[ -f "${reference}/node_modules/vite/bin/vite.js" ]] || { echo "Install the pinned Craft reference dependencies first." >&2; exit 1; }
      cd "${reference}"
      node "${reference}/node_modules/vite/bin/vite.js" --config "${reference}/apps/electron/vite.config.ts" --host "${HOST}" --port "${PORT}" --strictPort > "${LOG_FILE}" 2>&1 &
    else
      pnpm dev --host "${HOST}" --port "${PORT}" --strictPort > "${LOG_FILE}" 2>&1 &
    fi
    echo "$!" > "${PID_FILE}"
  )

  local pid ready=0
  for _ in {1..80}; do
    if pid="$(preview_listener_pid)"; then
      echo "${pid}" > "${PID_FILE}"
      ready=1
      break
    fi
    sleep 0.1
  done

  if [[ "${ready}" != "1" ]]; then
    echo "Preview failed to listen on http://${HOST}:${PORT}. See ${LOG_FILE}" >&2
    stop_pid "$(cat "${PID_FILE}")"
    rm -f "${PID_FILE}"
    return 1
  fi

  pid="$(cat "${PID_FILE}")"
  echo "Loom preview started"
  if [[ "${LOOM_PREVIEW_TARGET:-loom}" == "craft-reference" ]]; then
    echo "Reference: http://${HOST}:${PORT}/playground.html"
  elif [[ "${LOOM_PREVIEW_TARGET:-loom}" == "browser-probe" ]]; then
    echo "Browser fixture: http://${HOST}:${PORT}/"
  else
    echo "URL: http://${HOST}:${PORT}/?fixture=simple-chat"
  fi
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
      echo "URL: http://${HOST}:${PORT}/?fixture=simple-chat"
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
