# Architecture decision records

One file per major decision: `NNNN-title.md`. Each record has the sections Context, Decision, and Consequences.

Each accepted ADR is a locked decision. It changes only through a new ADR that the owner approves. The new ADR names the ADR that it replaces, and the old ADR gets the status "Superseded by NNNN".

| ADR | Title | Status |
|---|---|---|
| 0001 | PDF engine: PDFium and qpdf | Accepted |
| 0002 | Shell: Tauri 2 | Accepted |
| 0003 | UI: Svelte 5 and TypeScript | Accepted |
| 0004 | Command model | Accepted |
| 0005 | Stable IDs and PDF coordinates | Accepted |
| 0006 | Mods are WASM components | Accepted |
| 0007 | Permissions come from imports | Accepted |
| 0008 | `.trex` project and journal format | Accepted |
| 0009 | `.tmod` mod package format | Accepted |
| 0010 | Licensing split | Accepted |
| 0011 | Offline policy | Accepted |
| 0012 | Mod API evolution and the transaction resource | Accepted |
| 0013 | PDFium binding: our own thin binding over the C API | Accepted |
| 0014 | qpdf binding: the `qpdf` crate, built from source | Accepted |
| 0015 | Engine operations as data, and a check after each page rewrite | Accepted |
| 0016 | Licenses of native libraries inside PDFium and qpdf | Accepted (amends 0010) |
| 0017 | New text and images read upright, and mods can measure text | Accepted |
| 0018 | GUI threads, tiles, and the bridge to mods | Accepted |
