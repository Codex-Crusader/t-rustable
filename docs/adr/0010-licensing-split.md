# 0010. Licensing split

Status: Accepted (2026-10-16). Amended by ADR 0016 (native libraries inside PDFium and qpdf).

## Context

The owner wants the app to be free for non-commercial use, and wants to keep the right to sell commercial licenses. Mod authors must be free to choose the license of their own mods.

## Decision

| Part | License |
|---|---|
| App code: core, engine, plugin host, GUI, CLI, built-in mods | PolyForm-Noncommercial-1.0.0 |
| `wit/`, `t-rustable-sdk`, `mods/template/`, `mods/examples/` | MIT |
| `docs/mod-api/` | MIT |
| Other `docs/` | CC-BY-NC-4.0 |
| Name and logos (`branding/`) | Not licensed (`TRADEMARKS.md`, `LicenseRef-T-Rustable-Trademark`) |

- `REUSE.toml` gives the license of each file. `reuse lint` runs in CI. When more than one entry matches a file, the last entry wins.
- `mods/builtin/` is not on the MIT list, so it is PolyForm.
- MIT parts never depend on PolyForm parts.
- Dependencies: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, MPL-2.0. `cargo deny` and an npm license check enforce this. Exceptions only for our own crates.
- `Unicode-3.0` is also allowed. It is a permissive license for Unicode data. `unicode-ident` uses it, and almost every Rust project depends on that crate. The owner approved this on 2026-10-16.
- `Apache-2.0 WITH LLVM-exception` is also allowed. wasmtime and Cranelift use it. The exception only removes conditions from Apache-2.0. A test on 2026-10-16 with `wasmtime` and `wasmtime-wasi` in the tree passed the license and ban checks.
- No AGPL or GPL dependencies.
- Only OFL fonts, with their license files.
- `cargo-about` makes `THIRD_PARTY_LICENSES` in CI.
- Contributors sign a CLA that grants a license, not an assignment (`CLA.md`).
- The project is "source-available, free for non-commercial use". Never "open source".

## Consequences

- Mods can use any license, because the API and the SDK are MIT.
- PDFium and qpdf binaries are not Rust crates. Their licenses need a manual entry in `THIRD_PARTY_LICENSES` (ADR 0001). ADR 0016 lists the licenses of the native libraries inside them.
- `CLA.md` is adapted from the Harmony CLA, so it is CC-BY-3.0.

## Resolved questions (2026-10-16)

The owner said: "do whatever is best long term". The owner also wants to say that the app is free and that nobody can make a paid product of it.

1. wasmtime license: allowed, as above.
2. Code samples in the docs: `docs/mod-api/` is MIT. A mod author can copy a sample into any mod, also a paid mod.
3. Main license: PolyForm Noncommercial stays. PolyForm Noncommercial blocks paid products. But it is not "free for everyone", because a company cannot use the app for its business without a separate license. Other licenses that permit business use and block resale (PolyForm Perimeter, PolyForm Shield, the Commons Clause) are not on the SPDX list, and tools such as GitHub, REUSE, and `cargo deny` do not recognize them. The main reason is that the change is one way. The owner can make the license more permissive later, or give a company a license, because the CLA permits relicensing. The owner cannot make the license stricter for versions that people already have. The README therefore says "free for non-commercial use", and it does not say "free for everyone".
