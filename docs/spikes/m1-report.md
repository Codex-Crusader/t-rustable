# Milestone 1: risk spike report

Status: complete. Spikes are in `spikes/`. Run `bash spikes/fetch-deps.sh` first.

## Summary

The four spikes pass on Windows. On Linux (WSL2), spikes 2, 3, and 4 pass, and spike 1 passes for render speed and tile delivery. Scrolling smoothness on Linux is not measured, because WSLg had no GPU (see spike 1). No result breaks a locked decision.

Decisions for the owner before Milestone 3 (both decided on 2026-10-17):

1. PDFium binding and engine restart (one decision). ADR 0001 says M1 selects the binding.
   - `pdfium-render` 0.9 keeps its raw API and its document and page handles private. A raw call such as `FPDF_RenderPageBitmap_Start` can therefore not reach a document that `pdfium-render` holds. Progressive render and cancel of the edited document need that call.
   - `pdfium-render` binds PDFium once per process, so the engine cannot restart PDFium after a Rust panic.
   - With the raw C API, PDFium can shut down (`FPDF_DestroyLibrary`) and start again in the same process. Tested: 3 rounds on 2 files, Windows and Linux, 2 to 32 ms per cycle.
   - Option A: our own thin binding over the C API (`libloading` and the functions that we use). It gives progressive render, cancel, and a restart after a Rust panic. It costs more code in the engine crate.
   - Option B: `pdfium-render` for everything. No progressive render or cancel for edited documents, and after an engine panic the user must restart the app (the journal recovers the work).
   - Recommendation: option A. Progressive render and cancel are part of the locked engine design (ADR 0001 and ARCHITECTURE section 2).
   - Decided: option A (ADR 0013).
2. Rust mods are built for `wasm32-unknown-unknown` and wrapped with `wasm-tools component new`. A `wasm32-wasip2` build imports WASI, and the host refuses it. This shapes the mod template and the guide in M2. Decided: the M2 template uses this build.

Not measured in M1:

- Scrolling smoothness on a real Linux desktop. Run the spike 1 GUI command on the Arch install.
- Glyph coverage of fonts that are embedded in third-party PDFs. M1 checked only our bundled font. Embedded fonts are often subsets, and reading their data needs the raw API. Moved to the `ReplaceText` milestone.

## Test setup## Test setup

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
| Shut down and start PDFium again in one process | Works through the raw API | Works | `FPDF_DestroyLibrary`, then `FPDF_InitLibrary`, open, render. 3 rounds. Not possible with `pdfium-render`. Run: `-- reinit <file.pdf>`. |

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
2. `pdfium-render` 0.9 hides the raw FPDF API and its handles (crate-private). The `progressive` test ran in a process without `pdfium-render`. A raw layer next to `pdfium-render` cannot reach a document that `pdfium-render` holds. See Summary, decision 1.
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

### Findings for later milestones

1. Mods must be built for `wasm32-unknown-unknown` and wrapped with `wasm-tools component new`. A `wasm32-wasip2` build imports WASI interfaces, and the host refuses it. The mod template and the 15-minute guide (M2) must use this build. The CLI command `mods new` (M6) can run both steps. This supports ADR 0007.
2. Install-time import checks work as ADR 0007 describes. The host can show the exact import list on the permission screen.
3. A trapped instance must be thrown away. The host keeps no mod state across a trap, so per-mod settings must live in the host (`host` interface), not in guest memory. This matches the planned API.
4. A memory-limit failure looks the same as a panic. To tell the user "out of memory", M3 can set `trap_on_grow_failure(true)` in `StoreLimits`, or count `StoreLimits` denials.
5. The epoch deadline must count real time, not ticks, because tick length depends on the platform timer.
6. The call cost of about 0.5 µs is low enough for a pointer event on each mouse move.

## Dependency check (all four spikes)

Run in `spikes/`: `cargo deny --config ../deny.toml check licenses bans sources`. The tree contains Tauri 2.12.4, wasmtime 49.0.3, pdfium-render 0.9.4, png, ttf-parser, and libloading.

- Licenses: every third-party crate passes the allow list. The only rejections are the four spike crates, which are our own PolyForm code.
- Bans: the first run found `reqwest`, `hyper`, and `hyper-util`. `tauri` 2.12.4 depends on them only for Android and iOS (`Cargo.toml`: `cfg(any(target_os = "android", ...))`), for its mobile dev server. `cargo tree --target` shows none of them for `x86_64` or `aarch64` Linux and Windows. `deny.toml` now checks only the shipped targets (`[graph] targets`). With that, the ban list passes. ADR 0011 records this.

## Spike 1: rendering speed

