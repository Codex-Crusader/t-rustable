# Milestone 3: spike report

Status: complete. These spikes ran before the M3 code. They are in `spikes/`. Run `bash spikes/fetch-deps.sh` first.

## Summary

| Spike | Result | Decision |
|---|---|---|
| qpdf binding | The `qpdf` crate builds qpdf from source on Windows and Linux. Load, check, and write work. | ADR 0014 |
| Object order after save and reload | Stable when PDFium can write the page again. PDFium cannot write some pages correctly. | ADR 0015 |
| Image objects | Alpha and JPEG work, also after save and reload. | No change |
| Replace a file that is open | Linux: works. Windows: fails if the other program did not allow delete. | Save pipeline error `file-in-use` |

No result changes a locked decision. One result limits `RemoveObject` on some pages (see spike 2).

## Spike 1: qpdf binding

Run: `cargo run --release -p spike-qpdf-save -- <file.pdf>...`

The `qpdf` crate 0.3.7 (MIT OR Apache-2.0) with the `vendored` feature compiles qpdf 12.4.2 and links it statically. The vendored sources are qpdf (Apache-2.0), IJG libjpeg 9f (IJG license), and zlib (Zlib). The crypto provider is the native one inside qpdf. The crate includes prebuilt bindings, so the build does not need libclang.

| Check | Windows | Linux (WSL2) |
|---|---|---|
| Clean build time | 61 s | 36 s |
| Load from memory, `check_pdf`, write with object streams | Pass, 3 files | Pass, 3 files |
| Reload of the output, same page count | Pass | Pass |
| Write time, 50 pages, 15.7 MB | 32 ms | 51 ms |
| Recovery of a file with the last 20 percent cut off | 3 of 4 files. The fourth file lost all page objects, so recovery is not possible. | Same |
| `cargo deny` (licenses, bans, sources) | Pass. Only the spike crate itself needs an exception. | |

The spike checked page counts and `check_pdf` only. It did not compare the content. The M3 save tests compare render and text before and after qpdf.

## Spike 2: object order after save and reload

Run: `cargo run --release -p spike-pdfium-ids -- ids <file.pdf>...`, then `-- verify <file.pdf>...`

The core maps each `ObjectId` to a PDFium object index. An undo checkpoint saves the document and loads it again. The map stays correct only if the object order and kinds stay the same after save and reload.

Test: on each page, remove the middle object, add a rectangle, add a Square annotation, and call `FPDFPage_GenerateContent`. Then compare the live page with the page after a close and reopen, and after save and reload. Files: the 4 spike files and 12 local PDFs of different origins.

| Files | Result |
|---|---|
| 4 spike files, 67 pages | Order and kinds equal. Text bounds move up to 0.8 pt, because PDFium measures the text again. |
| 10 local files, 68 pages | Order and kinds equal on 67 pages. On 1 page, one text object lost its only character. |
| 2 local files with Type3 fonts, 9 pages | All 9 pages lost most of their text. |
| Second save and reload of each result | No change. |

Cause of the Type3 result: when PDFium writes a text object that uses a Type3 font, it writes `q`, the colors, `BT`, and `Tm`, and then stops. It writes no `Tf`, no text, and no `ET`. The text after that point does not show and does not extract.

Adding objects is safe on the same pages. PDFium then adds a new content stream and does not write the old stream again. Text, objects, and pixels outside the new object stay the same (tested on 3 files). Removing an object makes PDFium write the old stream again.

Two ways to check a page after a rewrite were compared. Both found the same pages:

- Per object: the object kinds in order, and the text of each text object (`FPDFTextObj_GetText`).
- Per page: the page text (`FPDFText_GetText`). This text has spaces and line ends that PDFium guesses from glyph positions, so a small shift can change it.

Decision (ADR 0015): after each command that rewrites page content, the engine closes and reopens the page and runs the per-object check. On a difference, the transaction fails with `unsupported-content`, and the core rolls back from bytes. After the reopen, the reopened page is the only state, so a live session, a replay, and a checkpoint reload all hold the same objects.

## Spike 3: image objects

Run: `cargo run --release -p spike-pdfium-ids -- image <file.jpg>`

| Check | Result |
|---|---|
| `FPDFImageObj_SetBitmap` with a BGRA bitmap, half transparent | The transparent half renders white and the opaque half renders blue, live and after save and reload. |
| `FPDFImageObj_LoadJpegFileInline` | Works. After reload, the page has 2 image objects at the correct bounds. |

PNG files must be decoded to BGRA before `SetBitmap`. JPEG files go in as they are.

## Spike 4: replace a file that is open

Run: `cargo run --release -p spike-replace-file`

The last step of the save pipeline renames a temporary file over the target. On Windows:

| Other program opened the target with | Rename |
|---|---|
| Not open | Works |
| Rust `File::open` (read, write, and delete sharing) | Works |
| Read sharing only | Fails: "Access is denied". The target is unchanged. |
| Read and write sharing | Fails: "Access is denied". The target is unchanged. |
| Read, write, and delete sharing | Works |

Thus the save is safe: a failed rename leaves the target as it was. The save retries the rename for about one second, because antivirus and search programs often hold a new file for a short time. Then it returns `file-in-use`. On Linux, a rename replaces the directory entry, so another open file does not stop it.
