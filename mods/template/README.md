# Mod template

A minimal T-Rustable mod in Rust. It adds one action, "Rotate page", that rotates the current page by 90 degrees.

License: MIT. To start a new mod from this template, run `t-rustable mods new "My mod"`. The guide is `docs/mod-api/first-mod-in-15-minutes.md`.

## Build

1. Install the target: `rustup target add wasm32-unknown-unknown`
2. Run `./build.sh` (Linux) or `./build.ps1` (Windows).

The script runs `cargo build --release` and then `t-rustable mods pack .`. The result is a package such as `com.example.rotate-page-0.1.0.tmod` in this folder. Install it in the app with Mods, Install mod.

Inside the T-Rustable repository, without an installed `t-rustable` command, run `cargo run -p t-rustable-cli -- mods pack <this folder>` instead.
