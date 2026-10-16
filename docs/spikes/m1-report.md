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
