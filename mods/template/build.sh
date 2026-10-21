#!/usr/bin/env bash
# Builds the mod and writes the .tmod package into this folder.
# Needs: rustup target add wasm32-unknown-unknown, and the t-rustable command.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
t-rustable mods pack .