Run: `cargo run --release -p spike-render-tiles -- bench [extra.pdf]` for numbers without a window, and `cargo run --release -p spike-render-tiles` for the GUI test. The GUI opens a Tauri window, loads 512 px tiles through the `trtile` URI scheme, scrolls by itself at 3,000 CSS px per second for 6 seconds, writes `spikes/out/render-gui-<os>.json`, and closes.

Setup: one engine thread owns PDFium and renders each tile into a tile-sized bitmap (`set_fixed_size` plus `set_origin`). PNG encoding runs outside the engine thread, with the `png` crate. 100 percent zoom is 96 dpi.

Test files: a generated 50-page A4 sample (text, 60 vector circles, and one image per page, 15 MB because the images are not compressed), and a local third-party book PDF (409 pages, 3.7 MB, first 50 pages measured, not in the repository).

### Bench (no window)

| Measure | Windows, sample | Windows, book | Linux, sample | Linux, book | Target |
|---|---|---|---|---|---|
| Open the file | 10.2 ms | 2.7 ms | 8.7 ms | 5.6 ms | |
| First page: open, 6 tiles, PNG | 20.9 ms | 14.6 ms | 21.7 ms | 17.8 ms | under 1 s |
| Page render at 100 percent, mean / p95 / max | 4.9 / 6.5 / 6.6 ms | 4.0 / 6.4 / 7.4 ms | 4.9 / 6.1 / 7.1 ms | 2.6 / 4.9 / 6.7 ms | under 100 ms |
| Page render at 200 percent, mean / p95 / max | 12.5 / 14.1 / 14.3 ms | 9.6 / 14.1 / 18.3 ms | 11.4 / 13.1 / 13.6 ms | 5.4 / 9.8 / 13.7 ms | |
| PNG encode per tile, Fast, mean / p95 | 0.5 / 1.2 ms | 0.4 / 1.1 ms | 0.4 / 0.9 ms | 0.3 / 0.7 ms | |
| PNG size per tile, Fast, mean | 64 KB | 28 KB | 64 KB | 28 KB | |
| PNG encode per tile, Balanced, mean / p95 | 1.7 / 4.1 ms | 2.7 / 10.9 ms | 1.6 / 3.9 ms | 2.6 / 10.4 ms | |
| PNG size per tile, Balanced, mean | 10 KB | 19 KB | 10 KB | 19 KB | |

### GUI (generated sample)

| Measure | Windows (WebView2 155) | Linux (WSLg, WebKitGTK 2.52) |
|---|---|---|
| URL form that loads | `http://trtile.localhost/...` only | `trtile://localhost/...` only |
| Device pixel ratio | 1.25 | 1 |
| First tile visible, from page start | 89 ms | 207 ms |
| First page complete, from page start | 103 ms | 211 ms |
| Tile latency in the page (request to `load`), mean / p95 | 12.9 / 21.9 ms | 11.2 / 14.0 ms |
| Frame time while scrolling, mean / p95 / max | 6.1 / 6.2 / 6.5 ms (165 Hz screen) | 47.8 / 58 / 77 ms |
| Frames over 33 ms | 0 of 991 | 124 of 126 |

The page times include the page load, one `invoke` call, and two probe requests for the URL forms.

On Linux, WSLg could not start a GPU driver (`EGL` and `ZINK` errors in the log), so WebKitGTK drew in software. The Linux frame times show this WSL setup, not desktop Linux. Tile delivery on Linux is as fast as on Windows. To measure scrolling on a real Linux desktop, run `bash spikes/fetch-deps.sh` and the GUI command above there.

### Findings for later milestones

1. Both targets are met with a large margin: first page about 20 ms in the bench on both platforms and about 100 ms in the Windows window, page render 3 to 14 ms.
2. PNG is fast enough. Use `Fast` compression for tiles: it is 3 to 6 times faster than `Balanced`, and the tiles never leave the computer, so the larger size does not matter.
3. The tile URL form is different per platform: `http://trtile.localhost/` on Windows and `trtile://localhost/` on Linux. The UI must build URLs with the same rule as Tauri's `convertFileSrc`, and the CSP `img-src` must list both forms. ARCHITECTURE section 11 now records this.
4. `pdfium-render` binds PDFium once per process (`PdfiumLibraryBindingsAlreadyInitialized` on a second bind). The raw C API can restart PDFium in the same process. See Summary, decision 1.
5. A tile render that uses `set_origin` disables form field rendering in PDFium. Forms come in a later milestone, so this needs a decision then: flatten form fields for display, or render form fields in a separate pass.

## Spike 2: mod panel isolation

Run: `cargo run --release -p spike-iframe-isolation -- locked` and `-- ipc-allowed`. Results go to `spikes/out/iframe-<os>-<mode>.json`.

