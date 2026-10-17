# Mod template

A minimal T-Rustable mod in Rust. It adds one action, "Rotate page", that rotates the current page by 90 degrees.

License: MIT. Copy this folder to start a new mod. The guide is `docs/mod-api/first-mod-in-15-minutes.md`.

## Build

1. Install the target: `rustup target add wasm32-unknown-unknown`
2. Install `wasm-tools`: https://github.com/bytecodealliance/wasm-tools/releases
3. Run `./build.sh` (Linux) or `./build.ps1` (Windows).

The result is `rotate_page.wasm`. The `.tmod` package format comes in Milestone 5.
