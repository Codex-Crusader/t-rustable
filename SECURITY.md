# Security

## Report a problem

Do not report a security problem in a public issue.

Use GitHub private vulnerability reporting: https://github.com/Codex-Crusader/t-rustable/security/advisories/new

Only you and the owner can see the report.

Give the version, the platform, the steps to reproduce, and a sample file if you can share it.

## Threat model

T-Rustable opens files from people that the user may not trust. The app must stay safe when a file is hostile. The app has no network access (ADR 0011), so every attack in this model comes through a file or through the build.

### In scope

| Threat | Entry point | Main defenses |
|---|---|---|
| Malicious PDF | The user opens a PDF. | PDFium and qpdf parse it on the engine thread. Encrypted files need the user's password. A Rust panic marks the session as broken. A native crash in PDFium or qpdf stops the app in v1, and the journal recovers the sessions on the next start. A later milestone moves the engine to a child process to contain both. Fuzz tests on input paths. The PDFium build has no JavaScript (V8) and no XFA support (ADR 0001). |
| Malicious `.tmod` | The user installs a mod package. | Strict zip validation: path traversal, zip bomb limits, Windows reserved names, case collisions, manifest schema, allowed file types. The host reads component imports before it instantiates the mod. The permission screen shows those imports. |
| Malicious mod code | An installed mod runs. | WASM sandbox. No WASI file, network, or socket access. Memory limit and epoch time limit per call. The host disables a mod after 3 failures in a row. Mods can only submit core commands, and the core validates each command. |
| Malicious `.trex` | The user opens a project or the app recovers a crash journal. | Same zip validation as `.tmod`. Each command in `commands.jsonl` goes through `validate` before `apply`. Assets must match their SHA-256 name. |
| Mod panel escape | A mod panel runs HTML and JavaScript. | The panel runs in an iframe with `sandbox="allow-scripts"` and a strict CSP. It has no Tauri IPC access. It can only send `postMessage` to the host. The host checks each message. |
| Supply chain | A dependency or build tool is compromised. | Permissive, reviewed dependencies only. `cargo deny` checks licenses, advisories, sources, and banned crates. Lock files are committed. CI actions are pinned. Release signing is a later milestone. |

### Out of scope

- A local attacker who has access to the user account. That attacker can change the app files, the mods folder, and the configuration directly.
- Attacks on the operating system, the webview, or the hardware.
- Data that the user shares on purpose, for example a `.trex` file. A `.trex` file contains the original document, and the app warns about this.

## Data safety

Redaction, "Cover and replace", and encrypted files have their own rules in `CLAUDE.md`, "Data safety rules". A failure of redaction verification is a security bug.
