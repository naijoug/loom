#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

# Production Chat repository/runtime/transport tests, including real local
# fixture subprocesses. No model requests; this is not a real-Agent acceptance.
cargo test --manifest-path src-tauri/Cargo.toml chat -- --nocapture
