#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
trap 'scripts/debug.sh stop' EXIT
scripts/debug.sh stop
scripts/debug.sh web
node scripts/simple-chat-smoke.mjs
