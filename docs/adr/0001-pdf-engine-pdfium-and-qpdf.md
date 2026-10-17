# 0001. PDF engine: PDFium and qpdf

Status: Accepted (2026-10-16)

## Context

The app must render PDF pages fast, edit page content, and write correct files. All dependencies must have a permissive license (ADR 0010).

Options:

- MuPDF: fast and complete, but AGPL. Not permitted.
- Pure Rust libraries (for example `lopdf`): permissive and good for structure, but no renderer.
- PDFium: the PDF engine of Chromium. BSD-style license. It renders well and has an edit API for page objects.
- qpdf: Apache-2.0. Strong for structure, repair, encryption, and linearization. No renderer.

## Decision

- PDFium does rendering, text extraction, and page object edits.
- qpdf does structure checks, repair, encryption, and the save post-process.
- Only `t-rustable-engine` calls them, from Rust.
- One engine actor thread owns all PDFium calls, because PDFium is not thread-safe. Other threads send requests with a request ID, a priority, and a cancel flag (`docs/ARCHITECTURE.md`, section 2).

## Consequences

- The app ships PDFium and qpdf binaries for Linux and Windows. Their licenses, and the licenses of the libraries inside them (for example FreeType and libjpeg-turbo), must go into `THIRD_PARTY_LICENSES`. `cargo-about` does not see them, so this needs a manual list.
- PDFium binding: our own thin binding over the C API (ADR 0013, after the M1 spikes). qpdf binding: the `qpdf` crate, built from source (ADR 0014).
- Milestone 1 must verify the PDFium edit APIs: remove and add text objects, embedded fonts, character positions, and progressive render with a pause callback.
- Use a PDFium build without V8 (JavaScript) and without XFA. JavaScript in a PDF must never run. Prebuilt PDFium is often available with and without V8, so Milestone 1 must check which build the selected binding loads.
- A native PDFium crash stops the app process in v1. A later milestone moves the engine to a child process.
