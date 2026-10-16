# Milestone 1: risk spike report

Status: in progress. Spikes are in `spikes/`. Run `bash spikes/fetch-deps.sh` first.

## Test setup

| Item | Value |
|---|---|
| Machine | Laptop, AMD Ryzen 7 7840HS (8 cores, 16 threads), 15 GB RAM, Radeon 780M and GeForce RTX 4060 Laptop GPU |
| Windows | Windows 11 Home, Rust 1.99.1 `x86_64-pc-windows-msvc`, WebView2 155.0.4283.45 |
| Linux | WSL2 Ubuntu 26.04 with WSLg on the same laptop, Rust 1.99.1, WebKitGTK 2.52.6 |
| PDFium | `bblanchon/pdfium-binaries` `chromium/8100`, non-V8 build. `args.gn` has `pdf_enable_v8 = false` and `pdf_enable_xfa = false`. |
| Binding | `pdfium-render` 0.9.4, plus `libloading` 0.8 for raw functions |

Linux numbers come from WSL2. They show that the code works on Linux, but they are not desktop Linux performance numbers. To measure on a real Linux install, run the commands in each section there.

## Spike 4: PDFium edit APIs

Run: `cargo run --release -p spike-pdfium-edit -- edit [third-party.pdf]`, then `-- heavy`, then `-- progressive spikes/out/heavy.pdf [page]`.

| Check | Windows | Linux (WSL2) | How |
|---|---|---|---|
| Add a text object with an embedded OFL font (Noto Sans) | Works | Works | `pdfium-render`. The font is embedded. Text extracts correctly after save and reopen. |
| Add non-Latin text (`Grüße ж € ‰`) with a simple TrueType font | Does not work | Does not work | Extracts as `Grÿße ÿ ÿ ‰`. Characters outside one byte are lost. |
| Add non-Latin text with a CID TrueType font | Works | Works | `load_true_type_from_bytes(bytes, true)`. Extracts exactly. |
| Remove a text object (our own file) | Works | Works | `remove_object_at_index`, then `regenerate_content`. The text is gone after save and reopen. |
| Remove a text object (a third-party PDF, 40 objects on page 1) | Works | Works | Same calls. Only the removed string is gone from the page text. |
| Glyph coverage check | Works with `ttf-parser` | Same | PDFium has no API for this. `ttf-parser` (MIT OR Apache-2.0) reads the font cmap: Latin, Cyrillic, and the euro sign are present; CJK and emoji are not. |
| Character positions | Works | Works | `chars()` and `tight_bounds()`. All 31 glyph boxes on the test page contain ink in the render at 200 percent. |
| Progressive render with a pause callback | Works through the raw API | Same | `FPDF_RenderPageBitmap_Start`, `FPDF_RenderPage_Continue`, and `IFSDK_PAUSE` through `libloading`. `pdfium-render` 0.9 does not expose them. |
| Cancel in the middle of a render | Works | Works | `FPDF_RenderPage_Close` after the first pause. Took less than 0.01 ms. The next full render of the page is correct. |
| No V8 and no XFA in the loaded build | Confirmed | Confirmed | `args.gn` of the build. |

Progressive render details (render at 200 percent, pause requested at every check):

| Page | Platform | Full render | Pause checks | Longest step (worst cancel delay) |
|---|---|---|---|---|
| 20,000 vector rectangles | Windows | 6.4 ms | 201 | 0.1 ms |
| 20,000 vector rectangles | Linux | 3.0 ms | 201 | less than 0.1 ms |
| 5,000 text objects | Windows | 12.3 ms | 51 | 0.6 ms |
| 5,000 text objects | Linux | 9.0 ms | 51 | 0.2 ms |
| Page 1 of a 60-page book PDF | Windows | 9.5 ms | 1 | 9.5 ms |

PDFium checks the pause callback about once per 100 page objects. A page with few objects renders in one step, and one large object (for example a big image) cannot be interrupted.

