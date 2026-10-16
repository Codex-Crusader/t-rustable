#!/usr/bin/env bash
# Builds the guest two ways and writes the components to spikes/out/.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/../out"; mkdir -p "$OUT"
cd "$HERE/guest"
cargo build --release --target wasm32-unknown-unknown -q
wasm-tools component new "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/spike_guest.wasm" -o "$OUT/guest-unknown.wasm"
cargo build --release --target wasm32-wasip2 -q
cp "${CARGO_TARGET_DIR:-target}/wasm32-wasip2/release/spike_guest.wasm" "$OUT/guest-wasip2.wasm"
ls -l "$OUT"/guest-*.wasm
