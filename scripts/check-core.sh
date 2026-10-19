#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo run --quiet -p loom-core --features bindings --bin export-workbench-types --locked -- --check
cargo run --quiet -p loom-runtime --features bindings --bin export-runtime-types --locked -- --check
pnpm exec tsc -p packages/protocol/tsconfig.json
