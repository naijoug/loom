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
  LOOM_PREVIEW_TARGET=craft-reference  Run the pinned M0 reference app.
  LOOM_PREVIEW_TARGET=browser-probe    Run the Rust Chromium M0 probe.
  LOOM_PREVIEW_BINARY                 Run a built Loom.app binary under src-tauri/target.
  LOOM_PREVIEW_SANDBOX_PROFILE        Optional absolute macOS sandbox profile for that binary.
USAGE
}

stop_pid() {
  local pid="$1"
  local original_command

  if [[ -z "${pid}" ]] || ! kill -0 "${pid}" 2>/dev/null; then
    return
  fi

  original_command="$(ps -p "${pid}" -o command= 2>/dev/null || true)"
  [[ -n "${original_command}" ]] || return 0

  kill "${pid}" 2>/dev/null || true

  # The application has an eight-second graceful exit budget.
  for _ in {1..100}; do
    if ! kill -0 "${pid}" 2>/dev/null; then
      return
    fi
    [[ "$(ps -p "${pid}" -o command= 2>/dev/null || true)" == "${original_command}" ]] || return 0
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
  local dev_pattern='(^|[[:space:]])([^[:space:]]*/)?tauri(\.js)?[[:space:]]+dev([[:space:]]|$)'
  # Match executable/argument boundaries. A rustc command also contains both
  # "src-tauri" and "cfg(dev)" and must never be stopped as a dev launcher.
  [[ "${command}" =~ ${dev_pattern} ]] ||
    [[ "${command}" == "node ${ROOT_DIR}/scripts/craft-reference.mjs" ]] ||
    [[ "${command}" == "node ${ROOT_DIR}/scripts/browser-probe.mjs" ]] ||
    [[ "${command}" == "target/debug/loom" || "${command}" == "target/debug/loom "* ]] ||
    [[ "${command}" == "${ROOT_DIR}/src-tauri/target/debug/loom" || "${command}" == "${ROOT_DIR}/src-tauri/target/debug/loom "* ]] ||
    [[ "${command}" == "${ROOT_DIR}/src-tauri/target/"*"/Loom.app/Contents/MacOS/loom" || "${command}" == "${ROOT_DIR}/src-tauri/target/"*"/Loom.app/Contents/MacOS/loom "* ]]
}

stop_desktop_dev() {
  local app_pids=() wrapper_pids=()
  local process_listing
  if ! process_listing="$(ps -axo pid=,command= 2>/dev/null)"; then
    echo "Cannot inspect processes; preview shutdown is unverified." >&2
    return 1
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

    if [[ "${command}" == *"target/debug/loom"* || "${command}" == *"Loom.app/Contents/MacOS/loom"* ]]; then
      app_pids+=("${pid}")
    else
      wrapper_pids+=("${pid}")
    fi
  done < <(printf '%s\n' "${process_listing}" | sed 's/^ *//')

  # Stop the app before its launcher/dev server can force it to disappear.
  # macOS /bin/bash 3.2 treats empty arrays as unset under `set -u`.
  for pid in ${app_pids[@]+"${app_pids[@]}"} ${wrapper_pids[@]+"${wrapper_pids[@]}"}; do
    local command
    command="$(ps -p "${pid}" -o command= 2>/dev/null || true)"
    if is_desktop_dev_command "${command}" && { [[ "${command}" == *"${ROOT_DIR}"* ]] || process_cwd_in_root "${pid}"; }; then
      stop_pid "${pid}"
    fi
  done
  # A PID file is only a hint; never signal an unrelated recycled PID from it.
  rm -f "${DESKTOP_PID_FILE}"
}

stop_all() {
  stop_desktop_dev || return $?
  "${ROOT_DIR}/scripts/preview.sh" stop >/dev/null 2>&1 || true
}

start_desktop() {
  command -v pnpm >/dev/null 2>&1 || {
    echo "pnpm is required. Install it before starting Loom." >&2
    exit 1
  }

  stop_all || return $?
  echo "$$" > "${DESKTOP_PID_FILE}"

  echo "Starting Loom desktop app at http://${HOST}:${PORT}..."
  echo "Stop: press Ctrl+C, or run scripts/debug.sh stop from another terminal."

  cd "${ROOT_DIR}"
  if [[ -n "${LOOM_PREVIEW_BINARY:-}" ]]; then
    if [[ "${LOOM_PREVIEW_BINARY}" != "${ROOT_DIR}/src-tauri/target/"*"/Loom.app/Contents/MacOS/loom" || ! -x "${LOOM_PREVIEW_BINARY}" ]]; then
      echo "LOOM_PREVIEW_BINARY must be an executable Loom.app built under this repository's src-tauri/target." >&2
      return 1
    fi
    # A bundled preview must not accidentally depend on the repository cwd.
    cd "$(dirname "${LOOM_PREVIEW_BINARY}")"
    if [[ -n "${LOOM_PREVIEW_SANDBOX_PROFILE:-}" ]]; then
      if [[ "${LOOM_PREVIEW_SANDBOX_PROFILE}" != /* || ! -f "${LOOM_PREVIEW_SANDBOX_PROFILE}" || "$(uname -s)" != Darwin ]]; then
        echo "LOOM_PREVIEW_SANDBOX_PROFILE requires an absolute profile file on macOS." >&2
        return 1
      fi
      exec /usr/bin/sandbox-exec -f "${LOOM_PREVIEW_SANDBOX_PROFILE}" "${LOOM_PREVIEW_BINARY}"
    fi
    exec "${LOOM_PREVIEW_BINARY}"
  fi
  if [[ "${LOOM_PREVIEW_TARGET:-loom}" == "browser-probe" ]]; then
    "${ROOT_DIR}/scripts/preview.sh" start
    exec node "${ROOT_DIR}/scripts/browser-probe.mjs"
  fi
  if [[ "${LOOM_PREVIEW_TARGET:-loom}" == "craft-reference" ]]; then
    "${ROOT_DIR}/scripts/preview.sh" start
    exec node "${ROOT_DIR}/scripts/craft-reference.mjs"
  fi
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

main() {
case "${1:-desktop}" in
  desktop)
    start_desktop
    ;;
  web)
    stop_desktop_dev || return $?
    exec "${ROOT_DIR}/scripts/preview.sh" start
    ;;
  stop)
    stop_all || return $?
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
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
