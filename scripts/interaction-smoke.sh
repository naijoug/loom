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
  if curl -sS -o /dev/null "${BASE_URL}?screen=board" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

LOOM_INTERACTION_BASE_URL="${BASE_URL}" node "${ROOT_DIR}/scripts/interaction-smoke.mjs"
