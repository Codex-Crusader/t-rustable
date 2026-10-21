# Builds the mod and writes the .tmod package into this folder.
# Needs: rustup target add wasm32-unknown-unknown, and the t-rustable command.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
t-rustable mods pack .
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
