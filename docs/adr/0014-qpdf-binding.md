# 0014. qpdf binding: the `qpdf` crate, built from source

Status: Accepted (2026-10-17). Completes ADR 0001, which left the qpdf binding to Milestone 3.

## Context

The save pipeline needs qpdf after PDFium: structure check, repair, and the final write (ARCHITECTURE section 9). qpdf is a C++ library under Apache-2.0. Options:

- The `qpdf` crate (MIT OR Apache-2.0). Its `vendored` feature compiles qpdf, zlib, and libjpeg from source and links them statically.
- Ship the `qpdf` program and call it through pipes. The qpdf project ships no Windows build for MSVC, only MinGW.
- Our own binding to the qpdf C API. More code, and we must still build or ship qpdf.

The M3 spike (`docs/spikes/m3-report.md`, spike 1) built the crate on Windows MSVC and on Linux. It loaded, checked, and wrote files, and `cargo deny` passed.

## Decision

`t-rustable-engine` uses the `qpdf` crate with the `vendored` feature. qpdf is linked into the app. qpdf has no shared state with PDFium, so the save can run qpdf outside the engine thread.

## Consequences

- No extra program or shared library to ship. The qpdf version comes from `Cargo.lock`.
- A clean build compiles about 200 C and C++ files (36 to 61 s). A C++ compiler is needed on each build machine. Windows needs the MSVC build tools, which Tauri needs anyway.
- The vendored C libraries are IJG libjpeg 9f and zlib. Their notices go into `THIRD_PARTY_LICENSES` (ADR 0016).
- The spike checked only page counts and the qpdf structure check. The M3 save tests compare render and text before and after qpdf.
- The crate wraps only part of qpdf. Encryption and linearization options get checked when a milestone needs them.
