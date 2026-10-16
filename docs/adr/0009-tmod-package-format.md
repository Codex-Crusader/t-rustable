# 0009. `.tmod` mod package format

Status: Accepted (2026-10-16)

## Context

Users install mods from files that they download. A zip file can attack the installer, for example with path traversal, a zip bomb, or names that Windows cannot handle.

## Decision

A `.tmod` file is a zip with a `manifest.json`, one WASM component, and optional UI files for panels.

The installer rejects the package if one of these checks fails:

- Paths: no absolute paths, no `..`, no backslashes, no drive letters.
- Size: limits for the total unpacked size, the file count, and the compression ratio.
- Windows: no reserved names (for example `CON` and `NUL`) and no two names that differ only in case.
- Manifest: valid against the JSON Schema from Milestone 2.
- Component: a valid WASM component whose imports match the manifest permissions (ADR 0007).
- UI files: only permitted file types, for example HTML, CSS, JavaScript, images, and fonts.

The app installs a mod to `mods/<id>/` and stores the SHA-256 of the package. Developer mode loads unpacked folders and shows a marker in the UI. There is no mod store. Signing comes in a later milestone.

## Consequences

- The validation code is security-critical. It gets fuzz tests in Milestone 5.
- Mod authors must follow strict rules for file names.
