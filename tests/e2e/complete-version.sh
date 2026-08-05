#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "${ROOT_DIR}"

pnpm check
pnpm smoke
pnpm smoke:interaction
pnpm smoke:visual

if [[ "${LOOM_E2E_RELEASE:-0}" == "1" ]]; then
  pnpm tauri build
fi
