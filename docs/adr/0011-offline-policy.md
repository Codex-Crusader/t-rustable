# 0011. Offline policy

Status: Accepted (2026-10-16)

## Context

Users open private documents. The app must never send data out, and users must be able to trust this without a network monitor. Each network feature also adds an attack surface.

## Decision

The app has no network access.

- No HTTP client crates anywhere in the dependency tree. `deny.toml` bans `reqwest`, `ureq`, `hyper`, `hyper-util`, `surf`, `isahc`, `attohttpc`, `minreq`, `curl`, and `curl-sys`. CI fails if one appears.
- No Tauri `http`, `shell`, or `updater` plugins. `deny.toml` bans them too. Capabilities allow only dialogs and files that the user picks.
- CSP `connect-src 'none'`. No remote scripts or assets. Everything is bundled.
- No auto-updater, no telemetry, no crash reporter. Errors go to a local log.
- Mods get no WASI network or socket access. `deny.toml` bans `wasmtime-wasi-http`.
- If the engine moves to a child process later, it uses stdin and stdout pipes, never a TCP socket.

## Consequences

- Users update the app by hand.
- For a bug report, the user must send the local log.
- A dependency that pulls in an HTTP client is blocked, even if the app never uses it. Milestone 1 checks the real dependency trees of Tauri and wasmtime against the ban list.
- Tauri 2 can send `invoke` as a fetch request to an IPC origin. Milestone 1 must verify that `invoke` works with `connect-src 'none'`. If it does not, the CSP change needs a new ADR.
- The ban list is a list of names. It does not catch a new HTTP crate with a different name. Reviewers must check new dependencies.
