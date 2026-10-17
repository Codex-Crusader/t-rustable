# 0013. PDFium binding: our own thin binding over the C API

Status: Accepted (2026-10-17). Completes ADR 0001, which left the binding choice to Milestone 1.

## Context

Milestone 1 tested `pdfium-render` 0.9.4 (`docs/spikes/m1-report.md`):

- It keeps the raw FPDF API and its document and page handles private. A raw call such as `FPDF_RenderPageBitmap_Start` cannot reach a document that `pdfium-render` holds. Progressive render and cancel of an edited document need that call.
- It binds PDFium once per process. After a Rust panic in the engine, it cannot restart PDFium.
- The raw C API can shut down PDFium (`FPDF_DestroyLibrary`) and start it again in the same process. This was tested on Windows and Linux.

## Decision

`t-rustable-engine` calls the PDFium C API through its own thin binding: `libloading` and declarations for the functions that the engine uses. The binding loads the pinned non-V8 PDFium build (ADR 0001).

## Consequences

- Progressive render with a pause callback, and cancel, work on edited documents (ARCHITECTURE section 2).
- After a Rust panic, the engine restarts PDFium in the same process, and the core reloads all open sessions from their journals.
- The engine crate has more code: each PDFium function needs a declaration and a safe wrapper. Unsafe code stays in one module of the engine crate, with tests.
- `pdfium-render` stays useful as a reference for function signatures. Its license (MIT OR Apache-2.0) permits copying small parts with attribution, if needed.
