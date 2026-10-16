# 0010. Licensing split

Status: Accepted (2026-10-16), with open questions

## Context

The owner wants the app to be free for non-commercial use, and wants to keep the right to sell commercial licenses. Mod authors must be free to choose the license of their own mods.

## Decision

| Part | License |
|---|---|
| App code: core, engine, plugin host, GUI, CLI, built-in mods | PolyForm-Noncommercial-1.0.0 |
| `wit/`, `t-rustable-sdk`, `mods/template/`, `mods/examples/` | MIT |
| `docs/` | CC-BY-NC-4.0 |
| Name and logos | Not licensed (`TRADEMARKS.md`) |

- `REUSE.toml` gives the license of each file. `reuse lint` runs in CI. When more than one entry matches a file, the last entry wins.
- `mods/builtin/` is not on the MIT list, so it is PolyForm.
- MIT parts never depend on PolyForm parts.
- Dependencies: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0. `cargo deny` and an npm license check enforce this. Exceptions only for our own crates.
- `Unicode-3.0` is also allowed. It is a permissive license for Unicode data. `unicode-ident` uses it, and almost every Rust project depends on that crate. The owner approved this on 2026-10-16.
- No AGPL or GPL dependencies.
- Only OFL fonts, with their license files.
- `cargo-about` makes `THIRD_PARTY_LICENSES` in CI.
- Contributors sign a CLA that grants a license, not an assignment (`CLA.md`).
- The project is "source-available, free for non-commercial use". Never "open source".

## Consequences

- Mods can use any license, because the API and the SDK are MIT.
- PDFium and qpdf binaries are not Rust crates. Their licenses need a manual entry in `THIRD_PARTY_LICENSES` (ADR 0001).
- `CLA.md` is adapted from the Harmony CLA, so it is CC-BY-3.0.

## Open questions for the owner

1. wasmtime license. The wasmtime crates use the expression `Apache-2.0 WITH LLVM-exception`. The current allow list does not accept it, so `cargo deny` will fail in Milestone 1. The LLVM exception only removes conditions. Proposal: add `Apache-2.0 WITH LLVM-exception` to the allow list.
2. Code samples in `docs/`. All of `docs/` is CC-BY-NC-4.0. The code samples in `docs/mod-api/` (for example the guide "Your first mod in 15 minutes") then have a non-commercial restriction. A mod author who copies a sample into a commercial mod breaks that license. Proposal: put the full sample code in `mods/examples/` (MIT) and refer to it from the docs, or mark `docs/mod-api/**` as MIT in `REUSE.toml`.
