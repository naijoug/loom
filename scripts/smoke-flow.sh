#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${LOOM_PREVIEW_HOST:-127.0.0.1}"
PORT="${LOOM_PREVIEW_PORT:-1420}"
BASE_URL="http://${HOST}:${PORT}/preview/planning"

cleanup() {
  "${ROOT_DIR}/scripts/preview.sh" stop >/dev/null
}

trap cleanup EXIT

"${ROOT_DIR}/scripts/preview.sh" restart >/dev/null

for _ in {1..60}; do
  if curl -sS -o /dev/null "${BASE_URL}?screen=planning" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

check_url() {
  local label="$1"
  local url="$2"
  local status

  status="$(curl -sS -o /dev/null -w "%{http_code}" "${url}")"
  if [[ "${status}" != "200" ]]; then
    echo "FAIL ${label}: HTTP ${status} ${url}" >&2
    exit 1
  fi

  echo "OK   ${label}"
}

for screen in planning board board-speaker new-task add-project session testing done; do
  check_url "screen:${screen}" "${BASE_URL}?screen=${screen}"
done

for tab in general appearance agents safety notifications about; do
  check_url "settings:${tab}" "${BASE_URL}?screen=settings&tab=${tab}"
done

echo "Smoke flow passed"
