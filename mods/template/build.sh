#!/usr/bin/env bash
# Builds the mod and wraps it as a WebAssembly component.
# Needs: rustup target add wasm32-unknown-unknown, and wasm-tools.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
wasm-tools component new target/wasm32-unknown-unknown/release/rotate_page.wasm -o rotate_page.wasm
wasm-tools validate rotate_page.wasm
echo "Built rotate_page.wasm. Its imports:"
wasm-tools component wit rotate_page.wasm | grep -E '^\s*import'
