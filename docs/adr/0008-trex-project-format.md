# 0008. `.trex` project and journal format

Status: Accepted (2026-10-16)

## Context

Users want to save work in progress and continue later. The app must recover after a crash. Both need the original file and the list of changes.

## Decision

A `.trex` file is a zip with this content:

```
manifest.json      format version, app version, ID seed, document info
original.pdf       the original file, unchanged
commands.jsonl     one transaction per line
assets/<sha256>    images and fonts that commands use, named by SHA-256
```

- The crash journal has the same content in a folder, so the core can append cheaply.
- After each transaction, the core appends one line and flushes it to disk.
- On load, each command goes through `validate`. Each asset must match its SHA-256 name.
- `manifest.json` has a format version. A later milestone adds migrations.

## Consequences

- A `.trex` file contains the original document, including content that the user removed or redacted. The app must warn the user about this before it saves a `.trex` file.
- A `.trex` file can come from a hostile source. It gets the same zip validation as a `.tmod` file.
- A broken last line in a journal (from a crash) is ignored.