### Findings for later milestones

1. `AddText` and `ReplaceText` must use CID fonts. A simple TrueType font loses every character outside one byte.
2. `pdfium-render` 0.9 hides the raw FPDF API (the accessor is crate-private). The engine needs raw calls for progressive render and probably for other gaps. Options for M3: use `pdfium-render` for most calls and a small raw layer for the gaps, or write our own thin binding. Both work, because the raw layer can load the same library. This does not change ADR 0001.
3. Cancel of a render is fast on pages with many objects. On simple pages, a render is short anyway (under 10 ms here). The cancel design in ARCHITECTURE section 2 holds.
4. Glyph coverage needs `ttf-parser` (or a similar crate) next to PDFium.

## Spike 3: WASM component host

Run: `bash spikes/wasm-host/build-guest.sh` (needs `wasm-tools` and the `wasm32-unknown-unknown` and `wasm32-wasip2` targets), then `cargo run --release -p spike-wasm-host`.

Setup: wasmtime 49.0.3, guest built with `wit-bindgen` 0.63 and `wasm-tools` 1.262.0. Memory limit 64 MB through `StoreLimits`. Epoch interruption with one tick per millisecond and a deadline of 100 ticks per call.

| Check | Windows | Linux (WSL2) | Detail |
|---|---|---|---|
| Read the imports before instantiation | Works | Works | `Component::component_type().imports()`. The `wasm32-unknown-unknown` guest imports only `spike:api/host@0.1.0`. |
| Refuse a guest with WASI imports | Works | Works | The same guest built for `wasm32-wasip2` imports 14 `wasi:*` interfaces (`io`, `clocks`, `cli` stdin, stdout, environment, exit, terminal). Instantiation fails with "a matching implementation was not found in the linker". |
| Normal call, host import, and `result` errors | Works | Works | `run(0)` returns `Ok(50)` and logs through the host. `run(MAX)` returns the guest error. |
| Call cost | 0.57 µs | 0.50 µs | One guest call that also calls one host import. Average of 100,000 calls. |
| Compile and instantiate a 16 KB component | 8.7 ms and 0.14 ms | 8.0 ms and 0.09 ms | |
| Memory limit stops a guest that asks for 128 MB | Works | Works | 16 MB works. 128 MB traps as `unreachable`, because the Rust allocator aborts when growth fails. |
| Epoch interruption stops an endless loop | 153 ms | 108 ms | Trap `interrupt`. On Windows, a 1 ms sleep lasts about 1.5 ms, so 100 ticks take longer. |
| The host survives a trap | Works | Works | After a trap, the instance is not used again. A fresh store and instance work at once. |
| Three failures in a row disable the mod | Works | Works | A counter in the host wrapper. |

Dependency check: `cargo deny --config ../deny.toml check licenses bans sources` on the spikes workspace (wasmtime, pdfium-render, ttf-parser, libloading). All third-party crates pass the license list. The ban list finds no HTTP client. The only license errors are the two spike crates themselves, which are our own PolyForm code.

### Findings for later milestones

1. Mods must be built for `wasm32-unknown-unknown` and wrapped with `wasm-tools component new`. A `wasm32-wasip2` build imports WASI interfaces, and the host refuses it. The mod template and the 15-minute guide (M2) must use this build. The CLI command `mods new` (M6) can run both steps. This supports ADR 0007.
2. Install-time import checks work as ADR 0007 describes. The host can show the exact import list on the permission screen.
3. A trapped instance must be thrown away. The host keeps no mod state across a trap, so per-mod settings must live in the host (`host` interface), not in guest memory. This matches the planned API.
4. A memory-limit failure looks the same as a panic. To tell the user "out of memory", M3 can set `trap_on_grow_failure(true)` in `StoreLimits`, or count `StoreLimits` denials.
5. The epoch deadline must count real time, not ticks, because tick length depends on the platform timer.
6. The call cost of about 0.5 µs is low enough for a pointer event on each mouse move.