Setup:

- Host page CSP (`locked`): `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'none'; frame-src trpanel: http://trpanel.localhost; object-src 'none'; base-uri 'none'; form-action 'none'`. The `ipc-allowed` mode changes only `connect-src` to `ipc: http://ipc.localhost`.
- One capability with `core:default`, for the main window only. `withGlobalTauri` is off.
- The app has a command `secret`. The Rust side counts every call. Any call from a panel is a leak, whatever the page reports.
- Panels are served by the custom scheme `trpanel` with this CSP header: `default-src 'none'; script-src trpanel: http://trpanel.localhost; style-src (same); img-src (same) data:; connect-src 'none'; form-action 'none'`.
- The panel script tries 20 escapes: the Tauri globals in its own window, in `parent`, and in `top`; `invoke('secret')` and `invoke('ping')`; direct IPC requests to `ipc://localhost` and `http://ipc.localhost`; `window.ipc.postMessage` and `chrome.webview.postMessage`; the parent DOM; `localStorage` and cookies; reading app files; an external fetch; `window.open`; navigation of the top window; a form submit; and a forged host message.

### Result for the design (panel from a scheme URL, `sandbox="allow-scripts"`, panel CSP)

| Attack | Windows | Linux (WSL2) |
|---|---|---|
| Calls to `secret` counted in Rust | 0 | 0 |
| `invoke` from the panel (`secret` and `ping`) | No answer. Tauri drops IPC from the frame. | No Tauri globals in the frame. |
| Direct fetch to the IPC origins | Blocked | Blocked |
| `parent` and `top` Tauri globals, parent DOM | Blocked (cross-origin) | Blocked |
| `localStorage`, cookies | Blocked (sandbox) | Blocked |
| App files, external fetch | Blocked | Blocked |
| `window.open`, top navigation, form submit | Blocked (sandbox) | Blocked |
| Forged host message | Arrives with origin `null`, and `event.source` identifies the panel frame | Same |

Same result in both CSP modes and in 5 repeated Windows runs.

On Windows, Tauri injects its bridge script into every frame, so the panel sees `__TAURI_INTERNALS__`, `window.ipc`, and `chrome.webview`. It can send messages on them without an error, but Tauri never runs them: `ping` gets no answer and the `secret` counter stays at 0. On Linux, the panel has none of these objects.

### Which setting blocks which attack

| Variant | What leaked |
|---|---|
| srcdoc, `sandbox="allow-scripts"` | Nothing. (In the first of 6 Windows runs, its script did not run. This did not repeat.) |
| Scheme URL, `sandbox="allow-scripts"`, no panel CSP | Nothing in this test. The sandbox and the cross-origin URL already block these attacks. Keep the panel CSP as a second layer. |
| CONTROL: scheme URL, no `sandbox` attribute | Top navigation of the host page, form submit, `localStorage`. Still no IPC, because the frame is cross-origin. |
| CONTROL: srcdoc with `allow-scripts allow-same-origin` | Full escape: the parent DOM, and `parent.__TAURI_INTERNALS__.invoke('secret')` works (2 calls counted). |

### `invoke` under `connect-src 'none'`

`invoke` works. The CSP blocks Tauri's fetch to the IPC origin (one `securitypolicyviolation` for `http://ipc.localhost/ping` on Windows and `ipc://localhost/ping` on Linux), and Tauri falls back to its postMessage channel.

| | Windows | Linux (WSL2) |
|---|---|---|
| Mean `invoke` time, `locked` (fallback) | 0.18 ms | 0.28 ms |
| Mean `invoke` time, `ipc-allowed` (fetch) | 0.95 ms | 0.42 ms |

The fallback is not slower, so the locked CSP can stay. ADR 0011 does not change.

### Findings for later milestones

1. The design in the mod system rules holds: a panel loaded from a custom scheme URL with `sandbox="allow-scripts"` cannot reach Tauri IPC or the host page on either platform.
2. Never add `allow-same-origin`, and never serve a panel from the app origin. Together they give a full escape.
3. The `sandbox` attribute is required. Without it, a panel can navigate the host window.
4. The panel CSP must list the panel scheme in `script-src`. `'self'` does not match the custom scheme in an opaque-origin document on WebKitGTK, and the panel script does not run.
5. The host must accept panel messages only when `event.source` is that panel's `contentWindow`. The origin is always `null`, so it cannot identify the panel.
6. On Windows, the Tauri bridge objects exist inside panel frames. They do nothing today, because Tauri checks where IPC comes from. Add a regression test in M4 that keeps the `secret` counter check, so that a Tauri update cannot change this without notice.
