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

  "${CHROME}" --headless=new --disable-gpu --virtual-time-budget=2500 --dump-dom "${url}" > "${dom_file}" 2>>"${chrome_log}"
  if ! rg -q "${expected}" "${dom_file}"; then
    echo "FAIL ${label}: missing DOM text /${expected}/" >&2
    exit 1
  fi

  "${CHROME}" --headless=new --disable-gpu --virtual-time-budget=2500 --hide-scrollbars --force-device-scale-factor=2 \
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

  capture "${prefix}planning" "${BASE_URL}?screen=planning&step=review&theme=${theme}" "规划讨论室|并行起草|确认计划并生成任务"
  capture "${prefix}planning-running" "${BASE_URL}?screen=planning&step=running&theme=${theme}" "讨论进行中|Drafting milestones|排队中"
  capture "${prefix}planning-empty" "${BASE_URL}?screen=planning&step=empty&theme=${theme}" "先让多个 Agent 把方案谈清楚|并行起草|最终计划"
  capture "${prefix}board" "${BASE_URL}?screen=board&theme=${theme}" "任务看板|进行中|测试中|已完成"
  capture "${prefix}board-speaker" "${BASE_URL}?screen=board-speaker&theme=${theme}" "Children Three Kingdoms|TTS narration pipeline|Voice preset"
  capture "${prefix}new-task" "${BASE_URL}?screen=new-task&theme=${theme}" "New task|Invite agents to discuss|Suggested primary|Start discussion"
  capture "${prefix}add-project" "${BASE_URL}?screen=add-project&theme=${theme}" "Add project|Open a local directory|Detected"
  capture "${prefix}session" "${BASE_URL}?screen=session&theme=${theme}" "Claude Code|子任务|标记为可测试"
  capture "${prefix}testing" "${BASE_URL}?screen=testing&theme=${theme}" "调试验收|验收门禁|添加截图或文件"
  capture "${prefix}done" "${BASE_URL}?screen=done&theme=${theme}" "完成的需求|验证证据|开始后续任务"

  capture "${prefix}settings-general" "${BASE_URL}?screen=settings&tab=general&theme=${theme}" "Current project|Runtime model|Project stacks"
  capture "${prefix}settings-appearance" "${BASE_URL}?screen=settings&tab=appearance&theme=${theme}" "Theme|Effective theme|Typography and density"
  capture "${prefix}settings-agents" "${BASE_URL}?screen=settings&tab=agents&theme=${theme}" "Installed agents|Add custom Agent"
  capture "${prefix}settings-safety" "${BASE_URL}?screen=settings&tab=safety&theme=${theme}" "Project terminal slots|Execution guards|Redact secrets"
  capture "${prefix}settings-about" "${BASE_URL}?screen=settings&tab=about&theme=${theme}" "Loom|Backend health|Resources"
}

capture_theme "dark"
capture_theme "light"

node "${ROOT_DIR}/scripts/visual-diff.mjs" --check "${OUT_DIR}"

echo "Visual smoke passed"
echo "Screenshots: ${OUT_DIR}"
