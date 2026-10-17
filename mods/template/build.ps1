# Builds the mod and wraps it as a WebAssembly component.
# Needs: rustup target add wasm32-unknown-unknown, and wasm-tools.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
wasm-tools component new target/wasm32-unknown-unknown/release/rotate_page.wasm -o rotate_page.wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
wasm-tools validate rotate_page.wasm
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output 'Built rotate_page.wasm. Its imports:'
wasm-tools component wit rotate_page.wasm | Select-String '^\s*import'
