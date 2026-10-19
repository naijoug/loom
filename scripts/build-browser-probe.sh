#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILDER="${ROOT_DIR}/.ref/repos/cef-rs/target/debug/bundle-cef-app"
[[ -x "${BUILDER}" ]] || { echo 'Build the pinned cef-rs bundle-cef-app utility first (see experiments/browser-host/UPSTREAM.md).' >&2; exit 1; }
[[ -x "${ROOT_DIR}/.ref/toolchain/bin/cmake" && -x "${ROOT_DIR}/.ref/toolchain/bin/ninja" ]] || { echo 'Install CMake and Ninja into .ref/toolchain first.' >&2; exit 1; }
export CEF_PATH="${ROOT_DIR}/.ref/cef-runtime"
export CMAKE="${ROOT_DIR}/.ref/toolchain/bin/cmake"
export PATH="${ROOT_DIR}/.ref/toolchain/bin:${PATH}"
[[ -d "${CEF_PATH}" ]] || { echo 'Download the pinned CEF runtime first.' >&2; exit 1; }
cd "${ROOT_DIR}/experiments/browser-host"
cargo fmt -- --check
cargo clippy --locked --offline -- -D warnings
"${BUILDER}" loom-browser-host -o target/bundle -i com.naijoug.loom.browser-probe -d Loom
