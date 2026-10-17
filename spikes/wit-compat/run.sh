#!/usr/bin/env bash
# Evidence for ADR 0012. Builds two old guests and runs them against newer hosts.
set -euo pipefail
cd "$(dirname "$0")"
OUT=../out; mkdir -p "$OUT"
for g in variant semver; do
  (cd "guest-$g" && cargo build -q --release --target wasm32-unknown-unknown)
  wasm-tools component new "guest-$g/target/wasm32-unknown-unknown/release/compat_guest_$g.wasm" -o "$OUT/compat-$g.wasm"
done
echo "Imports of the semver guest (it calls only 'used'):"
wasm-tools component wit "$OUT/compat-semver.wasm" | grep -E '^\s*import'
cargo run -q --release -p spike-wit-compat -- "$OUT/compat-variant.wasm" "$OUT/compat-semver.wasm"
