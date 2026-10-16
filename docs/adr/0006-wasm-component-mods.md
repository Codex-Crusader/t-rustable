# 0006. Mods are WASM components

Status: Accepted (2026-10-16)

## Context

Modding is the main feature. Mods come from people that the user may not trust. Mods must not get access to files, the network, or other mods. Mod authors want to use more than one language.

Options:

- Native plugins (shared libraries): full system access. Not safe.
- A scripting language in the app (for example Lua or JavaScript): one language, and the sandbox is harder to make correct.
- WebAssembly components on wasmtime: a strong sandbox, an API defined in WIT, and support for more than one source language.

## Decision

- Mods are WASM components that run on wasmtime.
- The API is the WIT package `t-rustable:api@<semver>` in `wit/`.
- Two worlds: `interactive` (GUI) and `batch` (CLI). A mod can export both.
- Built-in tools are mods and use the same API. If a built-in tool needs something that the API does not give, the API is incomplete, and we fix the API.
- Each call has a memory limit and a time limit (epoch interruption). A mod that fails 3 times in a row is disabled.
- Documents are WIT resources, so a batch job can use more than one document.
- No mod-to-mod calls in v1.
- Within a major version, only additive changes. The host keeps support for old majors.

## Consequences

- Rust mods work well today. Before the docs promise JavaScript or Python mods, we must check the maturity of ComponentizeJS and componentize-py.
- Each call between the host and a mod copies data. The API must avoid large data in frequent calls, for example on each pointer move.
- wasmtime uses `Apache-2.0 WITH LLVM-exception`. ADR 0010 allows it.
