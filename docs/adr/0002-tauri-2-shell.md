# 0002. Shell: Tauri 2

Status: Accepted (2026-10-16)

## Context

The app needs a desktop GUI for Linux and Windows. The core is Rust. Mod panels need HTML and JavaScript in a sandbox.

Options:

- Electron: large download, a second runtime (Node.js), and a large attack surface.
- Native Rust GUI toolkits (for example egui, iced, Slint): small, but weaker for accessibility, text layout, and sandboxed HTML panels.
- Tauri 2: Rust back end, system webview, a capability system that limits what the front end can call, and custom URI schemes.

## Decision

Use Tauri 2. The webview shows the Svelte UI. The Rust side is a thin layer over `t-rustable-core`.

- Capabilities allow only dialogs and files that the user picks.
- No `http`, `shell`, or `updater` plugins (ADR 0011).
- Tiles load through a custom URI scheme (`docs/ARCHITECTURE.md`, section 11).

## Consequences

- The webview is different on each platform: WebKitGTK on Linux and WebView2 on Windows. The UI must be tested on both.
- Milestone 1 must prove that a sandboxed iframe in the webview cannot call Tauri commands and cannot read data of the parent page.
- Accessibility depends on the webview and its connection to the screen readers of each platform.
- On Windows, Tauri builds need the MSVC build tools.
