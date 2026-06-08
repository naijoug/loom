#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST="${LOOM_PREVIEW_HOST:-127.0.0.1}"
PORT="${LOOM_PREVIEW_PORT:-1420}"
BASE_URL="http://${HOST}:${PORT}/preview/planning"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
OUT_DIR="${LOOM_VISUAL_SMOKE_DIR:-/tmp/loom-visual-smoke}"

if [[ ! -x "${CHROME}" ]]; then
  echo "Chrome executable not found: ${CHROME}" >&2
  exit 1
fi

cleanup() {
  "${ROOT_DIR}/scripts/preview.sh" stop >/dev/null
}

trap cleanup EXIT

rm -rf "${OUT_DIR}"
mkdir -p "${OUT_DIR}"

"${ROOT_DIR}/scripts/preview.sh" restart >/dev/null

for _ in {1..60}; do
  if curl -sS -o /dev/null "${BASE_URL}?screen=planning" 2>/dev/null; then
    break
  fi
  sleep 0.2
done

capture() {
  local label="$1"
  local url="$2"
  local expected="$3"
  local dom_file="${OUT_DIR}/${label}.html"
  local png_file="${OUT_DIR}/${label}.png"
  local chrome_log="${OUT_DIR}/chrome.log"
  local size

  "${CHROME}" --headless=new --disable-gpu --dump-dom "${url}" > "${dom_file}" 2>>"${chrome_log}"
  if ! rg -q "${expected}" "${dom_file}"; then
    echo "FAIL ${label}: missing DOM text /${expected}/" >&2
    exit 1
  fi

  "${CHROME}" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
    --window-size=1440,940 --screenshot="${png_file}" "${url}" >/dev/null 2>>"${chrome_log}"
  size="$(wc -c < "${png_file}")"
  if (( size < 50000 )); then
    echo "FAIL ${label}: screenshot too small (${size} bytes)" >&2
    exit 1
  fi

  echo "OK   ${label} -> ${png_file}"
}

capture_theme() {
  local theme="$1"
  local prefix=""
  if [[ "${theme}" == "light" ]]; then
    prefix="light-"
  fi

  capture "${prefix}planning" "${BASE_URL}?screen=planning&theme=${theme}" "Planning room|Consensus|Create tasks from plan"
  capture "${prefix}board" "${BASE_URL}?screen=board&theme=${theme}" "Project board|In Progress|Testing|Done"
  capture "${prefix}board-speaker" "${BASE_URL}?screen=board-speaker&theme=${theme}" "Children Three Kingdoms|TTS narration pipeline|Voice preset"
  capture "${prefix}new-task" "${BASE_URL}?screen=new-task&theme=${theme}" "New task|Invite agents to discuss|Suggested primary|Start discussion"
  capture "${prefix}add-project" "${BASE_URL}?screen=add-project&theme=${theme}" "Add project|Open a local directory|Detected"
  capture "${prefix}session" "${BASE_URL}?screen=session&theme=${theme}" "Claude Code|Subtasks|Mark ready for testing"
  capture "${prefix}testing" "${BASE_URL}?screen=testing&theme=${theme}" "Debug agent|Frontend|Backend|Detected error"
  capture "${prefix}done" "${BASE_URL}?screen=done&theme=${theme}" "Completed items|Verification evidence|Start follow-up"

  capture "${prefix}settings-general" "${BASE_URL}?screen=settings&tab=general&theme=${theme}" "Workspace|Startup|Privacy"
  capture "${prefix}settings-appearance" "${BASE_URL}?screen=settings&tab=appearance&theme=${theme}" "Theme|Accent color|Typography"
  capture "${prefix}settings-agents" "${BASE_URL}?screen=settings&tab=agents&theme=${theme}" "Installed agents|Add custom Agent"
  capture "${prefix}settings-safety" "${BASE_URL}?screen=settings&tab=safety&theme=${theme}" "Command presets|High-risk action policy"
  capture "${prefix}settings-notifications" "${BASE_URL}?screen=settings&tab=notifications&theme=${theme}" "Notify me when|Delivery"
  capture "${prefix}settings-about" "${BASE_URL}?screen=settings&tab=about&theme=${theme}" "Loom|Resources|Update channel"
}

capture_theme "dark"
capture_theme "light"

echo "Visual smoke passed"
echo "Screenshots: ${OUT_DIR}"
