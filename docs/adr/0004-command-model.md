# 0004. Command model

Status: Accepted (2026-10-16)

## Context

The app needs undo, crash recovery, batch jobs, and mods that change documents. All of these need one clear way to change a document.

## Decision

- Every change is a core command. A command is plain data.
- Each command type has `validate`, `apply`, and `describe` handlers in a registry.
- `apply` is deterministic. It does not read the clock, a random source, or the file system.
- Commands are grouped in atomic transactions. If one command fails, nothing changes.
- Undo replays from the original document, with checkpoints to make it fast (`docs/ARCHITECTURE.md`, section 7).
- Mods cannot add command types. Mods submit transactions of core commands. The journal records core commands, not mod calls.

## Consequences

- Replay never needs a mod. A project file stays usable after a mod is removed.
- Each new editing feature needs a new core command. A mod that needs a new kind of change must wait until the core adds it.
- Each command type needs unit tests for all three handlers.
- Replay tests compare render output and text, not file bytes, because PDFium output bytes can change between versions.
