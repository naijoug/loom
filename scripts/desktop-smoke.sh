#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY="${LOOM_DESKTOP_BINARY:-${ROOT_DIR}/src-tauri/target/release/bundle/macos/Loom.app/Contents/MacOS/loom}"
ARTIFACT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/loom-desktop-smoke.XXXXXX")"
ACTIVE_PID=""

cleanup() {
  if [[ -n "${ACTIVE_PID}" ]] && kill -0 "${ACTIVE_PID}" 2>/dev/null; then
    kill "${ACTIVE_PID}" 2>/dev/null || true
    wait "${ACTIVE_PID}" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Desktop smoke currently requires the macOS app bundle." >&2
  exit 2
fi

if [[ ! -x "${BINARY}" ]]; then
  echo "Release desktop binary not found: ${BINARY}" >&2
  echo "Build it first with: pnpm tauri build" >&2
  exit 1
fi

"${ROOT_DIR}/scripts/start-local.sh" stop >/dev/null 2>&1 || true

run_cycle() {
  local cycle="$1"
  local log_file="${ARTIFACT_DIR}/cycle-${cycle}.log"

  "${BINARY}" >"${log_file}" 2>&1 &
  ACTIVE_PID="$!"

  for _ in {1..20}; do
    if ! kill -0 "${ACTIVE_PID}" 2>/dev/null; then
      echo "FAIL desktop cycle ${cycle}: process exited during startup" >&2
      sed -n '1,160p' "${log_file}" >&2
      exit 1
    fi
    sleep 0.2
  done

  if rg -i "panic|fatal error|segmentation fault" "${log_file}" >/dev/null 2>&1; then
    echo "FAIL desktop cycle ${cycle}: fatal output detected" >&2
    sed -n '1,160p' "${log_file}" >&2
    exit 1
  fi

  kill "${ACTIVE_PID}" 2>/dev/null || true
  wait "${ACTIVE_PID}" 2>/dev/null || true
  ACTIVE_PID=""
  echo "OK   desktop cycle ${cycle}"
}

run_cycle 1
run_cycle 2

echo "Desktop launch/restart smoke passed"
echo "Logs: ${ARTIFACT_DIR}"
