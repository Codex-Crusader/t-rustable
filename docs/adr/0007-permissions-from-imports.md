# 0007. Permissions come from imports

Status: Accepted (2026-10-16)

## Context

A permission list in a manifest is only a promise from the mod author. A WASM component declares the exact interfaces that it imports, and the host can read them before it runs any mod code.

## Decision

- Each WIT interface maps to one permission name. Milestone 2 gives the table.
- The host reads the imports of the component before instantiation.
- Install fails if the imports and the permissions in `manifest.json` do not match.
- The permission screen shows the permissions from the imports.
- On an update, if the permissions grow, the host asks the user again.
- No WASI file, network, or socket imports are permitted. Files come only through the host dialog `files.pick`.

## Consequences

- A mod cannot get more access than the user saw at install time.
- The WIT interfaces must be split along permission lines. One interface must not mix a safe function and a sensitive function.
- Milestone 1 must verify that wasmtime gives the component imports before instantiation.
